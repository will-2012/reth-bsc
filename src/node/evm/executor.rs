use super::patch::{
    patch_chapel_after_tx, patch_chapel_before_tx, patch_mainnet_after_tx, patch_mainnet_before_tx,
};
use crate::{
    consensus::{MAX_SYSTEM_REWARD, SYSTEM_ADDRESS, SYSTEM_REWARD_PERCENT},
    evm::transaction::BscTxEnv,
    hardforks::BscHardforks,
    system_contracts::{
        get_upgrade_system_contracts, is_system_transaction, SystemContract, STAKE_HUB_CONTRACT,
        SYSTEM_REWARD_CONTRACT,
    },
};
use alloy_consensus::{Transaction, TxReceipt};
use alloy_eips::{eip7685::Requests, Encodable2718};
use alloy_evm::{block::ExecutableTx, eth::receipt_builder::ReceiptBuilderCtx};
use alloy_primitives::{uint, Address, TxKind, U256, BlockNumber, Bytes};
use alloy_sol_macro::sol;
use alloy_sol_types::SolCall;
use reth_chainspec::{EthChainSpec, EthereumHardforks, Hardforks};
use reth_evm::{
    block::{BlockValidationError, CommitChanges},
    eth::{receipt_builder::ReceiptBuilder, EthBlockExecutionCtx},
    execute::{BlockExecutionError, BlockExecutor},
    system_calls::SystemCaller,
    Database, Evm, FromRecoveredTx, FromTxWithEncoded, IntoTxEnv, OnStateHook, RecoveredTx,
};
use reth_primitives::TransactionSigned;
use reth_primitives_traits::SignerRecoverable;
use reth_provider::BlockExecutionResult;
use reth_revm::State;
use revm::{
    context::{
        result::{ExecutionResult, ResultAndState},
        TxEnv,
    },
    state::Bytecode,
    Database as _, DatabaseCommit,
};
use tracing::info;
use alloy_eips::eip2935::{HISTORY_STORAGE_ADDRESS, HISTORY_STORAGE_CODE};
use alloy_primitives::keccak256;

pub struct BscBlockExecutor<'a, EVM, Spec, R: ReceiptBuilder>
where
    Spec: EthChainSpec,
{
    /// Reference to the specification object.
    spec: Spec,
    /// Inner EVM.
    evm: EVM,
    /// Gas used in the block.
    gas_used: u64,
    /// Receipts of executed transactions.
    receipts: Vec<R::Receipt>,
    /// System txs
    system_txs: Vec<R::Transaction>,
    /// Receipt builder.
    receipt_builder: R,
    /// System contracts used to trigger fork specific logic.
    system_contracts: SystemContract<Spec>,
    /// Context for block execution.
    _ctx: EthBlockExecutionCtx<'a>,
    /// Utility to call system caller.
    system_caller: SystemCaller<Spec>,
}

