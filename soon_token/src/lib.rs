//! $SOON — a public-fungible test token for OotleSwap.
//!
//! On creation, mints 1,000,000 SOON (with divisibility 6 → 1e12 micro-units)
//! into an internal vault. Anyone can call `faucet()` to claim 100 SOON for testing.

use tari_template_lib::prelude::*;

#[template]
mod soon_token {
    use super::*;

    pub struct SoonToken {
        token_vault: Vault,
        faucet_count: u64,
    }

    // 1,000,000 SOON total supply at divisibility 6.
    const INITIAL_SUPPLY_MICRO: u64 = 1_000_000_000_000;
    // 100 SOON per faucet drip.
    const FAUCET_AMOUNT_MICRO: u64 = 100_000_000;

    impl SoonToken {
        pub fn new() -> Component<Self> {
            let initial: Bucket = ResourceBuilder::public_fungible()
                .with_token_symbol("SOON")
                .metadata("name", "Soon Token")
                .metadata("description", "OotleSwap test token")
                .with_divisibility(6)
                .initial_supply(Amount::from(INITIAL_SUPPLY_MICRO));

            Component::new(Self {
                token_vault: Vault::from_bucket(initial),
                faucet_count: 0,
            })
            .with_access_rules(ComponentAccessRules::allow_all())
            .create()
        }

        /// Drip 100 SOON to the caller. Open to anyone for testnet convenience.
        pub fn faucet(&mut self) -> Bucket {
            assert!(
                !self.token_vault.balance().is_zero(),
                "Faucet is empty"
            );
            self.faucet_count += 1;
            let drip = Amount::from(FAUCET_AMOUNT_MICRO);
            assert!(
                self.token_vault.balance() >= drip,
                "Faucet has insufficient remaining balance for a full drip"
            );
            self.token_vault.withdraw(drip)
        }

        pub fn balance(&self) -> Amount {
            self.token_vault.balance()
        }

        pub fn resource_address(&self) -> ResourceAddress {
            self.token_vault.resource_address()
        }

        pub fn faucet_count(&self) -> u64 {
            self.faucet_count
        }
    }
}
