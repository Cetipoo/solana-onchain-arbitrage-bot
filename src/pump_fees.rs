//! Off-chain choice of the Pump fee recipient a swap pays.
use crate::v10 as abi;
use anyhow::{ensure, Context, Result};
use rand::seq::SliceRandom;
use solana_sdk::{account::Account, pubkey::Pubkey};

pub(crate) struct FeeRecipients {
    quote: Pubkey,
    token_program: Pubkey,
    candidates: Vec<(Pubkey, Pubkey)>,
}

impl FeeRecipients {
    pub fn new(global: &[u8], mayhem: bool, quote: Pubkey, token_program: Pubkey) -> Result<Self> {
        let offsets: Vec<_> = if mayhem {
            std::iter::once(385)
                .chain((0..7).map(|i| 418 + i * 32))
                .collect()
        } else {
            (0..8).map(|i| 57 + i * 32).collect()
        };
        Self::from_offsets(global, offsets, quote, token_program)
    }

    /// The buyback fee recipients, one of whose quote accounts a v2 swap pays.
    pub fn buyback(global: &[u8], quote: Pubkey, token_program: Pubkey) -> Result<Self> {
        Self::from_offsets(
            global,
            (0..8).map(|i| 643 + i * 32).collect(),
            quote,
            token_program,
        )
    }

    fn from_offsets(
        global: &[u8],
        offsets: Vec<usize>,
        quote: Pubkey,
        token_program: Pubkey,
    ) -> Result<Self> {
        let mut candidates = Vec::new();
        for offset in offsets {
            let recipient = Pubkey::new_from_array(
                global
                    .get(offset..offset + 32)
                    .context("truncated Pump fee recipients")?
                    .try_into()?,
            );
            if recipient != Pubkey::default() {
                let ata = abi::MintAccounts::ata(&recipient, quote, token_program)?.wallet;
                if !candidates.contains(&(recipient, ata)) {
                    candidates.push((recipient, ata));
                }
            }
        }
        ensure!(!candidates.is_empty(), "no Pump fee recipients");
        Ok(Self {
            quote,
            token_program,
            candidates,
        })
    }

    pub fn addresses(&self) -> Vec<Pubkey> {
        self.candidates.iter().map(|(_, ata)| *ata).collect()
    }

    /// Preserve ordinary account creation when none exists; otherwise use only
    /// warm ATAs. Returns whether any exists.
    pub fn prefer_initialized(&mut self, accounts: &[Option<Account>]) -> Result<bool> {
        ensure!(
            accounts.len() == self.candidates.len(),
            "Pump fee account response length mismatch"
        );
        let initialized: Vec<_> = self
            .candidates
            .iter()
            .zip(accounts)
            .filter_map(|(pair, account)| {
                let a = account.as_ref()?;
                (a.owner == self.token_program
                    && a.data.len() >= 165
                    && a.data.get(..32) == Some(self.quote.as_ref())
                    && a.data.get(32..64) == Some(pair.0.as_ref())
                    && a.data[108] == 1)
                    .then_some(*pair)
            })
            .collect();
        if initialized.is_empty() {
            return Ok(false);
        }
        self.candidates = initialized;
        Ok(true)
    }

    pub fn choose(&self) -> (Pubkey, Pubkey) {
        *self
            .candidates
            .choose(&mut rand::thread_rng())
            .expect("nonempty recipients")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn initialized(owner: Pubkey, mint: Pubkey, program: Pubkey) -> Account {
        let mut data = vec![0; 165];
        data[..32].copy_from_slice(mint.as_ref());
        data[32..64].copy_from_slice(owner.as_ref());
        data[108] = 1;
        Account {
            owner: program,
            data,
            ..Default::default()
        }
    }

    #[test]
    fn selects_only_initialized_allowed_accounts_for_each_mint_and_program() {
        let mut global = vec![0; 643];
        for offset in (0..8)
            .map(|i| 57 + i * 32)
            .chain(std::iter::once(385))
            .chain((0..7).map(|i| 418 + i * 32))
        {
            global[offset..offset + 32].copy_from_slice(Pubkey::new_unique().as_ref());
        }
        for mayhem in [false, true] {
            for program in [abi::TOKEN, abi::TOKEN_2022] {
                for quote in [abi::SOL, abi::USDC, Pubkey::new_unique()] {
                    let mut recipients =
                        FeeRecipients::new(&global, mayhem, quote, program).unwrap();
                    let pairs = recipients.candidates.clone();
                    let mut accounts: Vec<_> = pairs
                        .iter()
                        .map(|(owner, _)| Some(initialized(*owner, quote, program)))
                        .collect();
                    accounts[0] = None;
                    accounts[1].as_mut().unwrap().owner = Pubkey::new_unique();
                    accounts[2].as_mut().unwrap().data[..32].fill(0);
                    accounts[3].as_mut().unwrap().data[32..64].fill(0);
                    accounts[4].as_mut().unwrap().data[108] = 0;
                    accounts[5].as_mut().unwrap().data[108] = 2;
                    recipients.prefer_initialized(&accounts).unwrap();
                    assert_eq!(recipients.candidates, pairs[6..]);
                    for _ in 0..32 {
                        assert!(pairs[6..].contains(&recipients.choose()));
                    }
                    for (owner, ata) in &recipients.candidates {
                        assert_eq!(
                            *ata,
                            abi::MintAccounts::ata(owner, quote, program)
                                .unwrap()
                                .wallet
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn buyback_accounts_report_whether_a_v2_swap_can_pay_one() {
        let mut global = vec![0; 643 + 8 * 32];
        let owners: Vec<_> = (0..8).map(|_| Pubkey::new_unique()).collect();
        for (i, owner) in owners.iter().enumerate() {
            global[643 + i * 32..675 + i * 32].copy_from_slice(owner.as_ref());
        }
        let mut buyback = FeeRecipients::buyback(&global, abi::USDC, abi::TOKEN).unwrap();
        let pairs = buyback.candidates.clone();
        assert_eq!(
            pairs.iter().map(|(owner, _)| *owner).collect::<Vec<_>>(),
            owners
        );
        // V2 never creates the account: none existing keeps v1's candidates.
        assert!(!buyback.prefer_initialized(&vec![None; 8]).unwrap());
        assert_eq!(buyback.candidates, pairs);
        let mut accounts = vec![None; 8];
        accounts[3] = Some(initialized(owners[3], abi::USDC, abi::TOKEN));
        assert!(buyback.prefer_initialized(&accounts).unwrap());
        assert_eq!(buyback.choose(), pairs[3]);
    }

    #[test]
    fn missing_accounts_preserve_allowed_fallback_and_bad_responses_fail() {
        let mut global = vec![0; 643];
        let owner = Pubkey::new_unique();
        global[57..89].copy_from_slice(owner.as_ref());
        let mut recipients = FeeRecipients::new(&global, false, abi::SOL, abi::TOKEN).unwrap();
        assert!(recipients.prefer_initialized(&[]).is_err());
        recipients.prefer_initialized(&[None]).unwrap();
        assert_eq!(recipients.choose().0, owner);
        assert!(FeeRecipients::new(&global[..57], false, abi::SOL, abi::TOKEN).is_err());
        assert!(FeeRecipients::new(&[0; 643], false, abi::SOL, abi::TOKEN).is_err());
    }
}
