use std::sync::Arc;
use lazy_static::lazy_static;
use std::sync::RwLock;

use schnellru::LruMap;
use schnellru::ByLength;
use alloy_primitives::{Address, B256};
use secp256k1::{SECP256K1, Message, ecdsa::{RecoveryId, RecoverableSignature}};
use crate::hardforks::BscHardforks;
use reth_chainspec::EthChainSpec;
use alloy_consensus::{Header, BlockHeader};
use alloy_rlp::Decodable;
use super::{
    VoteAttestation, ParliaConsensusError,
    constants::{
        EXTRA_VANITY, EXTRA_SEAL, VALIDATOR_NUMBER_SIZE, 
        VALIDATOR_BYTES_LEN_AFTER_LUBAN, VALIDATOR_BYTES_LEN_BEFORE_LUBAN, TURN_LENGTH_SIZE
    },
    hash_with_chain_id
};

const RECOVERED_PROPOSER_CACHE_NUM: usize = 4096;

lazy_static! {
    // recovered proposer cache map by block_number: proposer_address
    static ref RECOVERED_PROPOSER_CACHE: RwLock<LruMap<B256, Address, ByLength>> = RwLock::new(LruMap::new(ByLength::new(RECOVERED_PROPOSER_CACHE_NUM as u32)));
}

pub struct Parlia<ChainSpec> {
    chain_spec: Arc<ChainSpec>,
    epoch: u64, // The epoch number
    // period: u64, // The period of block proposal
}

