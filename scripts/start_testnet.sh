if [ "$(uname)" == "Linux" ]; then
    RUSTFLAGS='-C link-arg=-lgcc'
fi

cargo clean &&cargo update && cargo build --bin reth-bsc --release
#tip_block=0x8b841b96cb2863e21d9b87ba086e405684b8657e2d1b9ec75d6b70bb25725684 # 1W
tip_block=0xd16058f981cd556bf454a4c422cb10fd5a3c7938b232be433c6ccf3f08ef506e # 10W
#tip_block=0x32ba3474696050e50e21b53b2a29b38180ddaf92605b667ec4537cd81ac5bade # 100W

RUST_LOG=INFO ./target/release/reth-bsc node \
    --chain=bsc-testnet \
    --http --http.api="eth, net, txpool, web3, rpc" \
    --datadir=./target/data_dir/bsc-testnet/data_dir \
    --log.file.directory ./target/data_dir/bsc-testnet/logs \
    --debug.tip $tip_block \
    --trusted-peers=enode://428b12bcbbe4f607f6d83f91decbce549be5f0819d793ac32b0c7280f159dbb6125837b24d39ad1d568bc42d35e0754600429ea48044a44555e8af2113084ec7@18.181.52.189:30311,enode://28daea97a03f0bff6f061c3fbb2e7b61d61b8683240eb03310dfa2fd1d56f3551f714bb09515c3e389bae6ff11bd85e45075460408696f5f9a782b9ffb66e1d1@34.242.33.165:30311 \
    --metrics 0.0.0.0:6060 