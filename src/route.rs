//! Configured transactions: each pool's role checked against its mints at
//! startup, then the V10 instruction built from fresh chain state.
use crate::archer;
use crate::config::{DirectConfig, TransactionConfig, TriangleConfig};
use crate::loader::AccountLoader;
use crate::v10::{
    self as abi, ConversionAccounts, DirectGroup, MarketGroup, MintAccounts, PoolAccounts,
};
use anyhow::{bail, ensure, Context, Result};
use executor_v10_abi::cost::{
    each_route_pools, transaction_cu, BasketGroup, BasketPool, DEFAULT_CONVERTER,
};
use executor_v10_abi::MAX_GROUPS;
use solana_sdk::{instruction::Instruction, pubkey, pubkey::Pubkey};
use std::fmt;
use tracing::warn;

/// The on-chain arbitrage program.
pub const EXECUTOR: Pubkey = pubkey!("MEViEnscUm6tsQRoGd9h6nLQaQspKj7DB2M5FwM3Xvz");

/// A configured transaction whose pools trade the mints their roles need.
pub struct Route {
    index: usize,
    settlement: Pubkey,
    header: abi::Header,
    additional_fee_collector: Option<Pubkey>,
    groups: Groups<Direct, Triangle>,
}

/// The program trades either direct groups or triangle groups in one
/// instruction, never both: `Direct` and `Triangle` as configured, then
/// `DirectGroup` and `MarketGroup` as loaded.
enum Groups<D, T> {
    Direct(Vec<D>),
    Triangle(Vec<T>),
}

impl<D, T> Groups<D, T> {
    fn len(&self) -> usize {
        match self {
            Self::Direct(groups) => groups.len(),
            Self::Triangle(groups) => groups.len(),
        }
    }
}

/// A configured group, loaded into the instruction builder's form.
trait Group {
    type Loaded;
    fn target(&self) -> Pubkey;
    fn load(
        &self,
        loader: &mut AccountLoader,
        settlement: MintAccounts,
        wallet: Pubkey,
    ) -> Result<Self::Loaded>;
}

/// Loads each group, leaving out any that cannot trade.
fn load_groups<G: Group>(
    groups: &[G],
    loader: &mut AccountLoader,
    settlement: MintAccounts,
    wallet: Pubkey,
) -> Vec<G::Loaded> {
    groups
        .iter()
        .filter_map(|group| {
            group
                .load(loader, settlement, wallet)
                .map_err(|e| warn!("Skipping group for target {}: {e:#}", group.target()))
                .ok()
        })
        .collect()
}

/// Settlement -> target -> settlement (2 hops), or through the SOL/USDC
/// conversion when a pool is quoted in the other mint (3 hops).
struct Direct {
    target: Pubkey,
    pools: Vec<Pubkey>,
}

/// Settlement -> target -> stock -> quote, then quote -> settlement through
/// the SOL/USDC conversion when the quote is the other mint (3 or 4 hops).
struct Triangle {
    target: Pubkey,
    stock: Pubkey,
    quote: Pubkey,
    intermediate: Pubkey,
    bridges: Vec<Pubkey>,
    direct: Vec<Pubkey>,
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "transaction #{}", self.index)
    }
}

fn is_quote(mint: &Pubkey) -> bool {
    [abi::SOL, abi::USDC].contains(mint)
}

/// The other of SOL and USDC, which trades into `settlement` through the
/// Raydium SOL/USDC pool.
fn other_quote(settlement: Pubkey) -> Pubkey {
    if settlement == abi::SOL {
        abi::USDC
    } else {
        abi::SOL
    }
}

/// A pool's two mints.
fn mints(loader: &mut AccountLoader, pool: Pubkey) -> Result<[Pubkey; 2]> {
    let account = loader.get(pool)?;
    Ok(abi::PoolPair::read(account.owner, &account.data)
        .with_context(|| format!("pool {pool}"))?
        .mints)
}

