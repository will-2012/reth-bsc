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
use tracing::{debug, info, warn, error};

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
        info!("🤝 BSC handshake: Starting upgrade_status negotiation, eth_version: {:?}", negotiated_status.version);
        debug!("BSC handshake: Negotiated status details: {:?}", negotiated_status);
        
        if negotiated_status.version > EthVersion::Eth66 {
            // Send upgrade status message allowing peer to broadcast transactions
            let upgrade_msg = UpgradeStatus {
                extension: UpgradeStatusExtension { disable_peer_tx_broadcast: false },
            };
            info!("📤 BSC handshake: Sending UpgradeStatus message");
            unauth.start_send_unpin(upgrade_msg.into_rlpx())?;

            // Receive peer's upgrade status response
            info!("📥 BSC handshake: Waiting for peer's UpgradeStatus response...");
            let their_msg = match unauth.next().await {
                Some(Ok(msg)) => {
                    info!("📨 BSC handshake: Received message from peer, length: {}", msg.len());
                    debug!("BSC handshake: Message content: {:02x?}", &msg[..msg.len().min(32)]);
                    msg
                },
                Some(Err(e)) => {
                    error!("❌ BSC handshake: Error receiving peer response: {:?}", e);
                    return Err(EthStreamError::from(e));
                },
                None => {
                    error!("❌ BSC handshake: No response from peer");
                    unauth.disconnect(DisconnectReason::DisconnectRequested).await?;
                    return Err(EthStreamError::EthHandshakeError(EthHandshakeError::NoResponse));
                }
            };

            // Decode their response
            info!("🔍 BSC handshake: Attempting to decode peer's UpgradeStatus response");
            match UpgradeStatus::decode(&mut their_msg.as_ref()).map_err(|e| {
                error!("❌ BSC handshake: Decode error: {:?}, msg={:02x?}", e, &their_msg[..their_msg.len().min(32)]);
                EthStreamError::InvalidMessage(e.into())
            }) {
                Ok(upgrade_status) => {
                    info!("✅ BSC handshake: Successfully decoded UpgradeStatus: {:?}", upgrade_status);
                    return Ok(negotiated_status);
                }
                Err(decode_error) => {
                    error!("❌ BSC handshake: Failed to decode UpgradeStatus, disconnecting with ProtocolBreach");
                    unauth.disconnect(DisconnectReason::ProtocolBreach).await?;
                    return Err(EthStreamError::EthHandshakeError(
                        EthHandshakeError::NonStatusMessageInHandshake,
                    ));
                }
            }
        } else {
            info!("ℹ️  BSC handshake: Eth version <= Eth66, skipping UpgradeStatus exchange");
        }

        info!("✅ BSC handshake: upgrade_status negotiation completed successfully");
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
                info!("🤝 BSC handshake: Starting full BSC handshake process");
                info!("🌐 BSC handshake: Initiating standard Ethereum handshake first");
                debug!("BSC handshake: Initial status = {:?}, fork_filter = {:?}", status, fork_filter);
                
                let negotiated_status =
                    EthereumEthHandshake(unauth).eth_handshake(status, fork_filter).await?;
                    
                info!("✅ BSC handshake: Standard Ethereum handshake completed, negotiated_status: {:?}", negotiated_status);
                info!("🔄 BSC handshake: Starting BSC-specific upgrade_status phase");
                
                Self::upgrade_status(unauth, negotiated_status).await
            };
            
            debug!("⏱️  BSC handshake: Setting timeout limit: {:?}", timeout_limit);
            match timeout(timeout_limit, fut).await {
                Ok(result) => {
                    info!("✅ BSC handshake: Full handshake completed successfully");
                    result
                },
                Err(_) => {
                    error!("⏰ BSC handshake: Handshake timed out after {:?}", timeout_limit);
                    Err(EthStreamError::StreamTimeout)
                }
            }
        })
    }
}
