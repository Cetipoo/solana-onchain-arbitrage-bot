//! Hylo's conversions as V10 pools. The Exchange program mints hyUSD against
//! USDC, and hyUSD or a levercoin against a liquid-staking token it holds or
//! an Exo collateral; it redeems each, and converts each levercoin to and
//! from hyUSD. The Stability Pool issues sHYUSD shares of its hyUSD. Every
//! account in a conversion's executor block has a fixed address.
//!
//! A conversion's `pool` is the account the executor quotes it from: the
//! USDC pair, the pool config, an LST header or an Exo pair, or `Hylo` for
//! xSOL against hyUSD. A header pools its LST's hyUSD and xSOL conversions,
//! and a pair its collateral's hyUSD and levercoin conversions and that
//! levercoin against hyUSD, so the mints tell a pool's conversions apart.
use crate::v10::{PoolPair, USDC};
use anyhow::{bail, Result};
use executor_v10_abi::Venue;
use solana_sdk::{instruction::AccountMeta, pubkey, pubkey::Pubkey};

pub const HYLO_EXCHANGE: Pubkey = pubkey!("HYEXCHtHkBagdStcJCp3xbbb9B7sdMdWXFNj6mdsG4hn");
pub const HYLO_STABILITY: Pubkey = pubkey!("HysTabVUfmQBFcmzu1ctRd1Y1fxd66RBpboy1bmtDSQQ");
pub const HYUSD: Pubkey = pubkey!("5YMkXAYccHSGnHn9nob9xEvv6Pvka9DZWH7nTbotTu9E");
pub const SHYUSD: Pubkey = pubkey!("HnnGv3HrSqjRpgdFmx7vQGjntNEoex1SU4e9Lxcxuihz");
/// The levercoin of every LST, which `Hylo` records.
pub const XSOL: Pubkey = pubkey!("4sWNB8zGWHkh6UnmwiEtzNxL4XrN7uK9tosbESbJFfVs");

/// The Exchange program's protocol state, which both programs read.
const HYLO: Pubkey = pubkey!("9cd2sAfbBvKs4SX9YKo4dcjwP3TgTVQ8dT5koshGcDND");
const USDC_PAIR: Pubkey = pubkey!("CMNPACEDebyvNJDgBxRc5fbScF8kmx52ZPBY4Cu4wuwS");
const HYUSD_MINT_AUTHORITY: Pubkey = pubkey!("CfuSViqf6wvUKEprLhtuCsSanvfAsMbDmkAW92FP95qe");
const XSOL_MINT_AUTHORITY: Pubkey = pubkey!("J8rGkrzsvqinX9kfwD8SkP3mRzXAk3uiDRUaiZKXM4as");
const USDC_VAULT_AUTHORITY: Pubkey = pubkey!("FVVUPkXtkiwYz8epMSGMgNwZNVKcyyWfN9iQSJ9z5SX4");
const USDC_FEE_AUTHORITY: Pubkey = pubkey!("39ACqviD7R5XyGBcwwV1YVru4uvSmz5Pgt7S9RxPRPkL");
const HYUSD_FEE_AUTHORITY: Pubkey = pubkey!("3HT6dD6APJh89XJs9rkn3BmsvkXE9jPG9dWJmUjWu6TS");
const USDC_VAULT: Pubkey = pubkey!("5ykRvDpEXwKEdoE6yfZ45zeuZ62CcRQRUoZoye4tTJd7");
const USDC_FEE_VAULT: Pubkey = pubkey!("Bz6Xsr7vTo2oSFxjmJVpoC4bVNGVpP5egbdVk4Jeu1JZ");
const HYUSD_FEE_VAULT: Pubkey = pubkey!("Hh8N3Fdauxgq1jjcKdzGBR3D8cdkpLZrFEVumL1tYQLp");
/// Pyth's USDC/USD price feed account, which the USDC pair's handlers read.
const USDC_ORACLE: Pubkey = pubkey!("6HAuqASbHEh4w4REJEUUUCginTLfj1kwCh215ZLtMkrT");
/// Pyth's SOL/USD price feed account, which HYLO records for the LST handlers.
const SOL_ORACLE: Pubkey = pubkey!("7AviUf9nL62mcxNbQGKm4nKDQnPjswo6c5MX4D57HmyE");
const EXCHANGE_EVENT_AUTHORITY: Pubkey = pubkey!("4VzpNE51Be5vD5Yg8MC3z6TVHq5gGbLJptjv18QbD6WP");

const POOL_CONFIG: Pubkey = pubkey!("2jk7miWrsTbt5hUSaCXPkEQPvuUMgbFLpgMzMQw3Z6ar");
const POOL_AUTHORITY: Pubkey = pubkey!("5YrRAQag9BbJkauDtJkd1vsTquXT6N46oU8rJ66GDxHd");
const HYUSD_POOL: Pubkey = pubkey!("EqozKyMj7FVnLHc2cJj3VC25aBr4AhVh1cGM2WDajGe9");
const SHYUSD_MINT_AUTHORITY: Pubkey = pubkey!("5YWerkcqAXTSCzKC1X52BXtfv2aoNCB6wzv7wEXuGWpq");
const STABILITY_EVENT_AUTHORITY: Pubkey = pubkey!("8fjUWoZTb8ox8JFRJTb7WznL1V8oJT9o21kQKHJzbTS8");