/// The mint `pool` trades `mint` against.
fn counterpart(loader: &mut AccountLoader, pool: Pubkey, mint: Pubkey) -> Result<Pubkey> {
    match mints(loader, pool)? {
        [a, b] if a == mint => Ok(b),
        [a, b] if b == mint => Ok(a),
        _ => bail!("pool {pool} does not trade {mint}"),
    }
}

/// The one mint every pool trades `mint` against.
fn common_counterpart(
    loader: &mut AccountLoader,
    pools: &[Pubkey],
    mint: Pubkey,
) -> Result<Pubkey> {
    let mut common = None;
    for &pool in pools {
        let other = counterpart(loader, pool, mint)?;
        let expected = *common.get_or_insert(other);
        ensure!(
            expected == other,
            "pool {pool} trades {mint} against {other}, not {expected}"
        );
    }
    common.context("no pools")
}

impl Route {
    /// Checks every pool's mints against its role, so configuration errors
    /// surface at startup.
    pub fn resolve(
        index: usize,
        config: &TransactionConfig,
        loader: &mut AccountLoader,
    ) -> Result<Self> {
        let settlement = config.settlement.mint();
        let groups = match (config.direct.is_empty(), config.triangle.is_empty()) {
            (false, true) => Groups::Direct(
                config
                    .direct
                    .iter()
                    .map(|group| Direct::resolve(group, settlement, loader))
                    .collect::<Result<_>>()?,
            ),
            (true, false) => {
                let groups: Vec<Triangle> = config
                    .triangle
                    .iter()
                    .map(|group| Triangle::resolve(group, settlement, loader))
                    .collect::<Result<_>>()?;
                ensure!(
                    groups.iter().all(|g| g.quote == groups[0].quote),
                    "every triangle's bridges must be quoted in the same mint"
                );
                Groups::Triangle(groups)
            }
            _ => bail!("configure either direct or triangle groups"),
        };
        ensure!(
            groups.len() <= MAX_GROUPS,
            "at most {MAX_GROUPS} groups per transaction"
        );
        let additional_fee = config.additional_fee.as_ref();
        Ok(Self {
            index,
            settlement,
            header: abi::Header {
                minimum_profit: config.minimum_profit,
                compute_unit_limit: 0,
                no_failure: config.no_failure,
                additional_fee_bp: additional_fee.map_or(0, |fee| fee.bps),
                use_flashloan: config.flashloan,
                trade_size: config.trade_size,
                constant_conversion: config.constant_conversion,
            },
            additional_fee_collector: additional_fee.map(|fee| fee.collector),
            groups,
        })
    }

    pub fn targets(&self) -> Vec<Pubkey> {
        match &self.groups {
            Groups::Direct(groups) => groups.iter().map(Group::target).collect(),
            Groups::Triangle(groups) => groups.iter().map(Group::target).collect(),
        }
    }

    /// Loads every pool from fresh state. A pool that does not load, such as
    /// a full Manifest market, is left out, and so is a group left unable to
    /// trade.
    pub fn load(&self, loader: &mut AccountLoader, wallet: Pubkey) -> Result<Basket> {
        let settlement = loader.mint(wallet, self.settlement)?;
        let groups = match &self.groups {
            Groups::Direct(groups) => {
                Groups::Direct(load_groups(groups, loader, settlement, wallet))
            }
            Groups::Triangle(groups) => {
                Groups::Triangle(load_groups(groups, loader, settlement, wallet))
            }
        };
        ensure!(groups.len() > 0, "no group can trade");
        let other = other_quote(settlement.mint);
        let converts = groups
            .shapes()
            .iter()
            .flat_map(|shape| &shape.pools)
            .any(|pool| pool.base_mint() == other);
        let conversion = converts
            .then(|| conversion_accounts(loader, settlement, wallet))
            .transpose()?;
        Ok(Basket {
            wallet,
            settlement,
            header: abi::Header {
                constant_conversion: self.header.constant_conversion && conversion.is_some(),
                ..self.header
            },
            additional_fee_collector: self.additional_fee_collector,
            conversion,
            groups,
        })
    }
}

