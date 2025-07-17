use crate::node::evm::config::{BscBlockExecutorFactory, BscEvmConfig};
use reth_primitives::Block;
use alloy_consensus::Header;
use reth_evm::{
    block::BlockExecutionError,
    execute::{BlockAssembler, BlockAssemblerInput},
};

impl BlockAssembler<BscBlockExecutorFactory> for BscEvmConfig {
    type Block = Block;

    fn assemble_block(
        &self,
        input: BlockAssemblerInput<'_, '_, BscBlockExecutorFactory, Header>,
    ) -> Result<Self::Block, BlockExecutionError> {
        let Block { header, body: inner } = self.block_assembler.assemble_block(input)?;
        Ok(Block {
            header,
            body: inner,
        })
    }
}
