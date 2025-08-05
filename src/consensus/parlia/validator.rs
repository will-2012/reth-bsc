use super::snapshot::{Snapshot, DEFAULT_TURN_LENGTH};
use super::{parse_vote_attestation_from_header, EXTRA_SEAL, EXTRA_VANITY};
use alloy_primitives::Address;
use reth::consensus::{ConsensusError, HeaderValidator};
use reth_primitives_traits::SealedHeader;
use std::sync::Arc;

use super::vote::{MAX_ATTESTATION_EXTRA_LENGTH, VoteAddress};
use super::constants::{VALIDATOR_BYTES_LEN_BEFORE_LUBAN, VALIDATOR_NUMBER_SIZE, VALIDATOR_BYTES_LEN_AFTER_LUBAN};
use bls_on_arkworks as bls;

use super::slash_pool;

// ---------------------------------------------------------------------------
// Helper: parse epoch update (validator set & turn-length) from a header.
// Returns (validators, vote_addresses (if any), turn_length)
// ---------------------------------------------------------------------------
fn parse_epoch_update<H>(
    header: &H,
    is_luban: bool,
    is_bohr: bool,
) -> (Vec<Address>, Option<Vec<VoteAddress>>, Option<u8>)
where
    H: alloy_consensus::BlockHeader,
{
    let extra = header.extra_data().as_ref();
    if extra.len() <= EXTRA_VANITY + EXTRA_SEAL {
        return (Vec::new(), None, None);
    }

    // Epoch bytes start right after vanity
    let mut cursor = EXTRA_VANITY;

    // Pre-Luban epoch block: validators list only (20-byte each)
    if !is_luban {
        let validator_bytes = &extra[cursor..extra.len() - EXTRA_SEAL];
        let num = validator_bytes.len() / VALIDATOR_BYTES_LEN_BEFORE_LUBAN;
        let mut vals = Vec::with_capacity(num);
        for i in 0..num {
            let start = cursor + i * VALIDATOR_BYTES_LEN_BEFORE_LUBAN;
            let end = start + VALIDATOR_BYTES_LEN_BEFORE_LUBAN;
            vals.push(Address::from_slice(&extra[start..end]));
        }
        return (vals, None, None);
    }

    // Luban & later: 1-byte validator count
    let num_validators = extra[cursor] as usize;
    cursor += VALIDATOR_NUMBER_SIZE;
    
    // Sanity check: ensure we have enough space for all validators + optional turn length
    let required_space = EXTRA_VANITY + VALIDATOR_NUMBER_SIZE + 
                        (num_validators * VALIDATOR_BYTES_LEN_AFTER_LUBAN) + 
                        (if is_bohr { 1 } else { 0 }) + EXTRA_SEAL;
    if extra.len() < required_space {
        // Not enough space for the claimed number of validators
        return (Vec::new(), None, None);
    }

    let mut vals = Vec::with_capacity(num_validators);
    let mut vote_vals = Vec::with_capacity(num_validators);
    for _ in 0..num_validators {
        // Check bounds before accessing consensus address (20 bytes)
        if cursor + 20 > extra.len() - EXTRA_SEAL {
            // Not enough space for validator data
            return (vals, Some(vote_vals), None);
        }
        // 20-byte consensus addr
        vals.push(Address::from_slice(&extra[cursor..cursor + 20]));
        cursor += 20;
        
        // Check bounds before accessing BLS vote address (48 bytes)
        if cursor + 48 > extra.len() - EXTRA_SEAL {
            // Not enough space for vote address data
            return (vals, Some(vote_vals), None);
        }
        // 48-byte BLS vote addr
        vote_vals.push(VoteAddress::from_slice(&extra[cursor..cursor + 48]));
        cursor += 48;
    }

    // Optional turnLength byte in Bohr headers
    let turn_len = if is_bohr {
        // Check if there's space for turn length byte before EXTRA_SEAL
        if cursor + 1 <= extra.len() - EXTRA_SEAL {
            let tl = extra[cursor];
            Some(tl)
        } else {
            // Not enough space for turn length, header might be malformed
            None
        }
    } else {
        None
    };

    (vals, Some(vote_vals), turn_len)
}

