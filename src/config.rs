use crate::v10::{SOL, USDC};
use anyhow::Context;
use serde::{de::Error, Deserialize, Deserializer};
use solana_sdk::pubkey::Pubkey;
use std::{env, fs, str::FromStr};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub rpc: RpcConfig,
    pub wallet: WalletConfig,
    #[serde(default)]
    pub bot: BotConfig,
    pub transactions: Vec<TransactionConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcConfig {
    #[serde(deserialize_with = "string_or_env")]
    pub url: String,
    /// Where transactions are sent; `url` when empty.
    #[serde(default)]
    pub send_urls: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WalletConfig {
    #[serde(deserialize_with = "string_or_env")]
    pub private_key: String,
}

impl std::fmt::Debug for WalletConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalletConfig")
            .field("private_key", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BotConfig {
    /// Delay between sends of each transaction.
    pub process_delay_ms: u64,
    /// Priority fee in microlamports per CU.
    pub compute_unit_price: u64,
    /// Retries each RPC makes for each send.
    pub max_retries: usize,
}

impl Default for BotConfig {
    fn default() -> Self {
        Self {
            process_delay_ms: 400,
            compute_unit_price: 1000,
            max_retries: 3,
        }
    }
}

/// One V10 instruction: its header options and the route groups it trades.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactionConfig {
    pub settlement: Settlement,
    /// Borrow the trade from the program's vault instead of the wallet.
    #[serde(default)]
    pub flashloan: bool,
    /// Profit, in settlement-mint base units, below which the trade fails.
    #[serde(default)]
    pub minimum_profit: u64,
    /// Fixed input in settlement-mint base units; 0 lets the program search.
    #[serde(default)]
    pub trade_size: u64,
    /// Succeed without trading when there is no profit, instead of failing.
    #[serde(default)]
    pub no_failure: bool,
    /// Price the SOL/USDC conversion at its quoted rate instead of its curve.
    #[serde(default)]
    pub constant_conversion: bool,
    pub additional_fee: Option<AdditionalFee>,
    #[serde(default)]
    pub direct: Vec<DirectConfig>,
    #[serde(default)]
    pub triangle: Vec<TriangleConfig>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Settlement {
    #[serde(rename = "SOL")]
    Sol,
    #[serde(rename = "USDC")]
    Usdc,
}

impl Settlement {
    pub fn mint(self) -> Pubkey {
        match self {
            Self::Sol => SOL,
            Self::Usdc => USDC,
        }
    }
}

/// A share of the profit above `minimum_profit` paid to a third party.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdditionalFee {
    pub bps: u16,
    /// Settlement-mint token account that receives the fee.
    #[serde(deserialize_with = "pubkey")]
    pub collector: Pubkey,
}

/// Pools trading one token against SOL or USDC.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectConfig {
    #[serde(deserialize_with = "pubkeys")]
    pub pools: Vec<Pubkey>,
}

/// A token, a second token it trades against, and SOL or USDC.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriangleConfig {
    /// Trades the target against the stock.
    #[serde(deserialize_with = "pubkey")]
    pub intermediate: Pubkey,
    /// Trade the stock against SOL or USDC.
    #[serde(deserialize_with = "pubkeys")]
    pub bridges: Vec<Pubkey>,
    /// Trade the target against the settlement mint.
    #[serde(deserialize_with = "pubkeys")]
    pub direct: Vec<Pubkey>,
}

/// The value, or the environment variable it names as `$NAME`.
fn string_or_env<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    match value.strip_prefix('$') {
        Some(name) => env::var(name)
            .map_err(|_| D::Error::custom(format!("environment variable `{name}` is not set"))),
        None => Ok(value),
    }
}

fn parse_pubkey<E: Error>(value: &str) -> Result<Pubkey, E> {
    Pubkey::from_str(value).map_err(|e| E::custom(format!("invalid address `{value}`: {e}")))
}

fn pubkey<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Pubkey, D::Error> {
    parse_pubkey(&String::deserialize(deserializer)?)
}

fn pubkeys<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Pubkey>, D::Error> {
    Vec::<String>::deserialize(deserializer)?
        .iter()
        .map(|value| parse_pubkey(value))
        .collect()
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let contents =
            fs::read_to_string(path).with_context(|| format!("Failed to read config {path}"))?;
        Ok(toml::from_str(&contents)?)
    }
}
