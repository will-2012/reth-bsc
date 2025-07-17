use crate::{
    chainspec::BscChainSpec,
    hardforks::BscHardforks,
    node::primitives::BscPrimitives,
};
use consensus::{BscConsensusBuilder, BscConsensus};
use engine::BscPayloadServiceBuilder;
use rpc::engine_api::validator::BscEngineValidator;
use evm::{BscExecutorBuilder, BscEvmConfig};
use network::BscNetworkBuilder;
use reth::{
    api::{FullNodeComponents, FullNodeTypes, NodeTypes},
    builder::{
        components::{ComponentsBuilder, PoolBuilder, PayloadServiceBuilder, NetworkBuilder, ExecutorBuilder, ConsensusBuilder}, rpc::RpcAddOns, DebugNode, Node, NodeAdapter,
        NodeComponentsBuilder,
    },
};
use reth_engine_local::LocalPayloadAttributesBuilder;
use reth_engine_primitives::BeaconConsensusEngineHandle;
use reth_node_ethereum::node::EthereumPoolBuilder;
use reth_payload_primitives::{PayloadAttributesBuilder, PayloadTypes};
use reth_primitives::BlockBody;
use reth_trie_db::MerklePatriciaTrie;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

// Import BSC-specific RPC builders
use rpc::{
    BscEthApiBuilder,
    engine_api::{
        builder::BscEngineApiBuilder,
        payload::BscPayloadTypes,
        validator::BscEngineValidatorBuilder,
    },
};

// Import BSC storage
use storage::BscStorage;

pub mod builder;
pub mod consensus;
pub mod engine;
pub mod evm;
pub mod network;
pub mod primitives;
pub mod rpc;
pub mod storage;

/// Bsc addons configuring RPC types
pub type BscNodeAddOns<N> = RpcAddOns<
    N,
    BscEthApiBuilder,
    BscEngineValidatorBuilder,
    BscEngineApiBuilder<BscEngineValidatorBuilder>,
>;

/// Type configuration for a regular BSC node.
#[derive(Debug, Clone)]
pub struct BscNode {
    engine_handle_rx:
        Arc<Mutex<Option<oneshot::Receiver<BeaconConsensusEngineHandle<BscPayloadTypes>>>>>,
}

impl BscNode {
    pub fn new() -> (Self, oneshot::Sender<BeaconConsensusEngineHandle<BscPayloadTypes>>) {
        let (tx, rx) = oneshot::channel();
        (Self { engine_handle_rx: Arc::new(Mutex::new(Some(rx))) }, tx)
    }
}

impl Default for BscNode {
    fn default() -> Self {
        // Create a fresh channel and discard the sender. The receiver side is stored inside the
        // node instance and is later consumed by `BscNetworkBuilder` when the node launches. This
        // mirrors the behaviour of `BscNode::new()` while satisfying the `Default` requirement
        // imposed by the e2e-test-utils `NodeBuilderHelper` blanket implementation.
        let (node, _sender) = Self::new();
        node
    }
}



impl NodeTypes for BscNode {
    type Primitives = BscPrimitives;
    type ChainSpec = BscChainSpec;
    type StateCommitment = MerklePatriciaTrie;
    type Storage = BscStorage;
    type Payload = BscPayloadTypes;
}

/// Custom BSC Components Builder that bypasses the generic ComponentsBuilder
pub struct BscNodeComponentsBuilder {
    engine_handle_rx: Arc<Mutex<Option<oneshot::Receiver<BeaconConsensusEngineHandle<BscPayloadTypes>>>>>,
}

impl<N> NodeComponentsBuilder<N> for BscNodeComponentsBuilder
where
    N: FullNodeTypes<Types = BscNode>,
{
    type Components = reth::builder::components::Components<
        N,
        reth_network::NetworkHandle<crate::node::network::BscNetworkPrimitives>,
        reth_transaction_pool::EthTransactionPool<N::Provider, reth_transaction_pool::blobstore::DiskFileBlobStore>,
        crate::node::evm::BscEvmConfig,
        Arc<dyn reth::consensus::FullConsensus<crate::node::primitives::BscPrimitives, Error = reth::consensus::ConsensusError>>,
    >;

    async fn build_components(
        self,
        ctx: &reth::builder::BuilderContext<N>,
    ) -> eyre::Result<Self::Components> {
        // Build each component manually using the proper traits
        let pool_builder = EthereumPoolBuilder::default();
        let pool = PoolBuilder::build_pool(pool_builder, ctx).await?;

        let executor_builder = BscExecutorBuilder;
        let evm_config = ExecutorBuilder::build_evm(executor_builder, ctx).await?;

        let network_builder = BscNetworkBuilder { engine_handle_rx: self.engine_handle_rx.clone() };
        let network = NetworkBuilder::build_network(network_builder, ctx, pool.clone()).await?;

        let payload_builder = BscPayloadServiceBuilder::default();
        let payload_builder_handle = payload_builder.spawn_payload_builder_service(ctx, pool.clone(), evm_config.clone()).await?;

        let consensus_builder = BscConsensusBuilder;
        let consensus = ConsensusBuilder::build_consensus(consensus_builder, ctx).await?;

        Ok(reth::builder::components::Components {
            transaction_pool: pool,
            evm_config,
            network,
            payload_builder_handle,
            consensus,
        })
    }
}

impl<N> Node<N> for BscNode
where
    N: FullNodeTypes<Types = Self>,
{
    type ComponentsBuilder = BscNodeComponentsBuilder;

    type AddOns = BscNodeAddOns<
        NodeAdapter<N, <Self::ComponentsBuilder as NodeComponentsBuilder<N>>::Components>,
    >;

    fn components_builder(&self) -> Self::ComponentsBuilder {
        BscNodeComponentsBuilder {
            engine_handle_rx: self.engine_handle_rx.clone(),
        }
    }

    fn add_ons(&self) -> Self::AddOns {
        BscNodeAddOns::default()
    }
}

impl<N> DebugNode<N> for BscNode
where
    N: FullNodeComponents<Types = Self>,
{
    type RpcBlock = alloy_rpc_types::Block;

    fn rpc_to_primitive_block(rpc_block: Self::RpcBlock) -> reth_primitives::Block {
        let alloy_rpc_types::Block { header, transactions, withdrawals, .. } = rpc_block;
        reth_primitives::Block {
            header: header.inner,
            body: reth_primitives::BlockBody {
                transactions: transactions
                    .into_transactions()
                    .map(|tx| tx.inner.into_inner().into())
                    .collect(),
                ommers: Default::default(),
                withdrawals,
            },
        }
    }

    fn local_payload_attributes_builder(
        chain_spec: &Self::ChainSpec,
    ) -> impl PayloadAttributesBuilder<<Self::Payload as PayloadTypes>::PayloadAttributes> {
        // Return a builder that always sets withdrawals to None to satisfy BSC rules.
        struct Builder { spec: Arc<BscChainSpec> }
        impl PayloadAttributesBuilder<reth_node_ethereum::engine::EthPayloadAttributes> for Builder {
            fn build(&self, timestamp: u64) -> reth_node_ethereum::engine::EthPayloadAttributes {
                reth_node_ethereum::engine::EthPayloadAttributes {
                    timestamp,
                    prev_randao: alloy_primitives::B256::random(),
                    suggested_fee_recipient: alloy_primitives::Address::random(),
                    withdrawals: None,
                    parent_beacon_block_root: None,
                }
            }
        }
        Builder { spec: Arc::new(chain_spec.clone()) }
    }
}
