use crate::{
    chainspec::{bsc::head, BscChainSpec},
    consensus::ParliaConsensus,
    node::{
        network::block_import::{handle::ImportHandle, service::ImportService, BscBlockImport},
        rpc::engine_api::payload::BscPayloadTypes,
    },
};
use alloy_consensus::BlobTransactionSidecar;
use alloy_primitives::B256;
use alloy_rlp::{Decodable, Encodable, Header, RlpDecodable, RlpEncodable};
use handshake::BscHandshake;
use reth::{
    api::{FullNodeTypes, NodeTypes, TxTy},
    builder::{components::NetworkBuilder, BuilderContext},
    transaction_pool::{PoolTransaction, TransactionPool},
};
use reth_chainspec::Hardforks;
use reth_discv4::{Discv4Config, NodeRecord};
use reth_engine_primitives::BeaconConsensusEngineHandle;
use reth_eth_wire::{BasicNetworkPrimitives, NewBlock, NewBlockPayload};
use reth_ethereum_primitives::{EthPrimitives, PooledTransactionVariant};
use reth_network::{NetworkConfig, NetworkHandle, NetworkManager};
use reth_network_api::PeersInfo;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::info;

pub mod block_import;
pub mod handshake;
pub(crate) mod upgrade_status;

/// BSC representation of a EIP-4844 sidecar.
#[derive(Debug, Clone, PartialEq, Eq, RlpEncodable, RlpDecodable)]
pub struct BscP2PSidecar {
    pub inner: BlobTransactionSidecar,
    pub block_number: u64,
    pub block_hash: B256,
    pub tx_index: u64,
    pub tx_hash: B256,
}

/// BSC `NewBlock` message value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BscNewBlock {
    pub inner: NewBlock,
    pub sidecars: Vec<BscP2PSidecar>,
}

impl BscNewBlock {
    fn rlp_header(&self) -> Header {
        Header {
            list: true,
            payload_length: self.inner.block.length() +
                self.inner.td.length() +
                self.sidecars.length(),
        }
    }
}

impl Encodable for BscNewBlock {
    fn encode(&self, out: &mut dyn bytes::BufMut) {
        self.rlp_header().encode(out);
        self.inner.block.encode(out);
        self.inner.td.encode(out);
        self.sidecars.encode(out);
    }

    fn length(&self) -> usize {
        self.rlp_header().length_with_payload()
    }
}

impl Decodable for BscNewBlock {
    fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
        let header = Header::decode(buf)?;
        if !header.list {
            return Err(alloy_rlp::Error::UnexpectedString);
        }
        let remaining = buf.len();

        let this = Self {
            inner: NewBlock { block: Decodable::decode(buf)?, td: Decodable::decode(buf)? },
            sidecars: Decodable::decode(buf)?,
        };

        if buf.len() + header.payload_length != remaining {
            return Err(alloy_rlp::Error::UnexpectedLength);
        }

        Ok(this)
    }
}

impl NewBlockPayload for BscNewBlock {
    type Block = reth_ethereum_primitives::Block;

    fn block(&self) -> &Self::Block {
        &self.inner.block
    }
}

/// Network primitives for BSC.
pub type BscNetworkPrimitives =
    BasicNetworkPrimitives<EthPrimitives, PooledTransactionVariant, BscNewBlock>;

/// A basic bsc network builder.
#[derive(Debug)]
pub struct BscNetworkBuilder {
    pub(crate) engine_handle_rx:
        Arc<Mutex<Option<oneshot::Receiver<BeaconConsensusEngineHandle<BscPayloadTypes>>>>>,
}