const HYLO_DISCRIMINATOR: [u8; 8] = [114, 161, 169, 210, 204, 175, 149, 174];
const USDC_PAIR_DISCRIMINATOR: [u8; 8] = [130, 97, 194, 78, 22, 254, 137, 107];
const POOL_CONFIG_DISCRIMINATOR: [u8; 8] = [26, 108, 14, 123, 116, 230, 129, 43];
const LST_HEADER_DISCRIMINATOR: [u8; 8] = [125, 135, 217, 151, 122, 202, 138, 59];
const EXO_PAIR_DISCRIMINATOR: [u8; 8] = [251, 244, 72, 181, 40, 119, 232, 48];
const HYLO_LEVER_MINT_OFFSET: usize = 136;
/// Both an LST header and an Exo pair record their collateral mint here.
const COLLATERAL_OFFSET: usize = 8;
const LST_HEADER_VAULT_OFFSET: usize = 40;
const EXO_PAIR_ORACLE_OFFSET: usize = 44;

/// One of Hylo's conversions: the account the executor quotes it from, the
/// mint it issues against, and the token account reserving that mint, or
/// the mint itself where no account holds it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HyloPool {
    pub(crate) pool: Pubkey,
    backing: Pubkey,
    reserve: Pubkey,
    kind: Kind,
}

