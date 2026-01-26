# 🧪 Solana On-Chain Arbitrage Bot (Reference Implementation)

[![Discord](https://dcbadge.limes.pink/api/server/https://discord.gg/ejEuhN5kcV)](https://discord.gg/ejEuhN5kcV)

A **reference Solana on-chain arbitrage bot** demonstrating how to parse liquidity pools, build arbitrage routes, and invoke the on-chain arbitrage program.

This repository focuses on **pool parsing and program interaction**, and is intended as a **technical reference for advanced users**.

---

## ⚠️ Important Notice

> **This is NOT a fully featured or production-ready bot.**

- This repo is a **demo / reference implementation**
- It shows **how to parse pools and call the on-chain program**
- It is **not optimized**, **not fully automated**, and **not recommended for beginners**

### ✅ Recommended for new users

Use the **full featured production bot** instead:

- Full bot repo:  
  👉 https://github.com/Cetipoo/solana-mev-bot

- Getting started guide:  
  👉 https://docs.solanamevbot.com/home/onchain-bot/getting-started

---

## 📚 Documentation

- **On-chain program documentation**  
  👉 https://docs.solanamevbot.com/home/onchain-bot/onchain-program

---

## 🔗 On-Chain References

- **Program ID**  
  https://solscan.io/account/MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz

- **Example transaction**  
  https://solscan.io/tx/2JtgbXAgwPib9L5Ruc5vLhQ5qeX5EMhVDQbcCaAYVJKpEFn22ArEqXhipu5fFyhrEwosiHWzRUhWispJUCYyAnKT

---

## ✨ Features (Demo Scope)

- Load configuration from a TOML config file
- Automatically create ATAs if missing
- Send transactions through multiple RPC endpoints (spam mode)
- Built-in flashloan integration
- Parse multiple Solana AMM pool types
- Auto-detect DEX type by account owner
- Group pools by mint for arbitrage routing

---

## 🏦 Supported DEXes

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

---

## 🚀 Getting Started

### Prerequisites

- Rust & Cargo
- A Solana wallet funded with SOL
- One or more Solana RPC endpoints

---

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