impl<'a, DB, EVM, Spec, R: ReceiptBuilder> BscBlockExecutor<'a, EVM, Spec, R>
where
    DB: Database + 'a,
    EVM: Evm<
        DB = &'a mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction>
                + FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    Spec: EthereumHardforks + BscHardforks + EthChainSpec + Hardforks + Clone,
    R: ReceiptBuilder<Transaction = TransactionSigned, Receipt: TxReceipt>,
    <R as ReceiptBuilder>::Transaction: Unpin + From<TransactionSigned>,
    <EVM as alloy_evm::Evm>::Tx: FromTxWithEncoded<<R as ReceiptBuilder>::Transaction>,
    BscTxEnv: IntoTxEnv<<EVM as alloy_evm::Evm>::Tx>,
    R::Transaction: Into<TransactionSigned>,
{
    /// Creates a new BscBlockExecutor.
    pub fn new(
        evm: EVM,
        _ctx: EthBlockExecutionCtx<'a>,
        spec: Spec,
        receipt_builder: R,
        system_contracts: SystemContract<Spec>,
    ) -> Self {
        let spec_clone = spec.clone();
        Self {
            spec,
            evm,
            gas_used: 0,
            receipts: vec![],
            system_txs: vec![],
            receipt_builder,
            system_contracts,
            _ctx,
            system_caller: SystemCaller::new(spec_clone),
        }
    }

    /// Applies system contract upgrades if the Feynman fork is not yet active.
    fn upgrade_contracts(&mut self) -> Result<(), BlockExecutionError> {
        let contracts = get_upgrade_system_contracts(
            &self.spec,
            self.evm.block().number.to(),
            self.evm.block().timestamp.to(),
            self.evm.block().timestamp.to::<u64>() - 3_000, /* TODO: how to get parent block
                                                             * timestamp? */
        )
        .map_err(|_| BlockExecutionError::msg("Failed to get upgrade system contracts"))?;

        for (address, maybe_code) in contracts {
            if let Some(code) = maybe_code {
                self.upgrade_system_contract(address, code)?;
            }
        }

        Ok(())
    }

    /// Initializes the feynman contracts
    fn initialize_feynman_contracts(
        &mut self,
        beneficiary: Address,
    ) -> Result<(), BlockExecutionError> {
        // Exit early if contracts are already initialized
        if !self
            .evm
            .db_mut()
            .storage(STAKE_HUB_CONTRACT, U256::ZERO)
            .map_err(BlockExecutionError::other)?
            .is_zero()
        {
            return Ok(());
        }

        let txs = self.system_contracts.feynman_contracts_txs();
        for tx in txs {
            self.transact_system_tx(&tx, beneficiary)?;
        }
        Ok(())
    }

    /// Initializes the genesis contracts
    fn deploy_genesis_contracts(
        &mut self,
        beneficiary: Address,
    ) -> Result<(), BlockExecutionError> {
        let txs = self.system_contracts.genesis_contracts_txs();

        for tx in txs {
            self.transact_system_tx(&tx, beneficiary)?;
        }
        Ok(())
    }

    pub(crate) fn transact_system_tx(
        &mut self,
        tx: &TransactionSigned,
        sender: Address,
    ) -> Result<(), BlockExecutionError> {
        // TODO: Consensus handle reverting slashing system txs (they shouldnt be in the block)
        // https://github.com/bnb-chain/reth/blob/main/crates/bsc/evm/src/execute.rs#L602

        let account = self
            .evm
            .db_mut()
            .basic(sender)
            .map_err(BlockExecutionError::other)?
            .unwrap_or_default();

        let tx_env = BscTxEnv {
            base: TxEnv {
                caller: sender,
                kind: TxKind::Call(tx.to().unwrap()),
                nonce: account.nonce,
                gas_limit: u64::MAX / 2,
                value: tx.value(),
                data: tx.input().clone(),
                // Setting the gas price to zero enforces that no value is transferred as part of
                // the call, and that the call will not count against the block's
                // gas limit
                gas_price: 0,
                // The chain ID check is not relevant here and is disabled if set to None
                chain_id: Some(self.spec.chain().id()),
                // Setting the gas priority fee to None ensures the effective gas price is
                //derived         // from the `gas_price` field, which we need to be zero
                gas_priority_fee: None,
                access_list: Default::default(),
                // blob fields can be None for this tx
                blob_hashes: Vec::new(),
                max_fee_per_blob_gas: 0,
                tx_type: 0,
                authorization_list: Default::default(),
            },
            is_system_transaction: true,
        };

        let result_and_state = self.evm.transact(tx_env).map_err(BlockExecutionError::other)?;

        let ResultAndState { result, state } = result_and_state;

        let tx = tx.clone();
        let gas_used = result.gas_used();
        self.gas_used += gas_used;
        self.receipts.push(self.receipt_builder.build_receipt(ReceiptBuilderCtx {
            tx: &tx,
            evm: &self.evm,
            result,
            state: &state,
            cumulative_gas_used: self.gas_used,
        }));
        self.evm.db_mut().commit(state);

        Ok(())
    }

    /// Replaces the code of a system contract in state.
    fn upgrade_system_contract(
        &mut self,
        address: Address,
        code: Bytecode,
    ) -> Result<(), BlockExecutionError> {
        let account =
            self.evm.db_mut().load_cache_account(address).map_err(BlockExecutionError::other)?;

        let mut info = account.account_info().unwrap_or_default();
        info.code_hash = code.hash_slow();
        info.code = Some(code);

        let transition = account.change(info, Default::default());
        self.evm.db_mut().apply_transition(vec![(address, transition)]);
        Ok(())
    }

    /// Handle slash system tx
    fn handle_slash_tx(&mut self, tx: &TransactionSigned) -> Result<(), BlockExecutionError> {
        sol!(
            function slash(
                address amounts,
            );
        );

        let input = tx.input();
        let is_slash_tx = input.len() >= 4 && input[..4] == slashCall::SELECTOR;

        if is_slash_tx {
            let signer = tx.recover_signer().map_err(BlockExecutionError::other)?;
            self.transact_system_tx(tx, signer)?;
        }

        Ok(())
    }

    /// Handle finality reward system tx.
    /// Activated by <https://github.com/bnb-chain/BEPs/blob/master/BEPs/BEP-319.md>
    /// at <https://www.bnbchain.org/en/blog/announcing-v1-2-9-a-significant-hard-fork-release-for-bsc-mainnet>
    fn handle_finality_reward_tx(
        &mut self,
        tx: &TransactionSigned,
    ) -> Result<(), BlockExecutionError> {
        sol!(
            function distributeFinalityReward(
                address[] validators,
                uint256[] weights
            );
        );

        let input = tx.input();
        let is_finality_reward_tx =
            input.len() >= 4 && input[..4] == distributeFinalityRewardCall::SELECTOR;

        if is_finality_reward_tx {
            let signer = tx.recover_signer().map_err(BlockExecutionError::other)?;
            self.transact_system_tx(tx, signer)?;
        }

        Ok(())
    }

    /// Handle update validatorsetv2 system tx.
    /// Activated by <https://github.com/bnb-chain/BEPs/pull/294>
    fn handle_update_validator_set_v2_tx(
        &mut self,
        tx: &TransactionSigned,
    ) -> Result<(), BlockExecutionError> {
        sol!(
            function updateValidatorSetV2(
                address[] _consensusAddrs,
                uint64[] _votingPowers,
                bytes[] _voteAddrs
            );
        );

        let input = tx.input();
        let is_update_validator_set_v2_tx =
            input.len() >= 4 && input[..4] == updateValidatorSetV2Call::SELECTOR;

        if is_update_validator_set_v2_tx {
            let signer = tx.recover_signer().map_err(BlockExecutionError::other)?;
            self.transact_system_tx(tx, signer)?;
        }

        Ok(())
    }

    /// Distributes block rewards to the validator.
    fn distribute_block_rewards(&mut self, validator: Address) -> Result<(), BlockExecutionError> {
        let system_account = self
            .evm
            .db_mut()
            .load_cache_account(SYSTEM_ADDRESS)
            .map_err(BlockExecutionError::other)?;

        if system_account.account.is_none() ||
            system_account.account.as_ref().unwrap().info.balance == U256::ZERO
        {
            return Ok(());
        }

        let (mut block_reward, mut transition) = system_account.drain_balance();
        transition.info = None;
        self.evm.db_mut().apply_transition(vec![(SYSTEM_ADDRESS, transition)]);
        let balance_increment = vec![(validator, block_reward)];

        self.evm
            .db_mut()
            .increment_balances(balance_increment)
            .map_err(BlockExecutionError::other)?;

        let system_reward_balance = self
            .evm
            .db_mut()
            .basic(SYSTEM_REWARD_CONTRACT)
            .map_err(BlockExecutionError::other)?
            .unwrap_or_default()
            .balance;

        // Kepler introduced a max system reward limit, so we need to pay the system reward to the
        // system contract if the limit is not exceeded.
        if !self.spec.is_kepler_active_at_timestamp(self.evm.block().timestamp.to()) &&
            system_reward_balance < U256::from(MAX_SYSTEM_REWARD)
        {
            let reward_to_system = block_reward >> SYSTEM_REWARD_PERCENT;
            if reward_to_system > 0 {
                let tx = self.system_contracts.pay_system_tx(reward_to_system);
                self.transact_system_tx(&tx, validator)?;
            }

            block_reward -= reward_to_system;
        }

        let tx = self.system_contracts.pay_validator_tx(validator, block_reward);
        self.transact_system_tx(&tx, validator)?;
        Ok(())
    }

    pub(crate) fn apply_history_storage_account(
        &mut self,
        block_number: BlockNumber,
    ) -> Result<bool, BlockExecutionError> {
        info!(
            target: "bsc::executor",
            "=== HISTORY STORAGE ACCOUNT INITIALIZATION START ==="
        );
        info!(
            target: "bsc::executor",
            "Initializing history storage account {:?} at height {:?}",
            HISTORY_STORAGE_ADDRESS, block_number
        );

        // Get current account state before modification
        let account = self.evm.db_mut().load_cache_account(HISTORY_STORAGE_ADDRESS).map_err(|err| {
            BlockExecutionError::other(err)
        })?;
        
        let old_info = account.account_info();
        let old_info_clone = old_info.clone();
        info!(
            target: "bsc::executor",
            "Current account state - exists: {}, nonce: {}, balance: {}, has_code: {}",
            old_info.is_some(),
            old_info.as_ref().map(|info| info.nonce).unwrap_or(0),
            old_info.as_ref().map(|info| info.balance).unwrap_or_default(),
            old_info.as_ref().map(|info| info.code.is_some()).unwrap_or(false)
        );

        let mut new_info = old_info.unwrap_or_default();
        let old_code_hash = new_info.code_hash;
        let old_nonce = new_info.nonce;
        let old_balance = new_info.balance;
        
        new_info.code_hash = keccak256(HISTORY_STORAGE_CODE.clone());
        new_info.code = Some(Bytecode::new_raw(Bytes::from_static(&HISTORY_STORAGE_CODE)));
        new_info.nonce = 1_u64;
        new_info.balance = U256::ZERO;

        info!(
            target: "bsc::executor",
            "Account state changes:"
        );
        info!(
            target: "bsc::executor",
            "  code_hash: {:?} -> {:?}",
            old_code_hash, new_info.code_hash
        );
        info!(
            target: "bsc::executor",
            "  nonce: {} -> {}",
            old_nonce, new_info.nonce
        );
        info!(
            target: "bsc::executor",
            "  balance: {} -> {}",
            old_balance, new_info.balance
        );
        info!(
            target: "bsc::executor",
            "  code: {} -> {}",
            old_info_clone.as_ref().map(|info| info.code.is_some()).unwrap_or(false),
            new_info.code.is_some()
        );

        let transition = account.change(new_info, Default::default());
        self.evm.db_mut().apply_transition(vec![(HISTORY_STORAGE_ADDRESS, transition)]);
        
        info!(
            target: "bsc::executor",
            "History storage account transition applied successfully"
        );
        info!(
            target: "bsc::executor",
            "=== HISTORY STORAGE ACCOUNT INITIALIZATION COMPLETED ==="
        );
        
        Ok(true)
    }
}