/// Loads each pool, leaving out any that do not load.
fn load_pools(
    loader: &mut AccountLoader,
    pools: &[Pubkey],
    mut load: impl FnMut(&mut AccountLoader, Pubkey) -> Result<PoolAccounts>,
) -> Vec<PoolAccounts> {
    pools
        .iter()
        .filter_map(|&pool| {
            load(loader, pool)
                .map_err(|e| warn!("Skipping pool {pool}: {e:#}"))
                .ok()
        })
        .collect()
}

impl Direct {
    fn resolve(
        config: &DirectConfig,
        settlement: Pubkey,
        loader: &mut AccountLoader,
    ) -> Result<Self> {
        ensure!(config.pools.len() >= 2, "a direct group needs two pools");
        let mut target = None;
        let mut settled = false;
        for &pool in &config.pools {
            let (pool_target, quote) = match mints(loader, pool)? {
                [a, b] if is_quote(&a) && is_quote(&b) => {
                    bail!("pool {pool} trades SOL against USDC, which is the conversion")
                }
                [a, b] if is_quote(&b) => (a, b),
                [a, b] if is_quote(&a) => (b, a),
                _ => bail!("pool {pool} does not trade against SOL or USDC"),
            };
            let expected = *target.get_or_insert(pool_target);
            ensure!(
                expected == pool_target,
                "pool {pool} trades {pool_target}, not the group's target {expected}"
            );
            settled |= quote == settlement;
        }
        ensure!(
            settled,
            "a direct group needs a pool quoted in {settlement}"
        );
        Ok(Self {
            target: target.context("no pools")?,
            pools: config.pools.clone(),
        })
    }
}

impl Group for Direct {
    type Loaded = DirectGroup;

    fn target(&self) -> Pubkey {
        self.target
    }

    fn load(
        &self,
        loader: &mut AccountLoader,
        settlement: MintAccounts,
        wallet: Pubkey,
    ) -> Result<DirectGroup> {
        loader.prefetch_pools(self.pools.iter().copied())?;
        let target = loader.mint(wallet, self.target)?;
        let pools = load_pools(loader, &self.pools, |loader, pool| {
            loader.load_direct_pool(pool, target, wallet)
        });
        ensure!(pools.len() >= 2, "fewer than two pools loaded");
        ensure!(
            pools.iter().any(|p| p.base_mint() == settlement.mint),
            "no settlement-quoted pool loaded"
        );
        Ok(DirectGroup { target, pools })
    }
}

impl Triangle {
    fn resolve(
        config: &TriangleConfig,
        settlement: Pubkey,
        loader: &mut AccountLoader,
    ) -> Result<Self> {
        ensure!(!config.direct.is_empty(), "a triangle needs a direct pool");
        ensure!(!config.bridges.is_empty(), "a triangle needs a bridge pool");
        let target = common_counterpart(loader, &config.direct, settlement)?;
        let stock = counterpart(loader, config.intermediate, target)?;
        ensure!(
            !is_quote(&target) && !is_quote(&stock),
            "a triangle's target and stock cannot be SOL or USDC"
        );
        let quote = common_counterpart(loader, &config.bridges, stock)?;
        ensure!(
            is_quote(&quote),
            "bridges must trade {stock} against SOL or USDC"
        );
        Ok(Self {
            target,
            stock,
            quote,
            intermediate: config.intermediate,
            bridges: config.bridges.clone(),
            direct: config.direct.clone(),
        })
    }
}

impl Group for Triangle {
    type Loaded = MarketGroup;

    fn target(&self) -> Pubkey {
        self.target
    }