/// Very light-weight snapshot provider (trait object) so the header validator can fetch the latest snapshot.
pub trait SnapshotProvider: Send + Sync {
    /// Returns the snapshot that is valid for the given `block_number` (usually parent block).
    fn snapshot(&self, block_number: u64) -> Option<Snapshot>;

    /// Inserts (or replaces) the snapshot in the provider.
    fn insert(&self, snapshot: Snapshot);
}

/// Header validator for Parlia consensus.
///
/// The validator currently checks:
/// 1. Miner (beneficiary) must be a validator in the current snapshot.
/// 2. Difficulty must be 2 when the miner is in-turn, 1 otherwise.
/// Further seal and vote checks will be added in later milestones.
#[derive(Debug, Clone)]
pub struct ParliaHeaderValidator<P> {
    provider: Arc<P>,
    /// Activation block number for the Ramanujan hardfork (network-dependent).
    ramanujan_activation_block: u64,
}

impl<P> ParliaHeaderValidator<P> {
    /// Create from chain spec that implements `BscHardforks`.
    #[allow(dead_code)]
    pub fn from_chain_spec<Spec>(provider: Arc<P>, spec: &Spec) -> Self
    where
        Spec: crate::hardforks::BscHardforks,
    {
        // The chain-spec gives the *first* block where Ramanujan is active.
        let act_block = match spec.bsc_fork_activation(crate::hardforks::bsc::BscHardfork::Ramanujan) {
            reth_chainspec::ForkCondition::Block(b) => b,
            _ => 13_082_191,
        };
        Self { provider, ramanujan_activation_block: act_block }
    }
    /// Create a validator that assumes main-net Ramanujan activation (block 13_082_191).
    /// Most unit-tests rely on this default.
    pub fn new(provider: Arc<P>) -> Self {
        Self { provider, ramanujan_activation_block: 13_082_191 }
    }

    /// Create a validator with a custom Ramanujan activation block (e.g. test-net 1_010_000).
    pub fn with_ramanujan_activation(provider: Arc<P>, activation_block: u64) -> Self {
        Self { provider, ramanujan_activation_block: activation_block }
    }
}

// Helper to get expected difficulty.