#[derive(Debug, PartialEq, Eq)]
enum Kind {
    UsdcPair,
    StabilityPool,
    /// hyUSD or the collateral's levercoin against the collateral.
    Collateral(&'static Collateral, Coin),
    /// xSOL against hyUSD.
    LstConvert,
    /// An Exo levercoin against hyUSD.
    ExoConvert(&'static Collateral),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Coin {
    Stable,
    Lever,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Family {
    Lst,
    Exo,
}

/// An LST behind its header or an Exo collateral behind its pair, with the
/// block accounts its conversions list besides those every conversion of
/// the program shares.
#[derive(Debug, PartialEq, Eq)]
struct Collateral {
    /// The LST header or Exo pair.
    state: Pubkey,
    family: Family,
    mint: Pubkey,
    vault: Pubkey,
    vault_authority: Pubkey,
    fee_authority: Pubkey,
    fee_vault: Pubkey,
    /// The price feed the handlers read: [`SOL_ORACLE`] for an LST, and the
    /// feed the pair records for an Exo.
    oracle: Pubkey,
    /// [`XSOL`] for an LST, the pair's own levercoin for an Exo.
    lever: Pubkey,
    lever_authority: Pubkey,
}

/// The USDC pair's executor block after the program and declared base mint,
/// and whether each account is writable.
const USDC_PAIR_BLOCK: [(Pubkey, bool); 12] = [
    (USDC_PAIR, true),
    (HYLO, false),
    (HYUSD_MINT_AUTHORITY, false),
    (USDC_VAULT_AUTHORITY, false),
    (USDC_FEE_AUTHORITY, false),
    (HYUSD_FEE_AUTHORITY, false),
    (USDC_VAULT, true),
    (USDC_FEE_VAULT, true),
    (HYUSD_FEE_VAULT, true),
    (HYUSD, true),
    (USDC_ORACLE, false),
    (EXCHANGE_EVENT_AUTHORITY, false),
];

/// The Stability Pool's executor block. Deposits only read the pool config;
/// withdrawals write it.
const STABILITY_POOL_BLOCK: [(Pubkey, bool); 9] = [
    (POOL_CONFIG, true),
    (HYLO, false),
    (POOL_AUTHORITY, false),
    (HYUSD_POOL, true),
    (SHYUSD_MINT_AUTHORITY, false),
    (HYUSD_FEE_AUTHORITY, false),
    (HYUSD_FEE_VAULT, true),
    (SHYUSD, true),
    (STABILITY_EVENT_AUTHORITY, false),
];

/// xSOL's conversions' executor block. Both mints are writable because the
/// basket lists a group's mints read-only.
const LST_CONVERT_BLOCK: [(Pubkey, bool); 9] = [
    (HYLO, true),
    (SOL_ORACLE, false),
    (HYUSD_MINT_AUTHORITY, false),
    (XSOL_MINT_AUTHORITY, false),
    (HYUSD_FEE_AUTHORITY, false),
    (HYUSD_FEE_VAULT, true),
    (HYUSD, true),
    (XSOL, true),
    (EXCHANGE_EVENT_AUTHORITY, false),
];

const HYLO_USDC_PAIR: HyloPool = HyloPool {
    pool: USDC_PAIR,
    backing: USDC,
    reserve: USDC_VAULT,
    kind: Kind::UsdcPair,
};

const HYLO_STABILITY_POOL: HyloPool = HyloPool {
    pool: POOL_CONFIG,
    backing: HYUSD,
    reserve: HYUSD_POOL,
    kind: Kind::StabilityPool,
};

const JITOSOL: Collateral = Collateral {
    state: pubkey!("8Ri52tZXZehgAHKbx1MQiXhWXXkVsvAL9op6C5HytDKF"),
    family: Family::Lst,
    mint: pubkey!("J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn"),
    vault: pubkey!("2Y3TLkdGoJwbdizxqrZmQwNLYJyGKTgzC4tbetbkvQ43"),
    vault_authority: pubkey!("82MNhUCha26wY4kohTUEC965b4ypEe7RPa4itp9UMrKK"),
    fee_authority: pubkey!("FpLaqELxKRm6S3bjfNSknwZu43TL89VYkwuMDwsRMj59"),
    fee_vault: pubkey!("3JENUTyYnMMtZUSg5ErSHEvowjQteYD7wr7RDNw12bei"),
    oracle: SOL_ORACLE,
    lever: XSOL,
    lever_authority: XSOL_MINT_AUTHORITY,
};

const HYLOSOL: Collateral = Collateral {
    state: pubkey!("GMC2Q4ukPjiMjnhmxpZHDThQ4znC8oBAdveL62Tcu9rK"),
    family: Family::Lst,
    mint: pubkey!("hy1oXYgrBW6PVcJ4s6s2FKavRdwgWTXdfE69AxT7kPT"),
    vault: pubkey!("7VNBQCDKt4cxLWW51suV8a6VAYC4R66CfyySiYJek7Rj"),
    vault_authority: pubkey!("FD4gzYopoeeQYzYiyV62VjCRkyYD36uoVaRnZGE9kwZF"),
    fee_authority: pubkey!("925PhdF3ZXqEEWvgnSQDSSHZVoS3rhMLEfBot2cWmgpu"),
    fee_vault: pubkey!("CZbazc6YTRC9QyvxqPJpmerChyuzEHAdX854CB7PbQGb"),
    oracle: SOL_ORACLE,
    lever: XSOL,
    lever_authority: XSOL_MINT_AUTHORITY,
};

const CBBTC: Collateral = Collateral {
    state: pubkey!("8mgw2TsNxTMndWPyswLELw3V2tPrPvtqS7Ex9RyiGhML"),
    family: Family::Exo,
    mint: pubkey!("cbbtcf3aa214zXHbiAZQwf4122FBYbraNdFqgw4iMij"),
    vault: pubkey!("Hrdm1Q417g1kChwzZbdV1KhyPVGgzWvKnCSdafpqXNy"),
    vault_authority: pubkey!("7mxHjVnX6BdWo9Zv4t67ZSKkqpe98PNXV9Afgi9n8Cmy"),
    fee_authority: pubkey!("BqhiD7AdKeYfR6mex2zYhhheoEXpPTfsYG6vnTJ9ERk2"),
    fee_vault: pubkey!("oacVCceELzvyK944xDJaMGhVKzwkaY9yBHw2uifhLJ8"),
    oracle: pubkey!("APgzQGGdv2qCgBkX6aHVkrGePtBVDDg68GiqaM7rmtf5"),
    lever: pubkey!("2zCo6bUowJMvr89ajxuWsPadAqJ2F9akCkxumNsSdgsL"),
    lever_authority: pubkey!("UPC3Je1Ee93eX4XYzxzupoyBzDAEPyHADczktf5WUTs"),
};

const HYPE: Collateral = Collateral {
    state: pubkey!("42GzNWvZ1H1hwXaBZ8mbeVZaSEgh6zMm8BABdS3v1BEB"),
    family: Family::Exo,
    mint: pubkey!("98sMhvDwXj1RQi5c5Mndm3vPe9cBqPrbLaufMXFNMh5g"),
    vault: pubkey!("FWPLy9vhx2VRto91bcqh7f8xSUd285Sz2N12H6JC29rr"),
    vault_authority: pubkey!("GwxovvRAGWvhpmveKmFLZeojeyfzDG3FcBTgAeWnVW1Z"),
    fee_authority: pubkey!("8Xrf6qAvuH3kXfVWJGT49aBLSEUGHyG8ETuvvnu5VRSr"),
    fee_vault: pubkey!("DHZ2JuDXcmuTnkVfmx4fYKk6sStqdW9oBTjoPAhtKF37"),
    oracle: pubkey!("9dAoWJ5ua81c43ntstN4xyg3Uo9Zt93x7BjCxSc27V6V"),
    lever: pubkey!("7ga6rtE9qSb3wdEiDCpTu2kHqoGVfT52jD8ign1rYTvx"),
    lever_authority: pubkey!("BKdLWcUDE1Kmga2BfSNaMR2wbXSL4YouTbNQ8mTdcqTp"),
};

const ZEC: Collateral = Collateral {
    state: pubkey!("Atssq1iqXhUrPX1qV9e2UyQCWdeAd6DXQrccK9eu3Bc"),
    family: Family::Exo,
    mint: pubkey!("A7bdiYdS5GjqGFtxf17ppRHtDKPkkRqbKtR27dxvQXaS"),
    vault: pubkey!("AHF55B4PYC3YFtzsKdaAPefqm4fdaxL3fyRJFAXQKqTW"),
    vault_authority: pubkey!("GSdzTNg5iSuBcu5qHGXKWs9FTGhjProZ1rsrEXtDrwPj"),
    fee_authority: pubkey!("Fw5s3m7RNBBZgdoR6qv3rMVAGm2BVJviWnY7ifoajjCR"),
    fee_vault: pubkey!("BBrDawyruEpaknpHHgWAJZf2F3BY4iQzCa1txDLUZF8S"),
    oracle: pubkey!("J6PtzWsBtLcuMM7cFS86XVRH4vQfDMcGmBUzSRCMrWMa"),
    lever: pubkey!("FgLfLhT3QoPWynZ53L3KUUsPhspWbdPUfMtmPfezXci6"),
    lever_authority: pubkey!("GEwgNdd1wzAGNSePyM1Duaq3tPJTKuNiZLmxN6CBh94a"),
};

const fn collateral(collateral: &'static Collateral, coin: Coin) -> HyloPool {
    HyloPool {
        pool: collateral.state,
        backing: collateral.mint,
        reserve: collateral.vault,
        kind: Kind::Collateral(collateral, coin),
    }
}

/// Issued against hyUSD, which no account holds. Only an Exo pair's
/// levercoin converts against its own pair; xSOL converts at `Hylo`.
const fn exo_convert(collateral: &'static Collateral) -> HyloPool {
    assert!(matches!(collateral.family, Family::Exo));
    HyloPool {
        pool: collateral.state,
        backing: HYUSD,
        reserve: HYUSD,
        kind: Kind::ExoConvert(collateral),
    }
}

const HYLO_LST_CONVERT: HyloPool = HyloPool {
    pool: HYLO,
    backing: HYUSD,
    reserve: HYUSD,
    kind: Kind::LstConvert,
};

static HYLO_POOLS: [HyloPool; 16] = [
    HYLO_USDC_PAIR,
    HYLO_STABILITY_POOL,
    collateral(&JITOSOL, Coin::Stable),
    collateral(&HYLOSOL, Coin::Stable),
    collateral(&CBBTC, Coin::Stable),
    collateral(&HYPE, Coin::Stable),
    collateral(&ZEC, Coin::Stable),
    collateral(&JITOSOL, Coin::Lever),
    collateral(&HYLOSOL, Coin::Lever),
    collateral(&CBBTC, Coin::Lever),
    collateral(&HYPE, Coin::Lever),
    collateral(&ZEC, Coin::Lever),
    HYLO_LST_CONVERT,
    exo_convert(&CBBTC),
    exo_convert(&HYPE),
    exo_convert(&ZEC),
];

impl HyloPool {
    /// Every conversion whose `pool` is an account of `program` holding
    /// `data`.
    pub(crate) fn conversions<'a>(
        program: &'a Pubkey,
        data: &'a [u8],
    ) -> impl Iterator<Item = &'static Self> + 'a {
        HYLO_POOLS
            .iter()
            .filter(move |pool| pool.program() == *program && pool.describes(data))
    }

    /// The conversion trading `mints` either way round whose `pool` is an
    /// account of `program` holding `data`.
    pub(crate) fn read(program: &Pubkey, data: &[u8], mints: [Pubkey; 2]) -> Result<&'static Self> {
        let Some(pool) = Self::conversions(program, data).find(|pool| pool.trades(mints)) else {
            bail!("not the pool of a Hylo conversion the executor trades between these mints");
        };
        Ok(pool)
    }

    fn trades(&self, [a, b]: [Pubkey; 2]) -> bool {
        let pair = [self.issued(), self.backing];
        pair == [a, b] || pair == [b, a]
    }

    /// Whether `data` is this conversion's `pool`. `Hylo` records xSOL, and
    /// a header or pair its collateral mint, which tells it from the others,
    /// and its LST vault or Exo oracle, which catches a block whose account
    /// Hylo replaced. Only the executor checks the SOL/USD feed `Hylo`
    /// records for the LSTs.
    fn describes(&self, data: &[u8]) -> bool {
        match self.kind {
            Kind::UsdcPair => records(data, USDC_PAIR_DISCRIMINATOR, &[]),
            Kind::StabilityPool => records(data, POOL_CONFIG_DISCRIMINATOR, &[]),
            Kind::LstConvert => {
                records(data, HYLO_DISCRIMINATOR, &[(HYLO_LEVER_MINT_OFFSET, &XSOL)])
            }
            Kind::Collateral(c, _) | Kind::ExoConvert(c) => match c.family {
                Family::Exo => records(
                    data,
                    EXO_PAIR_DISCRIMINATOR,
                    &[
                        (COLLATERAL_OFFSET, &c.mint),
                        (EXO_PAIR_ORACLE_OFFSET, &c.oracle),
                    ],
                ),
                Family::Lst => records(
                    data,
                    LST_HEADER_DISCRIMINATOR,
                    &[
                        (COLLATERAL_OFFSET, &c.mint),
                        (LST_HEADER_VAULT_OFFSET, &c.vault),
                    ],
                ),
            },
        }
    }

    /// The program whose handlers perform the conversion.
    pub(crate) fn program(&self) -> Pubkey {
        match self.kind {
            Kind::StabilityPool => HYLO_STABILITY,
            _ => HYLO_EXCHANGE,
        }
    }

    /// The mint the conversion issues against its backing.
    pub(crate) fn issued(&self) -> Pubkey {
        match self.kind {
            Kind::StabilityPool => SHYUSD,
            Kind::UsdcPair | Kind::Collateral(_, Coin::Stable) => HYUSD,
            Kind::Collateral(c, Coin::Lever) | Kind::ExoConvert(c) => c.lever,
            Kind::LstConvert => XSOL,
        }
    }

    pub(crate) fn venue(&self) -> Venue {
        match self.kind {
            Kind::UsdcPair => Venue::HyloUsdc,
            Kind::StabilityPool => Venue::HyloStability,
            Kind::Collateral(c, coin) => match (c.family, coin) {
                (Family::Lst, Coin::Stable) => Venue::HyloLst,
                (Family::Exo, Coin::Stable) => Venue::HyloExo,
                (Family::Lst, Coin::Lever) => Venue::HyloLstLever,
                (Family::Exo, Coin::Lever) => Venue::HyloExoLever,
            },
            Kind::LstConvert => Venue::HyloLstConvert,
            Kind::ExoConvert(_) => Venue::HyloExoConvert,
        }
    }

    /// The mints `[issued, backing]`, whose vaults are the issued mint itself
    /// and the reserve.
    pub(crate) fn pair(&self) -> PoolPair {
        let issued = self.issued();
        PoolPair::issued([issued, self.backing], [issued, self.reserve])
    }

    /// The executor's block after the program and declared base mint.
    pub(crate) fn accounts(&self) -> impl Iterator<Item = AccountMeta> {
        let block = match self.kind {
            Kind::UsdcPair => USDC_PAIR_BLOCK.to_vec(),
            Kind::StabilityPool => STABILITY_POOL_BLOCK.to_vec(),
            Kind::Collateral(c, coin) => self.collateral_block(c, coin),
            Kind::LstConvert => LST_CONVERT_BLOCK.to_vec(),
            Kind::ExoConvert(c) => exo_convert_block(c),
        };
        block.into_iter().map(|(key, writable)| {
            if writable {
                AccountMeta::new(key, false)
            } else {
                AccountMeta::new_readonly(key, false)
            }
        })
    }

    /// The handlers' shared order for both coins, with the coin's mint
    /// authority and mint.
    fn collateral_block(&self, c: &Collateral, coin: Coin) -> Vec<(Pubkey, bool)> {
        // The executor quotes an LST's conversions from the state `Hylo`
        // holds, which their trades change, and an Exo's from its pair.
        let lst = c.family == Family::Lst;
        let mint_authority = match coin {
            Coin::Stable => HYUSD_MINT_AUTHORITY,
            Coin::Lever => c.lever_authority,
        };
        vec![
            (c.state, !lst),
            (HYLO, lst),
            (mint_authority, false),
            (c.vault_authority, false),
            (c.fee_authority, false),
            (c.vault, true),
            (c.fee_vault, true),
            (self.issued(), true),
            (c.oracle, false),
            (EXCHANGE_EVENT_AUTHORITY, false),
        ]
    }
}

/// An Exo levercoin's conversions' block, which values the pair's vault
/// and charges its fee in hyUSD.
fn exo_convert_block(c: &Collateral) -> Vec<(Pubkey, bool)> {
    vec![
        (HYLO, false),
        (c.state, true),
        (c.lever_authority, false),
        (HYUSD_MINT_AUTHORITY, false),
        (c.vault_authority, false),
        (HYUSD_FEE_AUTHORITY, false),
        (c.vault, false),
        (HYUSD_FEE_VAULT, true),
        (HYUSD, true),
        (c.lever, true),
        (c.mint, false),
        (c.oracle, false),
        (EXCHANGE_EVENT_AUTHORITY, false),
    ]
}

/// Whether `data` has `discriminator` and records each key at its offset.
fn records(data: &[u8], discriminator: [u8; 8], recorded: &[(usize, &Pubkey)]) -> bool {
    data.get(..8) == Some(&discriminator[..])
        && recorded
            .iter()
            .all(|&(offset, key)| data.get(offset..offset + 32) == Some(key.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v10::{MintAccounts, PoolAccounts, TOKEN, TOKEN_2022};

    /// An account with `discriminator` recording each key at its offset.
    fn account(discriminator: [u8; 8], recorded: &[(usize, Pubkey)]) -> Vec<u8> {
        let mut data = vec![0u8; 200];
        data[..8].copy_from_slice(&discriminator);
        for &(offset, key) in recorded {
            data[offset..offset + 32].copy_from_slice(key.as_ref());
        }
        data
    }

    fn usdc_pair() -> Vec<u8> {
        account(USDC_PAIR_DISCRIMINATOR, &[])
    }

    fn pool_config() -> Vec<u8> {
        account(POOL_CONFIG_DISCRIMINATOR, &[])
    }

    fn hylo() -> Vec<u8> {
        account(HYLO_DISCRIMINATOR, &[(136, XSOL)])
    }

    fn jitosol_header() -> Vec<u8> {
        account(
            LST_HEADER_DISCRIMINATOR,
            &[(8, JITOSOL.mint), (40, JITOSOL.vault)],
        )
    }

    fn cbbtc_pair() -> Vec<u8> {
        account(
            EXO_PAIR_DISCRIMINATOR,
            &[(8, CBBTC.mint), (44, CBBTC.oracle)],
        )
    }

    fn venues(program: Pubkey, data: &[u8]) -> Vec<Venue> {
        HyloPool::conversions(&program, data)
            .map(HyloPool::venue)
            .collect()
    }

    #[test]
    fn each_account_pools_the_conversions_its_mints_tell_apart() {
        let exchange = HYLO_EXCHANGE;
        assert_eq!(venues(exchange, &usdc_pair()), [Venue::HyloUsdc]);
        assert_eq!(
            venues(HYLO_STABILITY, &pool_config()),
            [Venue::HyloStability]
        );
        // The pool config belongs to the Stability Pool's program.
        assert_eq!(venues(exchange, &pool_config()), []);
        assert_eq!(venues(exchange, &hylo()), [Venue::HyloLstConvert]);
        assert_eq!(
            venues(exchange, &jitosol_header()),
            [Venue::HyloLst, Venue::HyloLstLever]
        );
        assert_eq!(
            venues(exchange, &cbbtc_pair()),
            [Venue::HyloExo, Venue::HyloExoLever, Venue::HyloExoConvert]
        );
        // A header naming another vault, or a pair another oracle, is one
        // whose account Hylo replaced.
        let moved = account(
            LST_HEADER_DISCRIMINATOR,
            &[(8, JITOSOL.mint), (40, Pubkey::new_unique())],
        );
        assert_eq!(venues(exchange, &moved), []);
        let repriced = account(
            EXO_PAIR_DISCRIMINATOR,
            &[(8, CBBTC.mint), (44, Pubkey::new_unique())],
        );
        assert_eq!(venues(exchange, &repriced), []);
        assert_eq!(venues(exchange, &account(HYLO_DISCRIMINATOR, &[])), []);
        let mut other = usdc_pair();
        other[0] ^= 1;
        assert_eq!(venues(exchange, &other), []);
    }

    #[test]
    fn a_pool_pair_is_read_only_from_an_account_of_one_conversion() -> Result<()> {
        let mints = |program, data: &[u8]| PoolPair::read(program, data).map(|pair| pair.mints);
        assert_eq!(mints(HYLO_EXCHANGE, &usdc_pair())?, [HYUSD, USDC]);
        assert_eq!(mints(HYLO_STABILITY, &pool_config())?, [SHYUSD, HYUSD]);
        assert_eq!(mints(HYLO_EXCHANGE, &hylo())?, [XSOL, HYUSD]);
        let error = |data: &[u8]| mints(HYLO_EXCHANGE, data).unwrap_err().to_string();
        let several = "not the pool of exactly one conversion the executor trades";
        assert_eq!(error(&jitosol_header()), several);
        assert_eq!(error(&cbbtc_pair()), several);
        assert_eq!(
            error(&pool_config()),
            "not the pool of a Hylo conversion the executor trades"
        );
        Ok(())
    }

    #[test]
    fn an_lst_conversion_writes_hylo_and_reads_its_header() -> Result<()> {
        let wallet = Pubkey::new_unique();
        let mint = |mint| MintAccounts::ata(&wallet, mint, TOKEN);
        let (jitosol, hyusd, xsol) = (mint(JITOSOL.mint)?, mint(HYUSD)?, mint(XSOL)?);
        let header = jitosol_header();
        let load = |x: MintAccounts, base: MintAccounts| {
            PoolAccounts::from_state(JITOSOL.state, HYLO_EXCHANGE, &header, x, base, &[], None)
        };
        let listed = |pool: &PoolAccounts| -> Vec<_> {
            pool.accounts()
                .iter()
                .map(|meta| (meta.pubkey, meta.is_writable))
                .collect()
        };
        let block = |base: Pubkey, mint_authority: Pubkey, issued: Pubkey| {
            vec![
                (HYLO_EXCHANGE, false),
                (base, false),
                (JITOSOL.state, false),
                (HYLO, true),
                (mint_authority, false),
                (JITOSOL.vault_authority, false),
                (JITOSOL.fee_authority, false),
                (JITOSOL.vault, true),
                (JITOSOL.fee_vault, true),
                (issued, true),
                (SOL_ORACLE, false),
                (EXCHANGE_EVENT_AUTHORITY, false),
            ]
        };
        for (x, base) in [(hyusd, jitosol), (jitosol, hyusd)] {
            let pool = load(x, base)?;
            assert_eq!(pool.venue(), Venue::HyloLst);
            assert_eq!(listed(&pool), block(base.mint, HYUSD_MINT_AUTHORITY, HYUSD));
        }
        let lever = load(xsol, jitosol)?;
        assert_eq!(lever.venue(), Venue::HyloLstLever);
        assert_eq!(
            listed(&lever),
            block(jitosol.mint, XSOL_MINT_AUTHORITY, XSOL)
        );

        // Another account, mints the header does not pool, a Token-2022
        // account and listed arrays are refused.
        let error = |pool, x, base, arrays: &[Pubkey]| {
            PoolAccounts::from_state(pool, HYLO_EXCHANGE, &header, x, base, arrays, None)
                .unwrap_err()
                .to_string()
        };
        let other = Pubkey::new_unique();
        assert_eq!(
            error(other, hyusd, jitosol, &[]),
            format!("{other} is not Hylo's {}", JITOSOL.state)
        );
        for (x, base) in [(xsol, hyusd), (mint(USDC)?, hyusd)] {
            assert_eq!(
                error(JITOSOL.state, x, base, &[]),
                "not the pool of a Hylo conversion the executor trades between these mints"
            );
        }
        let token_2022 = MintAccounts::ata(&wallet, JITOSOL.mint, TOKEN_2022)?;
        assert_eq!(
            error(JITOSOL.state, hyusd, token_2022, &[]),
            "an issuer's mints are legacy SPL tokens"
        );
        assert_eq!(
            error(JITOSOL.state, hyusd, jitosol, &[Pubkey::new_unique()]),
            "pool does not use arrays"
        );
        Ok(())
    }

    /// The accounts a config lists for the conversions one account pools
    /// alone, each traded either way round: the program, the declared base
    /// mint and the conversion's block.
    #[test]
    fn a_single_conversion_pool_lists_its_block_after_its_base_mint() -> Result<()> {
        let wallet = Pubkey::new_unique();
        let mint = |mint| MintAccounts::ata(&wallet, mint, TOKEN);
        let (usdc, hyusd, shyusd, xsol) = (mint(USDC)?, mint(HYUSD)?, mint(SHYUSD)?, mint(XSOL)?);
        let cases = [
            (
                pubkey!("CMNPACEDebyvNJDgBxRc5fbScF8kmx52ZPBY4Cu4wuwS"),
                HYLO_EXCHANGE,
                usdc_pair(),
                [hyusd, usdc],
                Venue::HyloUsdc,
                vec![
                    (USDC_PAIR, true),
                    (HYLO, false),
                    (HYUSD_MINT_AUTHORITY, false),
                    (USDC_VAULT_AUTHORITY, false),
                    (USDC_FEE_AUTHORITY, false),
                    (HYUSD_FEE_AUTHORITY, false),
                    (USDC_VAULT, true),
                    (USDC_FEE_VAULT, true),
                    (HYUSD_FEE_VAULT, true),
                    (HYUSD, true),
                    (USDC_ORACLE, false),
                    (EXCHANGE_EVENT_AUTHORITY, false),
                ],
            ),
            (
                pubkey!("2jk7miWrsTbt5hUSaCXPkEQPvuUMgbFLpgMzMQw3Z6ar"),
                HYLO_STABILITY,
                pool_config(),
                [shyusd, hyusd],
                Venue::HyloStability,
                vec![
                    (POOL_CONFIG, true),
                    (HYLO, false),
                    (POOL_AUTHORITY, false),
                    (HYUSD_POOL, true),
                    (SHYUSD_MINT_AUTHORITY, false),
                    (HYUSD_FEE_AUTHORITY, false),
                    (HYUSD_FEE_VAULT, true),
                    (SHYUSD, true),
                    (STABILITY_EVENT_AUTHORITY, false),
                ],
            ),
            (
                pubkey!("9cd2sAfbBvKs4SX9YKo4dcjwP3TgTVQ8dT5koshGcDND"),
                HYLO_EXCHANGE,
                hylo(),
                [xsol, hyusd],
                Venue::HyloLstConvert,
                vec![
                    (HYLO, true),
                    (SOL_ORACLE, false),
                    (HYUSD_MINT_AUTHORITY, false),
                    (XSOL_MINT_AUTHORITY, false),
                    (HYUSD_FEE_AUTHORITY, false),
                    (HYUSD_FEE_VAULT, true),
                    (HYUSD, true),
                    (XSOL, true),
                    (EXCHANGE_EVENT_AUTHORITY, false),
                ],
            ),
        ];
        let mut listed = 0;
        for (pool, program, data, [issued, backing], venue, block) in &cases {
            for (x, base) in [(*issued, *backing), (*backing, *issued)] {
                let accounts = PoolAccounts::from_state(*pool, *program, data, x, base, &[], None)?;
                assert_eq!(accounts.venue(), *venue, "{pool}");
                let mut expected = vec![(*program, false), (base.mint, false)];
                expected.extend(block.iter().copied());
                let keys: Vec<_> = accounts
                    .accounts()
                    .iter()
                    .map(|meta| (meta.pubkey, meta.is_writable))
                    .collect();
                assert_eq!(keys, expected, "{pool} with base {}", base.mint);
                listed += 1;
            }
        }
        assert_eq!(listed, 6);
        Ok(())
    }

    /// An Exo pair's three conversions, each traded either way round: the
    /// collateral's hyUSD and levercoin write the pair, and the levercoin
    /// against hyUSD values the pair's vault read-only.
    #[test]
    fn an_exo_pair_lists_the_block_of_each_conversion_it_pools() -> Result<()> {
        let wallet = Pubkey::new_unique();
        let mint = |mint| MintAccounts::ata(&wallet, mint, TOKEN);
        let (cbbtc, hyusd, lever) = (mint(CBBTC.mint)?, mint(HYUSD)?, mint(CBBTC.lever)?);
        let collateral_block = |mint_authority: Pubkey, issued: Pubkey| {
            vec![
                (CBBTC.state, true),
                (HYLO, false),
                (mint_authority, false),
                (CBBTC.vault_authority, false),
                (CBBTC.fee_authority, false),
                (CBBTC.vault, true),
                (CBBTC.fee_vault, true),
                (issued, true),
                (CBBTC.oracle, false),
                (EXCHANGE_EVENT_AUTHORITY, false),
            ]
        };
        let cases = [
            (
                [hyusd, cbbtc],
                Venue::HyloExo,
                collateral_block(HYUSD_MINT_AUTHORITY, HYUSD),
            ),
            (
                [lever, cbbtc],
                Venue::HyloExoLever,
                collateral_block(CBBTC.lever_authority, CBBTC.lever),
            ),
            (
                [lever, hyusd],
                Venue::HyloExoConvert,
                vec![
                    (HYLO, false),
                    (CBBTC.state, true),
                    (CBBTC.lever_authority, false),
                    (HYUSD_MINT_AUTHORITY, false),
                    (CBBTC.vault_authority, false),
                    (HYUSD_FEE_AUTHORITY, false),
                    (CBBTC.vault, false),
                    (HYUSD_FEE_VAULT, true),
                    (HYUSD, true),
                    (CBBTC.lever, true),
                    (CBBTC.mint, false),
                    (CBBTC.oracle, false),
                    (EXCHANGE_EVENT_AUTHORITY, false),
                ],
            ),
        ];
        let pair = cbbtc_pair();
        let mut listed = 0;
        for ([issued, backing], venue, block) in &cases {
            for (x, base) in [(*issued, *backing), (*backing, *issued)] {
                let accounts = PoolAccounts::from_state(
                    CBBTC.state,
                    HYLO_EXCHANGE,
                    &pair,
                    x,
                    base,
                    &[],
                    None,
                )?;
                assert_eq!(accounts.venue(), *venue);
                let mut expected = vec![(HYLO_EXCHANGE, false), (base.mint, false)];
                expected.extend(block.iter().copied());
                let keys: Vec<_> = accounts
                    .accounts()
                    .iter()
                    .map(|meta| (meta.pubkey, meta.is_writable))
                    .collect();
                assert_eq!(keys, expected, "{venue:?} with base {}", base.mint);
                listed += 1;
            }
        }
        assert_eq!(listed, 6);
        Ok(())
    }

    /// Each fixed account the programs derive, from its seeds, and each
    /// vault as its authority's associated token account. The price feeds
    /// and the mints are not derived.
    #[test]
    fn the_fixed_accounts_are_the_programs_derived_addresses() {
        let exchange = |seeds: &[&[u8]]| Pubkey::find_program_address(seeds, &HYLO_EXCHANGE).0;
        let stability = |seeds: &[&[u8]]| Pubkey::find_program_address(seeds, &HYLO_STABILITY).0;
        let ata = |owner: Pubkey, mint: Pubkey| {
            let seeds = [owner.as_ref(), TOKEN.as_ref(), mint.as_ref()];
            Pubkey::find_program_address(&seeds, &crate::v10::ASSOCIATED).0
        };
        let mint_auth = |mint: Pubkey| exchange(&[b"mint_auth", mint.as_ref()]);
        let fee_auth = |mint: Pubkey| exchange(&[b"fee_auth", mint.as_ref()]);
        let usdc_vault_auth = exchange(&[b"usdc_vault_auth", USDC.as_ref()]);
        let pool_auth = stability(&[b"pool_auth"]);
        let fixed = [
            ("Hylo", HYLO, exchange(&[b"hylo"])),
            ("USDC pair", USDC_PAIR, exchange(&[b"usdc_pair"])),
            (
                "exchange event authority",
                EXCHANGE_EVENT_AUTHORITY,
                exchange(&[b"__event_authority"]),
            ),
            (
                "hyUSD mint authority",
                HYUSD_MINT_AUTHORITY,
                mint_auth(HYUSD),
            ),
            ("xSOL mint authority", XSOL_MINT_AUTHORITY, mint_auth(XSOL)),
            (
                "USDC vault authority",
                USDC_VAULT_AUTHORITY,
                usdc_vault_auth,
            ),
            ("USDC fee authority", USDC_FEE_AUTHORITY, fee_auth(USDC)),
            ("hyUSD fee authority", HYUSD_FEE_AUTHORITY, fee_auth(HYUSD)),
            ("USDC vault", USDC_VAULT, ata(usdc_vault_auth, USDC)),
            ("USDC fee vault", USDC_FEE_VAULT, ata(fee_auth(USDC), USDC)),
            (
                "hyUSD fee vault",
                HYUSD_FEE_VAULT,
                ata(fee_auth(HYUSD), HYUSD),
            ),
            ("pool config", POOL_CONFIG, stability(&[b"pool_config"])),
            ("pool authority", POOL_AUTHORITY, pool_auth),
            ("hyUSD pool", HYUSD_POOL, ata(pool_auth, HYUSD)),
            (
                "sHYUSD mint authority",
                SHYUSD_MINT_AUTHORITY,
                stability(&[b"mint_auth", SHYUSD.as_ref()]),
            ),
            (
                "stability event authority",
                STABILITY_EVENT_AUTHORITY,
                stability(&[b"__event_authority"]),
            ),
        ];
        for (name, constant, derived) in fixed {
            assert_eq!(constant, derived, "{name}");
        }
        for c in [&JITOSOL, &HYLOSOL, &CBBTC, &HYPE, &ZEC] {
            let (state_seed, vault_auth_seed): (&[u8], &[u8]) = match c.family {
                Family::Lst => (b"lst_header", b"vault_auth"),
                Family::Exo => (b"exo_pair", b"exo_vault_auth"),
            };
            let mint = c.mint.as_ref();
            let vault_authority = exchange(&[vault_auth_seed, mint]);
            let expected = Collateral {
                state: exchange(&[state_seed, mint]),
                vault: ata(vault_authority, c.mint),
                vault_authority,
                fee_authority: fee_auth(c.mint),
                fee_vault: ata(fee_auth(c.mint), c.mint),
                lever_authority: mint_auth(c.lever),
                ..*c
            };
            assert_eq!(*c, expected, "{}", c.mint);
        }
    }
}
