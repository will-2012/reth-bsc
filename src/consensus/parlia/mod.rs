pub mod vote;
pub mod snapshot;
pub mod provider;
pub mod hertz_patch;
pub mod constants;
pub mod gas;
pub mod util;
pub mod error;
pub mod consensus;
pub mod validations;
pub mod db;

pub use snapshot::{Snapshot, ValidatorInfo, CHECKPOINT_INTERVAL};
pub use vote::{VoteAddress, VoteAttestation, VoteData, VoteEnvelope, VoteSignature, ValidatorsBitSet};
pub use constants::*;
pub use hertz_patch::{HertzPatchManager, StoragePatch};
pub use error::ParliaConsensusError;
pub use util::hash_with_chain_id;
pub use provider::SnapshotProvider;
pub use consensus::Parlia;

/// Epoch length.
pub const EPOCH: u64 = 200;