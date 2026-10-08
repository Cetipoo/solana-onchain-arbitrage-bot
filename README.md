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
- Every transaction pays network fees even when it finds no profit and fails. Each `[[transactions]]` entry sends one every `process_delay_ms`, so the example config, with five entries every 400 ms, pays about 5 SOL a day in base fees alone. Start with one transaction and a longer delay

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

---

## ✨ Features (Demo Scope)

- Build the on-chain program's V10 instruction for every route shape it trades: 2-, 3- and 4-hop routes, several per transaction
- Expose every instruction option: settlement mint, flashloan, minimum profit, fixed trade size, no-failure mode, constant-rate conversion, additional fee
- Infer every mint from the pools and check each pool's role at startup
- Size the compute unit limit from the pools in each transaction
- Send v1 transactions (SIMD-0385, up to 64 accounts, no lookup tables) through one or more RPC endpoints
- Create the wallet's WSOL and USDC token accounts if missing

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
- PancakeSwap
- Byreal
- Manifest

---

## 🚀 Getting Started

### Prerequisites

- Rust 1.97.1 or newer, with Cargo
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

Copy the example and edit it:

```
cp config.toml.example config.toml
```

Every address is a pool; the bot reads each pool's DEX from its owner and its mints from its state, so no pool types or mints are configured.

## Routes

Each `[[transactions]]` entry is one V10 instruction. It settles in SOL or USDC, and holds up to 4 groups: all direct groups or all triangle groups, since the program cannot mix them in one instruction. The program picks the most profitable route across the groups, sizes the trade and executes it, or fails with `NoProfit`.

| Route | Group | Hops |
|---|---|---|
| settlement → token → settlement | `direct`, all pools quoted in the settlement mint | 2 |
| settlement → token → other → settlement | `direct` with pools quoted in the other of SOL/USDC | 3 |
| settlement → token → stock → settlement | `triangle` with bridges quoted in the settlement mint | 3 |
| settlement → token → stock → other → settlement | `triangle` with bridges quoted in the other of SOL/USDC | 4 |

"Other → settlement" always goes through the Raydium SOL/USDC pool (`58oQChx4yWmvKdwLLZzBi4ChoCc2fqCUWBkwMihLYQo2`); the bot adds it when a route needs it.

- **Direct group** (`pools`): two or more pools trading one token against SOL or USDC, at least one quoted in the settlement mint.
- **Triangle group**:
  - `intermediate`: trades the token against a second token, the stock.
  - `bridges`: trade the stock against SOL or USDC. Every triangle in a transaction uses the same quote mint.
  - `direct`: trade the token against the settlement mint.

A transaction may hold at most 16 pools, including the conversion, and must fit a v1 transaction (64 accounts, 4096 bytes); the bot reports an oversized transaction instead of sending it. Pool state is reloaded every 5 seconds. A pool that cannot be traded at that moment, such as a full Manifest market, is left out until the next reload.

## Configuration Options

### `[rpc]`

- `url`: RPC used to read chain state (`$NAME` reads the environment variable `NAME`)
- `send_urls`: RPCs that transactions are sent to; `url` when omitted

### `[wallet]`

- `private_key`: base58 keypair or keypair file path (`$NAME` reads the environment variable `NAME`)

### `[bot]` (optional)

- `process_delay_ms`: delay between sends of each transaction (default 400)
- `compute_unit_price`: priority fee in microlamports per compute unit (default 1000)
- `max_retries`: retries each RPC makes for each send (default 3)

### `[[transactions]]`

- `settlement`: `"SOL"` or `"USDC"`
- `flashloan`: borrow the trade from the program's vault instead of the wallet (default false)
- `minimum_profit`: profit, in settlement-mint base units, below which the trade fails (default 0)
- `trade_size`: fixed input in settlement-mint base units; 0 lets the program find the best size (default 0)
- `no_failure`: succeed without trading when there is no profit, instead of failing (default false)
- `constant_conversion`: price the SOL/USDC conversion at its quoted rate instead of walking its curve, which costs fewer compute units (default false)
- `additional_fee`: `{ bps, collector }`, a share of the profit above `minimum_profit` paid to `collector`, a settlement-mint token account (at most 8500 bps)
- `[[transactions.direct]]` or `[[transactions.triangle]]`: the groups, described above

## License

MIT
