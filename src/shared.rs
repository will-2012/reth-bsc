//! Shared global state for BSC node components
//! 
//! This module provides global access to the snapshot provider so that
//! both the consensus builder and RPC modules can access the same instance.

use crate::consensus::parlia::SnapshotProvider;
use crate::BscPrimitives;
use reth::consensus::{ConsensusError, FullConsensus};
use std::sync::{Arc, OnceLock};

/// Global shared access to the snapshot provider for RPC
static SNAPSHOT_PROVIDER: OnceLock<Arc<dyn SnapshotProvider + Send + Sync>> = OnceLock::new();

static PARLIA_CONSENSUS: OnceLock<Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError> + Send + Sync>> = OnceLock::new();

/// Store the snapshot provider globally
pub fn set_snapshot_provider(provider: Arc<dyn SnapshotProvider + Send + Sync>) -> Result<(), Arc<dyn SnapshotProvider + Send + Sync>> {
    SNAPSHOT_PROVIDER.set(provider)
}

/// Get the global snapshot provider
pub fn get_snapshot_provider() -> Option<&'static Arc<dyn SnapshotProvider + Send + Sync>> {
    SNAPSHOT_PROVIDER.get()
}

/// Store the parlia consensus globally
pub fn set_parlia_consensus(consensus: Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError> + Send + Sync>) -> Result<(), Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError> + Send + Sync>> {
    PARLIA_CONSENSUS.set(consensus)
}

/// Get the global parlia consensus
pub fn get_parlia_consensus() -> Option<&'static Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError> + Send + Sync>> {
    PARLIA_CONSENSUS.get()
}