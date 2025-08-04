#!/bin/bash

# BSC Testnet Server Mode (after sync completion)
RUST_LOG=DEBUG ./target/release/reth-bsc node \
    --chain=bsc-testnet \
    --http --http.api="eth, net, txpool, web3, rpc" \
    --datadir=./target/data_dir/bsc-testnet/data_dir \
    --log.file.directory ./target/data_dir/bsc-testnet/logs \
    --trusted-peers=enode://428b12bcbbe4f607f6d83f91decbce549be5f0819d793ac32b0c7280f159dbb6125837b24d39ad1d568bc42d35e0754600429ea48044a44555e8af2113084ec7@18.181.52.189:30311,enode://28daea97a03f0bff6f061c3fbb2e7b61d61b8683240eb03310dfa2fd1d56f3551f714bb09515c3e389bae6ff11bd85e45075460408696f5f9a782b9ffb66e1d1@34.242.33.165:30311 \
    --metrics 0.0.0.0:6060
    # Note: --debug.tip removed to keep server running

echo "BSC node is now running in server mode!"
echo "RPC API available at: http://127.0.0.1:8545"  
echo "You can now test: curl -X POST -H 'Content-Type: application/json' --data '{\"jsonrpc\":\"2.0\",\"method\":\"parlia_getSnapshot\",\"params\":[\"0x4e20\"],\"id\":3}' http://127.0.0.1:8545"
