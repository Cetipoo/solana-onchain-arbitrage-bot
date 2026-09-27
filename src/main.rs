use clap::Parser;
use tracing::{info, Level};

/// A simplified Solana onchain arbitrage bot
#[derive(Parser)]
#[command(version)]
struct Args {
    /// Config file
    #[arg(short, long, value_name = "FILE", default_value = "config.toml")]
    config: String,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    let args = Args::parse();
    info!("Using config file: {}", args.config);
    solana_onchain_arbitrage_bot::bot::run_bot(&args.config)
}
