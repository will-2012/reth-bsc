use std::sync::Arc;

use alloy_consensus::Header;
use alloy_primitives::{Address, Bytes, B256, U256};
use reth_bsc::consensus::parlia::{self, InMemorySnapshotProvider, ParliaHeaderValidator, SnapshotProvider};
use reth::consensus::HeaderValidator;
use reth_bsc::consensus::parlia::snapshot::{Snapshot, DEFAULT_EPOCH_LENGTH};
use reth_primitives_traits::SealedHeader;

/// Returns address with last byte repeated `b`.
fn addr(b: u8) -> Address { Address::repeat_byte(b) }

#[test]
fn parlia_header_basic_validation_passes() {
    // --- Step 1: genesis header ------------------------------------------
    let mut genesis = Header::default();
    genesis.number = 0;
    genesis.beneficiary = addr(1);
    genesis.timestamp = 0;
    genesis.difficulty = U256::from(1);
    genesis.gas_limit = 30_000_000;
    // extra-data := 32-byte vanity + 65-byte seal (all zeros) → legacy format.
    genesis.extra_data = Bytes::from(vec![0u8; parlia::constants::EXTRA_VANITY + parlia::constants::EXTRA_SEAL]);

    let sealed_genesis = SealedHeader::seal_slow(genesis.clone());

    // --- Step 2: initial snapshot seeded from genesis --------------------
    let validators = vec![addr(1), addr(2), addr(3)];
    let snapshot = Snapshot::new(validators.clone(), 0, sealed_genesis.hash(), DEFAULT_EPOCH_LENGTH, None);

    let provider = Arc::new(InMemorySnapshotProvider::default());
    provider.insert(snapshot);

    let validator = ParliaHeaderValidator::new(provider);

    // --- Step 3: construct block #1 header -------------------------------
    let mut h1 = Header::default();
    h1.parent_hash = sealed_genesis.hash();
    h1.number = 1;
    h1.beneficiary = addr(2); // in-turn validator for block 1
    h1.timestamp = 1;         // > parent.timestamp
    h1.difficulty = U256::from(2); // in-turn ⇒ difficulty 2
    h1.gas_limit = 30_000_000;
    h1.extra_data = Bytes::from(vec![0u8; parlia::constants::EXTRA_VANITY + parlia::constants::EXTRA_SEAL]);

    let sealed_h1 = SealedHeader::seal_slow(h1.clone());

    // --- Step 4: run validations -----------------------------------------
    validator.validate_header(&sealed_h1).expect("header-level validation");
    validator
        .validate_header_against_parent(&sealed_h1, &sealed_genesis)
        .expect("parent-linked validation");
} 