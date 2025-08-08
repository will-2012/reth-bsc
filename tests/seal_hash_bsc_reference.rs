//! Test to compare our seal hash implementation with bsc-erigon reference
//! 
//! This test replicates the exact bsc-erigon EncodeSigHeader logic and compares
//! the result with our SealContent struct approach.

use alloy_primitives::{address, b256, hex, Bloom, Bytes, B64, U256, B256, keccak256};
use alloy_rlp::Encodable;
use alloy_consensus::Header;
use reth_bsc::evm::precompiles::double_sign::SealContent;

/// Create a test header that matches BSC testnet block 1 structure
fn create_bsc_testnet_block1_header() -> Header {
    // Based on our debug logs from the actual BSC testnet block 1
    Header {
        parent_hash: b256!("6d3c66c5357ec91d5c43af47e234a939b22557cbb552dc45bebbceeed90fbe34"),
        ommers_hash: b256!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347"), 
        beneficiary: address!("35552c16704d214347f29fa77f77da6d75d7c752"),
        state_root: b256!("0b9279d6596c22b580a56e87110ab3f78a3dce913ffb7a2b157e2ed7b7146859"),
        transactions_root: b256!("55d9e133e90c56fbf87c3119e8a6d832ff6a70ffda15a065e93fbde632ab6c20"),
        receipts_root: b256!("b534060b55eac5a7ac214b6402ae4d0b31e4ca848996bc29cebeb8fbcfd6af45"),
        logs_bloom: Bloom::from_slice(&hex::decode("08000000000000000000000000000000000000000000000000000000000000000000000000000000000000000020000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000").unwrap()),
        difficulty: U256::from(2),
        number: 1,
        gas_limit: 39843751,
        gas_used: 1509960,
        timestamp: 1594281440,
        extra_data: Bytes::from(hex::decode("d983010000846765746889676f312e31322e3137856c696e75780000000000006293f9b74e142a538e4c53951c51ed93100cacedfcd0d3097cfbc705497cd5bc70d0018ce71deb0c488f1a3a83ed27be281ebd07578f0d8766068f9f8682485c00").unwrap()),
        mix_hash: b256!("0000000000000000000000000000000000000000000000000000000000000000"),
        nonce: B64::ZERO,
        ..Default::default()
    }
}

/// Replicate bsc-erigon's EncodeSigHeader exactly using manual RLP array encoding
fn bsc_erigon_encode_sig_header(header: &Header, chain_id: u64) -> Vec<u8> {
    const EXTRA_SEAL: usize = 65;
    
    let extra_without_seal = if header.extra_data.len() >= EXTRA_SEAL {
        &header.extra_data[..header.extra_data.len() - EXTRA_SEAL]
    } else {
        &header.extra_data[..]
    };
    
    // Create the exact toEncode array that bsc-erigon uses
    let extra_bytes = Bytes::from(extra_without_seal.to_vec());
    let to_encode: Vec<&dyn Encodable> = vec![
        &chain_id,                                    // chainId
        &header.parent_hash,                          // header.ParentHash
        &header.ommers_hash,                          // header.UncleHash
        &header.beneficiary,                          // header.Coinbase
        &header.state_root,                           // header.Root
        &header.transactions_root,                    // header.TxHash
        &header.receipts_root,                        // header.ReceiptHash
        &header.logs_bloom,                           // header.Bloom
        &header.difficulty,                           // header.Difficulty
        &header.number,                               // header.Number
        &header.gas_limit,                            // header.GasLimit
        &header.gas_used,                             // header.GasUsed
        &header.timestamp,                            // header.Time
        &extra_bytes,                                 // header.Extra[:len(header.Extra)-extraSeal]
        &header.mix_hash,                             // header.MixDigest
        &header.nonce,                                // header.Nonce
    ];
    
    // Note: We skip post-merge fields since our test block doesn't have ParentBeaconBlockRoot
    
    // Encode as RLP array (matching Go's rlp.Encode(w, toEncode))
    alloy_rlp::encode(to_encode)
}

