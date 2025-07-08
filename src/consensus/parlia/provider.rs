use super::snapshot::Snapshot;
use super::validator::SnapshotProvider;
use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;

/// Very simple `SnapshotProvider` that keeps the most recent `max_entries` snapshots in memory.
/// Keys are the **block number** the snapshot is valid for (i.e. the last block of the snapshot’s
/// epoch). For historical sync this is sufficient – we can switch to an MDBX-backed provider later.
#[derive(Clone, Debug)]
pub struct InMemorySnapshotProvider {
    inner: Arc<RwLock<BTreeMap<u64, Snapshot>>>,
    max_entries: usize,
}

impl InMemorySnapshotProvider {
    /// Create a new provider keeping at most `max_entries` snapshots.
    pub fn new(max_entries: usize) -> Self {
        Self { inner: Arc::new(RwLock::new(BTreeMap::new())), max_entries }
    }
}

impl Default for InMemorySnapshotProvider {
    fn default() -> Self { Self::new(2048) }
}

impl SnapshotProvider for InMemorySnapshotProvider {
    fn snapshot(&self, block_number: u64) -> Option<Snapshot> {
        let guard = self.inner.read();
        // Find the greatest key <= block_number.
        if let Some((_, snap)) = guard.range(..=block_number).next_back() {
            return Some(snap.clone());
        }
        None
    }

    fn insert(&self, snapshot: Snapshot) {
        let mut guard = self.inner.write();
        guard.insert(snapshot.block_number, snapshot);
        // clamp size
        while guard.len() > self.max_entries {
            // remove the smallest key
            if let Some(first_key) = guard.keys().next().cloned() {
                guard.remove(&first_key);
            }
        }
    }
} 