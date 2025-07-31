cargo update && cargo build --bin reth-bsc --release


RUST_LOG=debug ./target/release/reth-bsc node \
    --chain=bsc-testnet \
    --datadir=./target/data_dir/bsc-testnet/data_dir \
    --log.file.directory ./target/data_dir/bsc-testnet/logs \
    --debug.tip 0xf82cfbcd2bed10f5b747d89e1d5efbb0dfcdb75f62d1f4dee7e99b4059af7f9f \
    --trusted-peers=enode://428b12bcbbe4f607f6d83f91decbce549be5f0819d793ac32b0c7280f159dbb6125837b24d39ad1d568bc42d35e0754600429ea48044a44555e8af2113084ec7@18.181.52.189:30311,enode://28daea97a03f0bff6f061c3fbb2e7b61d61b8683240eb03310dfa2fd1d56f3551f714bb09515c3e389bae6ff11bd85e45075460408696f5f9a782b9ffb66e1d1@34.242.33.165:30311 \
    --metrics 0.0.0.0:6060