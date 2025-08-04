use reth_bsc::rpc::parlia::SnapshotResult;
use reth_bsc::consensus::parlia::{InMemorySnapshotProvider, Snapshot, SnapshotProvider};
use std::sync::Arc;
use alloy_primitives::{Address, B256};

#[tokio::main]
async fn main() {
    // Create a test snapshot provider
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::new(100));
    
    // Create a mock snapshot with some validators and recent proposers
    let mut test_snapshot = Snapshot::default();
    test_snapshot.block_number = 1192242;
    test_snapshot.block_hash = B256::from_slice(&[0x42; 32]); // Example hash
    test_snapshot.epoch_num = 200;
    test_snapshot.turn_length = Some(1);
    
    // Add some test validators
    test_snapshot.validators = vec![
        "0x03073aedceaeeae639c465a009ee1012272d20b4".parse::<Address>().unwrap(),
        "0x04dd54a9e32f1edd035e0081e882c836346cbb46".parse::<Address>().unwrap(),
        "0x07490c0dca97d7f3bb6ea8cc81cd36abe450c706".parse::<Address>().unwrap(),
    ];
    
    // Add some recent proposers
    use std::collections::BTreeMap;
    let mut recent_proposers = BTreeMap::new();
    recent_proposers.insert(1192240, "0xa2959d3f95eae5dc7d70144ce1b73b403b7eb6e0".parse::<Address>().unwrap());
    recent_proposers.insert(1192241, "0xa2e5f9e8db4b38ac8529f79c4f3b582952b3d3dc".parse::<Address>().unwrap());
    recent_proposers.insert(1192242, "0x0af7d8b7d4eb50fa0eddd643d11120c94ad61248".parse::<Address>().unwrap());
    test_snapshot.recent_proposers = recent_proposers;
    
    // Insert the snapshot
    snapshot_provider.insert(test_snapshot);
    
    // Convert to the BSC API format
    let snapshot_result: SnapshotResult = snapshot_provider.snapshot(1192242).unwrap().into();
    
    // Print the result in JSON format
    let json = serde_json::to_string_pretty(&snapshot_result).unwrap();
    println!("BSC Parlia Snapshot API Response:");
    println!("{}", json);
    
    println!("\n✅ Test completed successfully!");
    println!("📝 This output should match the BSC official API format");
    println!("🔗 Compare with: loocapro_reth_bsc/examples/parlia_api/parlia_getSnapshot/response.json");
}