    fn load(
        &self,
        loader: &mut AccountLoader,
        settlement: MintAccounts,
        wallet: Pubkey,
    ) -> Result<MarketGroup> {
        let pools = [self.intermediate]
            .into_iter()
            .chain(self.bridges.iter().copied())
            .chain(self.direct.iter().copied());
        loader.prefetch_pools(pools)?;
        let target = loader.mint(wallet, self.target)?;
        let stock = loader.mint(wallet, self.stock)?;
        let quote = loader.mint(wallet, self.quote)?;
        let intermediate = loader.load_pool(self.intermediate, target, stock, wallet)?;
        let bridges = load_pools(loader, &self.bridges, |loader, pool| {
            loader.load_pool(pool, stock, quote, wallet)
        });
        let direct = load_pools(loader, &self.direct, |loader, pool| {
            loader.load_pool(pool, target, settlement, wallet)
        });
        ensure!(!bridges.is_empty(), "no bridge pool loaded");
        ensure!(!direct.is_empty(), "no direct pool loaded");
        Ok(MarketGroup {
            target,
            base: stock,
            intermediate,
            bridges,
            direct,
        })
    }
}

/// The Raydium SOL/USDC pool, converting the other settlement mint.
fn conversion_accounts(
    loader: &mut AccountLoader,
    settlement: MintAccounts,
    wallet: Pubkey,
) -> Result<ConversionAccounts> {
    let quote = loader.mint(wallet, other_quote(settlement.mint))?;
    let pool = loader.load_pool(abi::RAYDIUM_CONVERSION, quote, settlement, wallet)?;
    Ok(ConversionAccounts { quote, pool })
}

/// A route's pools loaded from one snapshot, ready to build instructions.
pub struct Basket {
    wallet: Pubkey,
    settlement: MintAccounts,
    header: abi::Header,
    additional_fee_collector: Option<Pubkey>,
    conversion: Option<ConversionAccounts>,
    groups: Groups<DirectGroup, MarketGroup>,
}

/// A loaded group as the cost model sees it: its pools in instruction order
/// (a triangle's intermediate, bridges, then direct pools), how many are
/// bridges, and the mints whose token program matters.
struct Shape<'a> {
    bridges: usize,
    pools: Vec<&'a PoolAccounts>,
    target: &'a MintAccounts,
    stock: Option<&'a MintAccounts>,
}

impl Groups<DirectGroup, MarketGroup> {
    fn shapes(&self) -> Vec<Shape<'_>> {
        match self {
            Self::Direct(groups) => groups
                .iter()
                .map(|g| Shape {
                    bridges: 0,
                    pools: g.pools.iter().collect(),
                    target: &g.target,
                    stock: None,
                })
                .collect(),
            Self::Triangle(groups) => groups
                .iter()
                .map(|g| Shape {
                    bridges: g.bridges.len(),
                    pools: std::iter::once(&g.intermediate)
                        .chain(&g.bridges)
                        .chain(&g.direct)
                        .collect(),
                    target: &g.target,
                    stock: Some(&g.base),
                })
                .collect(),
        }
    }
}

impl Basket {
    pub fn settlement(&self) -> Pubkey {
        self.settlement.mint
    }

    pub fn pool_count(&self) -> usize {
        self.groups.shapes().iter().map(|s| s.pools.len()).sum()
    }

    /// CU the executor needs for whichever route the basket allows.
    pub fn executor_cu(&self) -> u32 {
        // Only a pool quoted in the conversion's mint crosses settlements; a
        // triangle's intermediate trades the target against the stock.
        let converted = self.conversion.as_ref().map(|c| c.quote.mint);
        let shapes = self.groups.shapes();
        let costs: Vec<Vec<BasketPool>> = shapes
            .iter()
            .map(|shape| {
                shape
                    .pools
                    .iter()
                    .map(|pool| BasketPool {
                        venue: pool.venue(),
                        settlement_quoted: converted != Some(pool.base_mint()),
                        pump_quote_is_base: pool.pump_quote_is_base(),
                        adaptive_fee: pool.whirlpool_adaptive_fee(),
                    })
                    .collect()
            })
            .collect();
        let token_2022 = |mint: &MintAccounts| mint.token_program == abi::TOKEN_2022;
        let groups: Vec<BasketGroup> = shapes
            .iter()
            .zip(&costs)
            .map(|(shape, pools)| BasketGroup {
                bridges: shape.bridges,
                pools,
                target_token_2022: token_2022(shape.target),
                base_token_2022: shape.stock.is_some_and(token_2022),
            })
            .collect();
        let converter = self
            .conversion
            .as_ref()
            .map_or(DEFAULT_CONVERTER, |c| c.pool.venue());
        transaction_cu(&groups, converter, self.header.use_flashloan)
            + archer_book_cu(&shapes, &groups)
    }

