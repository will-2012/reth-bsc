#!/bin/bash

# BSC Mainnet startup script with debug settings and trusted peers

# Official BSC mainnet bootnodes from params/bootnodes.go
OFFICIAL_BSC_BOOTNODES=(
    "enode://433c8bfdf53a3e2268ccb1b829e47f629793291cbddf0c76ae626da802f90532251fc558e2e0d10d6725e759088439bf1cd4714716b03a259a35d4b2e4acfa7f@52.69.102.73:30311"
    "enode://571bee8fb902a625942f10a770ccf727ae2ba1bab2a2b64e121594a99c9437317f6166a395670a00b7d93647eacafe598b6bbcef15b40b6d1a10243865a3e80f@35.73.84.120:30311"
    "enode://fac42fb0ba082b7d1eebded216db42161163d42e4f52c9e47716946d64468a62da4ba0b1cac0df5e8bf1e5284861d757339751c33d51dfef318be5168803d0b5@18.203.152.54:30311"
    "enode://3063d1c9e1b824cfbb7c7b6abafa34faec6bb4e7e06941d218d760acdd7963b274278c5c3e63914bd6d1b58504c59ec5522c56f883baceb8538674b92da48a96@34.250.32.100:30311"
    "enode://ad78c64a4ade83692488aa42e4c94084516e555d3f340d9802c2bf106a3df8868bc46eae083d2de4018f40e8d9a9952c32a0943cd68855a9bc9fd07aac982a6d@34.204.214.24:30311"
    "enode://5db798deb67df75d073f8e2953dad283148133acb520625ea804c9c4ad09a35f13592a762d8f89056248f3889f6dcc33490c145774ea4ff2966982294909b37a@107.20.191.97:30311"
)

# Known trusted BSC peers (example - you should add real trusted nodes)
TRUSTED_PEERS=(
    # Add your trusted peer enodes here, for example:
    # "enode://pubkey@ip:port"
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
    CMD="$CMD --no-discovery"
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
CMD="$CMD --debug.continuous"
CMD="$CMD --log.file.directory=\"$DATADIR/logs\""
CMD="$CMD -vvv"

echo "Command: $CMD"
echo ""

# Execute
eval $CMD 