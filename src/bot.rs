use crate::ata::create_missing_settlement_atas;
use crate::config::Config;
use crate::loader::AccountLoader;
use crate::route::{Basket, Route};
use crate::transaction::{build_transaction, send_transaction};
use anyhow::{Context, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::hash::Hash;
use solana_sdk::signature::Keypair;
use solana_sdk::signer::Signer;
use std::sync::RwLock;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{error, info};

const BLOCKHASH_REFRESH_INTERVAL: Duration = Duration::from_secs(10);
/// How often pool state (vaults, tick/bin arrays, Pump fee recipient) is reloaded.
const POOL_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

pub fn run_bot(config_path: &str) -> Result<()> {
    let config = Config::load(config_path)?;
    let bot = Bot::new(&config)?;
    let mut loader = AccountLoader::new(&bot.rpc);
    let routes = config
        .transactions
        .iter()
        .enumerate()
        .map(|(i, transaction)| {
            Route::resolve(i + 1, transaction, &mut loader)
                .with_context(|| format!("transaction #{}", i + 1))
        })
        .collect::<Result<Vec<_>>>()?;
    anyhow::ensure!(!routes.is_empty(), "No transactions configured");

    // The on-chain program creates target token ATAs as needed.
    create_missing_settlement_atas(&bot.rpc, &bot.wallet)?;

    thread::scope(|scope| {
        scope.spawn(|| bot.refresh_blockhash());
        for route in &routes {
            info!("Starting {route} for targets {:?}", route.targets());
            scope.spawn(|| bot.trade(route));
        }
    });
    Ok(())
}

/// What every transaction's loop shares: clients, wallet, the latest
/// blockhash and the sending settings.
struct Bot {
    rpc: RpcClient,
    senders: Vec<RpcClient>,
    wallet: Keypair,
    blockhash: RwLock<Hash>,
    compute_unit_price: u64,
    max_retries: usize,
    process_delay: Duration,
}

impl Bot {
    fn new(config: &Config) -> Result<Self> {
        let rpc = RpcClient::new(config.rpc.url.clone());
        let send_urls = match config.rpc.send_urls.as_slice() {
            [] => vec![config.rpc.url.clone()],
            urls => urls.to_vec(),
        };
        let wallet =
            load_keypair(&config.wallet.private_key).context("Failed to load wallet keypair")?;
        info!("Wallet loaded: {}", wallet.pubkey());
        Ok(Self {
            blockhash: RwLock::new(rpc.get_latest_blockhash()?),
            rpc,
            senders: send_urls.into_iter().map(RpcClient::new).collect(),
            wallet,
            compute_unit_price: config.bot.compute_unit_price,
            max_retries: config.bot.max_retries,
            process_delay: Duration::from_millis(config.bot.process_delay_ms),
        })
    }

    fn refresh_blockhash(&self) {
        loop {
            thread::sleep(BLOCKHASH_REFRESH_INTERVAL);
            match self.rpc.get_latest_blockhash() {
                Ok(latest) => {
                    *self.blockhash.write().unwrap() = latest;
                    info!("Blockhash refreshed: {}", latest);
                }
                Err(e) => error!("Failed to refresh blockhash: {:?}", e),
            }
        }
    }

    fn trade(&self, route: &Route) {
        let mut basket = None;
        let mut next_refresh = Instant::now();
        loop {
            if Instant::now() >= next_refresh {
                // A failed reload keeps trading the last loaded pools.
                basket = self.load(route).or(basket);
                next_refresh = Instant::now() + POOL_REFRESH_INTERVAL;
            }
            if let Some(basket) = &basket {
                self.send(route, basket);
            }
            thread::sleep(self.process_delay);
        }
    }

    fn load(&self, route: &Route) -> Option<Basket> {
        match route.load(&mut AccountLoader::new(&self.rpc), self.wallet.pubkey()) {
            Ok(basket) => {
                info!(
                    "Pool data refreshed for {route}: {} pools settled in {}",
                    basket.pool_count(),
                    basket.settlement()
                );
                Some(basket)
            }
            Err(e) => {
                error!("Failed to refresh pool data for {route}: {e:#}");
                None
            }
        }
    }

    fn send(&self, route: &Route, basket: &Basket) {
        let blockhash = *self.blockhash.read().unwrap();
        match build_transaction(&self.wallet, basket, self.compute_unit_price, blockhash) {
            Ok(tx) => {
                let accepted = send_transaction(&self.senders, &tx, self.max_retries);
                info!(
                    "{route}: transaction {} accepted by {accepted}/{} RPCs",
                    tx.signatures[0],
                    self.senders.len()
                );
            }
            Err(e) => error!("Error building {route}: {e:#}"),
        }
    }
}

/// A base58 keypair, or the path of a keypair file. The value may be the
/// secret itself, so errors never echo it.
fn load_keypair(private_key: &str) -> Result<Keypair> {
    if let Ok(keypair) = Keypair::try_from_base58_string(private_key) {
        return Ok(keypair);
    }
    solana_sdk::signature::read_keypair_file(private_key).map_err(|_| {
        anyhow::anyhow!(
            "wallet.private_key is neither a base58 keypair nor a readable keypair file"
        )
    })
}
