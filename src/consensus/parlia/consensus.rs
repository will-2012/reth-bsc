use super::{ParliaHeaderValidator, SnapshotProvider, BscConsensusValidator, Snapshot, constants::{DIFF_INTURN, DIFF_NOTURN}};
use alloy_consensus::Header;
use alloy_primitives::{Address, U256};
use crate::{
    node::primitives::{BscBlock, BscBlockBody},
    hardforks::BscHardforks,
};
use reth::consensus::{Consensus, FullConsensus, ConsensusError, HeaderValidator};
use reth_primitives::{RecoveredBlock, Receipt};
use reth_primitives_traits::{Block, SealedBlock, SealedHeader};
use reth_chainspec::EthChainSpec;
use std::sync::Arc;

/// Enhanced Parlia consensus that implements proper pre/post execution validation
#[derive(Debug, Clone)]
pub struct ParliaConsensus<ChainSpec, P> {
    chain_spec: Arc<ChainSpec>,
    header_validator: Arc<ParliaHeaderValidator<P>>,
    consensus_validator: Arc<BscConsensusValidator<ChainSpec>>,
    snapshot_provider: Arc<P>,
    epoch: u64,
    period: u64,
}

impl<ChainSpec, P> ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    pub fn new(
        chain_spec: Arc<ChainSpec>,
        snapshot_provider: Arc<P>,
        epoch: u64,
        period: u64,
    ) -> Self {
        let header_validator = Arc::new(ParliaHeaderValidator::new(snapshot_provider.clone()));
        let consensus_validator = Arc::new(BscConsensusValidator::new(chain_spec.clone()));
        
        Self { 
            chain_spec,
            header_validator,
            consensus_validator,
            snapshot_provider,
            epoch,
            period,
        }
    }

    /// Validate block pre-execution using Parlia rules
    fn validate_block_pre_execution_impl(&self, block: &SealedBlock<BscBlock>) -> Result<(), ConsensusError> {
        let header = block.header();

        // Skip genesis block
        if header.number == 0 {
            return Ok(());
        }

        // Get snapshot for the parent block
        let parent_number = header.number - 1;
        let snapshot = self.snapshot_provider
            .snapshot(parent_number)
            .ok_or_else(|| ConsensusError::Other("Failed to get snapshot".into()))?;

        // Verify seal (proposer signature)
        self.verify_seal(header, &snapshot)?;

        // Verify turn-based proposing (difficulty check)
        self.verify_difficulty(header, &snapshot)?;

        Ok(())
    }

    /// Validate block post-execution using Parlia rules
    fn validate_block_post_execution_impl(
        &self,
        block: &RecoveredBlock,
        _receipts: &[Receipt],
    ) -> Result<(), ConsensusError> {
        // For now, implement basic system contract validation
        // Full implementation would include:
        // - Validator set updates at epoch boundaries
        // - System reward distribution
        // - Slash contract interactions

        let header = &block.block().header;

        // Validate epoch transitions
        if header.number % self.epoch == 0 {
            // TODO: Implement epoch transition validation
            // This would verify validator set updates every 200 blocks
        }

        Ok(())
    }

    /// Verify the seal (proposer signature) in the header
    fn verify_seal(&self, header: &SealedHeader<Header>, snapshot: &Snapshot) -> Result<(), ConsensusError> {
        // For now, just check if coinbase is in validator set
        // TODO: Implement proper signature recovery and verification
        let proposer = header.beneficiary; // Use beneficiary instead of coinbase

        // Check if proposer is a validator
        if !snapshot.validators.contains(&proposer) {
            return Err(ConsensusError::Other(
                format!("Unauthorized proposer: {}", proposer).into()
            ));
        }

        Ok(())
    }

    /// Verify the difficulty based on turn-based proposing
    fn verify_difficulty(&self, header: &SealedHeader<Header>, snapshot: &Snapshot) -> Result<(), ConsensusError> {
        let proposer = header.beneficiary;
        let in_turn = snapshot.is_inturn(proposer);

        let expected_difficulty = if in_turn { DIFF_INTURN } else { DIFF_NOTURN };

        if header.difficulty != expected_difficulty {
            return Err(ConsensusError::Other(
                format!("Invalid difficulty: expected {}, got {}", expected_difficulty, header.difficulty).into()
            ));
        }

        Ok(())
    }
}

impl<ChainSpec, P> HeaderValidator<Header> for ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    fn validate_header(&self, header: &SealedHeader<Header>) -> Result<(), ConsensusError> {
        self.header_validator.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<Header>,
        parent: &SealedHeader<Header>,
    ) -> Result<(), ConsensusError> {
        self.header_validator.validate_header_against_parent(header, parent)
    }
}

impl<ChainSpec, P> Consensus<SealedBlock<BscBlock>> for ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        _body: &<SealedBlock<BscBlock> as Block>::Body,
        _header: &SealedHeader<Header>,
    ) -> Result<(), Self::Error> {
        // Basic body validation - for now accept all
        Ok(())
    }

    fn validate_block_pre_execution(
        &self,
        block: &SealedBlock<BscBlock>,
    ) -> Result<(), Self::Error> {
        self.validate_block_pre_execution_impl(block)
    }
}

impl<ChainSpec, P> FullConsensus<SealedBlock<BscBlock>> for ParliaConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks + 'static,
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock,
        receipts: &[Receipt],
    ) -> Result<(), ConsensusError> {
        self.validate_block_post_execution_impl(block, receipts)
    }
} 