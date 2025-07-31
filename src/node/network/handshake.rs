use super::upgrade_status::{UpgradeStatus, UpgradeStatusExtension};
use alloy_rlp::Decodable;
use futures::SinkExt;
use reth_eth_wire::{
    errors::{EthHandshakeError, EthStreamError},
    handshake::{EthRlpxHandshake, EthereumEthHandshake, UnauthEth},
    UnifiedStatus,
};
use reth_eth_wire_types::{DisconnectReason, EthVersion};
use reth_ethereum_forks::ForkFilter;
use std::{future::Future, pin::Pin};
use tokio::time::{timeout, Duration};
use tokio_stream::StreamExt;
use tracing::{debug, trace};

#[derive(Debug, Default)]
/// The Binance Smart Chain (BSC) P2P handshake.
#[non_exhaustive]
pub struct BscHandshake;

impl BscHandshake {
    /// Negotiate the upgrade status message.
    pub async fn upgrade_status(
        unauth: &mut dyn UnauthEth,
        negotiated_status: UnifiedStatus,
    ) -> Result<UnifiedStatus, EthStreamError> {
        debug!(
            target: "net::session::bad_message_debug",
            "BSC handshake: starting upgrade status negotiation, version={:?}",
            negotiated_status.version
        );
        
        if negotiated_status.version > EthVersion::Eth66 {
            // Send upgrade status message allowing peer to broadcast transactions
            let upgrade_msg = UpgradeStatus {
                extension: UpgradeStatusExtension { disable_peer_tx_broadcast: false },
            };
            
            debug!(
                target: "net::session::bad_message_debug",
                "BSC handshake: sending upgrade status message"
            );
            
            unauth.start_send_unpin(upgrade_msg.into_rlpx())?;

            // Receive peer's upgrade status response
            debug!(
                target: "net::session::bad_message_debug",
                "BSC handshake: waiting for peer's upgrade status response"
            );
            
            let their_msg = match unauth.next().await {
                Some(Ok(msg)) => {
                    debug!(
                        target: "net::session::bad_message_debug",
                        "BSC handshake: received peer response, msg_len={}",
                        msg.len()
                    );
                    msg
                },
                Some(Err(e)) => {
                    debug!(
                        target: "net::session::bad_message_debug",
                        "BSC handshake: error receiving peer response: {:?}",
                        e
                    );
                    return Err(EthStreamError::from(e))
                },
                None => {
                    debug!(
                        target: "net::session::bad_message_debug",
                        "BSC handshake: no response from peer, disconnecting"
                    );
                    unauth.disconnect(DisconnectReason::DisconnectRequested).await?;
                    return Err(EthStreamError::EthHandshakeError(EthHandshakeError::NoResponse));
                }
            };

            // Decode their response
            debug!(
                target: "net::session::bad_message_debug",
                "BSC handshake: decoding peer's upgrade status response"
            );
            
            match UpgradeStatus::decode(&mut their_msg.as_ref()).map_err(|e| {
                debug!(
                    target: "net::session::bad_message_debug",
                    "BSC handshake: decode error in upgrade status response: msg={:x}, error={:?}",
                    their_msg, e
                );
                EthStreamError::InvalidMessage(e.into())
            }) {
                Ok(_) => {
                    debug!(
                        target: "net::session::bad_message_debug",
                        "BSC handshake: successful upgrade status negotiation"
                    );
                    // Successful handshake
                    return Ok(negotiated_status);
                }
                Err(e) => {
                    debug!(
                        target: "net::session::bad_message_debug",
                        "BSC handshake: protocol breach, disconnecting: {:?}",
                        e
                    );
                    unauth.disconnect(DisconnectReason::ProtocolBreach).await?;
                    return Err(e);
                }
            }
        }

        debug!(
            target: "net::session::bad_message_debug",
            "BSC handshake: no upgrade status needed for version {:?}",
            negotiated_status.version
        );
        Ok(negotiated_status)
    }
}

impl EthRlpxHandshake for BscHandshake {
    fn handshake<'a>(
        &'a self,
        unauth: &'a mut dyn UnauthEth,
        status: UnifiedStatus,
        fork_filter: ForkFilter,
        timeout_limit: Duration,
    ) -> Pin<Box<dyn Future<Output = Result<UnifiedStatus, EthStreamError>> + 'a + Send>> {
        Box::pin(async move {
            let fut = async {
                let negotiated_status =
                    EthereumEthHandshake(unauth).eth_handshake(status, fork_filter).await?;
                Self::upgrade_status(unauth, negotiated_status).await
            };
            timeout(timeout_limit, fut).await.map_err(|_| EthStreamError::StreamTimeout)?
        })
    }
}
