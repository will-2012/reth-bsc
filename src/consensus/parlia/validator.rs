use super::snapshot::Snapshot;
use alloy_primitives::{Address, U256};
use reth::consensus::{ConsensusError, HeaderValidator};
use reth_primitives_traits::SealedHeader;
use std::sync::Arc;

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
}

impl<P> ParliaHeaderValidator<P> {
    pub fn new(provider: Arc<P>) -> Self { Self { provider } }
}

// Helper to get expected difficulty.
fn expected_difficulty(inturn: bool) -> u64 { if inturn { 2 } else { 1 } }

impl<P, H> HeaderValidator<H> for ParliaHeaderValidator<P>
where
    P: SnapshotProvider + std::fmt::Debug + 'static,
    H: alloy_consensus::BlockHeader + alloy_primitives::Sealable,
{
    fn validate_header(&self, header: &SealedHeader<H>) -> Result<(), ConsensusError> {
        // Genesis header is considered valid.
        if header.number() == 0 {
            return Ok(());
        }

        // Fetch snapshot for parent block.
        let parent_number = header.number() - 1;
        let Some(snap) = self.provider.snapshot(parent_number) else {
            return Err(ConsensusError::Other("missing snapshot".to_string()));
        };

        let miner: Address = header.beneficiary();
        if !snap.validators.contains(&miner) {
            return Err(ConsensusError::Other("unauthorised validator".to_string()));
        }

        let inturn = snap.inturn_validator() == miner;
        let expected_diff = U256::from(expected_difficulty(inturn));
        if header.difficulty() != expected_diff {
            return Err(ConsensusError::Other("wrong difficulty for proposer turn".to_string()));
        }

        // Advance snapshot so provider is ready for child validations.
        if let Some(parent_snap) = self.provider.snapshot(parent_number) {
            if let Some(new_snap) = parent_snap.apply(
                miner,
                header.header(),
                Vec::new(),
                None,
                None,
                None,
                false,
            ) {
                self.provider.insert(new_snap);
            }
        }
        Ok(())
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<H>,
        parent: &SealedHeader<H>,
    ) -> Result<(), ConsensusError> {
        // basic chain-order check (number increment already done by stages, but keep)
        if header.number() != parent.number() + 1 {
            return Err(ConsensusError::ParentBlockNumberMismatch {
                parent_block_number: parent.number(),
                block_number: header.number(),
            });
        }
        // timestamp checks
        if header.timestamp() <= parent.timestamp() {
            return Err(ConsensusError::TimestampIsInPast {
                parent_timestamp: parent.timestamp(),
                timestamp: header.timestamp(),
            });
        }

        // Ensure block is not too far in the future w.r.t configured interval.
        let interval_secs = if let Some(snap) = self.provider.snapshot(parent.number()) {
            snap.block_interval
        } else {
            3 // default safety fallback
        };

        if header.timestamp() > parent.timestamp() + interval_secs {
            return Err(ConsensusError::Other("timestamp exceeds expected block interval".into()));
        }
        Ok(())
    }
} 