impl BscNetworkBuilder {
    /// Returns the [`NetworkConfig`] that contains the settings to launch the p2p network.
    ///
    /// This applies the configured [`BscNetworkBuilder`] settings.
    pub fn network_config<Node>(
        self,
        ctx: &BuilderContext<Node>,
    ) -> eyre::Result<NetworkConfig<Node::Provider, BscNetworkPrimitives>>
    where
        Node: FullNodeTypes<Types: NodeTypes<ChainSpec: Hardforks>>,
    {
        let Self { engine_handle_rx } = self;

        let network_builder = ctx.network_config_builder()?;
        let mut discv4 = Discv4Config::builder();
        discv4.add_boot_nodes(boot_nodes()).lookup_interval(Duration::from_millis(500));

        let (to_import, from_network) = mpsc::unbounded_channel();
        let (to_network, import_outcome) = mpsc::unbounded_channel();

        let handle = ImportHandle::new(to_import, import_outcome);
        let consensus = Arc::new(ParliaConsensus { provider: ctx.provider().clone() });

        ctx.task_executor().spawn_critical("block import", async move {
            let handle = engine_handle_rx
                .lock()
                .await
                .take()
                .expect("node should only be launched once")
                .await
                .unwrap();

            ImportService::new(consensus, handle, from_network, to_network).await.unwrap();
        });

        let network_builder = network_builder
            .boot_nodes(boot_nodes())
            .set_head(head())
            .with_pow()
            .block_import(Box::new(BscBlockImport::new(handle)))
            .discovery(discv4)
            .eth_rlpx_handshake(Arc::new(BscHandshake::default()));

        let network_config = ctx.build_network_config(network_builder);

        Ok(network_config)
    }
}

impl<Node, Pool> NetworkBuilder<Node, Pool> for BscNetworkBuilder
where
    Node: FullNodeTypes<Types: NodeTypes<ChainSpec = BscChainSpec, Primitives = EthPrimitives>>,
    Pool: TransactionPool<
            Transaction: PoolTransaction<
                Consensus = TxTy<Node::Types>,
                Pooled = PooledTransactionVariant,
            >,
        > + Unpin
        + 'static,
{
    type Network = NetworkHandle<BscNetworkPrimitives>;

    async fn build_network(
        self,
        ctx: &BuilderContext<Node>,
        pool: Pool,
    ) -> eyre::Result<Self::Network> {
        let network_config = self.network_config(ctx)?;
        let network = NetworkManager::builder(network_config).await?;
        let handle = ctx.start_network(network, pool);
        info!(target: "reth::cli", enode=%handle.local_node_record(), "P2P networking initialized");

        Ok(handle)
    }
}

/// BSC mainnet bootnodes <https://github.com/bnb-chain/bsc/blob/master/params/bootnodes.go#L23>
static BOOTNODES : [&str; 6] = [
    "enode://433c8bfdf53a3e2268ccb1b829e47f629793291cbddf0c76ae626da802f90532251fc558e2e0d10d6725e759088439bf1cd4714716b03a259a35d4b2e4acfa7f@52.69.102.73:30311",
	"enode://571bee8fb902a625942f10a770ccf727ae2ba1bab2a2b64e121594a99c9437317f6166a395670a00b7d93647eacafe598b6bbcef15b40b6d1a10243865a3e80f@35.73.84.120:30311",
	"enode://fac42fb0ba082b7d1eebded216db42161163d42e4f52c9e47716946d64468a62da4ba0b1cac0df5e8bf1e5284861d757339751c33d51dfef318be5168803d0b5@18.203.152.54:30311",
	"enode://3063d1c9e1b824cfbb7c7b6abafa34faec6bb4e7e06941d218d760acdd7963b274278c5c3e63914bd6d1b58504c59ec5522c56f883baceb8538674b92da48a96@34.250.32.100:30311",
	"enode://ad78c64a4ade83692488aa42e4c94084516e555d3f340d9802c2bf106a3df8868bc46eae083d2de4018f40e8d9a9952c32a0943cd68855a9bc9fd07aac982a6d@34.204.214.24:30311",
	"enode://5db798deb67df75d073f8e2953dad283148133acb520625ea804c9c4ad09a35f13592a762d8f89056248f3889f6dcc33490c145774ea4ff2966982294909b37a@107.20.191.97:30311",

];

pub fn boot_nodes() -> Vec<NodeRecord> {
    BOOTNODES[..].iter().map(|s| s.parse().unwrap()).collect()
}
