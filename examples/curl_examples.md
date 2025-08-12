# BSC Parlia Snapshot API - Curl Examples

Your BSC node now supports the official `parlia_getSnapshot` API. Here are 3 curl examples to get snapshots at different block heights for testnet:

## **1. Get snapshot at ~1 week (200,000 blocks = 0x30d40):**
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "parlia_getSnapshot",
    "params": ["0x30d40"],
    "id": 1
  }'
```

## **2. Get snapshot at ~2 weeks (400,000 blocks = 0x61a80):**
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "parlia_getSnapshot",
    "params": ["0x61a80"],
    "id": 2
  }'
```

## **3. Get snapshot at ~3 weeks (600,000 blocks = 0x927c0):**
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "parlia_getSnapshot",
    "params": ["0x927c0"],
    "id": 3
  }'
```

## **Alternative: Using Decimal Block Numbers**
Your API also accepts decimal format:
```bash
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "parlia_getSnapshot",
    "params": ["200000"],
    "id": 4
  }'
```

## **Expected Response Format**
The response will match the BSC official format with:
- `number`: Block number
- `hash`: Block hash
- `epoch_length`: 200 (BSC epoch length)
- `block_interval`: 3000 (BSC block interval in milliseconds)
- `turn_length`: 1 (default turn length)
- `validators`: Map of validator addresses to validator info
- `recents`: Map of recent block numbers to proposer addresses
- `recent_fork_hashes`: Map of recent block numbers to fork hashes
- `attestation:omitempty`: null (for compatibility)

## **Notes:**
- Make sure your BSC node is running with RPC enabled (`--http --http.api="eth, net, txpool, web3, rpc"`)
- The node must be synced to the requested block height
- If a snapshot doesn't exist at the requested block, the API will return `null`