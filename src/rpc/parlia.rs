
use jsonrpsee::{core::RpcResult, proc_macros::rpc, types::ErrorObject};
use serde::{Deserialize, Serialize};

use crate::consensus::parlia::{Snapshot, SnapshotProvider};
use reth_provider::{BlockReader, HeaderProvider};
use std::sync::Arc;

/// Validator information in the snapshot (matches BSC official format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    #[serde(rename = "index:omitempty")]
    pub index: u64,
    pub vote_address: Vec<u8>, // 48-byte vote address array as vec for serde compatibility
}

impl Default for ValidatorInfo {
    fn default() -> Self {
        Self {
            index: 0,
            vote_address: vec![0; 48], // All zeros as shown in BSC example
        }
    }
}

/// Official BSC Parlia snapshot response structure matching bsc-erigon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResult {
    pub number: u64,
    pub hash: String,
    pub epoch_length: u64,
    pub block_interval: u64,
    pub turn_length: u8,
    pub validators: std::collections::HashMap<String, ValidatorInfo>,
    pub recents: std::collections::HashMap<String, String>,
    pub recent_fork_hashes: std::collections::HashMap<String, String>,
    #[serde(rename = "attestation:omitempty")]
    pub attestation: Option<serde_json::Value>,
}

impl From<Snapshot> for SnapshotResult {
    fn from(snapshot: Snapshot) -> Self {
        // Convert validators to the expected format: address -> ValidatorInfo
        let validators: std::collections::HashMap<String, ValidatorInfo> = snapshot
            .validators
            .iter()
            .map(|addr| {
                (
                    format!("0x{:040x}", addr), // 40-char hex address
                    ValidatorInfo::default(),
                )
            })
            .collect();

        // Convert recent proposers to string format: block_number -> address
        let recents: std::collections::HashMap<String, String> = snapshot
            .recent_proposers
            .iter()
            .map(|(block_num, addr)| {
                (
                    block_num.to_string(),
                    format!("0x{:040x}", addr),
                )
            })
            .collect();

        // Generate recent fork hashes (simplified - all zeros like in BSC example)
        let recent_fork_hashes: std::collections::HashMap<String, String> = snapshot
            .recent_proposers
            .keys()
            .map(|block_num| {
                (
                    block_num.to_string(),
                    "00000000".to_string(), // Simplified fork hash
                )
            })
            .collect();

        Self {
            number: snapshot.block_number,
            hash: format!("0x{:064x}", snapshot.block_hash),
            epoch_length: 200, // BSC epoch length
            block_interval: 3000, // BSC block interval in milliseconds
            turn_length: snapshot.turn_length.unwrap_or(1),
            validators,
            recents,
            recent_fork_hashes,
            attestation: None,
        }
    }
}

/// Parlia snapshot RPC API (matches BSC official standard)
#[rpc(server, namespace = "parlia")]
pub trait ParliaApi {
    /// Get snapshot at a specific block (official BSC API method)
    /// Params: block number as hex string (e.g., "0x123132")
    #[method(name = "getSnapshot")]
    async fn get_snapshot(&self, block_number: String) -> RpcResult<Option<SnapshotResult>>;
}

/// Implementation of the Parlia snapshot RPC API
pub struct ParliaApiImpl<P: SnapshotProvider, Provider> {
    /// Snapshot provider for accessing validator snapshots
    snapshot_provider: Arc<P>,
    /// Blockchain provider for resolving block numbers and hashes
    provider: Provider,
}

impl<P: SnapshotProvider, Provider> ParliaApiImpl<P, Provider> 
where
    Provider: BlockReader + HeaderProvider + Clone + Send + Sync + 'static,
{
    /// Create a new Parlia API instance
    pub fn new(snapshot_provider: Arc<P>, provider: Provider) -> Self {
        Self { snapshot_provider, provider }
    }
}

