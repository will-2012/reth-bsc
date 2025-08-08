use alloy_primitives::{Address, B256};
use reth_db::{init_db, mdbx::DatabaseArguments, Database, transaction::DbTx, cursor::DbCursorRO};
use reth_bsc::consensus::parlia::{
    provider::DbSnapshotProvider, 
    snapshot::Snapshot,
    SnapshotProvider,
};
use std::sync::Arc;

/// Simple tool to check MDBX snapshot persistence
fn main() -> eyre::Result<()> {
    println!("🔍 BSC Parlia Snapshot Checker");
    
    // Initialize database (use temporary path for testing)
    let db_path = std::env::temp_dir().join("bsc_test_db");
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }
    std::fs::create_dir_all(&db_path)?;
    
    let database = Arc::new(init_db(&db_path, DatabaseArguments::new(Default::default()))?);
    println!("📦 Database initialized at: {}", db_path.display());
    
    // Create DbSnapshotProvider
    let provider = DbSnapshotProvider::new(database.clone(), 256);
    println!("⚡ Created DbSnapshotProvider with 256-entry LRU cache");
    
    // Create test snapshots
    let mut test_snapshots = Vec::new();
    for i in 0..5 {
        let block_number = (i + 1) * 1024; // Checkpoint intervals
        let mut snapshot = Snapshot::default();
        snapshot.block_number = block_number;
        snapshot.block_hash = B256::random();
        snapshot.validators = vec![
            Address::random(),
            Address::random(), 
            Address::random(),
        ];
        snapshot.epoch_num = 200;
        snapshot.turn_length = Some(1);
        
        test_snapshots.push(snapshot);
    }
    
    // Insert snapshots
    println!("\n📝 Inserting {} test snapshots...", test_snapshots.len());
    for (i, snapshot) in test_snapshots.iter().enumerate() {
        provider.insert(snapshot.clone());
        println!("  ✅ Snapshot {} at block {}", i + 1, snapshot.block_number);
    }
    
    // Verify snapshots
    println!("\n🔍 Verifying snapshot retrieval...");
    for (i, expected) in test_snapshots.iter().enumerate() {
        if let Some(retrieved) = provider.snapshot(expected.block_number) {
            if retrieved.block_number == expected.block_number && 
               retrieved.block_hash == expected.block_hash &&
               retrieved.validators.len() == expected.validators.len() {
                println!("  ✅ Snapshot {} verified successfully", i + 1);
            } else {
                println!("  ❌ Snapshot {} data mismatch", i + 1);
            }
        } else {
            println!("  ❌ Snapshot {} not found", i + 1);
        }
    }
    
    // Test range queries (should find nearest)
    println!("\n🎯 Testing range queries...");
    let test_blocks = vec![500, 1500, 2048, 3000, 5120];
    for block in test_blocks {
        if let Some(snapshot) = provider.snapshot(block) {
            println!("  ✅ Block {} → found snapshot at block {}", block, snapshot.block_number);
        } else {
            println!("  ❌ Block {} → no snapshot found", block);
        }
    }
    
    // Check direct database access
    println!("\n🗃️ Checking raw database storage...");
    let tx = database.tx()?;
    let mut cursor = tx.cursor_read::<reth_bsc::consensus::parlia::db::ParliaSnapshots>()?;
    let mut count = 0;
    for item in cursor.walk(None)? {
        let (_key, _value) = item?;
        count += 1;
    }
    println!("  📊 Found {} raw entries in ParliaSnapshots table", count);
    
    // Cleanup
    println!("\n🧹 Cleaning up test database...");
    drop(provider);
    drop(database);
    drop(tx);
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }
    
    println!("✨ Snapshot persistence verification complete!");
    
    Ok(())
}