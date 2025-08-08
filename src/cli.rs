use clap::{Args, Parser, Subcommand};
use reth_node_core::args::{NetworkArgs, DatabaseArgs, DatadirArgs, RpcServerArgs};
use std::path::PathBuf;

/// BSC Fullnode CLI arguments
#[derive(Debug, Args)]
pub struct BscNodeArgs {
    /// Network configuration
    #[command(flatten)]
    pub network: NetworkArgs,

    /// Database configuration  
    #[command(flatten)]
    pub database: DatabaseArgs,

    /// Data directory configuration
    #[command(flatten)]
    pub datadir: DatadirArgs,

    /// RPC server configuration
    #[command(flatten)]
    pub rpc: RpcServerArgs,

    /// Chain specification to use (bsc, bsc-testnet)
    #[arg(long, default_value = "bsc")]
    pub chain: String,

    /// Enable sync mode for initial blockchain sync
    #[arg(long, default_value = "true")]
    pub sync: bool,

    /// Maximum number of peers to connect to
    #[arg(long, default_value = "50")]
    pub max_peers: usize,

    /// Enable prometheus metrics
    #[arg(long)]
    pub metrics: bool,

    /// Prometheus metrics port
    #[arg(long, default_value = "9001")]
    pub metrics_port: u16,

    /// Custom bootnodes (comma separated)
    #[arg(long, value_delimiter = ',')]
    pub bootnodes: Vec<String>,

    /// Disable discovery
    #[arg(long)]
    pub no_discovery: bool,

    /// Enable validator mode (for block production)
    #[arg(long)]
    pub validator: bool,

    /// Validator key file (required if --validator is enabled)
    #[arg(long)]
    pub validator_key: Option<PathBuf>,
}

impl Default for BscNodeArgs {
    fn default() -> Self {
        Self {
            network: NetworkArgs::default(),
            database: DatabaseArgs::default(),
            datadir: DatadirArgs::default(),
            rpc: RpcServerArgs::default(),
            chain: "bsc".to_string(),
            sync: true,
            max_peers: 50,
            metrics: false,
            metrics_port: 9001,
            bootnodes: Vec::new(),
            no_discovery: false,
            validator: false,
            validator_key: None,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum BscCommands {
    /// Run BSC fullnode
    Node(BscNodeArgs),
    /// Initialize database and genesis
    Init {
        /// Chain specification (bsc, bsc-testnet)
        #[arg(long, default_value = "bsc")]
        chain: String,
        /// Data directory
        #[arg(long)]
        datadir: Option<PathBuf>,
    },
    /// Show node information
    Info,
}

#[derive(Debug, Parser)]
#[command(author, version, about = "BSC Reth - High performance BSC client")]
pub struct BscCli {
    #[command(subcommand)]
    pub command: BscCommands,

    /// Enable debug logging
    #[arg(long, short)]
    pub debug: bool,

    /// Log level
    #[arg(long, default_value = "info")]
    pub log_level: String,
}

impl BscCli {
    /// Parse CLI arguments
    pub fn parse() -> Self {
        Parser::parse()
    }

    /// Validate CLI arguments
    pub fn validate(&self) -> eyre::Result<()> {
        match &self.command {
            BscCommands::Node(args) => {
                if args.validator && args.validator_key.is_none() {
                    return Err(eyre::eyre!("Validator mode requires --validator-key"));
                }

                if !["bsc", "bsc-testnet"].contains(&args.chain.as_str()) {
                    return Err(eyre::eyre!("Unsupported chain: {}", args.chain));
                }

                Ok(())
            }
            BscCommands::Init { chain, .. } => {
                if !["bsc", "bsc-testnet"].contains(&chain.as_str()) {
                    return Err(eyre::eyre!("Unsupported chain: {}", chain));
                }
                Ok(())
            }
            BscCommands::Info => Ok(()),
        }
    }
} 