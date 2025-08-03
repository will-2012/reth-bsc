use crate::{
    node::BscNode, 
    BscPrimitives,
    consensus::parlia::{ParliaConsensus, InMemorySnapshotProvider, EPOCH},
};
use reth::{
    api::FullNodeTypes,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{ConsensusError, FullConsensus},
};
use std::sync::Arc;

/// A basic Bsc consensus builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BscConsensusBuilder;

impl<Node> ConsensusBuilder<Node> for BscConsensusBuilder
where
    Node: FullNodeTypes<Types = BscNode>,
{
    type Consensus = Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError>>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        // Create an in-memory snapshot provider for now
        // TODO: Replace with persistent provider in later milestone
        let snapshot_provider = Arc::new(InMemorySnapshotProvider::new(1000)); // 1000 max entries
        
        // Create the enhanced Parlia consensus with BSC-specific validation
        let consensus = ParliaConsensus::new(
            ctx.chain_spec(), 
            snapshot_provider,
            EPOCH,
            3, // 3 second block period on BSC
        );
        
        Ok(Arc::new(consensus))
    }
}

// The old BscConsensus has been replaced with the enhanced ParliaConsensus
// from crate::consensus::parlia::ParliaConsensus which provides proper
// Parlia consensus validation including seal verification, turn-based proposing,
// and epoch transition handling.
