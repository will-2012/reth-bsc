use crate::{
    node::BscNode, 
    BscPrimitives,
    consensus::parlia::{ParliaConsensus, provider::EnhancedDbSnapshotProvider, EPOCH},
};
use reth::{
    api::FullNodeTypes,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{ConsensusError, FullConsensus},
};

use std::sync::Arc;
use reth_chainspec::EthChainSpec;


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
        // 🚀 ENABLING PERSISTENT MDBX SNAPSHOTS!
        // We'll extract the database through the provider factory interface
        
        tracing::info!("🔧 [BSC] Initializing persistent MDBX snapshots...");
        
        // Always use persistent snapshots with on-demand creation - no fallback
        let snapshot_provider = try_create_ondemand_snapshots(ctx)
            .unwrap_or_else(|e| {
                panic!("Failed to initialize on-demand MDBX snapshots: {}", e);
            });
            
        tracing::info!(
            "🚀 [BSC] ON-DEMAND SNAPSHOTS ENABLED! \
             Using OnDemandSnapshotProvider with MDBX persistence, LRU cache, and automatic snapshot creation. \
             Snapshots will persist across node restarts and be created on-demand for missing blocks."
        );
        
        let consensus = ParliaConsensus::new(
            ctx.chain_spec(), 
            snapshot_provider,
            EPOCH,
            3, // 3 second block period on BSC
        );
        
        Ok(Arc::new(consensus))
    }
}



/// Attempts to create on-demand snapshots using a separate database instance
/// and access to the blockchain provider for header lookups
/// 
/// This follows a safe pattern where we create a separate database connection
/// for snapshot storage, avoiding the need for unsafe access to provider internals.
fn try_create_ondemand_snapshots<Node>(
    ctx: &BuilderContext<Node>,
) -> eyre::Result<Arc<EnhancedDbSnapshotProvider<Arc<reth_db::DatabaseEnv>, Node::Provider>>>
where
    Node: FullNodeTypes<Types = BscNode>,
{
    // Create a separate database instance for snapshot storage in its own directory
    // This avoids conflicts with the main database
    let datadir = ctx.config().datadir.clone();
    let main_dir = datadir.resolve_datadir(ctx.chain_spec().chain());
    let db_path = main_dir.data_dir().join("parlia_snapshots");
    
    // Initialize our own database instance for snapshot storage
    use reth_db::{init_db, mdbx::DatabaseArguments};
    
    let snapshot_db = Arc::new(init_db(
        &db_path,
        DatabaseArguments::new(Default::default())
    ).map_err(|e| eyre::eyre!("Failed to initialize snapshot database: {}", e))?);
    
    tracing::info!("📦 [BSC] Created separate database instance for persistent snapshots");
    
    // Get access to the blockchain provider for header lookups
    let blockchain_provider = Arc::new(ctx.provider().clone());
    
    // Create EnhancedDbSnapshotProvider with backward walking capability (zoro_reth/bsc-erigon style)
    let snapshot_provider = Arc::new(EnhancedDbSnapshotProvider::new(
        snapshot_db,
        2048, // Production LRU cache size
        blockchain_provider,
        ctx.chain_spec().clone(),
    ));
    
    tracing::info!("🚀 [BSC] ENHANCED SNAPSHOTS ENABLED! Using EnhancedDbSnapshotProvider with zoro_reth/bsc-erigon style backward walking, MDBX persistence, LRU cache, and automatic snapshot creation. Snapshots will persist across node restarts and be created on-demand for missing blocks.");
    
    Ok(snapshot_provider)
}

// The old BscConsensus has been replaced with the enhanced ParliaConsensus
// from crate::consensus::parlia::ParliaConsensus which provides proper
// Parlia consensus validation including seal verification, turn-based proposing,
// and epoch transition handling.
