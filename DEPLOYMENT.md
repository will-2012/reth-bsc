# BSC Reth Deployment Guide

## 🚀 Quick Start

BSC Reth is now ready for fullnode deployment! This guide covers setting up and running a BSC node on mainnet or testnet.

## ✅ Prerequisites

- **Rust 1.80+** with cargo
- **Git** for cloning repositories
- **SSD storage** (minimum 2TB for mainnet, 500GB for testnet)
- **8GB+ RAM** (16GB+ recommended for mainnet)
- **Stable internet connection** with >100Mbps

## 🏗️ Building from Source

### 1. Clone and Build

```bash
git clone https://github.com/your-username/loocapro_reth_bsc.git
cd loocapro_reth_bsc
cargo build --release --bin reth-bsc
```

### 2. Verify Installation

```bash
./target/release/reth-bsc --help
```

## 🌐 Network Configuration

### BSC Mainnet

```bash
./target/release/reth-bsc node \
  --chain bsc \
  --http \
  --http.api eth,net,web3,debug,trace \
  --ws \
  --metrics 127.0.0.1:9001
```

### BSC Testnet (Chapel)

```bash
./target/release/reth-bsc node \
  --chain bsc-testnet \
  --http \
  --http.api eth,net,web3,debug,trace \
  --ws \
  --metrics 127.0.0.1:9001
```

## ⚙️ Configuration Options

### Basic Options

| Flag | Description | Default |
|------|-------------|---------|
| `--chain` | Network to connect to (`bsc`, `bsc-testnet`) | `bsc` |
| `--datadir` | Data directory for blockchain data | OS default |
| `--http` | Enable HTTP RPC server | disabled |
| `--ws` | Enable WebSocket RPC server | disabled |

### Network Options

| Flag | Description | Default |
|------|-------------|---------|
| `--port` | P2P listening port | `30303` |
| `--max-outbound-peers` | Maximum outbound connections | `100` |
| `--max-inbound-peers` | Maximum inbound connections | `30` |
| `--bootnodes` | Custom bootstrap nodes | Built-in |

### Performance Options

| Flag | Description | Default |
|------|-------------|---------|
| `--full` | Run as full node (pruned) | enabled |
| `--metrics` | Enable Prometheus metrics | disabled |
| `--db.max-size` | Maximum database size | automatic |

### BSC-Specific Options

| Flag | Description | Default |
|------|-------------|---------|
| `--debug` | Enable debug logging | disabled |
| `--validator` | Enable validator mode | disabled |

## 🔧 Example Configurations

### Home User (Light Sync)

```bash
./target/release/reth-bsc node \
  --chain bsc \
  --http \
  --http.addr 127.0.0.1 \
  --http.port 8545 \
  --max-outbound-peers 25 \
  --max-inbound-peers 10
```

### Production Server (Full Node)

```bash
./target/release/reth-bsc node \
  --chain bsc \
  --datadir /data/bsc-reth \
  --http \
  --http.addr 0.0.0.0 \
  --http.api eth,net,web3,trace \
  --ws \
  --ws.addr 0.0.0.0 \
  --metrics 0.0.0.0:9001 \
  --max-outbound-peers 100 \
  --max-inbound-peers 50 \
  --db.max-size 4TB
```

### Validator Node

```bash
./target/release/reth-bsc node \
  --chain bsc \
  --validator \
  --http \
  --authrpc.jwtsecret /path/to/jwt.hex \
  --bootnodes "enode://your-trusted-nodes" \
  --trusted-only
```

### Testnet Development

```bash
./target/release/reth-bsc node \
  --chain bsc-testnet \
  --debug \
  --http \
  --http.api eth,net,web3,debug,trace,txpool \
  --ws \
  --metrics 127.0.0.1:9001 \
  -vvv
```

## 📊 Monitoring & Maintenance

### Metrics (Prometheus)

Add `--metrics` flag to enable metrics on `http://localhost:9001/metrics`

Key metrics to monitor:
- `reth_sync_block_number` - Current sync progress
- `reth_network_peers` - Connected peer count  
- `reth_consensus_state` - Consensus state
- `reth_txpool_pending` - Transaction pool size

### Logging

Set log levels with verbosity flags:
- `-v` - Errors only
- `-vv` - Warnings  
- `-vvv` - Info (recommended)
- `-vvvv` - Debug
- `-vvvvv` - Trace (very verbose)

### Health Checks

Check node health:
```bash
curl -X POST -H "Content-Type: application/json" \
  --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' \
  http://localhost:8545
```

## 🔥 Performance Tuning

### Storage Optimization

- **Use NVMe SSD** for best performance
- **Separate data/logs** on different drives
- **Enable compression** with `--db.growth-step 4GB`

### Memory Settings

```bash
# For 32GB RAM system
export MALLOC_CONF="dirty_decay_ms:1000,muzzy_decay_ms:1000"
ulimit -n 65536  # Increase file descriptor limit
```

### Network Optimization

```bash
# Linux network tuning
echo 'net.core.rmem_max = 16777216' >> /etc/sysctl.conf
echo 'net.core.wmem_max = 16777216' >> /etc/sysctl.conf
sysctl -p
```

## 🛠️ Troubleshooting

### Common Issues

**Sync is slow:**
- Check peer count with metrics
- Verify network bandwidth  
- Increase `--max-outbound-peers`

**High memory usage:**
- Reduce `--engine.memory-block-buffer-target`
- Enable pruning with `--full`
- Monitor with `--metrics`

**Connection issues:**
- Check firewall settings for port 30303
- Verify bootnodes are reachable
- Try `--disable-nat` if behind NAT

**Database corruption:**
- Stop node safely (SIGTERM, not SIGKILL)
- Check disk space and health
- Consider `reth db stats` for analysis

### Debug Mode

Enable comprehensive logging:
```bash
./target/release/reth-bsc node --chain bsc --debug -vvvv
```

## 🔒 Security Considerations

### Firewall Rules

```bash
# Allow P2P connections
ufw allow 30303/tcp
ufw allow 30303/udp

# RPC access (restrict as needed)
ufw allow from YOUR_IP to any port 8545
ufw allow from YOUR_IP to any port 8546
```

### JWT Authentication

For authenticated RPC:
```bash
# Generate JWT secret
openssl rand -hex 32 > jwt.hex

# Use with node
./target/release/reth-bsc node \
  --authrpc.jwtsecret jwt.hex \
  --rpc.jwtsecret $(cat jwt.hex)
```

## 📈 Sync Times & Storage

### Expected Sync Times (estimates)

| Network | Storage | Time |
|---------|---------|------|
| BSC Mainnet | 2TB+ | 2-7 days |
| BSC Testnet | 500GB+ | 12-24 hours |

### Storage Growth

| Network | Daily Growth |
|---------|--------------|
| BSC Mainnet | ~20-50GB |
| BSC Testnet | ~5-10GB |

## 🚀 Next Steps

1. **Monitor sync progress** with metrics
2. **Set up automatic restarts** with systemd
3. **Configure log rotation** 
4. **Plan storage upgrades** based on growth
5. **Join BSC community** for updates

## 📚 Additional Resources

- [BSC Documentation](https://docs.binance.org/)
- [Reth Book](https://reth.rs/) 
- [BSC Network Status](https://bscscan.com/)
- [Community Discord](#) 

---

**⚠️ Important:** Always test on testnet before mainnet deployment! 