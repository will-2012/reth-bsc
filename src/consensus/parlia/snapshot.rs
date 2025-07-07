use std::collections::{BTreeMap, HashMap};

use super::vote::{VoteAddress, VoteAttestation, VoteData};
use alloy_primitives::{Address, BlockNumber, B256};
use serde::{Deserialize, Serialize};
use reth_db::table::{Compress, Decompress};
use reth_db::DatabaseError;
use bytes::BufMut;

/// Number of blocks after which we persist snapshots to DB.
pub const CHECKPOINT_INTERVAL: u64 = 1024;

/// `ValidatorInfo` holds metadata for a validator at a given epoch.
#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    /// 1-based index (offset by +1) within `validators` list.
    pub index: u64,
    /// Validator's BLS vote address (optional before Bohr upgrade; zero bytes if unknown).
    pub vote_addr: VoteAddress,
}

/// In-memory snapshot of Parlia epoch state.
#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Current epoch length. (200 for legacy, changes after Bohr).
    pub epoch_num: u64,
    /// Block number of the epoch boundary.
    pub block_number: BlockNumber,
    /// Hash of that block.
    pub block_hash: B256,
    /// Sorted validator set (ascending by address).
    pub validators: Vec<Address>,
    /// Extra information about validators (index + vote addr).
    pub validators_map: HashMap<Address, ValidatorInfo>,
    /// Map of recent proposers: block → proposer address.
    pub recent_proposers: BTreeMap<BlockNumber, Address>,
    /// Latest vote data attested by the validator set.
    pub vote_data: VoteData,
    /// Configurable turn-length (default = 1 before Bohr).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_length: Option<u8>,
}

