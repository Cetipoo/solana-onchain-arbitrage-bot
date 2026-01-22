# Solana Onchain Arbitrage Bot

[![Discord](https://dcbadge.limes.pink/api/server/https://discord.gg/ejEuhN5kcV)](https://discord.gg/ejEuhN5kcV)

A simple Solana onchain arbitrage bot for arbitrage opportunities. This bot calculate the most optimal trade size between various DEX pools on Solana and executes trades when profitable opportunities are found. This repository utilizes the onchain program for executing arbitrage trades.

The full featured bot can be found here:
https://github.com/Cetipoo/solana-mev-bot

**This is a demo bot to show how to parse each pool and call the onchain program.**
**This is NOT a fully functional bot. This is only recommanded for advanced users to use as a reference.**
**For new users please use the full featured bot to get started:**
**https://docs.solanamevbot.com/home/onchain-bot/getting-started**

Full documentation for the onchain program:
https://docs.solanamevbot.com/home/onchain-bot/onchain-program

Example transaction:
https://solscan.io/tx/2JtgbXAgwPib9L5Ruc5vLhQ5qeX5EMhVDQbcCaAYVJKpEFn22ArEqXhipu5fFyhrEwosiHWzRUhWispJUCYyAnKT

Program:
https://solscan.io/account/MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz

## Features

- Load configuration from a config file
- Create ATA if not exist
- Send transactions through multiple RPC endpoints (spam)
- Buildin flashloan integration
- Parse all available pool types (Raydium, DLMM, Whirlpool, etc.)

## Supported DEXes

- Pump AMM
- Raydium V4
- Raydium CPMM
- Raydium CLMM
- Meteora DLMM
- Meteora Dynamic AMM
- Meteora DAMM V2
- Orca Whirlpool
- Vertigo
- Heaven
- Futarchy
- Humidifi
- PancakeSwap
- Byreal

## Getting Started

### Prerequisites

- Rust and Cargo installed
- A Solana wallet with SOL

### Installation

1. Clone the repository

   ```
   git clone https://github.com/cetipoo/solana-onchain-arbitrage-bot.git
   cd solana-onchain-arbitrage-bot
   ```

2. Update config.toml file

3. Run the bot
   ```
   cargo run --release --bin solana-onchain-arbitrage-bot -- --config config.toml
   ```

### Configuration

1. Copy the example configuration file:
   ```
   cp config.toml.example config.toml
   ```
2. Edit `config.toml` and configure your:
   - Private key for your Solana wallet
   - RPC endpoint URL(s)
3. Add pool addresses to the `markets` list:
   - DEX type is auto-detected by account owner (no need to specify pool type)
   - Pools are automatically grouped by mint for arbitrage routing
   - Optionally add lookup table accounts for transaction optimization

## Configuration Options

### Bot Configuration (`[bot]`)

- `compute_unit_limit`: Maximum compute unit limit per transaction

### Routing Configuration (`[routing.markets]`)

- `markets`: List of pool/market addresses (DEX type is auto-detected by account owner)
- `lookup_table_accounts`: List of lookup table accounts (optional, shared across all pools)
- `process_delay`: Delay between processing cycles in milliseconds

### RPC Configuration (`[rpc]`)

- `url`: RPC URL for the Solana network (supports environment variables with `$VAR_NAME`)

### Spam Configuration (`[spam]`)

- `enabled`: Enable spam transactions (send through multiple RPC endpoints)
- `sending_rpc_urls`: List of RPC URLs for sending transactions
- `compute_unit_price`: Fixed compute unit price in microlamports
- `max_retries`: Maximum retries for transaction sending

### Wallet Configuration (`[wallet]`)

- `private_key`: Private key - can be base58 string, file path, or environment variable (`$VAR_NAME`)

### Flashloan Configuration (`[flashloan]`)

- `enabled`: Enable flashloan integration

## License

MIT
