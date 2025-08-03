# feat_parlia_20250804 Status 

## Validation Pipeline Structure:
```rust
validate_block_pre_execution_impl()
├── validate_basic_block_fields()          // Standard Ethereum validation
│   ├── transaction_root_validation()
│   └── cancun_blob_gas_validation()
└── validate_parlia_specific_fields()      // BSC-specific Parlia rules
    ├── verify_block_timing()             // Ramanujan constraints
    ├── verify_vote_attestation()         // Plato BLS signatures  
    ├── verify_seal()                     // Enhanced proposer authorization
    ├── verify_difficulty()               // Turn-based INTURN/NOTURN
    └── verify_turn_length()              // Bohr epoch boundaries

validate_block_post_execution_impl()
├── validate_basic_post_execution_fields() // Standard validation
│   ├── gas_used_verification()
│   └── verify_receipts_and_logs()
└── validate_parlia_post_execution_fields() // BSC-specific
    └── epoch_transition_validation()
```




# Branch Current Status
- It is working on EC2 for testnet. (in execution stage, 1800w)
- It can successfully run for testnet by specifying debug.tip to 100ws
- AI say:

Below is a high-level gap analysis between

• your working tree `loocapro_reth_bsc`  
• the abandoned but complete Rust prototype `zoro_reth`, and  
• the production Go implementation in `bsc-erigon`

focusing only on what is required to run a **fully-functional Parlia (PoSA) consensus** node.

════════════════════════════════════════════════════════════════
1. What is ALREADY in `loocapro_reth_bsc`
────────────────────────────────────────────────────────────────
✓ Basic data-structures (snapshot, vote, validator maps, constants).  
✓ Header-level checks (`ParliaHeaderValidator`) incl.  
  – proposer turn, seal / ECDSA recovery,  
  – block-time & attestation checks for recent hard-forks.  
✓ In-memory snapshot provider (`InMemorySnapshotProvider`).  
✓ Hertz-gas patch scaffolding and hard-fork flag helpers.  
✓ Minimal `ParliaConsensus` wrapper that forwards *header* checks.

This lets a test-chain advance blocks, but **only header validity is enforced.**
════════════════════════════════════════════════════════════════
2. Components still MISSING (relative to `zoro_reth` & `bsc-erigon`)
────────────────────────────────────────────────────────────────
A. Consensus Engine integration
   • `ParliaEngine` is a stub — it does NOT implement `reth::consensus::Consensus`
     nor is it wired into the node’s builder/pipeline.
   • Missing `ParliaEngineBuilder` & `ParliaEngineTask` (see
     `zoro_reth/crates/bsc/engine/src/{lib.rs,task.rs}`) that spawn the
     background seal-verification / fork-choice worker.

B. Pre-/Post-execution validation & block finalisation
   • `validate_block_pre_execution`, `validate_block_post_execution`,
     `validate_body_against_header` are currently `Ok(())`.
   • Logic required (all present in `zoro_reth` / `bsc-erigon`):
     – split user vs. system txs (`SlashIndicator`, `StakeHub`, etc.)  
     – epoch checkpoints (every 200 blocks) and validator-set updates  
     – block-reward & system-reward contracts  
     – diffInTurn / diffNoTurn difficulty checks  
     – slashing & BLS aggregate-signature verification paths after Luban/Maxwell.

C. Snapshot persistence & pruning
   • Only an *in-memory* provider exists.  
   • Needed: KV-backed snapshot DB, checkpointing every 10 000 blocks and
     LRU caches (see `bsc-erigon/consensus/parlia/parlia.go` `snapshot` helpers).

D. Node / CLI plumbing
   • No builder component that injects Parlia into the `NodeComponents`.
   • Pipeline stages (`StageParliaExecution`, `StageParliaFinalize`) absent.
   • CLI flags (`--consensus=parlia`, epoch/period parameters) not exposed.

E. Hard-fork feature gates
   • Helpers for Pascal, Lorentz, Maxwell exist, but
     `ChainSpec` extensions that activate them are still TODO.
   • Time-based fork checks (`isPrague`, `isFeynman`, …) implemented in Go
     need Rust equivalents.

F. Testing
   • No dedicated consensus test-vectors (snapshots, fork transition cases).  
   • Integration tests in `zoro_reth/tests/` not ported.

