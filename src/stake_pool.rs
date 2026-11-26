//! SPL stake pools' SOL conversions as V10 pools: DepositSol issues the pool
//! mint for SOL, and WithdrawSol pays SOL from the reserve stake for it. The
//! executor trades both against the group's WSOL account, so the pair's
//! backing mint is WSOL.
use crate::v10::{read, MintAccounts, PoolPair, SOL, TOKEN};
use anyhow::{ensure, Result};
use solana_sdk::{instruction::AccountMeta, pubkey, pubkey::Pubkey, sysvar::clock};

pub const SPL_STAKE_POOL: Pubkey = pubkey!("SPoo1Ku8WFXoNDMHPsrGSTSG1Y47rzgn41SLUNakuHy");
pub const STAKE_PROGRAM: Pubkey = pubkey!("Stake11111111111111111111111111111111111111");
/// solana-sdk 4 no longer exports this sysvar's ID.
const STAKE_HISTORY: Pubkey = pubkey!("SysvarStakeHistory1111111111111111111111111");

/// `StakePool` offsets before its first variable-length field.
mod layout {
    pub const ACCOUNT_TYPE: usize = 0;
    pub const WITHDRAW_BUMP: usize = 97;
    pub const RESERVE: usize = 130;
    pub const POOL_MINT: usize = 162;
    pub const MANAGER_FEE: usize = 194;
    pub const TOKEN_PROGRAM: usize = 226;
}

/// The `AccountType` of a `StakePool`; the program also owns validator lists.
const STAKE_POOL_ACCOUNT: u8 = 1;

/// A stake pool the executor can trade: one whose mint is a legacy SPL token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StakePool {
    mint: Pubkey,
    reserve: Pubkey,
    manager_fee: Pubkey,
    withdraw_bump: u8,
}

fn key(data: &[u8], offset: usize) -> Result<Pubkey> {
    Ok(Pubkey::new_from_array(read(data, offset)?))
}

impl StakePool {
    /// The pool whose account holds `data`.
    pub(crate) fn read(data: &[u8]) -> Result<Self> {
        ensure!(
            data.get(layout::ACCOUNT_TYPE) == Some(&STAKE_POOL_ACCOUNT),
            "not an SPL stake pool"
        );
        ensure!(
            key(data, layout::TOKEN_PROGRAM)? == TOKEN,
            "the stake pool's mint is not a legacy SPL token"
        );
        Ok(Self {
            mint: key(data, layout::POOL_MINT)?,
            reserve: key(data, layout::RESERVE)?,
            manager_fee: key(data, layout::MANAGER_FEE)?,
            withdraw_bump: read::<1>(data, layout::WITHDRAW_BUMP)?[0],
        })
    }

    /// The pool whose account holds `data`, converting between `x` and
    /// `base` either way round. The executor rejects a pool whose manager fee
    /// account is the wallet's account of the issued mint.
    pub(crate) fn read_trading(data: &[u8], x: MintAccounts, base: MintAccounts) -> Result<Self> {
        let pool = Self::read(data)?;
        let issued = if x.mint == pool.mint { x } else { base };
        let pair = [pool.mint, SOL];
        ensure!(
            pair == [x.mint, base.mint] || pair == [base.mint, x.mint],
            "the stake pool converts {} and SOL, not {} and {}",
            pool.mint,
            x.mint,
            base.mint
        );
        ensure!(
            issued.wallet != pool.manager_fee,
            "the wallet's {} account {} is the stake pool's manager fee account",
            pool.mint,
            issued.wallet
        );
        Ok(pool)
    }

    /// The mints `[issued, WSOL]`, whose vaults are the issued mint itself
    /// and the reserve stake account, whose lamports back it.
    pub(crate) fn pair(&self) -> PoolPair {
        PoolPair::issued([self.mint, SOL], [self.mint, self.reserve])
    }

    /// The program checks the withdraw authority against the bump the pool
    /// records.
    fn withdraw_authority(&self, pool: Pubkey) -> Result<Pubkey> {
        Ok(Pubkey::create_program_address(
            &[pool.as_ref(), b"withdraw", &[self.withdraw_bump]],
            &SPL_STAKE_POOL,
        )?)
    }

