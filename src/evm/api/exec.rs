use crate::evm::{
    api::{BscContext, BscEvm},
    handler::BscHandler,
    transaction::BscTxEnv,
};

use reth_evm::Database;
use revm::{
    context::{BlockEnv, ContextSetters},
    context_interface::{
        result::{EVMError, ExecutionResult, ResultAndState},
        ContextTr,
    },
    handler::Handler,
    inspector::{InspectCommitEvm, InspectEvm, Inspector, InspectorHandler},
    state::EvmState,
    DatabaseCommit, ExecuteCommitEvm, ExecuteEvm,
};

impl<DB, INSP> ExecuteEvm for BscEvm<DB, INSP>
where
    DB: Database,
{
    type ExecutionResult = ExecutionResult;
    type State = EvmState;
    type Error = EVMError<DB::Error>;
    type Tx = BscTxEnv;
    type Block = BlockEnv;

    fn set_block(&mut self, block: Self::Block) {
        self.inner.set_block(block);
    }

    fn transact_one(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        self.inner.ctx.set_tx(tx);
        BscHandler::new().run(self)
    }

    fn finalize(&mut self) -> Self::State {
        self.inner.finalize()
    }

    fn replay(&mut self) -> Result<ResultAndState, Self::Error> {
        BscHandler::new().run(self).map(|result| {
            let state = self.finalize();
            ResultAndState::new(result, state)
        })
    }
}

impl<DB, INSP> ExecuteCommitEvm for BscEvm<DB, INSP>
where
    DB: Database + DatabaseCommit,
{
    fn commit(&mut self, state: Self::State) {
        self.inner.ctx.db_mut().commit(state);
    }
}

impl<DB, INSP> InspectEvm for BscEvm<DB, INSP>
where
    DB: Database,
    INSP: Inspector<BscContext<DB>>,
{
    type Inspector = INSP;

    fn set_inspector(&mut self, inspector: Self::Inspector) {
        self.inner.set_inspector(inspector);
    }

    fn inspect_one_tx(&mut self, tx: Self::Tx) -> Result<Self::ExecutionResult, Self::Error> {
        self.inner.ctx.set_tx(tx);
        BscHandler::new().inspect_run(self)
    }
}

impl<DB, INSP> InspectCommitEvm for BscEvm<DB, INSP>
where
    DB: Database + DatabaseCommit,
    INSP: Inspector<BscContext<DB>>,
{
}

// impl<DB, INSP> SystemCallEvm for BscEvm<DB, INSP>
// where
//     DB: Database,
// {
//     fn transact_system_call(
//         &mut self,
//         _contract: Address,
//         _data: Bytes,
//     ) -> Result<ExecutionResult, Self::Error> {
//         unimplemented!()
//     }

//     fn transact_system_call_with_caller(
//         &mut self,
//         caller: Address,
//         contract: Address,
//         data: Bytes,
//     ) -> Result<ExecutionResult, Self::Error> {
//         let tx = BscTxEnv {
//             base: revm::context::TxEnv {
//                 caller,
//                 kind: alloy_primitives::TxKind::Call(contract),
//                 nonce: 0,
//                 gas_limit: self.inner.ctx.block.gas_limit,
//                 value: alloy_primitives::U256::ZERO,
//                 data,
//                 gas_price: 0,
//                 chain_id: Some(self.inner.ctx.cfg.chain_id),
//                 gas_priority_fee: None,
//                 access_list: Default::default(),
//                 blob_hashes: Vec::new(),
//                 max_fee_per_blob_gas: 0,
//                 tx_type: 0,
//                 authorization_list: Default::default(),
//             },
//            is_system_transaction: true,
//         };
        
//         // disable nonce check for system calls
//         let original_disable_nonce_check = self.inner.ctx.cfg.disable_nonce_check;
//         self.inner.ctx.cfg.disable_nonce_check = true;
//         let result = self.transact_one(tx);
//         self.inner.ctx.cfg.disable_nonce_check = original_disable_nonce_check;
        
//         result
//     }
// }
