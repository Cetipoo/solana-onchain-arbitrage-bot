use serde::{Deserialize, Deserializer};
use std::{env, fs::File, io::Read};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub bot: BotConfig,
    pub routing: RoutingConfig,
    pub rpc: RpcConfig,
    pub spam: Option<SpamConfig>,
    pub wallet: WalletConfig,
    pub flashloan: Option<FlashloanConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BotConfig {
    pub compute_unit_limit: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RoutingConfig {
    pub markets: MarketsConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MarketsConfig {
    pub markets: Vec<String>,
    pub lookup_table_accounts: Option<Vec<String>>,
    pub process_delay: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcConfig {
    #[serde(deserialize_with = "serde_string_or_env")]
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SpamConfig {
    pub enabled: bool,
    pub sending_rpc_urls: Vec<String>,
    pub compute_unit_price: u64,
    pub max_retries: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WalletConfig {
    #[serde(deserialize_with = "serde_string_or_env")]
    pub private_key: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FlashloanConfig {
    pub enabled: bool,
}

pub fn serde_string_or_env<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value_or_env = String::deserialize(deserializer)?;
    let value = match value_or_env.chars().next() {
        Some('$') => env::var(&value_or_env[1..])
            .unwrap_or_else(|_| panic!("reading `{}` from env", &value_or_env[1..])),
        _ => value_or_env,
    };
    Ok(value)
}

impl Config {
    pub fn load(path: &str) -> anyhow::Result<Self> {
        let mut file = File::open(path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;

        let config: Config = toml::from_str(&contents)?;
        Ok(config)
    }
}