impl<'a, DB, E, Spec, R> BlockExecutor for BscBlockExecutor<'a, E, Spec, R>
where
    DB: Database + 'a,
    E: Evm<
        DB = &'a mut State<DB>,
        Tx: FromRecoveredTx<R::Transaction>
                + FromRecoveredTx<TransactionSigned>
                + FromTxWithEncoded<TransactionSigned>,
    >,
    Spec: EthereumHardforks + BscHardforks + EthChainSpec + Hardforks,
    R: ReceiptBuilder<Transaction = TransactionSigned, Receipt: TxReceipt>,
    <R as ReceiptBuilder>::Transaction: Unpin + From<TransactionSigned>,
    <E as alloy_evm::Evm>::Tx: FromTxWithEncoded<<R as ReceiptBuilder>::Transaction>,
    BscTxEnv: IntoTxEnv<<E as alloy_evm::Evm>::Tx>,
    R::Transaction: Into<TransactionSigned>,
{
    type Transaction = TransactionSigned;
    type Receipt = R::Receipt;
    type Evm = E;

    fn apply_pre_execution_changes(&mut self) -> Result<(), BlockExecutionError> {
        let block_number = self.evm.block().number.to::<u64>();
        let timestamp = self.evm.block().timestamp.to::<u64>();
        let parent_hash = self._ctx.parent_hash;
        
        info!(
            target: "bsc::executor",
            "=== EMPTY BLOCK PRE-EXECUTION START ==="
        );
        info!(
            target: "bsc::executor",
            "Block #{} (timestamp: {}, parent_hash: {:?})",
            block_number, timestamp, parent_hash
        );
        
        // Set state clear flag if the block is after the Spurious Dragon hardfork.
        let state_clear_flag =
            self.spec.is_spurious_dragon_active_at_block(self.evm.block().number.to());
        self.evm.db_mut().set_state_clear_flag(state_clear_flag);
        
        info!(
            target: "bsc::executor",
            "State clear flag set to: {} (Spurious Dragon active: {})",
            state_clear_flag,
            self.spec.is_spurious_dragon_active_at_block(self.evm.block().number.to())
        );

        // TODO: (Consensus Verify cascading fields)[https://github.com/bnb-chain/reth/blob/main/crates/bsc/evm/src/pre_execution.rs#L43]
        // TODO: (Consensus System Call Before Execution)[https://github.com/bnb-chain/reth/blob/main/crates/bsc/evm/src/execute.rs#L678]

        let feynman_active = self.spec.is_feynman_active_at_timestamp(timestamp);
        info!(
            target: "bsc::executor",
            "Feynman hardfork active: {} (timestamp: {})",
            feynman_active, timestamp
        );
        
        if !feynman_active {
            info!(
                target: "bsc::executor",
                "Upgrading contracts (Feynman not active)"
            );
            self.upgrade_contracts()?;
        }

        // enable BEP-440/EIP-2935 for historical block hashes from state
        let parent_timestamp = timestamp.saturating_sub(3);
        let prague_transition = self.spec.is_prague_transition_at_timestamp(timestamp, parent_timestamp);
        let prague_active = self.spec.is_prague_active_at_timestamp(timestamp);
        
        info!(
            target: "bsc::executor",
            "Prague hardfork - Transition: {} (current: {}, parent: {}), Active: {}",
            prague_transition, timestamp, parent_timestamp, prague_active
        );
        
        if prague_transition {
            info!(
                target: "bsc::executor",
                "=== APPLYING HISTORY STORAGE ACCOUNT (Prague transition) ==="
            );
            info!(
                target: "bsc::executor",
                "Initializing history storage account at block #{}",
                block_number
            );
            self.apply_history_storage_account(block_number)?;
            info!(
                target: "bsc::executor",
                "History storage account initialization completed"
            );
        }
        
        if prague_active {
            info!(
                target: "bsc::executor",
                "=== APPLYING BLOCKHASHES CONTRACT CALL (Prague active) ==="
            );
            info!(
                target: "bsc::executor",
                "Calling blockhashes contract with parent_hash: {:?}",
                parent_hash
            );
            self.system_caller.apply_blockhashes_contract_call(parent_hash, &mut self.evm)?;
            info!(
                target: "bsc::executor",
                "Blockhashes contract call completed"
            );
        }
        
        info!(
            target: "bsc::executor",
            "=== EMPTY BLOCK PRE-EXECUTION COMPLETED ==="
        );

        Ok(())
    }

    fn execute_transaction_with_commit_condition(
        &mut self,
        _tx: impl ExecutableTx<Self>,
        _f: impl FnOnce(&ExecutionResult<<Self::Evm as Evm>::HaltReason>) -> CommitChanges,
    ) -> Result<Option<u64>, BlockExecutionError> {
        Ok(Some(0))
    }

    fn execute_transaction_with_result_closure(
        &mut self,
        tx: impl ExecutableTx<Self>
            + IntoTxEnv<<E as alloy_evm::Evm>::Tx>
            + RecoveredTx<TransactionSigned>,
        f: impl for<'b> FnOnce(&'b ExecutionResult<<E as alloy_evm::Evm>::HaltReason>),
    ) -> Result<u64, BlockExecutionError> {
        // Check if it's a system transaction
        let signer = tx.signer();
        if is_system_transaction(tx.tx(), *signer, self.evm.block().beneficiary) {
            self.system_txs.push(tx.tx().clone());
            return Ok(0);
        }

        // apply patches before
        patch_mainnet_before_tx(tx.tx(), self.evm.db_mut())?;
        patch_chapel_before_tx(tx.tx(), self.evm.db_mut())?;

        let block_available_gas = self.evm.block().gas_limit - self.gas_used;
        if tx.tx().gas_limit() > block_available_gas {
            return Err(BlockValidationError::TransactionGasLimitMoreThanAvailableBlockGas {
                transaction_gas_limit: tx.tx().gas_limit(),
                block_available_gas,
            }
            .into());
        }
        let result_and_state = self
            .evm
            .transact(tx)
            .map_err(|err| BlockExecutionError::evm(err, tx.tx().trie_hash()))?;
        let ResultAndState { result, state } = result_and_state;

        f(&result);

        let gas_used = result.gas_used();
        self.gas_used += gas_used;
        self.receipts.push(self.receipt_builder.build_receipt(ReceiptBuilderCtx {
            tx: tx.tx(),
            evm: &self.evm,
            result,
            state: &state,
            cumulative_gas_used: self.gas_used,
        }));
        self.evm.db_mut().commit(state);

        // apply patches after
        patch_mainnet_after_tx(tx.tx(), self.evm.db_mut())?;
        patch_chapel_after_tx(tx.tx(), self.evm.db_mut())?;

        Ok(gas_used)
    }

    fn finish(
        mut self,
    ) -> Result<(Self::Evm, BlockExecutionResult<R::Receipt>), BlockExecutionError> {
        let block_number = self.evm.block().number.to::<u64>();
        let timestamp = self.evm.block().timestamp.to::<u64>();
        let beneficiary = self.evm.block().beneficiary;
        
        info!(
            target: "bsc::executor",
            "=== EMPTY BLOCK FINISH START ==="
        );
        info!(
            target: "bsc::executor",
            "Finishing block #{} (timestamp: {}, beneficiary: {:?}, gas_used: {})",
            block_number, timestamp, beneficiary, self.gas_used
        );
        info!(
            target: "bsc::executor",
            "System transactions count: {}, Regular transactions count: {}",
            self.system_txs.len(), self.receipts.len()
        );
        
        // TODO:
        // Consensus: Verify validators
        // Consensus: Verify turn length

        // If first block deploy genesis contracts
        if self.evm.block().number == uint!(1U256) {
            info!(
                target: "bsc::executor",
                "=== DEPLOYING GENESIS CONTRACTS ==="
            );
            self.deploy_genesis_contracts(self.evm.block().beneficiary)?;
            info!(
                target: "bsc::executor",
                "Genesis contracts deployment completed"
            );
        }

        let feynman_active = self.spec.is_feynman_active_at_timestamp(timestamp);
        info!(
            target: "bsc::executor",
            "Feynman hardfork active: {} (timestamp: {})",
            feynman_active, timestamp
        );
        
        if feynman_active {
            info!(
                target: "bsc::executor",
                "=== UPGRADING CONTRACTS (Feynman active) ==="
            );
            self.upgrade_contracts()?;
            info!(
                target: "bsc::executor",
                "Contract upgrade completed"
            );
        }

        let feynman_transition = feynman_active && 
            !self.spec.is_feynman_active_at_timestamp(timestamp.saturating_sub(100));
        info!(
            target: "bsc::executor",
            "Feynman transition check: {} (current: {}, 100 blocks ago: {})",
            feynman_transition, 
            feynman_active,
            self.spec.is_feynman_active_at_timestamp(timestamp.saturating_sub(100))
        );
        
        if feynman_transition {
            info!(
                target: "bsc::executor",
                "=== INITIALIZING FEYNMAN CONTRACTS ==="
            );
            self.initialize_feynman_contracts(self.evm.block().beneficiary)?;
            info!(
                target: "bsc::executor",
                "Feynman contracts initialization completed"
            );
        }

        let system_txs = self.system_txs.clone();
        info!(
            target: "bsc::executor",
            "Processing {} system transactions for slash handling",
            system_txs.len()
        );
        for (i, tx) in system_txs.iter().enumerate() {
            info!(
                target: "bsc::executor",
                "Processing system transaction #{}: {:?}",
                i, tx.hash()
            );
            self.handle_slash_tx(tx)?;
        }

        info!(
            target: "bsc::executor",
            "=== DISTRIBUTING BLOCK REWARDS ==="
        );
        info!(
            target: "bsc::executor",
            "Distributing rewards to beneficiary: {:?}",
            beneficiary
        );
        self.distribute_block_rewards(self.evm.block().beneficiary)?;
        info!(
            target: "bsc::executor",
            "Block rewards distribution completed"
        );

        let plato_active = self.spec.is_plato_active_at_block(block_number);
        info!(
            target: "bsc::executor",
            "Plato hardfork active: {} (block: {})",
            plato_active, block_number
        );
        
        if plato_active {
            info!(
                target: "bsc::executor",
                "=== PROCESSING FINALITY REWARDS (Plato active) ==="
            );
            for (i, tx) in system_txs.iter().enumerate() {
                info!(
                    target: "bsc::executor",
                    "Processing finality reward for system transaction #{}: {:?}",
                    i, tx.hash()
                );
                self.handle_finality_reward_tx(tx)?;
            }
            info!(
                target: "bsc::executor",
                "Finality rewards processing completed"
            );
        }

        // TODO: add breathe check and polish it later.
        let system_txs_v2 = self.system_txs.clone();
        info!(
            target: "bsc::executor",
            "Processing {} system transactions for validator set updates",
            system_txs_v2.len()
        );
        for (i, tx) in system_txs_v2.iter().enumerate() {
            info!(
                target: "bsc::executor",
                "Processing validator set update for system transaction #{}: {:?}",
                i, tx.hash()
            );
            self.handle_update_validator_set_v2_tx(tx)?;
        }

        // TODO:
        // Consensus: Slash validator if not in turn

        info!(
            target: "bsc::executor",
            "=== EMPTY BLOCK FINISH COMPLETED ==="
        );
        info!(
            target: "bsc::executor",
            "Final execution result - gas_used: {}, receipts: {}, requests: {}",
            self.gas_used, self.receipts.len(), 0
        );

        Ok((
            self.evm,
            BlockExecutionResult {
                receipts: self.receipts,
                requests: Requests::default(),
                gas_used: self.gas_used,
            },
        ))
    }

    fn set_state_hook(&mut self, _hook: Option<Box<dyn OnStateHook>>) {}

    fn evm_mut(&mut self) -> &mut Self::Evm {
        &mut self.evm
    }

    fn evm(&self) -> &Self::Evm {
        &self.evm
    }
}