    /// The executor's block for the pool at `pool`, after the program and
    /// declared base mint. The pool mint is relisted writable because the
    /// basket lists a group's mints read-only.
    pub(crate) fn accounts(&self, pool: Pubkey) -> Result<[AccountMeta; 8]> {
        Ok([
            AccountMeta::new(pool, false),
            AccountMeta::new_readonly(self.withdraw_authority(pool)?, false),
            AccountMeta::new(self.reserve, false),
            AccountMeta::new(self.manager_fee, false),
            AccountMeta::new(self.mint, false),
            AccountMeta::new_readonly(clock::ID, false),
            AccountMeta::new_readonly(STAKE_HISTORY, false),
            AccountMeta::new_readonly(STAKE_PROGRAM, false),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v10::{PoolAccounts, TOKEN_2022};
    use executor_v10_abi::Venue;

    const JITO_POOL: Pubkey = pubkey!("Jito4APyf642JPZPx3hGc6WWJ8zPKtRbRs4P815Awbb");
    const JITOSOL: Pubkey = pubkey!("J1toso1uCk3RLmjorhTtrVwY9HJ7X8V9yYac6Y7kGCPn");
    const JITO_RESERVE: Pubkey = pubkey!("BgKUXdS29YcHCFrPm5M8oLHiTzZaMDjsebggjoaQ6KFL");
    const JITO_MANAGER_FEE: Pubkey = pubkey!("8yoigZfzZ1nNaadumY9uPVD118225UYHTDpmjpr2nrSa");

    /// A `StakePool` prefix with Jito's keys and the canonical bump of its
    /// withdraw authority.
    fn jito_state(account_type: u8, token_program: Pubkey) -> Vec<u8> {
        let (_, bump) =
            Pubkey::find_program_address(&[JITO_POOL.as_ref(), b"withdraw"], &SPL_STAKE_POOL);
        let mut data = vec![0u8; 346];
        data[layout::ACCOUNT_TYPE] = account_type;
        data[layout::WITHDRAW_BUMP] = bump;
        for (offset, key) in [
            (layout::RESERVE, JITO_RESERVE),
            (layout::POOL_MINT, JITOSOL),
            (layout::MANAGER_FEE, JITO_MANAGER_FEE),
            (layout::TOKEN_PROGRAM, token_program),
        ] {
            data[offset..offset + 32].copy_from_slice(key.as_ref());
        }
        data
    }

    fn jito_pool() -> Vec<u8> {
        jito_state(STAKE_POOL_ACCOUNT, TOKEN)
    }

    #[test]
    fn either_orientation_lists_the_program_base_mint_and_pool_block() -> Result<()> {
        let state = jito_pool();
        let wallet = Pubkey::new_unique();
        let jitosol = MintAccounts::ata(&wallet, JITOSOL, TOKEN)?;
        let wsol = MintAccounts::ata(&wallet, SOL, TOKEN)?;
        let (authority, _) =
            Pubkey::find_program_address(&[JITO_POOL.as_ref(), b"withdraw"], &SPL_STAKE_POOL);
        let block = [
            (JITO_POOL, true),
            (authority, false),
            (JITO_RESERVE, true),
            (JITO_MANAGER_FEE, true),
            (JITOSOL, true),
            (clock::ID, false),
            (STAKE_HISTORY, false),
            (STAKE_PROGRAM, false),
        ];
        let load = |data: &[u8], x: MintAccounts, base: MintAccounts| {
            PoolAccounts::from_state(JITO_POOL, SPL_STAKE_POOL, data, x, base, &[], None)
        };
        for (x, base) in [(jitosol, wsol), (wsol, jitosol)] {
            let pool = load(&state, x, base)?;
            let listed: Vec<_> = pool
                .accounts()
                .iter()
                .map(|meta| (meta.pubkey, meta.is_writable))
                .collect();
            let expected: Vec<_> = [(SPL_STAKE_POOL, false), (base.mint, false)]
                .into_iter()
                .chain(block)
                .collect();
            assert_eq!(listed, expected);
            assert_eq!((pool.pool(), pool.x_mint()), (JITO_POOL, x.mint));
            assert_eq!(pool.venue(), Venue::StakePool);
        }
        assert_eq!(
            PoolPair::read(SPL_STAKE_POOL, &state)?.mints,
            [JITOSOL, SOL]
        );

        let error = |data: &[u8], x, base, arrays: &[Pubkey], bitmap| {
            PoolAccounts::from_state(JITO_POOL, SPL_STAKE_POOL, data, x, base, arrays, bitmap)
                .unwrap_err()
                .to_string()
        };
        let token_2022 = |mint| MintAccounts::ata(&wallet, mint, TOKEN_2022);
        let legacy_only = "an issuer's mints are legacy SPL tokens";
        assert_eq!(
            error(&state, token_2022(JITOSOL)?, wsol, &[], None),
            legacy_only
        );
        assert_eq!(
            error(&state, jitosol, token_2022(SOL)?, &[], None),
            legacy_only
        );
        let foreign = MintAccounts::ata(&wallet, Pubkey::new_unique(), TOKEN)?;
        for (x, base) in [(foreign, wsol), (jitosol, foreign), (jitosol, jitosol)] {
            assert_eq!(
                error(&state, x, base, &[], None),
                format!(
                    "the stake pool converts {JITOSOL} and SOL, not {} and {}",
                    x.mint, base.mint
                )
            );
        }
        let array = Pubkey::new_unique();
        for (arrays, bitmap) in [(&[array][..], None), (&[][..], Some(array))] {
            assert_eq!(
                error(&state, jitosol, wsol, arrays, bitmap),
                "pool does not use arrays"
            );
        }
        assert!(
            crate::v10::array_candidates(JITO_POOL, SPL_STAKE_POOL, &state, 1)?
                .0
                .is_empty()
        );

        // A wallet whose jitoSOL account is the manager fee account.
        let mut managed = state.clone();
        managed[layout::MANAGER_FEE..layout::MANAGER_FEE + 32]
            .copy_from_slice(jitosol.wallet.as_ref());
        for (x, base) in [(jitosol, wsol), (wsol, jitosol)] {
            assert_eq!(
                error(&managed, x, base, &[], None),
                format!(
                    "the wallet's {JITOSOL} account {} is the stake pool's manager fee account",
                    jitosol.wallet
                )
            );
        }
        Ok(())
    }

    #[test]
    fn rejects_other_accounts_and_token_2022_pools() {
        assert!(StakePool::read(&jito_pool()).is_ok());
        let error = |data: &[u8]| StakePool::read(data).unwrap_err().to_string();
        let validator_list = 2;
        assert_eq!(
            error(&jito_state(validator_list, TOKEN)),
            "not an SPL stake pool"
        );
        assert_eq!(
            error(&jito_state(STAKE_POOL_ACCOUNT, TOKEN_2022)),
            "the stake pool's mint is not a legacy SPL token"
        );
        assert_eq!(
            error(&jito_pool()[..layout::TOKEN_PROGRAM]),
            "truncated pool account"
        );
    }
}