impl<ChainSpec> Parlia<ChainSpec> 
where ChainSpec: EthChainSpec + BscHardforks + 'static, 
{
    pub fn new(chain_spec: Arc<ChainSpec>, epoch: u64) -> Self {
        Self { chain_spec, epoch }
    }

    /// Get epoch length from header
    pub fn get_epoch_length(&self, header: &Header) -> u64 {
        if self.chain_spec.is_maxwell_active_at_timestamp(header.timestamp()) {
            return crate::consensus::parlia::snapshot::MAXWELL_EPOCH_LENGTH;
        }
        if self.chain_spec.is_lorentz_active_at_timestamp(header.timestamp()) {
            return crate::consensus::parlia::snapshot::LORENTZ_EPOCH_LENGTH;
        }
        self.epoch
    }

    /// Get validator bytes from header extra data
    pub fn get_validator_bytes_from_header(&self, header: &Header) -> Option<Vec<u8>> {
        let extra_len = header.extra_data.len();
        if extra_len <= EXTRA_VANITY + EXTRA_SEAL {
            return None;
        }

        let is_luban_active = self.chain_spec.is_luban_active_at_block(header.number);
        let is_epoch = header.number % self.get_epoch_length(header) == 0;

        if is_luban_active {
            if !is_epoch {
                return None;
            }

            let count = header.extra_data[EXTRA_VANITY] as usize;
            let start = EXTRA_VANITY+VALIDATOR_NUMBER_SIZE;
            let end = start + count * VALIDATOR_BYTES_LEN_AFTER_LUBAN;

            let mut extra_min_len = end + EXTRA_SEAL;
            let is_bohr_active = self.chain_spec.is_bohr_active_at_timestamp(header.timestamp);
            if is_bohr_active {
                extra_min_len += TURN_LENGTH_SIZE;
            }
            if count == 0 || extra_len < extra_min_len {
                return None
            }
            Some(header.extra_data[start..end].to_vec())
        } else {
            if is_epoch &&
                (extra_len - EXTRA_VANITY - EXTRA_SEAL) %
                VALIDATOR_BYTES_LEN_BEFORE_LUBAN !=
                    0
            {
                return None;
            }

            Some(header.extra_data[EXTRA_VANITY..extra_len - EXTRA_SEAL].to_vec())
        }
    }

    /// Get turn length from header
    pub fn get_turn_length_from_header(&self, header: &Header) -> Result<Option<u8>, ParliaConsensusError> {
        if header.number % self.get_epoch_length(header) != 0 ||
            !self.chain_spec.is_bohr_active_at_timestamp(header.timestamp)
        {
            return Ok(None);
        }

        if header.extra_data.len() <= EXTRA_VANITY + EXTRA_SEAL {
            return Err(ParliaConsensusError::InvalidHeaderExtraLen {
                header_extra_len: header.extra_data.len() as u64,
            });
        }

        let num = header.extra_data[EXTRA_VANITY] as usize;
        let pos = EXTRA_VANITY + 1 + num * VALIDATOR_BYTES_LEN_AFTER_LUBAN;

        if header.extra_data.len() <= pos {
            return Err(ParliaConsensusError::ExtraInvalidTurnLength);
        }

        let turn_length = header.extra_data[pos];
        Ok(Some(turn_length))
    }

    /// Get vote attestation from header
    pub fn get_vote_attestation_from_header(&self, header: &Header) -> Result<Option<VoteAttestation>, ParliaConsensusError> {
        let extra_len = header.extra_data.len();
        if extra_len <= EXTRA_VANITY + EXTRA_SEAL {
            return Ok(None);
        }

        if !self.chain_spec.is_luban_active_at_block(header.number()) {
            return Ok(None);
        }

        let mut raw_attestation_data = if header.number() % self.get_epoch_length(header) != 0 {
            &header.extra_data[EXTRA_VANITY..extra_len - EXTRA_SEAL]
        } else {
            let validator_count =
                header.extra_data[EXTRA_VANITY + VALIDATOR_NUMBER_SIZE - 1] as usize;
            let mut start =
                EXTRA_VANITY + VALIDATOR_NUMBER_SIZE + validator_count * VALIDATOR_BYTES_LEN_AFTER_LUBAN;
            let is_bohr_active = self.chain_spec.is_bohr_active_at_timestamp(header.timestamp);
            if is_bohr_active {
                start += TURN_LENGTH_SIZE;
            }
            let end = extra_len - EXTRA_SEAL;
            if end <= start {
                return Ok(None)
            }
            &header.extra_data[start..end]
        };
        if raw_attestation_data.is_empty() {
            return Ok(None);
        }

        Ok(Some(
            Decodable::decode(&mut raw_attestation_data)
                .map_err(|_| ParliaConsensusError::ABIDecodeInnerError)?,
        ))
    }

    pub fn recover_proposer(&self, header: &Header) -> Result<Address, ParliaConsensusError> {
        let hash = header.hash_slow();
        
        { // Check cache first
            let mut cache = RECOVERED_PROPOSER_CACHE.write().unwrap();
            if let Some(proposer) = cache.get(&hash) {
                return Ok(*proposer);
            }
        }

        let extra_data = &header.extra_data;
        if extra_data.len() < EXTRA_VANITY + EXTRA_SEAL {
            return Err(ParliaConsensusError::ExtraSignatureMissing);
        }

        let signature_offset = extra_data.len() - EXTRA_SEAL;
        let recovery_byte = extra_data[signature_offset + EXTRA_SEAL - 1] as i32;
        let signature_bytes = &extra_data[signature_offset..signature_offset + EXTRA_SEAL - 1];

        let recovery_id = RecoveryId::try_from(recovery_byte)
            .map_err(|_| ParliaConsensusError::RecoverECDSAInnerError)?;
        let signature = RecoverableSignature::from_compact(signature_bytes, recovery_id)
            .map_err(|_| ParliaConsensusError::RecoverECDSAInnerError)?;

        let message = Message::from_digest_slice(
            hash_with_chain_id(header, self.chain_spec.chain().id()).as_slice(),
        )
        .map_err(|_| ParliaConsensusError::RecoverECDSAInnerError)?;
        let public = &SECP256K1
            .recover_ecdsa(&message, &signature)
            .map_err(|_| ParliaConsensusError::RecoverECDSAInnerError)?;

        let proposer =
            Address::from_slice(&alloy_primitives::keccak256(&public.serialize_uncompressed()[1..])[12..]);
        
        { // Update cache
            let mut cache = RECOVERED_PROPOSER_CACHE.write().unwrap();
            cache.insert(hash, proposer);
        }
        
        Ok(proposer)
    }

}