════════════════════════════════════════════════════════════════
3. Minimum NEXT STEPS to reach a runnable full node
────────────────────────────────────────────────────────────────
1. Port `crates/bsc/engine` from `zoro_reth`
   • Copy `ParliaEngine`, `Task`, `Builder` and adapt module paths
     (`reth_*` crates have drifted upstream).  
   • Implement `Consensus` trait for `ParliaEngine`; delegate header checks
     to existing `ParliaHeaderValidator`, add body/pre/post hooks.

2. Wire the engine into the node
   • Add a `ParliaComponent` to your node builder similar to
     `zoro_reth/crates/node/builder/src/components/parlia.rs`.  
   • Expose `--consensus parlia` (and `epoch`, `period`) in the CLI.

3. Persist snapshots
   • Create `kv_snapshot_provider.rs` backed by `reth_db::database::DatabaseEnv`.  
   • Maintain `recentSnaps` & `signatures` LRU caches (see lines 211-227 of
     `bsc-erigon/consensus/parlia/parlia.go`).

4. Implement block-level checks & rewards
   • Port `verify_turn_length`, `splitTxs`, `Finalize` and validator-set update
     logic from `bsc-erigon`.  
   • Ensure Hertz-gas patch and hard-fork–specific rules are called from
     `initialize()` / `finalize()`.

5. Extend `ChainSpec`
   • Add BSC fork timings & Parlia fields (`epoch`, `period`) to `reth_chainspec`.  
   • Hook time/height-based helpers into validation paths.

6. Tests
   • Port `tests/consensus_parlia.rs` from `zoro_reth`.  
   • Add regression tests for Lorentz & Maxwell header rules.

════════════════════════════════════════════════════════════════
4. How to proceed efficiently
────────────────────────────────────────────────────────────────
• Start by porting the Rust code from `zoro_reth` — it already follows
  the Reth architecture, so the diff against upstream `reth` is small.  
• Use Go reference (`bsc-erigon`) only for edge-cases not covered in Rust
  (e.g. stake contract ABI calls, daily validator refresh logic).  
• Keep each milestone compilable:
  – Step-1: engine compiles & headers validated.  
  – Step-2: pre-execution done, blocks execute.  
  – Step-3: post-execution & rewards, node fully syncs.

Implementing the six items above will close every functionality gap that
currently prevents `loocapro_reth_bsc` from acting as a **fully-featured
Parlia full node** on BSC mainnet/testnet.


# Reth @ BSC

A BSC-compatible Reth client implementation. This project is **not** a fork of Reth, but rather an extension that leverages Reth's powerful `NodeBuilder` API to provide BSC compatibility.

## About

This project aims to bring Reth's high-performance Ethereum client capabilities to the BSC network. By utilizing Reth's modular architecture and NodeBuilder API, we're building a BSC-compatible client that maintains compatibility with Reth's ecosystem while adding BSC-specific features.

## Current Status

This is a **Work in Progress** project that requires community contributions to achieve the following goals:

- [ ] Historical Sync
- [ ] BSC Pectra Support
- [ ] Live Sync

## Getting Started

Refer to the [Reth documentation](https://reth.rs/) for general guidance on running a node. Note that some BSC-specific configurations may be required.

### Historical Sync

To trigger historical sync, follow these steps:

1. Build the release version:

```bash
cargo build --bin reth-bsc --release
```

2. Run the node with logging enabled:

```bash
RUST_LOG=info ./target/release/reth-bsc node \
    --chain bsc \
    --debug.tip "" # set the tip to the block you want to sync to
```

## Contributing

We welcome community contributions! Whether you're interested in helping with historical sync implementation, BSC Pectra support, or live sync functionality, your help is valuable. Please feel free to open issues or submit pull requests. You can reach out to me on [Telegram](https://t.me/loocapro).

## Disclaimer

This project is experimental and under active development. Use at your own risk.

## Credits

This project is inspired by and builds upon the work of:

- [BNB Chain Reth](https://github.com/bnb-chain/reth) - The original BSC implementation of Reth
- The Reth team, especially [@mattsse](https://github.com/mattsse) for their invaluable contributions to the Reth ecosystem