    /// The executor instruction with `compute_unit_limit` for the executor.
    /// Flashloans repay the vault and keep the profit in the wallet;
    /// otherwise the platform collects its fee from the profit.
    pub fn instruction(&self, compute_unit_limit: u32) -> Result<Instruction> {
        let settlement = self.settlement.mint;
        let header = abi::Header {
            compute_unit_limit,
            ..self.header
        };
        let request = abi::V10Instruction {
            program_id: EXECUTOR,
            wallet: self.wallet,
            settlement: self.settlement,
            conversion: self.conversion.clone(),
            fee_collector: if header.use_flashloan {
                self.wallet
            } else {
                platform_collector(settlement)
            },
            additional_fee_collector: self.additional_fee_collector,
            flashloan: header.use_flashloan.then(|| {
                (
                    Pubkey::find_program_address(&[b"vault_authority"], &EXECUTOR).0,
                    Pubkey::find_program_address(
                        &[b"vault_token_account", settlement.as_ref()],
                        &EXECUTOR,
                    )
                    .0,
                )
            }),
            header,
            groups: match &self.groups {
                Groups::Direct(_) => vec![],
                Groups::Triangle(groups) => groups.clone(),
            },
        };
        match &self.groups {
            Groups::Direct(groups) => request.build_direct(groups),
            Groups::Triangle(_) => request.build(),
        }
    }
}

/// The Archer book work `transaction_cu` leaves to its caller: admitting the
/// books of every pool the transaction lists, and filling those of the route
/// with the most, which is all a transaction executes. `groups` are the
/// `shapes`' costs, in the same order.
fn archer_book_cu(shapes: &[Shape<'_>], groups: &[BasketGroup<'_>]) -> u32 {
    let parse_cu = archer::parse_cu(
        shapes
            .iter()
            .flat_map(|shape| shape.pools.iter().copied())
            .map(PoolAccounts::accounts),
    );
    let mut most = 0;
    each_route_pools(groups, |group, pools, _| {
        let route = pools
            .iter()
            .map(|&pool| archer::swap_cu(shapes[group].pools[pool].accounts()))
            .sum::<u32>();
        most = most.max(route);
    });
    parse_cu + most
}

