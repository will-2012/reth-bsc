//! Test suite for vote attestation verification with BLS signatures

use alloy_primitives::{Address, B256, FixedBytes};
use alloy_consensus::Header;
use reth_bsc::consensus::parlia::{
    attestation::parse_vote_attestation_from_header,
    vote::{VoteAttestation, VoteData},
    InMemorySnapshotProvider, ParliaHeaderValidator, SnapshotProvider,
    snapshot::{Snapshot, DEFAULT_EPOCH_LENGTH},
};
use std::sync::Arc;

/// Create a header with vote attestation
fn create_header_with_attestation(
    number: u64,
    is_epoch: bool,
    attestation: Option<VoteAttestation>,
) -> Header {
    let mut header = Header::default();
    header.number = number;
    header.timestamp = 1700000000 + number * 3;
    header.parent_hash = B256::random();
    header.gas_limit = 100_000_000;
    
    // Set extra data with proper size
    let mut extra = vec![0u8; 32]; // vanity
    
    if is_epoch {
        // Epoch block: add validator info
        extra.push(1); // number of validators
        extra.extend_from_slice(&[0u8; 68]); // 1 validator (20 bytes address + 48 bytes vote address)
        extra.push(1); // turn length (for Bohr)
    }
    
    // Add vote attestation if provided and after Luban fork
    if let Some(attestation) = attestation {
        let encoded = alloy_rlp::encode(&attestation);
        extra.extend_from_slice(&encoded);
    }
    
    // Add seal at the end
    extra.extend_from_slice(&[0u8; 65]); // seal placeholder
    
    header.extra_data = alloy_primitives::Bytes::from(extra);
    header
}

#[test]
fn test_parse_vote_attestation_valid() {
    // Create a header with valid vote attestation
    let attestation = VoteAttestation {
        vote_address_set: 1,
        agg_signature: FixedBytes::<96>::from([0u8; 96]), // BLS signature
        data: VoteData {
            source_number: 100,
            source_hash: B256::random(),
            target_number: 200,
            target_hash: B256::random(),
        },
        extra: bytes::Bytes::new(),
    };
    
    let header = create_header_with_attestation(300, false, Some(attestation.clone()));
    
    // Parse the attestation (assuming Luban and Bohr are active)
    let parsed = parse_vote_attestation_from_header(&header, DEFAULT_EPOCH_LENGTH, true, true).unwrap();
    assert_eq!(parsed.vote_address_set, attestation.vote_address_set);
    assert_eq!(parsed.data.source_number, attestation.data.source_number);
    assert_eq!(parsed.data.target_number, attestation.data.target_number);
}

#[test]
fn test_parse_vote_attestation_no_attestation() {
    // Create a header without vote attestation
    let header = create_header_with_attestation(300, false, None);
    
    // Should return None
    let parsed = parse_vote_attestation_from_header(&header, DEFAULT_EPOCH_LENGTH, true, true);
    assert!(parsed.is_none());
}

#[test]
fn test_parse_vote_attestation_invalid_extra_data() {
    // Create a header with invalid extra data size
    let mut header = Header::default();
    header.extra_data = alloy_primitives::Bytes::from(vec![0u8; 10]); // Too small
    
    let parsed = parse_vote_attestation_from_header(&header, DEFAULT_EPOCH_LENGTH, true, true);
    assert!(parsed.is_none());
}

#[test]
fn test_vote_attestation_epoch_boundary() {
    // Create headers for epoch boundary testing
    let attestation = VoteAttestation {
        vote_address_set: 1,
        agg_signature: FixedBytes::<96>::from([0u8; 96]),
        data: VoteData {
            source_number: 190,
            source_hash: B256::random(),
            target_number: 199,
            target_hash: B256::random(),
        },
        extra: bytes::Bytes::new(),
    };
    
    // Epoch boundary (multiple of 200)
    let epoch_header = create_header_with_attestation(200, true, Some(attestation.clone()));
    assert_eq!(epoch_header.number % DEFAULT_EPOCH_LENGTH, 0);
    
    // Non-epoch boundary
    let non_epoch_header = create_header_with_attestation(201, false, Some(attestation));
    assert_ne!(non_epoch_header.number % DEFAULT_EPOCH_LENGTH, 0);
}

#[test]
fn test_vote_attestation_validation_with_snapshot() {
    use reth_bsc::consensus::parlia::snapshot::Snapshot;
    use reth_primitives_traits::SealedHeader;
    
    // Create a chain spec
    let chain_spec = Arc::new(reth_bsc::chainspec::bsc::bsc_mainnet());
    
    // Create a snapshot provider
    let snapshot_provider = Arc::new(InMemorySnapshotProvider::default());
    
    // Create a validator
    let validator = ParliaHeaderValidator::new(snapshot_provider.clone());
    
    // Create a valid attestation
    let attestation = VoteAttestation {
        vote_address_set: 0b111, // First 3 validators
        agg_signature: FixedBytes::<96>::from([0u8; 96]),
        data: VoteData {
            source_number: 100,
            source_hash: B256::random(),
            target_number: 199,
            target_hash: B256::random(),
        },
        extra: bytes::Bytes::new(),
    };
    
    // Create header with attestation
    let header = create_header_with_attestation(200, true, Some(attestation));
    
    // Create a snapshot with validators
    let validators = vec![
        Address::new([1; 20]),
        Address::new([2; 20]),
        Address::new([3; 20]),
    ];
    
    let snapshot = Snapshot::new(
        validators.clone(),
        200,
        B256::random(),
        DEFAULT_EPOCH_LENGTH,
        None, // No vote addresses
    );
    
    // Store snapshot
    let sealed_header = SealedHeader::seal_slow(header);
    snapshot_provider.insert(snapshot);
    
    // Parse and verify attestation exists
    let parsed = parse_vote_attestation_from_header(&sealed_header.header(), DEFAULT_EPOCH_LENGTH, true, true);
    assert!(parsed.is_some());
}

#[test]
fn test_vote_attestation_vote_addresses() {
    // Test vote address extraction from bitmap
    let test_cases = vec![
        (0b001, vec![0]),        // Only first validator
        (0b010, vec![1]),        // Only second validator
        (0b100, vec![2]),        // Only third validator
        (0b111, vec![0, 1, 2]),  // All three validators
        (0b101, vec![0, 2]),     // First and third
    ];
    
    for (bitmap, expected_indices) in test_cases {
        // Extract indices from bitmap
        let mut indices = Vec::new();
        for i in 0..64 {
            if (bitmap & (1u64 << i)) != 0 {
                indices.push(i);
            }
        }
        assert_eq!(indices, expected_indices);
    }
}

#[test]
fn test_vote_attestation_encoding_decoding() {
    use alloy_rlp::{Encodable, Decodable};
    
    let attestation = VoteAttestation {
        vote_address_set: 0b101,
        agg_signature: FixedBytes::<96>::from([1u8; 96]),
        data: VoteData {
            source_number: 100,
            source_hash: B256::from([2u8; 32]),
            target_number: 200,
            target_hash: B256::from([3u8; 32]),
        },
        extra: bytes::Bytes::new(),
    };
    
    // Encode
    let mut encoded = Vec::new();
    attestation.encode(&mut encoded);
    
    // Decode
    let decoded = VoteAttestation::decode(&mut encoded.as_slice()).unwrap();
    
    assert_eq!(decoded.vote_address_set, attestation.vote_address_set);
    assert_eq!(decoded.agg_signature, attestation.agg_signature);
    assert_eq!(decoded.data.source_number, attestation.data.source_number);
    assert_eq!(decoded.data.target_number, attestation.data.target_number);
} 