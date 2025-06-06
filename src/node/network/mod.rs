#![allow(clippy::owned_cow)]
use crate::{
    chainspec::bsc::head,
    consensus::ParliaConsensus,
    node::{
        network::block_import::{handle::ImportHandle, service::ImportService, BscBlockImport},
        primitives::{BscBlobTransactionSidecar, BscPrimitives},
        rpc::engine_api::payload::BscPayloadTypes,
        BscNode,
    },
    BscBlock,
};
use alloy_rlp::{Decodable, Encodable};
use handshake::BscHandshake;
use reth::{
    api::{FullNodeTypes, TxTy},
    builder::{components::NetworkBuilder, BuilderContext},
    transaction_pool::{PoolTransaction, TransactionPool},
};
use reth_discv4::{Discv4Config, NodeRecord};
use reth_engine_primitives::BeaconConsensusEngineHandle;
use reth_eth_wire::{BasicNetworkPrimitives, NewBlock, NewBlockPayload};
use reth_ethereum_primitives::PooledTransactionVariant;
use reth_network::{NetworkConfig, NetworkHandle, NetworkManager};
use reth_network_api::PeersInfo;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::info;

pub mod block_import;
pub mod handshake;
pub(crate) mod upgrade_status;
/// BSC `NewBlock` message value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BscNewBlock(pub NewBlock<BscBlock>);

mod rlp {
    use super::*;
    use crate::BscBlockBody;
    use alloy_consensus::{BlockBody, Header};
    use alloy_primitives::U128;
    use alloy_rlp::{RlpDecodable, RlpEncodable};
    use alloy_rpc_types::Withdrawals;
    use reth_primitives::TransactionSigned;
    use std::borrow::Cow;

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    struct BlockHelper<'a> {
        header: Cow<'a, Header>,
        transactions: Cow<'a, Vec<TransactionSigned>>,
        ommers: Cow<'a, Vec<Header>>,
        withdrawals: Option<Cow<'a, Withdrawals>>,
    }

    #[derive(RlpEncodable, RlpDecodable)]
    #[rlp(trailing)]
    struct BscNewBlockHelper<'a> {
        block: BlockHelper<'a>,
        td: U128,
        sidecars: Option<Cow<'a, Vec<BscBlobTransactionSidecar>>>,
    }

    impl<'a> From<&'a BscNewBlock> for BscNewBlockHelper<'a> {
        fn from(value: &'a BscNewBlock) -> Self {
            let BscNewBlock(NewBlock {
                block:
                    BscBlock {
                        header,
                        body:
                            BscBlockBody {
                                inner: BlockBody { transactions, ommers, withdrawals },
                                sidecars,
                            },
                    },
                td,
            }) = value;

            Self {
                block: BlockHelper {
                    header: Cow::Borrowed(header),
                    transactions: Cow::Borrowed(transactions),
                    ommers: Cow::Borrowed(ommers),
                    withdrawals: withdrawals.as_ref().map(Cow::Borrowed),
                },
                td: *td,
                sidecars: sidecars.as_ref().map(Cow::Borrowed),
            }
        }
    }

    impl Encodable for BscNewBlock {
        fn encode(&self, out: &mut dyn bytes::BufMut) {
            BscNewBlockHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BscNewBlockHelper::from(self).length()
        }
    }

    impl Decodable for BscNewBlock {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            let BscNewBlockHelper {
                block: BlockHelper { header, transactions, ommers, withdrawals },
                td,
                sidecars,
            } = BscNewBlockHelper::decode(buf)?;

            Ok(BscNewBlock(NewBlock {
                block: BscBlock {
                    header: header.into_owned(),
                    body: BscBlockBody {
                        inner: BlockBody {
                            transactions: transactions.into_owned(),
                            ommers: ommers.into_owned(),
                            withdrawals: withdrawals.map(|w| w.into_owned()),
                        },
                        sidecars: sidecars.map(|s| s.into_owned()),
                    },
                },
                td,
            }))
        }
    }
}

impl NewBlockPayload for BscNewBlock {
    type Block = BscBlock;

    fn block(&self) -> &Self::Block {
        &self.0.block
    }
}

/// Network primitives for BSC.
pub type BscNetworkPrimitives =
    BasicNetworkPrimitives<BscPrimitives, PooledTransactionVariant, BscNewBlock>;

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
        Node: FullNodeTypes<Types = BscNode>,
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
    Node: FullNodeTypes<Types = BscNode>,
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