fn platform_collector(settlement: Pubkey) -> Pubkey {
    if settlement == abi::USDC {
        pubkey!("GzVRuLF349u78FHpr8KbqMhrZ1aDxnhSF59JWiZ6tbgt")
    } else {
        pubkey!("GPpkDpzCDmYJY5qNhYmM14c7rct1zmkjWc2CjR5g7RZ1")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v10::PoolKeys;
    use executor_v10_abi::cost::{ARCHER_BOOK_CU, ARCHER_BOOK_PARSE_CU};

    fn mint(wallet: Pubkey, mint: Pubkey) -> MintAccounts {
        MintAccounts::ata(&wallet, mint, abi::TOKEN).unwrap()
    }

    fn basket(
        wallet: Pubkey,
        target: MintAccounts,
        pools: Vec<PoolAccounts>,
        conversion: Option<ConversionAccounts>,
    ) -> Basket {
        Basket {
            wallet,
            settlement: mint(wallet, abi::SOL),
            header: abi::Header::default(),
            additional_fee_collector: None,
            conversion,
            groups: Groups::Direct(vec![DirectGroup { target, pools }]),
        }
    }

    fn archer(x: MintAccounts, base: MintAccounts, books: usize) -> PoolAccounts {
        let books: Vec<_> = (0..books).map(|_| Pubkey::new_unique()).collect();
        PoolAccounts::from_keys(
            Pubkey::new_unique(),
            x,
            base,
            [Pubkey::new_unique(), Pubkey::new_unique()],
            PoolKeys::Archer { books: &books },
        )
        .unwrap()
    }

    fn cpmm(x: MintAccounts, base: MintAccounts) -> PoolAccounts {
        PoolAccounts::from_keys(
            Pubkey::new_unique(),
            x,
            base,
            [Pubkey::new_unique(), Pubkey::new_unique()],
            PoolKeys::Cpmm {
                config: Pubkey::new_unique(),
                observation: Pubkey::new_unique(),
            },
        )
        .unwrap()
    }

    /// Every listed book is parsed, but only the books of the route with the
    /// most swap work are swapped.
    #[test]
    fn archer_books_cost_their_parse_everywhere_and_swaps_on_one_route() {
        let wallet = Pubkey::new_unique();
        let target = mint(wallet, Pubkey::new_unique());
        let (sol, usdc) = (mint(wallet, abi::SOL), mint(wallet, abi::USDC));
        let settled = |books: [usize; 3]| {
            let pools = books.map(|books| archer(target, sol, books)).to_vec();
            basket(wallet, target, pools, None).executor_cu()
        };
        // Pools of 6 and 3 books are the costliest pair.
        assert_eq!(
            settled([2, 6, 3]),
            settled([1, 1, 1]) + (9 - 2) * ARCHER_BOOK_CU + (11 - 3) * ARCHER_BOOK_PARSE_CU
        );
        let converted = |books: [usize; 3]| {
            let pools = vec![
                archer(target, sol, books[0]),
                archer(target, sol, books[1]),
                archer(target, usdc, books[2]),
            ];
            let conversion = ConversionAccounts {
                quote: usdc,
                pool: cpmm(usdc, sol),
            };
            basket(wallet, target, pools, Some(conversion)).executor_cu()
        };
        // The 6-book pool crossing through the 4-book USDC pool outweighs
        // the two settled pools' 8.
        assert_eq!(
            converted([2, 6, 4]),
            converted([1; 3]) + (10 - 2) * ARCHER_BOOK_CU + (12 - 3) * ARCHER_BOOK_PARSE_CU
        );
    }

    /// A compounding pool's swap costs 8,500 CU more, plus a twentieth of
    /// headroom.
    #[test]
    fn a_compounding_damm_v2_pool_reserves_its_costlier_swap() {
        let wallet = Pubkey::new_unique();
        let target = mint(wallet, Pubkey::new_unique());
        let sol = mint(wallet, abi::SOL);
        let cu = |compounding| {
            let pool = PoolAccounts::from_keys(
                Pubkey::new_unique(),
                target,
                sol,
                [Pubkey::new_unique(), Pubkey::new_unique()],
                PoolKeys::DammV2 { compounding },
            )
            .unwrap();
            basket(wallet, target, vec![pool, cpmm(target, sol)], None).executor_cu()
        };
        assert_eq!(cu(true), cu(false) + 8_500 + 425);
    }

    /// An adaptive-fee Whirlpool's walk also loads and traverses its fee
    /// groups: 4,200 + 1,500 CU, plus headroom.
    #[test]
    fn an_adaptive_fee_whirlpool_reserves_its_fee_group_walk() {
        let wallet = Pubkey::new_unique();
        let target = mint(wallet, Pubkey::new_unique());
        let sol = mint(wallet, abi::SOL);
        let arrays = [Pubkey::new_unique()];
        let cu = |adaptive_fee| {
            let pool = PoolAccounts::from_keys(
                Pubkey::new_unique(),
                target,
                sol,
                [Pubkey::new_unique(), Pubkey::new_unique()],
                PoolKeys::Whirlpool {
                    arrays: &arrays,
                    adaptive_fee,
                },
            )
            .unwrap();
            basket(wallet, target, vec![pool, cpmm(target, sol)], None).executor_cu()
        };
        let fixed = cu(Some(false));
        // A pool whose fee is unknown is costed as adaptive.
        for adaptive_fee in [Some(true), None] {
            assert_eq!(cu(adaptive_fee), fixed + 5_700 + 285, "{adaptive_fee:?}");
        }
    }
}
