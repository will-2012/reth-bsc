use super::executor::BscBlockExecutor;
use super::error::BscBlockExecutionError;
use super::util::set_nonce;
use crate::consensus::parlia::{DIFF_INTURN, VoteAddress, Snapshot, snapshot::DEFAULT_TURN_LENGTH};
use crate::evm::transaction::BscTxEnv;
use crate::system_contracts::SLASH_CONTRACT;
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{eth::receipt_builder::{ReceiptBuilder, ReceiptBuilderCtx}, execute::BlockExecutionError, Database, Evm, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv, block::StateChangeSource};
use reth_primitives::{TransactionSigned, Transaction};
use reth_revm::State;
use crate::node::evm::ResultAndState;
use revm::{context::{BlockEnv, TxEnv}, Database as RevmDatabase, DatabaseCommit};
use alloy_consensus::{Header, TxReceipt, Transaction as AlloyTransaction, SignableTransaction};
use alloy_primitives::{Address, hex, TxKind};
use std::collections::HashMap;
use tracing::{debug, warn};

impl<'a, DB, EVM, Spec, R: ReceiptBuilder> BscBlockExecutor<'a, EVM, Spec, R>
where
    DB: Database + 'a,
    EVM: Evm<
        DB = &'a mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction>
                + FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    Spec: EthereumHardforks + crate::hardforks::BscHardforks + EthChainSpec + Hardforks + Clone,
    R: ReceiptBuilder<Transaction = TransactionSigned, Receipt: TxReceipt>,
    <R as ReceiptBuilder>::Transaction: Unpin + From<TransactionSigned>,
    <EVM as alloy_evm::Evm>::Tx: FromTxWithEncoded<<R as ReceiptBuilder>::Transaction>,
    BscTxEnv: IntoTxEnv<<EVM as alloy_evm::Evm>::Tx>,
    R::Transaction: Into<TransactionSigned>,
{
    /// finalize the new block, post check and finalize the system tx.
    /// depends on parlia, header and snapshot.
    pub(crate) fn finalize_new_block(&mut self, block: &BlockEnv) -> Result<(), BlockExecutionError> {
        tracing::info!("Finalize new block, block_number: {}", block.number);

        self.verify_validators(self.inner_ctx.current_validators.clone(), self.inner_ctx.header.clone())?;
        self.verify_turn_length(self.inner_ctx.snap.clone(), self.inner_ctx.header.clone())?;

        // TODO: finalize the system txs.
        if block.difficulty != DIFF_INTURN {
            let snap = self.inner_ctx.snap.as_ref().unwrap();
            let spoiled_validator = snap.inturn_validator();
            let signed_recently = if self.spec.is_plato_active_at_block(block.number.to()) {
                snap.sign_recently(spoiled_validator)
            } else {
                snap.recent_proposers.iter().any(|(_, v)| *v == spoiled_validator)
            };
            if signed_recently {
                self.slash_spoiled_validator(block.beneficiary, spoiled_validator)?;
            }
        }
        Ok(())
    }

    fn verify_validators(&mut self, current_validators: Option<(Vec<Address>, HashMap<Address, VoteAddress>)>, header: Option<Header>) -> Result<(), BlockExecutionError> {
        let header_ref = header.as_ref().unwrap();
        let epoch_length = self.parlia_consensus.as_ref().unwrap().get_epoch_length(header_ref);
        if header_ref.number % epoch_length != 0 {
            tracing::info!("Skip verify validator, block_number {} is not an epoch boundary, epoch_length: {}", header_ref.number, epoch_length);
            return Ok(());
        }

        let (mut validators, mut vote_addrs_map) =
            current_validators.ok_or(BlockExecutionError::msg("Invalid current validators data"))?;
        validators.sort();

        let validator_num = validators.len();
        if self.spec.is_luban_transition_at_block(header_ref.number) {
            vote_addrs_map = validators
                .iter()
                .copied()
                .zip(vec![VoteAddress::default(); validator_num])
                .collect::<HashMap<_, _>>();
        }

        let validator_bytes: Vec<u8> = validators
            .into_iter()
            .flat_map(|v| {
                let mut bytes = v.to_vec();
                if self.spec.is_luban_active_at_block(header_ref.number) {
                    bytes.extend_from_slice(vote_addrs_map[&v].as_ref());
                }
                bytes
            })
            .collect();

        let expected = self.parlia_consensus.as_ref().unwrap().get_validator_bytes_from_header(header_ref).unwrap();
        if !validator_bytes.as_slice().eq(expected.as_slice()) {
            debug!("validator bytes: {:?}", hex::encode(validator_bytes));
            debug!("expected: {:?}", hex::encode(expected));
            return Err(BlockExecutionError::msg("Invalid validators"));
        }
        tracing::info!("Succeed to verify validators, block_number: {}, epoch_length: {}", header_ref.number, epoch_length);

        Ok(())
    }

    fn verify_turn_length(&mut self, _snap: Option<Snapshot>, header: Option<Header>) -> Result<(), BlockExecutionError> {
        let header_ref = header.as_ref().unwrap();
        let epoch_length = {
            let parlia = self.parlia_consensus.as_ref().unwrap();
            parlia.get_epoch_length(header_ref)
        };
        if header_ref.number % epoch_length != 0 || !self.spec.is_bohr_active_at_timestamp(header_ref.timestamp) {
            tracing::info!("Skip verify turn length, block_number {} is not an epoch boundary, epoch_length: {}", header_ref.number, epoch_length);
            return Ok(());
        }
        let turn_length_from_header = {
            let parlia = self.parlia_consensus.as_ref().unwrap();
            match parlia.get_turn_length_from_header(header_ref) {
                Ok(Some(length)) => length,
                Ok(None) => return Ok(()),
                Err(err) => return Err(BscBlockExecutionError::ParliaConsensusInnerError { error: Box::new(err) }.into()),
            }
        };
        let turn_length_from_contract = self.get_turn_length(header_ref)?.unwrap();
        if turn_length_from_header == turn_length_from_contract {
            tracing::info!("Succeed to verify turn length, block_number: {}", header_ref.number);
            return Ok(())
        }

        tracing::info!("Failed to verify turn length, block_number: {}, turn_length_from_header: {}, turn_length_from_contract: {}, epoch_length: {}", 
            header_ref.number, turn_length_from_header, turn_length_from_contract, epoch_length);
        Err(BscBlockExecutionError::MismatchingEpochTurnLengthError.into())
    }

    fn get_turn_length(
        &mut self,
        header: &Header,
    ) -> Result<Option<u8>, BlockExecutionError> {
        if self.spec.is_bohr_active_at_timestamp(header.timestamp) {
            let (to, data) = self.system_contracts.get_turn_length();
            let bz = self.eth_call(to, data)?;

            let turn_length = self.system_contracts.unpack_data_into_turn_length(bz.as_ref()).to::<u8>();
            return Ok(Some(turn_length))
        }

        Ok(Some(DEFAULT_TURN_LENGTH))
    }

    #[allow(dead_code)]
    fn slash_spoiled_validator(
        &mut self,
        validator: Address,
        spoiled_val: Address
    ) -> Result<(), BlockExecutionError> {
        self.transact_system_tx_v2(
            self.system_contracts.slash(spoiled_val),
            validator,
        )?;

        Ok(())
    }

    fn transact_system_tx_v2(&mut self, transaction: Transaction, sender: Address) -> Result<(), BlockExecutionError> {
        let account = self.evm
            .db_mut()
            .basic(sender)
            .map_err(BlockExecutionError::other)?
            .unwrap_or_default();

        let transaction = set_nonce(transaction, account.nonce);
        let hash = transaction.signature_hash();
        if self.system_txs.is_empty() || hash != self.system_txs[0].signature_hash() {
            // slash tx could fail and not in the block
            if let Some(to) = transaction.to() {
                if to == SLASH_CONTRACT &&
                    (self.system_txs.is_empty() ||
                        self.system_txs[0].to().unwrap_or_default() !=
                            SLASH_CONTRACT)
                {
                    warn!("slash validator failed");
                    return Ok(());
                }
            }
            debug!("unexpected transaction: {:?}", transaction);
            for tx in self.system_txs.iter() {
                debug!("left system tx: {:?}", tx);
            }
            return Err(BscBlockExecutionError::UnexpectedSystemTx.into());
        }
        let signed_tx = self.system_txs.remove(0);

        // Create TxEnv first (before moving transaction)
        let tx_env = BscTxEnv {
            base: TxEnv {
                caller: sender,
                kind: TxKind::Call(transaction.to().unwrap()),
                nonce: account.nonce,
                gas_limit: u64::MAX / 2,
                value: transaction.value(),
                data: transaction.input().clone(),
                gas_price: 0,
                chain_id: Some(self.spec.chain().id()),
                gas_priority_fee: None,
                access_list: Default::default(),
                blob_hashes: Vec::new(),
                max_fee_per_blob_gas: 0,
                tx_type: 0,
                authorization_list: Default::default(),
            },
            is_system_transaction: true,
        };

        let result_and_state = self.evm.transact(tx_env).map_err(BlockExecutionError::other)?;
        let ResultAndState { result, state } = result_and_state;
        if let Some(hook) = &mut self.hook {
            hook.on_state(StateChangeSource::Transaction(self.receipts.len()), &state);
        } 

        let gas_used = result.gas_used();
        self.gas_used += gas_used;
        self.receipts.push(self.receipt_builder.build_receipt(ReceiptBuilderCtx {
            tx: &signed_tx,
            evm: &self.evm,
            result,
            state: &state,
            cumulative_gas_used: self.gas_used,
        }));
        self.evm.db_mut().commit(state);

        Ok(())
    }
}