/// Our current SealContent struct approach
fn our_seal_content_encode(header: &Header, chain_id: u64) -> Vec<u8> {
    const EXTRA_SEAL: usize = 65;
    
    let extra_without_seal = if header.extra_data.len() >= EXTRA_SEAL {
        &header.extra_data[..header.extra_data.len() - EXTRA_SEAL]
    } else {
        &header.extra_data[..]
    };
    
    let seal_content = SealContent {
        chain_id,
        parent_hash: header.parent_hash.0,
        uncle_hash: header.ommers_hash.0,
        coinbase: header.beneficiary.0 .0,
        root: header.state_root.0,
        tx_hash: header.transactions_root.0,
        receipt_hash: header.receipts_root.0,
        bloom: header.logs_bloom.0 .0,
        difficulty: header.difficulty.clone(),
        number: header.number,
        gas_limit: header.gas_limit,
        gas_used: header.gas_used,
        time: header.timestamp,
        extra: Bytes::from(extra_without_seal.to_vec()),
        mix_digest: header.mix_hash.0,
        nonce: header.nonce.0,
    };
    
    alloy_rlp::encode(seal_content)
}

#[test]
fn test_seal_hash_matches_bsc_erigon_reference() {
    let header = create_bsc_testnet_block1_header();
    let chain_id = 97u64; // BSC testnet
    
    // Generate RLP using both approaches
    let bsc_erigon_rlp = bsc_erigon_encode_sig_header(&header, chain_id);
    let our_rlp = our_seal_content_encode(&header, chain_id);
    
    println!("🔍 BSC-Erigon RLP length: {}", bsc_erigon_rlp.len());
    println!("🔍 Our RLP length: {}", our_rlp.len());
    
    println!("🔍 BSC-Erigon RLP: {}", hex::encode(&bsc_erigon_rlp));
    println!("🔍 Our RLP:        {}", hex::encode(&our_rlp));
    
    // Calculate seal hashes 
    let bsc_erigon_seal_hash = keccak256(&bsc_erigon_rlp);
    let our_seal_hash = keccak256(&our_rlp);
    
    println!("🔍 BSC-Erigon seal hash: {:?}", bsc_erigon_seal_hash);
    println!("🔍 Our seal hash:        {:?}", our_seal_hash);
    
    // Compare the results
    assert_eq!(
        bsc_erigon_rlp, our_rlp,
        "RLP encoding must match bsc-erigon exactly"
    );
    
    assert_eq!(
        bsc_erigon_seal_hash, our_seal_hash,
        "Seal hash must match bsc-erigon exactly"
    );
    
    println!("✅ SUCCESS: Our implementation matches bsc-erigon reference!");
}

#[test]
fn test_individual_field_encoding() {
    let header = create_bsc_testnet_block1_header();
    let chain_id = 97u64;
    
    // Test individual field encoding to debug any differences
    println!("🔍 Individual field encoding comparison:");
    
    let fields = [
        ("chain_id", alloy_rlp::encode(chain_id)),
        ("parent_hash", alloy_rlp::encode(header.parent_hash)),
        ("ommers_hash", alloy_rlp::encode(header.ommers_hash)),
        ("beneficiary", alloy_rlp::encode(header.beneficiary)),
        ("state_root", alloy_rlp::encode(header.state_root)),
        ("transactions_root", alloy_rlp::encode(header.transactions_root)),
        ("receipts_root", alloy_rlp::encode(header.receipts_root)),
        ("logs_bloom", alloy_rlp::encode(header.logs_bloom)),
        ("difficulty", alloy_rlp::encode(header.difficulty)),
        ("number", alloy_rlp::encode(header.number)),
        ("gas_limit", alloy_rlp::encode(header.gas_limit)),
        ("gas_used", alloy_rlp::encode(header.gas_used)),
        ("timestamp", alloy_rlp::encode(header.timestamp)),
        ("mix_hash", alloy_rlp::encode(header.mix_hash)),
        ("nonce", alloy_rlp::encode(header.nonce)),
    ];
    
    for (name, encoded) in fields {
        println!("   {}: {} bytes - {}", name, encoded.len(), hex::encode(&encoded[..std::cmp::min(16, encoded.len())]));
    }
}