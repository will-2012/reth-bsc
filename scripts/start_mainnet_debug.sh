#!/bin/bash

# BSC Mainnet startup script with debug settings and trusted peers

# Official BSC mainnet bootnodes from params/bootnodes.go
OFFICIAL_BSC_BOOTNODES=(
    )

# Known trusted BSC peers (example - you should add real trusted nodes)
TRUSTED_PEERS=(
    # Add your trusted peer enodes here, for example:
    "enode://551c8009f1d5bbfb1d64983eeb4591e51ad488565b96cdde7e40a207cfd6c8efa5b5a7fa88ed4e71229c988979e4c720891287ddd7d00ba114408a3ceb972ccb@34.245.203.3:30311"
"enode://c637c90d6b9d1d0038788b163a749a7a86fed2e7d0d13e5dc920ab144bb432ed1e3e00b54c1a93cecba479037601ba9a5937a88fe0be949c651043473c0d1e5b@34.244.120.206:30311"
"enode://bac6a548c7884270d53c3694c93ea43fa87ac1c7219f9f25c9d57f6a2fec9d75441bc4bad1e81d78c049a1c4daf3b1404e2bbb5cd9bf60c0f3a723bbaea110bc@3.255.117.110:30311"
"enode://94e56c84a5a32e2ef744af500d0ddd769c317d3c3dd42d50f5ea95f5f3718a5f81bc5ce32a7a3ea127bc0f10d3f88f4526a67f5b06c1d85f9cdfc6eb46b2b375@3.255.231.219:30311"
"enode://5d54b9a5af87c3963cc619fe4ddd2ed7687e98363bfd1854f243b71a2225d33b9c9290e047d738e0c7795b4bc78073f0eb4d9f80f572764e970e23d02b3c2b1f@34.245.16.210:30311"
"enode://41d57b0f00d83016e1bb4eccff0f3034aa49345301b7be96c6bb23a0a852b9b87b9ed11827c188ad409019fb0e578917d722f318665f198340b8a15ae8beff36@34.245.72.231:30311"
"enode://1bb269476f62e99d17da561b1a6b0d0269b10afee029e1e9fdee9ac6a0e342ae562dfa8578d783109b80c0f100a19e03b057f37b2aff22d8a0aceb62020018fe@54.78.102.178:30311"
"enode://3c13113538f3ca7d898d99f9656e0939451558758fd9c9475cff29f020187a56e8140bd24bd57164b07c3d325fc53e1ef622f793851d2648ed93d9d5a7ce975c@34.254.238.155:30311"
"enode://d19fd92e4f061d82a92e32d377c568494edcc36883a02e9d527b69695b6ae9e857f1ace10399c2aee4f71f5885ca3fe6342af78c71ad43ec1ca890deb6aaf465@34.247.29.116:30311"
"enode://c014bbf48209cdf8ca6d3bf3ff5cf2fade45104283dcfc079df6c64e0f4b65e4afe28040fa1731a0732bd9cbb90786cf78f0174b5de7bd5b303088e80d8e6a83@54.74.101.143:30311"

)

# Join arrays with comma
BOOTNODES=$(IFS=,; echo "${OFFICIAL_BSC_BOOTNODES[*]}")
TRUSTED=$(IFS=,; echo "${TRUSTED_PEERS[*]}")

# Configuration options
DATADIR="${DATADIR:-$HOME/Library/Application Support/reth/bsc}"
HTTP_PORT="${HTTP_PORT:-8545}"
WS_PORT="${WS_PORT:-8546}"
METRICS_PORT="${METRICS_PORT:-9001}"
P2P_PORT="${P2P_PORT:-30311}"
USE_DISCOVERY="${USE_DISCOVERY:-false}"

# Clear previous state if requested
if [[ "$CLEAR_STATE" == "true" ]]; then
    echo "Cleaning up previous state..."
    rm -rf "$DATADIR"
fi

echo "Starting BSC mainnet node..."
echo "Fork ID: 098d24ac (includes Pascal, Lorentz, Maxwell)"
echo "Data directory: $DATADIR"
echo "Discovery: $USE_DISCOVERY"
echo ""

# Build command
CMD="RUST_LOG=\"info,reth_engine_tree=warn,net=debug\" ./target/release/reth-bsc node"
CMD="$CMD --chain bsc"
CMD="$CMD --datadir \"$DATADIR\""
CMD="$CMD --http --http.addr 0.0.0.0 --http.port $HTTP_PORT"
CMD="$CMD --http.api=\"eth,net,web3,debug,trace,txpool\""
CMD="$CMD --ws --ws.addr 0.0.0.0 --ws.port $WS_PORT"
CMD="$CMD --metrics 0.0.0.0:$METRICS_PORT"
CMD="$CMD --port $P2P_PORT"
CMD="$CMD --max-outbound-peers 100"
CMD="$CMD --max-inbound-peers 50"
CMD="$CMD --full"
CMD="$CMD --db.max-size=2TB"

# Add network options based on configuration
if [[ "$USE_DISCOVERY" == "false" ]]; then
    echo "Running without discovery, using only trusted peers..."
    CMD="$CMD --disable-discovery"
    if [[ -n "$TRUSTED" ]]; then
        CMD="$CMD --trusted-peers=\"$TRUSTED\""
    fi
else
    echo "Running with discovery enabled..."
    CMD="$CMD --bootnodes=\"$BOOTNODES\""
    if [[ -n "$TRUSTED" ]]; then
        CMD="$CMD --trusted-peers=\"$TRUSTED\""
    fi
fi

# Debug options
CMD="$CMD --log.file.directory=\"$DATADIR/logs\""
CMD="$CMD -vvv"

echo "Command: $CMD"
echo ""

# Execute
eval $CMD 