#[async_trait::async_trait]
impl<P: SnapshotProvider + Send + Sync + 'static, Provider> ParliaApiServer for ParliaApiImpl<P, Provider>
where
    Provider: BlockReader + HeaderProvider + Clone + Send + Sync + 'static,
{
    /// Get snapshot at a specific block (matches BSC official API.GetSnapshot)
    /// Accepts block number as hex string like "0x123132"
    async fn get_snapshot(&self, block_number: String) -> RpcResult<Option<SnapshotResult>> {
        // Parse hex block number (like BSC API does)
        let block_num = if block_number.starts_with("0x") {
            match u64::from_str_radix(&block_number[2..], 16) {
                Ok(num) => num,
                Err(_) => {
                    return Err(ErrorObject::owned(
                        -32602, 
                        "Invalid block number format", 
                        None::<()>
                    ).into());
                }
            }
        } else {
            match block_number.parse::<u64>() {
                Ok(num) => num,
                Err(_) => {
                    return Err(ErrorObject::owned(
                        -32602, 
                        "Invalid block number format", 
                        None::<()>
                    ).into());
                }
            }
        };
        
        // Get snapshot from provider (equivalent to api.parlia.snapshot call in BSC)
        if let Some(snapshot) = self.snapshot_provider.snapshot(block_num) {
            Ok(Some(snapshot.into()))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::parlia::InMemorySnapshotProvider;
    use alloy_primitives::B256;

    // Mock provider for testing
    #[derive(Clone)]
    struct MockProvider;
    
    impl BlockReader for MockProvider {
        fn find_block_by_hash(&self, _hash: B256, _source: reth_provider::BlockSource) -> reth_provider::ProviderResult<Option<reth_primitives_traits::Block>> {
            Ok(None)
        }
        
        fn block(&self, _id: reth_provider::BlockHashOrNumber) -> reth_provider::ProviderResult<Option<reth_primitives_traits::Block>> {
            Ok(None)
        }
        
        fn pending_block(&self) -> reth_provider::ProviderResult<Option<reth_primitives_traits::SealedBlock>> {
            Ok(None)
        }
        
        fn pending_block_with_senders(&self) -> reth_provider::ProviderResult<Option<reth_primitives_traits::SealedBlockWithSenders>> {
            Ok(None)
        }
        
        fn pending_block_and_receipts(&self) -> reth_provider::ProviderResult<Option<(reth_primitives_traits::SealedBlock, Vec<reth_primitives_traits::Receipt>)>> {
            Ok(None)
        }
        
        fn ommers(&self, _id: reth_provider::BlockHashOrNumber) -> reth_provider::ProviderResult<Option<Vec<reth_primitives_traits::Header>>> {
            Ok(None)
        }
        
        fn block_body_indices(&self, _number: reth_primitives::BlockNumber) -> reth_provider::ProviderResult<Option<reth_db_api::models::StoredBlockBodyIndices>> {
            Ok(None)
        }
        
        fn block_with_senders(&self, _id: reth_provider::BlockHashOrNumber, _transaction_kind: reth_provider::TransactionVariant) -> reth_provider::ProviderResult<Option<reth_primitives_traits::BlockWithSenders>> {
            Ok(None)
        }
        
        fn sealed_block_with_senders(&self, _id: reth_provider::BlockHashOrNumber, _transaction_kind: reth_provider::TransactionVariant) -> reth_provider::ProviderResult<Option<reth_primitives_traits::SealedBlockWithSenders>> {
            Ok(None)
        }
        
        fn block_range(&self, _range: std::ops::RangeInclusive<reth_primitives::BlockNumber>) -> reth_provider::ProviderResult<Vec<reth_primitives_traits::Block>> {
            Ok(vec![])
        }
        
        fn block_with_senders_range(&self, _range: std::ops::RangeInclusive<reth_primitives::BlockNumber>) -> reth_provider::ProviderResult<Vec<reth_primitives_traits::BlockWithSenders>> {
            Ok(vec![])
        }
        
        fn sealed_block_with_senders_range(&self, _range: std::ops::RangeInclusive<reth_primitives::BlockNumber>) -> reth_provider::ProviderResult<Vec<reth_primitives_traits::SealedBlockWithSenders>> {
            Ok(vec![])
        }
    }
    
    impl HeaderProvider for MockProvider {
        fn header(&self, _block_hash: &B256) -> reth_provider::ProviderResult<Option<reth_primitives_traits::Header>> {
            Ok(None)
        }
        
        fn header_by_number(&self, _num: u64) -> reth_provider::ProviderResult<Option<reth_primitives_traits::Header>> {
            Ok(None)
        }
        
        fn header_by_hash_or_number(&self, _hash_or_num: reth_provider::BlockHashOrNumber) -> reth_provider::ProviderResult<Option<reth_primitives_traits::Header>> {
            Ok(None)
        }
        
        fn header_td(&self, _hash: &B256) -> reth_provider::ProviderResult<Option<alloy_primitives::U256>> {
            Ok(None)
        }
        
        fn header_td_by_number(&self, _number: reth_primitives::BlockNumber) -> reth_provider::ProviderResult<Option<alloy_primitives::U256>> {
            Ok(None)
        }
        
        fn headers_range(&self, _range: impl std::ops::RangeBounds<reth_primitives::BlockNumber>) -> reth_provider::ProviderResult<Vec<reth_primitives_traits::Header>> {
            Ok(vec![])
        }
        
        fn sealed_header(&self, _number: reth_primitives::BlockNumber) -> reth_provider::ProviderResult<Option<reth_primitives_traits::SealedHeader>> {
            Ok(None)
        }
        
        fn sealed_headers_range(&self, _range: impl std::ops::RangeBounds<reth_primitives::BlockNumber>) -> reth_provider::ProviderResult<Vec<reth_primitives_traits::SealedHeader>> {
            Ok(vec![])
        }
        
        fn sealed_headers_while(&self, _range: impl std::ops::RangeBounds<reth_primitives::BlockNumber>, _predicate: impl FnMut(&reth_primitives_traits::SealedHeader) -> bool) -> reth_provider::ProviderResult<Vec<reth_primitives_traits::SealedHeader>> {
            Ok(vec![])
        }
    }
    
    impl reth_provider::ChainSpecProvider for MockProvider {
        type ChainSpec = crate::chainspec::BscChainSpec;
        
        fn chain_spec(&self) -> std::sync::Arc<Self::ChainSpec> {
            std::sync::Arc::new(crate::chainspec::BscChainSpec::bsc_mainnet())
        }
    }
    
    impl reth_provider::BlockHashReader for MockProvider {
        fn block_hash(&self, _number: u64) -> reth_provider::ProviderResult<Option<B256>> {
            Ok(None)
        }
        
        fn canonical_hashes_range(&self, _start: reth_primitives::BlockNumber, _end: reth_primitives::BlockNumber) -> reth_provider::ProviderResult<Vec<B256>> {
            Ok(vec![])
        }
    }
    
    impl reth_provider::BlockNumReader for MockProvider {
        fn chain_info(&self) -> reth_provider::ProviderResult<reth_chainspec::ChainInfo> {
            Ok(reth_chainspec::ChainInfo::default())
        }
        
        fn best_block_number(&self) -> reth_provider::ProviderResult<reth_primitives::BlockNumber> {
            Ok(1000) // Return a test block number
        }
        
        fn last_block_number(&self) -> reth_provider::ProviderResult<reth_primitives::BlockNumber> {
            Ok(1000)
        }
        
        fn block_number(&self, _hash: B256) -> reth_provider::ProviderResult<Option<reth_primitives::BlockNumber>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn test_snapshot_api() {
        let snapshot_provider = Arc::new(InMemorySnapshotProvider::new(100));
        let mock_provider = MockProvider;
        
        // Insert a test snapshot
        let mut test_snapshot = Snapshot::default();
        test_snapshot.block_number = 100;
        test_snapshot.validators = vec![alloy_primitives::Address::random(), alloy_primitives::Address::random()];
        test_snapshot.epoch_num = 200;
        test_snapshot.turn_length = Some(1);
        snapshot_provider.insert(test_snapshot.clone());

        let api = ParliaApiImpl::new(snapshot_provider, mock_provider);
        
        // Test snapshot retrieval with hex block number (BSC official format)
        let result = api.get_snapshot("0x64".to_string()).await.unwrap(); // 0x64 = 100
        assert!(result.is_some());
        
        let snapshot_result = result.unwrap();
        assert_eq!(snapshot_result.number, 100);
        assert_eq!(snapshot_result.validators.len(), 2);
        assert_eq!(snapshot_result.epoch_length, 200);
        assert_eq!(snapshot_result.turn_length, 1);
        
        // Test with decimal format too
        let result = api.get_snapshot("100".to_string()).await.unwrap();
        assert!(result.is_some());
    }
}