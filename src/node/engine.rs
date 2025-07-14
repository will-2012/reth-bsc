use std::sync::Arc;

use crate::{
    node::{rpc::engine_api::payload::BscPayloadTypes, BscNode},
    BscBlock, BscPrimitives,
};
use alloy_eips::eip7685::Requests;
use alloy_primitives::U256;
use reth::{
    api::FullNodeTypes,
    builder::{components::PayloadServiceBuilder, BuilderContext},
    payload::{PayloadBuilderHandle, PayloadServiceCommand},
    transaction_pool::TransactionPool,
};
use reth_evm::ConfigureEvm;
use reth_payload_primitives::BuiltPayload;
use reth_primitives::SealedBlock;
use tokio::sync::{broadcast, mpsc};
use tracing::warn;

// Additional imports for execution payload conversions
use alloy_rpc_types_engine::{
    BlobsBundleV1, BlobsBundleV2, ExecutionPayloadEnvelopeV2, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadEnvelopeV4, ExecutionPayloadEnvelopeV5, ExecutionPayloadFieldV2,
    ExecutionPayloadV1, ExecutionPayloadV3,
};
// Bring `Block` trait into scope so we can use its extension methods
use reth_primitives_traits::Block as _;
use core::convert::Infallible;

/// Built payload for BSC. This is similar to [`EthBuiltPayload`] but without sidecars as those
/// included into [`BscBlock`].
#[derive(Debug, Clone)]
pub struct BscBuiltPayload {
    /// The built block
    pub(crate) block: Arc<SealedBlock<BscBlock>>,
    /// The fees of the block
    pub(crate) fees: U256,
    /// The requests of the payload
    pub(crate) requests: Option<Requests>,
}

impl BuiltPayload for BscBuiltPayload {
    type Primitives = BscPrimitives;

    fn block(&self) -> &SealedBlock<BscBlock> {
        self.block.as_ref()
    }

    fn fees(&self) -> U256 {
        self.fees
    }

    fn requests(&self) -> Option<Requests> {
        self.requests.clone()
    }
}

// === Conversion impls to satisfy `EngineTypes` bounds ===

// V1 engine_getPayloadV1 response
impl From<BscBuiltPayload> for ExecutionPayloadV1 {
    fn from(value: BscBuiltPayload) -> Self {
        let sealed_block = Arc::unwrap_or_clone(value.block);
        // Convert custom BSC block into the canonical ethereum block representation so that
        // `from_block_unchecked` accepts it.
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Self::from_block_unchecked(sealed_block.hash(), &eth_block)
    }
}

// V2 engine_getPayloadV2 response
impl From<BscBuiltPayload> for ExecutionPayloadEnvelopeV2 {
    fn from(value: BscBuiltPayload) -> Self {
        let BscBuiltPayload { block, fees, .. } = value;

        let sealed_block = Arc::unwrap_or_clone(block);
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Self {
            block_value: fees,
            execution_payload: ExecutionPayloadFieldV2::from_block_unchecked(
                sealed_block.hash(),
                &eth_block,
            ),
        }
    }
}

impl TryFrom<BscBuiltPayload> for ExecutionPayloadEnvelopeV3 {
    type Error = Infallible;

    fn try_from(value: BscBuiltPayload) -> Result<Self, Self::Error> {
        let BscBuiltPayload { block, fees, .. } = value;

        let sealed_block = Arc::unwrap_or_clone(block);
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Ok(ExecutionPayloadEnvelopeV3 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                sealed_block.hash(),
                &eth_block,
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle: BlobsBundleV1::empty(),
        })
    }
}

impl TryFrom<BscBuiltPayload> for ExecutionPayloadEnvelopeV4 {
    type Error = Infallible;

    fn try_from(value: BscBuiltPayload) -> Result<Self, Self::Error> {
        let requests = value.requests.clone().unwrap_or_default();
        let envelope_inner: ExecutionPayloadEnvelopeV3 = value.try_into()?;

        Ok(ExecutionPayloadEnvelopeV4 { execution_requests: requests, envelope_inner })
    }
}

impl TryFrom<BscBuiltPayload> for ExecutionPayloadEnvelopeV5 {
    type Error = Infallible;

    fn try_from(value: BscBuiltPayload) -> Result<Self, Self::Error> {
        let BscBuiltPayload { block, fees, requests, .. } = value;

        let sealed_block = Arc::unwrap_or_clone(block);
        let eth_block = sealed_block.clone().into_block().into_ethereum_block();

        Ok(ExecutionPayloadEnvelopeV5 {
            execution_payload: ExecutionPayloadV3::from_block_unchecked(
                sealed_block.hash(),
                &eth_block,
            ),
            block_value: fees,
            should_override_builder: false,
            blobs_bundle: BlobsBundleV2::empty(),
            execution_requests: requests.unwrap_or_default(),
        })
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct BscPayloadServiceBuilder;

impl<Node, Pool, Evm> PayloadServiceBuilder<Node, Pool, Evm> for BscPayloadServiceBuilder
where
    Node: FullNodeTypes<Types = BscNode>,
    Pool: TransactionPool,
    Evm: ConfigureEvm,
{
    async fn spawn_payload_builder_service(
        self,
        ctx: &BuilderContext<Node>,
        _pool: Pool,
        _evm_config: Evm,
    ) -> eyre::Result<PayloadBuilderHandle<BscPayloadTypes>> {
        let (tx, mut rx) = mpsc::unbounded_channel();

        ctx.task_executor().spawn_critical("payload builder", async move {
            let mut subscriptions = Vec::new();

            while let Some(message) = rx.recv().await {
                match message {
                    PayloadServiceCommand::Subscribe(tx) => {
                        let (events_tx, events_rx) = broadcast::channel(100);
                        // Retain senders to make sure that channels are not getting closed
                        subscriptions.push(events_tx);
                        let _ = tx.send(events_rx);
                    }
                    message => warn!(?message, "Noop payload service received a message"),
                }
            }
        });

        Ok(PayloadBuilderHandle::new(tx))
    }
}

// impl From<EthBuiltPayload> for BscBuiltPayload {
//     fn from(value: EthBuiltPayload) -> Self {
//         let EthBuiltPayload { id, block, fees, sidecars, requests } = value;
//         BscBuiltPayload {
//             id,
//             block: block.into(),
//             fees,
//             requests,
//         }
//     }
// }

// pub struct BscPayloadBuilder<Inner> {
//     inner: Inner,
// }

// impl<Inner> PayloadBuilder for BscPayloadBuilder<Inner>
// where
//     Inner: PayloadBuilder<BuiltPayload = EthBuiltPayload>,
// {
//     type Attributes = Inner::Attributes;
//     type BuiltPayload = BscBuiltPayload;
//     type Error = Inner::Error;

//     fn try_build(
//         &self,
//         args: BuildArguments<Self::Attributes, Self::BuiltPayload>,
//     ) -> Result<BuildOutcome<Self::BuiltPayload>, PayloadBuilderError> {
//         let outcome = self.inner.try_build(args)?;
//     }
// }
