pub mod chainspec;
pub mod cli;
pub mod consensus;
mod evm;
mod hardforks;
pub mod node;
pub use node::primitives::BscPrimitives;
mod system_contracts;
pub use system_contracts::SLASH_CONTRACT;
#[path = "system_contracts/tx_maker_ext.rs"]
mod system_tx_ext;
pub use system_tx_ext::*;
