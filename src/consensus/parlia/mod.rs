//! Skeleton implementation for Parlia (Proof-of-Staked-Authority) consensus.
//!
//! This is **phase-1** of the full port. For now we only define the core data
//! structures (snapshot & signer) and stub traits so that other crates can
//! depend on them without compilation errors. Real validation logic will be
//! added in subsequent milestones.

use alloy_primitives::{address, Address, B256};
use std::collections::{BTreeMap, BTreeSet};

/// Epoch length (200 blocks on BSC main-net).
pub const EPOCH: u64 = 200;

// ============================================================================
// Snapshot
// ============================================================================

/// An in-memory view of the validator set at a specific block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Block number the snapshot corresponds to.
    pub number: u64,
    /// Block hash at that height.
    pub hash: B256,
    /// Ordered validator addresses (Proof-of-Authority).
    pub validators: BTreeSet<Address>,
}

impl Snapshot {
    /// Returns `true` if `addr` is an authorised validator in this snapshot.
    pub fn contains(&self, addr: &Address) -> bool {
        self.validators.contains(addr)
    }
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            number: 0,
            hash: B256::ZERO,
            validators: BTreeSet::from([address!("0x0000000000000000000000000000000000000000")]),
        }
    }
}

// ============================================================================
// Signer helper (future: recover signer from seal)
// ============================================================================

/// Helper that rotates proposers based on `block.number % epoch`.
#[derive(Debug, Clone)]
pub struct StepSigner {
    epoch: u64,
}

impl StepSigner {
    pub const fn new(epoch: u64) -> Self { Self { epoch } }

    /// Expected proposer index for `number` given a snapshot.
    pub fn proposer_index(&self, number: u64) -> u64 { number % self.epoch }
}

// ============================================================================
// Consensus Engine stub (will implement traits in PR-2)
// ============================================================================

#[derive(Debug, Default, Clone)]
pub struct ParliaEngine;

impl ParliaEngine {
    pub fn new() -> Self { Self }
}

// The real trait impls (HeaderValidator, Consensus, FullConsensus) will be
// added in a later milestone. For now we only ensure the module compiles. 