impl Snapshot {
    /// Create a brand-new snapshot at an epoch boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mut validators: Vec<Address>,
        block_number: BlockNumber,
        block_hash: B256,
        epoch_num: u64,
        vote_addrs: Option<Vec<VoteAddress>>, // one-to-one with `validators`
    ) -> Self {
        // Keep validators sorted.
        validators.sort();

        let mut validators_map = HashMap::new();
        if let Some(vote_addrs) = vote_addrs {
            assert_eq!(
                validators.len(),
                vote_addrs.len(),
                "validators and vote_addrs length not equal",
            );

            for (i, v) in validators.iter().enumerate() {
                let info = ValidatorInfo { index: i as u64 + 1, vote_addr: vote_addrs[i] };
                validators_map.insert(*v, info);
            }
        } else {
            // Pre-Bohr, vote addresses are unknown.
            for v in &validators {
                validators_map.insert(*v, Default::default());
            }
        }

        Self {
            epoch_num,
            block_number,
            block_hash,
            validators,
            validators_map,
            recent_proposers: Default::default(),
            vote_data: Default::default(),
            turn_length: Some(1),
        }
    }

    /// Apply `next_header` (proposed by `validator`) plus any epoch changes to produce a new snapshot.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &self,
        validator: Address,
        next_header: &alloy_consensus::Header,
        mut new_validators: Vec<Address>,
        vote_addrs: Option<Vec<VoteAddress>>, // for epoch switch
        attestation: Option<VoteAttestation>,
        turn_length: Option<u8>,
        is_bohr: bool,
    ) -> Option<Self> {
        let block_number = next_header.number;
        if self.block_number + 1 != block_number {
            return None; // non-continuous block
        }

        // Clone base.
        let mut snap = self.clone();
        snap.block_hash = next_header.hash_slow();
        snap.block_number = block_number;

        // Maintain recent proposer window.
        let limit = self.miner_history_check_len() + 1;
        if block_number >= limit {
            snap.recent_proposers.remove(&(block_number - limit));
        }

        // Validate proposer belongs to validator set and hasn't over-proposed.
        if !snap.validators.contains(&validator) {
            return None;
        }
        if snap.sign_recently(validator) {
            return None;
        }
        snap.recent_proposers.insert(block_number, validator);

        // Epoch change.
        let epoch_key = u64::MAX - next_header.number / snap.epoch_num;
        if !new_validators.is_empty() && (!is_bohr || !snap.recent_proposers.contains_key(&epoch_key)) {
            new_validators.sort();
            if let Some(tl) = turn_length { snap.turn_length = Some(tl) }

            if is_bohr {
                snap.recent_proposers = Default::default();
                snap.recent_proposers.insert(epoch_key, Address::default());
            } else {
                let new_limit = (new_validators.len() / 2 + 1) as u64;
                if new_limit < limit {
                    for i in 0..(limit - new_limit) {
                        snap.recent_proposers.remove(&(block_number - new_limit - i));
                    }
                }
            }

            // Build new validators map.
            let mut validators_map = HashMap::new();
            if let Some(vote_addrs) = vote_addrs {
                assert_eq!(
                    new_validators.len(),
                    vote_addrs.len(),
                    "validators and vote_addrs length not equal",
                );

                for (i, v) in new_validators.iter().enumerate() {
                    validators_map.insert(*v, ValidatorInfo { index: i as u64 + 1, vote_addr: vote_addrs[i] });
                }
            } else {
                for v in &new_validators { validators_map.insert(*v, Default::default()); }
            }
            snap.validators = new_validators;
            snap.validators_map = validators_map;
        }

        if let Some(att) = attestation { snap.vote_data = att.data; }

        Some(snap)
    }

    /// Returns `true` if `proposer` is in-turn according to snapshot rules.
    pub fn is_inturn(&self, proposer: Address) -> bool { self.inturn_validator() == proposer }

    /// Number of blocks to look back when checking proposer history.
    pub fn miner_history_check_len(&self) -> u64 {
        let turn = u64::from(self.turn_length.unwrap_or(1));
        (self.validators.len() / 2 + 1) as u64 * turn - 1
    }

    /// Validator that should propose the **next** block.
    pub fn inturn_validator(&self) -> Address {
        let turn = u64::from(self.turn_length.unwrap_or(1));
        self.validators[((self.block_number + 1) / turn) as usize % self.validators.len()]
    }

    /// Returns index in `validators` for `validator` if present.
    pub fn index_of(&self, validator: Address) -> Option<usize> {
        self.validators.iter().position(|&v| v == validator)
    }

    /// Count how many times each validator has signed in the recent window.
    pub fn count_recent_proposers(&self) -> HashMap<Address, u8> {
        let left_bound = if self.block_number > self.miner_history_check_len() {
            self.block_number - self.miner_history_check_len()
        } else { 0 };
        let mut counts = HashMap::new();
        for (&block, &v) in &self.recent_proposers {
            if block <= left_bound || v == Address::default() { continue; }
            *counts.entry(v).or_insert(0) += 1;
        }
        counts
    }

    /// Returns `true` if `validator` has signed too many blocks recently.
    pub fn sign_recently(&self, validator: Address) -> bool {
        self.sign_recently_by_counts(validator, &self.count_recent_proposers())
    }

    /// Helper that takes pre-computed counts.
    pub fn sign_recently_by_counts(&self, validator: Address, counts: &HashMap<Address, u8>) -> bool {
        if let Some(&times) = counts.get(&validator) {
            let allowed = u64::from(self.turn_length.unwrap_or(1));
            if u64::from(times) >= allowed { return true; }
        }
        false
    }
}

// ---------------------------------------------------------------------------
// DB compression helpers (same approach as zoro_reth)
// ---------------------------------------------------------------------------

impl Compress for Snapshot {
    type Compressed = Vec<u8>;

    fn compress(self) -> Self::Compressed { serde_cbor::to_vec(&self).expect("serialize Snapshot") }

    fn compress_to_buf<B: bytes::BufMut + AsMut<[u8]>>(&self, buf: &mut B) {
        let bytes = self.clone().compress();
        buf.put_slice(&bytes);
    }
}

impl Decompress for Snapshot {
    fn decompress(value: &[u8]) -> Result<Self, DatabaseError> {
        serde_cbor::from_slice(value).map_err(|_| DatabaseError::Decode)
    }
} 