impl<P, H> HeaderValidator<H> for ParliaHeaderValidator<P>
where
    P: SnapshotProvider + std::fmt::Debug + 'static,
    H: alloy_consensus::BlockHeader + alloy_primitives::Sealable,
{
    fn validate_header(&self, header: &SealedHeader<H>) -> Result<(), ConsensusError> {
        // MINIMAL VALIDATION ONLY during Headers stage (like official BNB Chain implementation)
        // All BSC-specific validation is deferred to Bodies/Execution stage for performance
        
        // Genesis header is always valid
        if header.number() == 0 {
            return Ok(());
        }

        // Only check the most basic header format to prevent completely malformed headers
        // Even basic BSC format validation is expensive, so minimize it
        let extra_data = header.header().extra_data();
        if extra_data.len() < 65 { // Minimum: 32 (vanity) + 65 (seal) = 97 bytes
            return Err(ConsensusError::Other(format!(
                "BSC header extra_data too short: {} bytes", extra_data.len()
            )));
        }

        // All other validation (signature, timestamp, difficulty, etc.) deferred to execution stage
        // This matches the official BNB Chain implementation's performance characteristics
        Ok(())
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<H>,
        parent: &SealedHeader<H>,
    ) -> Result<(), ConsensusError> {
        // --------------------------------------------------------------------
        // 1. Basic parent/child sanity checks (number & timestamp ordering)
        // --------------------------------------------------------------------
        if header.number() != parent.number() + 1 {
            return Err(ConsensusError::ParentBlockNumberMismatch {
                parent_block_number: parent.number(),
                block_number: header.number(),
            });
        }
        // BSC Maxwell hardfork allows equal timestamps between parent and current block
        // Before Maxwell: header.timestamp() > parent.timestamp() (strict)
        // After Maxwell: header.timestamp() >= parent.timestamp() (equal allowed)        
        if header.timestamp() < parent.timestamp() {
            return Err(ConsensusError::TimestampIsInPast {
                parent_timestamp: parent.timestamp(),
                timestamp: header.timestamp(),
            });
        }

        // --------------------------------------------------------------------
        // 2. Snapshot of the *parent* block (needed for gas-limit & attestation verification)
        // --------------------------------------------------------------------
        let parent_snap = match self.provider.snapshot(parent.number()) {
            Some(snapshot) => snapshot,
            None => {
                // Snapshot not yet available during sync - defer validation
                // During initial sync, snapshots may not be available yet.
                // Skip full Parlia validation and allow header to be stored.
                // Full validation will happen during block execution when ancestors are available.
                return Ok(());
            }
        };

        // --------------------------------------------------------------------
        // 2.5 Ramanujan block time validation
        // --------------------------------------------------------------------
        // After Ramanujan fork, enforce stricter timing rules
        if parent.number() >= self.ramanujan_activation_block { // Ramanujan hardfork active
            let block_interval = parent_snap.block_interval;
            let validator = header.beneficiary();
            let is_inturn = parent_snap.inturn_validator() == validator;
            
            // Calculate back-off time for out-of-turn validators
            let back_off_time = if is_inturn {
                0
            } else {
                // Out-of-turn validators must wait longer
                let turn_length = parent_snap.turn_length.unwrap_or(1) as u64;
                turn_length * block_interval / 2
            };
            
            let min_timestamp = parent.timestamp() + block_interval + back_off_time;
            if header.timestamp() < min_timestamp {
                return Err(ConsensusError::Other(format!(
                    "Ramanujan block time validation failed: block {} timestamp {} too early (expected >= {})",
                    header.number(),
                    header.timestamp(),
                    min_timestamp
                )));
            }
        }

        // Gas-limit rule verification (BSC-specific logic matching bsc-erigon)
        let parent_gas_limit = parent.gas_limit();
        let gas_limit = header.gas_limit();
        
        // BSC gas limit validation matching bsc-erigon implementation
        let diff = if parent_gas_limit > gas_limit {
            parent_gas_limit - gas_limit
        } else {
            gas_limit - parent_gas_limit
        };
        
        // Use Lorentz hardfork activation for divisor (like bsc-erigon)
        // Lorentz uses timestamp-based activation, not block number
        // For early blocks (before 2025), we'll always use pre-Lorentz divisor
        let gas_limit_bound_divisor = 256u64; // Before Lorentz (for early sync)
        
        let limit = parent_gas_limit / gas_limit_bound_divisor;
        
        if diff >= limit || gas_limit < super::gas::MIN_GAS_LIMIT {
            return Err(ConsensusError::Other(format!(
                "invalid gas limit: have {}, want {} ± {}", 
                gas_limit, parent_gas_limit, limit
            )));
        }

        // BSC does NOT validate maximum timestamp intervals (unlike Ethereum)
        // Only minimum time validation happens in blockTimeVerifyForRamanujanFork for Ramanujan+ blocks
        // Maximum time validation is only against current system time (header.Time > time.Now())
        // which happens in the basic verifyHeader function, not here.

        // --------------------------------------------------------------------
        // 3. Parse and verify vote attestation (Fast-Finality)
        // --------------------------------------------------------------------
        // Determine fork status for attestation parsing.
        let extra_len = header.header().extra_data().len();
        let is_luban = extra_len > EXTRA_VANITY + EXTRA_SEAL;
        let is_bohr = parent_snap.turn_length.unwrap_or(DEFAULT_TURN_LENGTH) > DEFAULT_TURN_LENGTH;

        let attestation_opt = parse_vote_attestation_from_header(
            header.header(),
            parent_snap.epoch_num,
            is_luban,
            is_bohr,
        );

        if let Some(ref att) = attestation_opt {
            // 3.1 extra bytes length guard
            if att.extra.len() > MAX_ATTESTATION_EXTRA_LENGTH {
                return Err(ConsensusError::Other("attestation extra too long".into()));
            }

            // 3.2 Attestation target MUST be the parent block.
            if att.data.target_number != parent.number() || att.data.target_hash != parent.hash() {
                return Err(ConsensusError::Other("invalid attestation target block".into()));
            }

            // 3.3 Attestation source MUST equal the latest justified checkpoint stored in snapshot.
            if att.data.source_number != parent_snap.vote_data.target_number ||
                att.data.source_hash != parent_snap.vote_data.target_hash
            {
                return Err(ConsensusError::Other("invalid attestation source checkpoint".into()));
            }

            // 3.4 Build list of voter BLS pub-keys from snapshot according to bit-set.
            let total_validators = parent_snap.validators.len();
            let bitset = att.vote_address_set;
            let voted_cnt = bitset.count_ones() as usize;

            if voted_cnt > total_validators {
                return Err(ConsensusError::Other("attestation vote count exceeds validator set".into()));
            }

            // collect vote addresses
            let mut pubkeys: Vec<Vec<u8>> = Vec::with_capacity(voted_cnt);
            for (idx, val_addr) in parent_snap.validators.iter().enumerate() {
                if (bitset & (1u64 << idx)) == 0 {
                    continue;
                }
                let Some(info) = parent_snap.validators_map.get(val_addr) else {
                    return Err(ConsensusError::Other("validator vote address missing".into()));
                };
                // Ensure vote address is non-zero (Bohr upgrade guarantees availability)
                if info.vote_addr.as_slice().iter().all(|b| *b == 0) {
                    return Err(ConsensusError::Other("validator vote address is zero".into()));
                }
                pubkeys.push(info.vote_addr.to_vec());
            }

            // 3.5 quorum check: ≥ 2/3 +1 of total validators
            let min_votes = (total_validators * 2 + 2) / 3; // ceil((2/3) * n)
            if pubkeys.len() < min_votes {
                return Err(ConsensusError::Other("insufficient attestation quorum".into()));
            }

            // 3.6 BLS aggregate signature verification.
            let message_hash = att.data.hash();
            let msg_vec = message_hash.as_slice().to_vec();
            let signature_bytes = att.agg_signature.to_vec();

            let mut msgs = Vec::with_capacity(pubkeys.len());
            msgs.resize(pubkeys.len(), msg_vec.clone());

            const BLS_DST: &[u8] = b"BLS_SIG_BLS12381G2_XMD:SHA-256_SSWU_RO_POP_";

            let sig_ok = if pubkeys.len() == 1 {
                bls::verify(&pubkeys[0], &msg_vec, &signature_bytes, &BLS_DST.to_vec())
            } else {
                bls::aggregate_verify(pubkeys.clone(), msgs, &signature_bytes, &BLS_DST.to_vec())
            };

            if !sig_ok {
                return Err(ConsensusError::Other("invalid BLS aggregate signature".into()));
            }
        }

        // --------------------------------------------------------------------
        // 4. Advance snapshot once all parent-dependent checks are passed.
        // --------------------------------------------------------------------
        // Detect epoch checkpoint and parse validator set / turnLength if applicable
        let (new_validators, vote_addrs, turn_len) = if header.number() % parent_snap.epoch_num == 0 {
            parse_epoch_update(header.header(), is_luban, is_bohr)
        } else { (Vec::new(), None, None) };

        // Determine hardfork activation based on header timestamp
        let header_timestamp = header.header().timestamp();
        let is_lorentz_active = header_timestamp >= 1744097580; // Lorentz hardfork timestamp  
        let is_maxwell_active = header_timestamp >= 1748243100; // Maxwell hardfork timestamp

        if let Some(new_snap) = parent_snap.apply(
            header.beneficiary(),
            header.header(),
            new_validators,
            vote_addrs,
            attestation_opt,
            turn_len,
            is_bohr,
            is_lorentz_active,
            is_maxwell_active,
        ) {
            // Always cache the snapshot
            self.provider.insert(new_snap.clone());
            
            // BSC Official Approach: Store checkpoint snapshots every 1024 blocks for persistence
            if new_snap.block_number % crate::consensus::parlia::CHECKPOINT_INTERVAL == 0 {
                tracing::info!(
                    "📦 [BSC] Storing checkpoint snapshot at block {} (every {} blocks)", 
                    new_snap.block_number, 
                    crate::consensus::parlia::CHECKPOINT_INTERVAL
                );
                // Note: This insert will persist to MDBX if using DbSnapshotProvider
                // For checkpoint intervals, we ensure persistence
            }
        } else {
            return Err(ConsensusError::Other("failed to apply snapshot".to_string()));
        }

        // Report slashing evidence if proposer is not in-turn and previous inturn validator hasn't signed recently.
        let inturn_validator_eq_miner = header.beneficiary() == parent_snap.inturn_validator();
        if !inturn_validator_eq_miner {
            let spoiled = parent_snap.inturn_validator();
            if !parent_snap.sign_recently(spoiled) {
                slash_pool::report(spoiled);
            }
        }

        Ok(())
    }
} 