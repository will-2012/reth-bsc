use super::{ParliaHeaderValidator, SnapshotProvider};
use alloy_consensus::Header;
use crate::node::primitives::{BscBlock, BscBlockBody};
use reth::consensus::{Consensus, FullConsensus, ConsensusError, HeaderValidator};
use reth_primitives_traits::{Block, SealedBlock, SealedHeader};
use std::sync::Arc;

/// Minimal Parlia consensus wrapper that delegates header checks to [`ParliaHeaderValidator`].
/// Other pre/post‐execution rules will be filled in later milestones.
#[derive(Debug, Clone)]
pub struct ParliaConsensus<P> {
    header_validator: Arc<ParliaHeaderValidator<P>>,
}

impl<P> ParliaConsensus<P> {
    pub fn new(header_validator: Arc<ParliaHeaderValidator<P>>) -> Self { Self { header_validator } }
}

impl<P> HeaderValidator<Header> for ParliaConsensus<P>
where
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

impl<P> Consensus<SealedBlock<BscBlock>> for ParliaConsensus<P>
where
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        _body: &<SealedBlock<BscBlock> as Block>::Body,
        _header: &SealedHeader<Header>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn validate_block_pre_execution(
        &self,
        _block: &SealedBlock<SealedBlock<BscBlock>>, // TODO implement full checks
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<P> FullConsensus<SealedBlock<BscBlock>> for ParliaConsensus<P>
where
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    fn validate_block_post_execution(
        &self,
        _block: &reth_primitives::BlockWithSenders,
        _result: &reth_evm::BlockExecutionResult<reth_primitives::Receipt>,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }
} 