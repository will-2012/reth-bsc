use crate::evm::{
    api::{BscContext, BscEvm},
    spec::BscSpecId,
    transaction::BscTxEnv,
};
use reth_evm::{precompiles::PrecompilesMap, Database, EvmEnv, EvmFactory};
use revm::{
    context::result::{EVMError, HaltReason},
    inspector::NoOpInspector,
    Inspector,
};

/// Factory producing [`BscEvm`].
#[derive(Debug, Default, Clone, Copy)]
#[non_exhaustive]
pub struct BscEvmFactory;

impl EvmFactory for BscEvmFactory {
    type Evm<DB: Database, I: Inspector<BscContext<DB>>> = BscEvm<DB, I>;
    type Context<DB: Database> = BscContext<DB>;
    type Tx = BscTxEnv;
    type Error<DBError: core::error::Error + Send + Sync + 'static> = EVMError<DBError>;
    type HaltReason = HaltReason;
    type Spec = BscSpecId;
    type Precompiles = PrecompilesMap;

    fn create_evm<DB: Database>(
        &self,
        db: DB,
        input: EvmEnv<BscSpecId>,
    ) -> Self::Evm<DB, NoOpInspector> {
        BscEvm::new(input, db, NoOpInspector {}, false)
    }

    fn create_evm_with_inspector<DB: Database, I: Inspector<Self::Context<DB>>>(
        &self,
        db: DB,
        input: EvmEnv<BscSpecId>,
        inspector: I,
    ) -> Self::Evm<DB, I> {
        BscEvm::new(input, db, inspector, true)
    }
}
