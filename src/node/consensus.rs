use crate::{hardforks::BscHardforks, node::primitives::BscPrimitives};
use reth_primitives::Block;
use reth::{
    api::{FullNodeTypes, NodeTypes},
    builder::{components::ConsensusBuilder, BuilderContext},
    consensus::{Consensus, ConsensusError, FullConsensus, HeaderValidator},
};
use reth_chainspec::EthChainSpec;
use reth_primitives::{Receipt, RecoveredBlock, SealedBlock, SealedHeader};
use reth_primitives_traits::Block as BlockT;
use reth_provider::BlockExecutionResult;
use std::sync::Arc;
// Parlia header validation integration ------------------------------------
use crate::consensus::parlia::{
    snapshot::Snapshot, InMemorySnapshotProvider, ParliaHeaderValidator, SnapshotProvider,
};
use std::fmt::Debug;
use reth_engine_primitives::{EngineValidator, PayloadValidator};
use alloy_consensus::BlockHeader;

/// A basic Bsc consensus builder.
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BscConsensusBuilder;

impl<Node> ConsensusBuilder<Node> for BscConsensusBuilder
where
    Node: FullNodeTypes,
    Node::Types: NodeTypes<Primitives = crate::node::primitives::BscPrimitives, ChainSpec = crate::chainspec::BscChainSpec, Payload = crate::node::rpc::engine_api::payload::BscPayloadTypes, StateCommitment = reth_trie_db::MerklePatriciaTrie, Storage = crate::node::storage::BscStorage>,
{
    type Consensus = Arc<dyn FullConsensus<BscPrimitives, Error = ConsensusError>>;

    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(BscConsensus::new(ctx.chain_spec())))
    }
}

/// BSC consensus implementation.
#[derive(Debug, Clone)]
pub struct BscConsensus<ChainSpec, P = InMemorySnapshotProvider> {
    /// Parlia‐specific header validator.
    parlia: ParliaHeaderValidator<P>,
    _phantom: std::marker::PhantomData<ChainSpec>,
}

impl<ChainSpec: EthChainSpec + BscHardforks> BscConsensus<ChainSpec> {
    /// Create a new instance of [`BscConsensus`] with an in-memory snapshot provider.
    pub fn new(chain_spec: Arc<ChainSpec>) -> Self {
        let provider = InMemorySnapshotProvider::new(1024);
        let snapshot = Snapshot::new(
            vec![chain_spec.genesis_header().beneficiary()],
            0,
            chain_spec.genesis_hash(),
            0,
            None,
        );
        provider.insert(snapshot);
        let parlia = ParliaHeaderValidator::new(Arc::new(provider));
        Self { parlia, _phantom: std::marker::PhantomData }
    }
}

impl<ChainSpec, P> PayloadValidator for BscConsensus<ChainSpec, P>
where
    ChainSpec: Send + Sync + 'static + Unpin,
    P: Send + Sync + 'static + Unpin,
{
    type Block = Block;
    type ExecutionData = alloy_rpc_types_engine::ExecutionData;

    fn ensure_well_formed_payload(
        &self,
        _payload: Self::ExecutionData,
    ) -> Result<RecoveredBlock<Self::Block>, reth_payload_primitives::NewPayloadError> {
        // This is a no-op validator, so we can just return an empty block.
        let block: Block = Block::default();
        let recovered = RecoveredBlock::new(
            block.clone(),
            Vec::new(),
            block.header.hash_slow(),
        );
        Ok(recovered)
    }
}

impl<ChainSpec, P, Types> EngineValidator<Types> for BscConsensus<ChainSpec, P>
where
    ChainSpec: Send + Sync + 'static + Unpin,
    P: Send + Sync + 'static + Unpin,
    Types: reth_node_api::PayloadTypes<ExecutionData = alloy_rpc_types_engine::ExecutionData>,
{
    fn validate_version_specific_fields(
        &self,
        _version: reth_payload_primitives::EngineApiMessageVersion,
        _payload_or_attrs: reth_payload_primitives::PayloadOrAttributes<
            '_,
            <Types as reth_node_api::PayloadTypes>::ExecutionData,
            <Types as reth_node_api::PayloadTypes>::PayloadAttributes,
        >,
    ) -> Result<(), reth_payload_primitives::EngineObjectValidationError> {
        Ok(())
    }

    fn ensure_well_formed_attributes(
        &self,
        _version: reth_payload_primitives::EngineApiMessageVersion,
        _attributes: &<Types as reth_node_api::PayloadTypes>::PayloadAttributes,
    ) -> Result<(), reth_payload_primitives::EngineObjectValidationError> {
        Ok(())
    }

    fn validate_payload_attributes_against_header(
        &self,
        _attr: &<Types as reth_node_api::PayloadTypes>::PayloadAttributes,
        _header: &<Self::Block as reth_primitives_traits::Block>::Header,
    ) -> Result<(), reth_payload_primitives::InvalidPayloadAttributesError> {
        // Skip default timestamp validation for BSC
        Ok(())
    }
}

impl<ChainSpec, P> HeaderValidator for BscConsensus<ChainSpec, P>
where
    ChainSpec: Send + Sync + 'static + Debug,
    P: SnapshotProvider + Debug + 'static,
{
    fn validate_header(&self, header: &SealedHeader) -> Result<(), ConsensusError> {
        self.parlia.validate_header(header)
    }

    fn validate_header_against_parent(
        &self,
        header: &SealedHeader,
        parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        self.parlia.validate_header_against_parent(header, parent)
    }
}

impl<ChainSpec, P> Consensus<Block> for BscConsensus<ChainSpec, P>
where
    ChainSpec: Send + Sync + 'static + Debug,
    P: SnapshotProvider + Debug + 'static,
{
    type Error = ConsensusError;

    fn validate_body_against_header(
        &self,
        _body: &<Block as BlockT>::Body,
        _header: &SealedHeader,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    fn validate_block_pre_execution(
        &self,
        _block: &SealedBlock<Block>,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }
}

impl<ChainSpec, P> FullConsensus<BscPrimitives> for BscConsensus<ChainSpec, P>
where
    ChainSpec: Send + Sync + 'static + Debug,
    P: SnapshotProvider + Debug + 'static,
{
    fn validate_block_post_execution(
        &self,
        _block: &RecoveredBlock<Block>,
        _result: &BlockExecutionResult<Receipt>,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }
}
