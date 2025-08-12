use reth_bsc::chainspec::bsc_testnet;
use reth_bsc::consensus::parlia::{ParliaConsensus, InMemorySnapshotProvider, EPOCH, SnapshotProvider};
use std::sync::Arc;
use alloy_consensus::BlockHeader;
use alloy_primitives::hex;
use reth_chainspec::EthChainSpec;

fn main() -> eyre::Result<()> {
    // Initialize logging  
    println!("🚀 Testing BSC Genesis Snapshot Creation");

    // Create BSC testnet chain spec
    let chain_spec = bsc_testnet();
    let bsc_chain_spec = reth_bsc::chainspec::BscChainSpec { inner: chain_spec };
    let bsc_chain_spec_arc = Arc::new(bsc_chain_spec);

    // Create consensus with in-memory snapshot provider
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::new(100));
    let _consensus = ParliaConsensus::new(
        bsc_chain_spec_arc.clone(),
        snapshot_provider.clone(),
        EPOCH,
        3
    );

    // Check if genesis snapshot was created
    if let Some(genesis_snapshot) = snapshot_provider.snapshot(0) {
        println!("🎯 Genesis snapshot created successfully!");
        println!("   Block number: {}", genesis_snapshot.block_number);
        println!("   Block hash: {:#x}", genesis_snapshot.block_hash);
        println!("   Validators count: {}", genesis_snapshot.validators.len());
        println!("   Epoch length: {}", genesis_snapshot.epoch_num);
        
        println!("\n📋 Genesis validators:");
        for (i, validator) in genesis_snapshot.validators.iter().enumerate() {
            println!("   {}. {:#x}", i + 1, validator);
        }
        
        println!("\n✅ Genesis snapshot initialization SUCCESS!");
    } else {
        println!("❌ Genesis snapshot was NOT created!");
        return Err(eyre::eyre!("Genesis snapshot missing"));
    }

    // Test extraData length
    println!("\n🔍 Testing BSC testnet extraData...");
    let genesis_header = bsc_chain_spec_arc.genesis_header();
    let extra_data = genesis_header.extra_data();
    println!("   ExtraData length: {} bytes", extra_data.len());
    println!("   ExtraData (first 100 bytes): 0x{}", hex::encode(&extra_data[..100.min(extra_data.len())]));
    
    // Expected BSC testnet validators count based on extraData analysis
    const EXTRA_VANITY_LEN: usize = 32;
    const EXTRA_SEAL_LEN: usize = 65;
    const VALIDATOR_BYTES_LENGTH: usize = 20;
    
    if extra_data.len() > EXTRA_VANITY_LEN + EXTRA_SEAL_LEN {
        let validator_bytes_len = extra_data.len() - EXTRA_VANITY_LEN - EXTRA_SEAL_LEN;
        let expected_validators = validator_bytes_len / VALIDATOR_BYTES_LENGTH;
        println!("   Expected validators from extraData: {}", expected_validators);
    }

    Ok(())
}