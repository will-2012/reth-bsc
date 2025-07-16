use crate::{hardforks::BscHardforks, node::BscNode, BscBlock, BscBlockBody, BscPrimitives};
use alloy_consensus::Header;
use reth::{
    api::FullNodeTypes,
    beacon_consensus::EthBeaconConsensus,
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{Consensus, ConsensusError, FullConsensus, HeaderValidator},
    consensus_common::validation::{
        validate_against_parent_4844, validate_against_parent_hash_number,
        validate_against_parent_timestamp,
    },
};
use reth_chainspec::EthChainSpec;
use reth_primitives::{Receipt, RecoveredBlock, SealedBlock, SealedHeader};
use reth_provider::BlockExecutionResult;
use std::sync::Arc;
// Parlia header validation integration ------------------------------------
use crate::consensus::parlia::{InMemorySnapshotProvider, ParliaHeaderValidator, SnapshotProvider};
use std::fmt::Debug;
use alloy_primitives::Address;
use alloy_consensus::BlockHeader;

/// A basic Bsc consensus builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BscConsensusBuilder;

impl<Node> ConsensusBuilder<Node> for BscConsensusBuilder
where
    Node: FullNodeTypes<Types = BscNode>,
{
    type Consensus = Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError>>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(BscConsensus::new(ctx.chain_spec())))
    }
}

/// BSC consensus implementation.
///
/// Provides basic checks as outlined in the execution specs.
#[derive(Debug, Clone)]
pub struct BscConsensus<ChainSpec, P = InMemorySnapshotProvider> {
    inner: EthBeaconConsensus<ChainSpec>,
    /// Parlia‐specific header validator.
    parlia: ParliaHeaderValidator<P>,
    chain_spec: Arc<ChainSpec>,
}

impl<ChainSpec: EthChainSpec + BscHardforks> BscConsensus<ChainSpec> {
    /// Create a new instance of [`BscConsensus`] with an in-memory snapshot provider.
    pub fn new(chain_spec: Arc<ChainSpec>) -> Self {
        // For now we keep a simple RAM snapshot cache. A DB-backed provider can be wired in later.
        // ----------------------------------------------------------------
        // 1. Build initial snapshot from the chain-spec genesis header.
        // ----------------------------------------------------------------
        use crate::consensus::parlia::snapshot::{Snapshot, DEFAULT_EPOCH_LENGTH};
        use crate::consensus::parlia::constants::{EXTRA_VANITY, EXTRA_SEAL};

        let genesis_header = chain_spec.genesis_header();
        let extra = genesis_header.extra_data().as_ref();

        // Extract validator addresses encoded in extra-data (legacy format).
        let mut validators = Vec::new();
        if extra.len() > EXTRA_VANITY + EXTRA_SEAL {
            let validator_bytes = &extra[EXTRA_VANITY..extra.len() - EXTRA_SEAL];
            for chunk in validator_bytes.chunks(20) {
                if chunk.len() == 20 {
                    validators.push(Address::from_slice(chunk));
                }
            }
        }

        // Fallback: include beneficiary if no list found – keeps snapshot non-empty.
        if validators.is_empty() {
            validators.push(genesis_header.beneficiary());
        }

        let genesis_hash = chain_spec.genesis_hash();
        let snapshot = Snapshot::new(
            validators,
            0,
            genesis_hash,
            DEFAULT_EPOCH_LENGTH,
            None,
        );

        // ----------------------------------------------------------------
        // 2. Create provider, seed snapshot, and instantiate Parlia validator.
        // ----------------------------------------------------------------
        let provider = Arc::new(InMemorySnapshotProvider::default());
        provider.insert(snapshot);

        let parlia = ParliaHeaderValidator::new(provider);

        Self { inner: EthBeaconConsensus::new(chain_spec.clone()), parlia, chain_spec }
    }
}

impl<ChainSpec, P> HeaderValidator for BscConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec + BscHardforks,
    P: SnapshotProvider + Debug + 'static,
{
    fn validate_header(&self, header: &SealedHeader) -> Result<(), ConsensusError> {
        // Run Parlia-specific validations.
        self.parlia.validate_header(header)?;
        Ok(())
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        // Parlia checks (gas-limit, attestation, snapshot advancement, …)
        self.parlia.validate_header_against_parent(header, parent)?;

        // Generic execution-layer parent checks reused from Beacon spec.
        validate_against_parent_hash_number(header.header(), parent)?;
        validate_against_parent_timestamp(header.header(), parent.header())?;

        // Ensure blob-gas fields consistency for Cancun and later.
        if let Some(blob_params) = self.chain_spec.blob_params_at_timestamp(header.timestamp) {
            validate_against_parent_4844(header.header(), parent.header(), blob_params)?;
        }

        Ok(())
    }
}

impl<ChainSpec, P> Consensus<BscBlock> for BscConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec<Header = Header> + BscHardforks,
    P: SnapshotProvider + Debug + 'static,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        body: &BscBlockBody,
        header: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        Consensus::<BscBlock>::validate_body_against_header(&self.inner, body, header)
    }

    fn validate_block_pre_execution(
        &self,
        _block: &SealedBlock<BscBlock>,
    ) -> Result<(), ConsensusError> {
        // Check ommers hash
        // let ommers_hash = block.body().calculate_ommers_root();
        // if Some(block.ommers_hash()) != ommers_hash {
        //     return Err(ConsensusError::BodyOmmersHashDiff(
        //         GotExpected {
        //             got: ommers_hash.unwrap_or(EMPTY_OMMER_ROOT_HASH),
        //             expected: block.ommers_hash(),
        //         }
        //         .into(),
        //     ))
        // }

        // // Check transaction root
        // if let Err(error) = block.ensure_transaction_root_valid() {
        //     return Err(ConsensusError::BodyTransactionRootDiff(error.into()))
        // }

        // if self.chain_spec.is_cancun_active_at_timestamp(block.timestamp()) {
        //     validate_cancun_gas(block)?;
        // } else {
        //     return Ok(())
        // }

        Ok(())
    }
}

impl<ChainSpec, P> FullConsensus<BscPrimitives> for BscConsensus<ChainSpec, P>
where
    ChainSpec: EthChainSpec<Header = Header> + BscHardforks,
    P: SnapshotProvider + Debug + 'static,
{
    fn validate_block_post_execution(
        &self,
        block: &RecoveredBlock<BscBlock>,
        result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), ConsensusError> {
        FullConsensus::<BscPrimitives>::validate_block_post_execution(&self.inner, block, result)
    }
}
