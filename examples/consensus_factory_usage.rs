use std::sync::Arc;
use reth_db::{init_db, mdbx::DatabaseArguments, DatabaseEnv};
use reth_bsc::{
    node::consensus_factory::BscConsensusFactory,
    chainspec::BscChainSpec,
    consensus::parlia::InMemorySnapshotProvider,
};

/// Example showing how to use BscConsensusFactory for different scenarios
fn main() -> eyre::Result<()> {
    println!("🔧 BSC Consensus Factory Usage Examples");
    
    // 1. Development/Testing: In-memory snapshots
    println!("\n1️⃣ Creating consensus with in-memory snapshots (development)");
    let dev_consensus = BscConsensusFactory::create_in_memory();
    println!("   ✅ Created in-memory consensus for development");
    
    // 2. Production: Database-backed snapshots
    println!("\n2️⃣ Creating consensus with persistent MDBX snapshots (production)");
    
    // Initialize database
    let db_path = std::env::temp_dir().join("bsc_consensus_example");
    if db_path.exists() {
        std::fs::remove_dir_all(&db_path)?;
    }
    std::fs::create_dir_all(&db_path)?;
    
    let database = Arc::new(init_db(&db_path, DatabaseArguments::new(Default::default()))?);
    let chain_spec = Arc::new(BscChainSpec { 
        inner: reth_bsc::chainspec::bsc::bsc_mainnet() 
    });
    
    let prod_consensus = BscConsensusFactory::create_with_database(
        database.clone(),
        chain_spec.clone(),
        512, // LRU cache size
    );
    println!("   ✅ Created persistent consensus for production");
    
    // 3. Custom: With specific provider
    println!("\n3️⃣ Creating consensus with custom snapshot provider");
    let custom_provider = Arc::new(InMemorySnapshotProvider::new(5000));
    let custom_consensus = BscConsensusFactory::create_with_provider(
        chain_spec.clone(),
        custom_provider,
    );
    println!("   ✅ Created custom consensus with specific provider");
    
    // Integration example
    println!("\n🔗 Integration Example:");
    println!("   // In your node launch code:");
    println!("   let consensus = BscConsensusFactory::create_with_database(");
    println!("       ctx.database().clone(),    // Access database from LaunchContext");
    println!("       ctx.chain_spec(),          // Get chain spec from context");
    println!("       1024,                      // LRU cache size");
    println!("   );");
    
    // Cleanup
    std::fs::remove_dir_all(&db_path)?;
    println!("\n✨ Example completed successfully!");
    
    Ok(())
}