#![allow(clippy::owned_cow)]
use crate::{
    consensus::ParliaConsensus,
    node::{
        engine_api::payload::BscPayloadTypes,
        network::block_import::{handle::ImportHandle, service::ImportService, BscBlockImport},
        primitives::{BscBlobTransactionSidecar, BscPrimitives},
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
use reth_chainspec::EthChainSpec;
use reth_discv4::Discv4Config;
use reth_engine_primitives::BeaconConsensusEngineHandle;
use reth_eth_wire::{BasicNetworkPrimitives, NewBlock, NewBlockPayload};
use reth_ethereum_primitives::PooledTransactionVariant;
use reth_network::{NetworkConfig, NetworkHandle, NetworkManager};
use reth_network_api::PeersInfo;
use std::{sync::Arc, time::Duration};
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, error, info};

pub mod block_import;
pub mod bootnodes;
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
            debug!(
                target: "bsc::network::rlp_encode",
                block_number=?self.0.block.header.number,
                block_hash=?self.0.block.header.hash_slow(),
                "Encoding BscNewBlock to RLP"
            );
            BscNewBlockHelper::from(self).encode(out);
        }

        fn length(&self) -> usize {
            BscNewBlockHelper::from(self).length()
        }
    }

    impl Decodable for BscNewBlock {
        fn decode(buf: &mut &[u8]) -> alloy_rlp::Result<Self> {
            debug!(
                target: "bsc::network::rlp_decode",
                buffer_length=?buf.len(),
                "Decoding BscNewBlock from RLP"
            );
            
            let result = BscNewBlockHelper::decode(buf);
            match &result {
                Ok(helper) => {
                    debug!(
                        target: "bsc::network::rlp_decode",
                        block_number=?helper.block.header.number,
                        block_hash=?helper.block.header.hash_slow(),
                        td=?helper.td,
                        has_sidecars=?helper.sidecars.is_some(),
                        "Successfully decoded BscNewBlock"
                    );
                }
                Err(e) => {
                    error!(
                        target: "bsc::network::rlp_decode",
                        error=?e,
                        buffer_length=?buf.len(),
                        "Failed to decode BscNewBlock from RLP"
                    );
                }
            }
            result.map(|helper| {
                let BscNewBlockHelper { block, td, sidecars } = helper;
                BscNewBlock(NewBlock {
                    block: BscBlock {
                        header: block.header.into_owned(),
                        body: BscBlockBody {
                            inner: BlockBody {
                                transactions: block.transactions.into_owned(),
                                ommers: block.ommers.into_owned(),
                                withdrawals: block.withdrawals.map(|w| w.into_owned()),
                            },
                            sidecars: sidecars.map(|s| s.into_owned()),
                        },
                    },
                    td,
                })
            })
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

        if let Some(boot_nodes) = ctx.chain_spec().bootnodes() {
            discv4.add_boot_nodes(boot_nodes);
        }
        discv4.lookup_interval(Duration::from_millis(500));

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
            .boot_nodes(ctx.chain_spec().bootnodes().unwrap_or_default())
            .set_head(ctx.chain_spec().head())
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
