use super::{ParliaHeaderValidator, SnapshotProvider};
use alloy_consensus::Header as AlloyHeader;
use reth::consensus::{Consensus, FullConsensus, ConsensusError, HeaderValidator};
use reth_primitives_traits::{Block, SealedBlock, SealedHeader, GotExpected};
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

impl<P> HeaderValidator<AlloyHeader> for ParliaConsensus<P>
where
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    fn validate_header(&self, header: &SealedHeader<AlloyHeader>) -> Result<(), ConsensusError> {
        self.header_validator.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader<AlloyHeader>,
        parent: &SealedHeader<AlloyHeader>,
    ) -> Result<(), ConsensusError> {
        self.header_validator.validate_header_against_parent(header, parent)
    }
}

impl<P> Consensus<SealedBlock<AlloyHeader>> for ParliaConsensus<P>
where
    P: SnapshotProvider + std::fmt::Debug + 'static,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        _body: &<SealedBlock<AlloyHeader> as Block>::Body,
        _header: &SealedHeader<AlloyHeader>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn validate_block_pre_execution(
        &self,
        _block: &SealedBlock<SealedBlock<AlloyHeader>>, // dummy type adjust later
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<P> FullConsensus<SealedBlock<AlloyHeader>> for ParliaConsensus<P>
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