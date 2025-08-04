use std::sync::Arc;
use reth_db::database::Database;
use crate::{
    BscPrimitives,
    consensus::parlia::{ParliaConsensus, provider::DbSnapshotProvider, InMemorySnapshotProvider, EPOCH},
    chainspec::BscChainSpec,
};
use reth::{
    consensus::{ConsensusError, FullConsensus},
};

/// Factory for creating BSC Parlia consensus instances
pub struct BscConsensusFactory;

impl BscConsensusFactory {
    /// Create consensus with in-memory snapshot provider (for development/testing)
    pub fn create_in_memory() -> Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError>> {
        let snapshot_provider = Arc::new(InMemorySnapshotProvider::new(10000));
        
        // Use default BSC mainnet chain spec for now
        // In production, this should come from the node configuration
        let chain_spec = Arc::new(BscChainSpec { inner: crate::chainspec::bsc::bsc_mainnet() });
        
        let consensus = ParliaConsensus::new(
            chain_spec,
            snapshot_provider,
            EPOCH,
            3, // 3 second block period on BSC
        );
        
        tracing::info!("🔄 [BSC] Created Parlia consensus with InMemorySnapshotProvider (10k cache)");
        Arc::new(consensus)
    }
    
    /// Create consensus with persistent MDBX snapshot provider (for production)
    pub fn create_with_database<DB: Database + 'static>(
        database: DB,
        chain_spec: Arc<BscChainSpec>,
        cache_size: usize,
    ) -> Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError>> {
        let snapshot_provider = Arc::new(DbSnapshotProvider::new(database, cache_size));
        
        let consensus = ParliaConsensus::new(
            chain_spec,
            snapshot_provider,
            EPOCH,
            3, // 3 second block period on BSC
        );
        
        tracing::info!(
            "🚀 [BSC] Created Parlia consensus with DbSnapshotProvider (cache={}, persistent=true)", 
            cache_size
        );
        Arc::new(consensus)
    }
    
    /// Create consensus with specific snapshot provider (for custom setups)
    pub fn create_with_provider<P>(
        chain_spec: Arc<BscChainSpec>,
        snapshot_provider: Arc<P>,
    ) -> Arc<ParliaConsensus<BscChainSpec, P>>
    where
        P: crate::consensus::parlia::SnapshotProvider + std::fmt::Debug + 'static,
    {
        let consensus = ParliaConsensus::new(
            chain_spec,
            snapshot_provider,
            EPOCH,
            3, // 3 second block period on BSC
        );
        
        tracing::info!("⚙️  [BSC] Created Parlia consensus with custom snapshot provider");
        Arc::new(consensus)
    }
}