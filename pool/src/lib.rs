//! OotleSwap — generic constant-product AMM for Tari Ootle.
//!
//! One template, instantiated per token pair. Holds two vaults of public-fungible
//! (or *revealed* tTARI) resources and mints LP tokens that represent
//! proportional ownership of the reserves.
//!
//! Math: x * y = k, with a 0.3% fee applied to swap inputs (997/1000).
//! Stealth/confidential funds are explicitly rejected at every entry point.

use tari_template_lib::prelude::*;

#[template]
mod pool {
    use super::*;

    // --- Pool state (must come first per #[template] macro requirement) ---

    pub struct Pool {
        vault_a: Vault,
        vault_b: Vault,
        lp_resource: ResourceAddress,
        lp_total_supply: Amount,
        // Held internally: this badge gives the pool authority to mint/burn its own LP token.
        admin_badge_vault: Vault,
    }

    // 0.3% fee. amount_in_with_fee = amount_in * 997 / 1000
    const FEE_NUMERATOR: u64 = 997;
    const FEE_DENOMINATOR: u64 = 1000;

    // Minimum-liquidity lock (Uniswap V2 pattern): a tiny chunk of LP is burned on bootstrap
    // so that no single early LP can manipulate share value via micro-deposits.
    // 1000 micro-units = 0.001 tokens at divisibility 6.
    const MINIMUM_LIQUIDITY: u64 = 1000;

    impl Pool {
        /// Create a new pool for a (resource_a, resource_b) pair.
        /// The pool starts empty — the first call to `add_liquidity` bootstraps it.
        pub fn new(resource_a: ResourceAddress, resource_b: ResourceAddress) -> Component<Self> {
            assert_ne!(
                resource_a, resource_b,
                "Pool requires two different resources"
            );

            // Internal admin badge: a single NFT used to authorize LP mint/burn.
            let admin_badge: Bucket = ResourceBuilder::non_fungible()
                .with_token_symbol("LPADM")
                .metadata("name", "OotleSwap LP Admin")
                .initial_supply_with_data(vec![(
                    NonFungibleId::from_u64(0),
                    (&metadata!["role" => "lp_admin"], &()),
                )]);
            let admin_resource = admin_badge.resource_address();

            // LP token: public fungible, mintable + burnable only by holder of the admin badge.
            let lp_resource: ResourceAddress = ResourceBuilder::public_fungible()
                .with_token_symbol("LP")
                .metadata("name", "OotleSwap LP")
                .with_divisibility(6)
                .mintable(rule!(resource(admin_resource)))
                .burnable(rule!(resource(admin_resource)))
                .build();

            Component::new(Self {
                vault_a: Vault::new_empty(resource_a),
                vault_b: Vault::new_empty(resource_b),
                lp_resource,
                lp_total_supply: Amount::from(0u64),
                admin_badge_vault: Vault::from_bucket(admin_badge),
            })
            .with_access_rules(ComponentAccessRules::allow_all())
            .create()
        }

        // --- Read-only views ---

        pub fn get_reserves(&self) -> Vec<Amount> {
            // [reserve_a, reserve_b] in the order given to `new`.
            vec![self.vault_a.balance(), self.vault_b.balance()]
        }

        pub fn get_lp_resource(&self) -> ResourceAddress {
            self.lp_resource
        }

        pub fn get_total_lp_supply(&self) -> Amount {
            self.lp_total_supply
        }

        pub fn get_resource_a(&self) -> ResourceAddress {
            self.vault_a.resource_address()
        }

        pub fn get_resource_b(&self) -> ResourceAddress {
            self.vault_b.resource_address()
        }

        // --- Liquidity ---

        /// Add liquidity. Accepts the two buckets in any order — direction is dispatched
        /// by resource address. Returns LP tokens.
        ///
        /// First call (bootstrap): mints `sqrt(amount_a * amount_b)` LP, locks
        /// `MINIMUM_LIQUIDITY` of it permanently.
        ///
        /// Subsequent calls: caller must deposit in proportion. Excess is NOT refunded —
        /// LP minted is `min(in_a * total_lp / reserve_a, in_b * total_lp / reserve_b)`.
        /// Use `quote_add_liquidity` off-chain to figure the right ratio first.
        pub fn add_liquidity(&mut self, bucket_x: Bucket, bucket_y: Bucket) -> Bucket {
            bucket_x.assert_contains_no_confidential_funds();
            bucket_y.assert_contains_no_confidential_funds();

            // Sort buckets into (bucket_a, bucket_b) matching the pool's resource ordering.
            let bucket_a_addr = self.vault_a.resource_address();
            let bucket_b_addr = self.vault_b.resource_address();
            let x_addr = bucket_x.resource_address();
            let y_addr = bucket_y.resource_address();

            let (bucket_a, bucket_b) = if x_addr == bucket_a_addr && y_addr == bucket_b_addr {
                (bucket_x, bucket_y)
            } else if x_addr == bucket_b_addr && y_addr == bucket_a_addr {
                (bucket_y, bucket_x)
            } else {
                panic!("Buckets do not match this pool's resources");
            };

            let amount_a_in = bucket_a.amount();
            let amount_b_in = bucket_b.amount();
            assert!(!amount_a_in.is_zero(), "Must deposit non-zero amount of A");
            assert!(!amount_b_in.is_zero(), "Must deposit non-zero amount of B");

            let lp_to_mint: Amount;
            let mut burn_minimum_liquidity = false;

            if self.lp_total_supply.is_zero() {
                // Bootstrap: LP issued = sqrt(a * b). Lock MINIMUM_LIQUIDITY forever.
                let prod = amount_a_in
                    .checked_mul(amount_b_in)
                    .expect("overflow in initial liquidity calculation");
                let initial_lp = isqrt_amount(prod);
                let min_liq = Amount::from(MINIMUM_LIQUIDITY);
                assert!(
                    initial_lp > min_liq,
                    "Insufficient initial liquidity: must mint more than MINIMUM_LIQUIDITY"
                );
                lp_to_mint = initial_lp;
                burn_minimum_liquidity = true;
            } else {
                // Proportional add. We compute against current reserves *before* deposit.
                let reserve_a = self.vault_a.balance();
                let reserve_b = self.vault_b.balance();
                let total_lp = self.lp_total_supply;

                let lp_from_a = mul_div(amount_a_in, total_lp, reserve_a);
                let lp_from_b = mul_div(amount_b_in, total_lp, reserve_b);
                lp_to_mint = if lp_from_a < lp_from_b {
                    lp_from_a
                } else {
                    lp_from_b
                };
                assert!(
                    !lp_to_mint.is_zero(),
                    "Insufficient liquidity minted (depositing wrong ratio?)"
                );
            }

            // Deposit reserves.
            self.vault_a.deposit(bucket_a);
            self.vault_b.deposit(bucket_b);

            // Mint LP tokens. `authorize()` returns a ProofAuth RAII guard;
            // it MUST be bound to a variable to keep the proof alive across
            // the call. Calling `.authorize();` as a bare statement is a no-op.
            let _auth = self.admin_badge_vault.authorize();
            let lp_manager = ResourceManager::get(self.lp_resource);
            let mut lp_bucket = lp_manager.mint_fungible(lp_to_mint);
            self.lp_total_supply = self
                .lp_total_supply
                .checked_add(lp_to_mint)
                .expect("overflow in lp_total_supply");

            if burn_minimum_liquidity {
                let lock = lp_bucket.take(Amount::from(MINIMUM_LIQUIDITY));
                lock.burn();
                self.lp_total_supply = self
                    .lp_total_supply
                    .checked_sub(Amount::from(MINIMUM_LIQUIDITY))
                    .expect("underflow in lp_total_supply");
            }

            lp_bucket
        }

        /// Burn LP tokens, return proportional shares of both reserves.
        /// Returns `vec![bucket_a, bucket_b]` in the pool's canonical resource order.
        pub fn remove_liquidity(&mut self, lp_in: Bucket) -> Vec<Bucket> {
            assert_eq!(
                lp_in.resource_address(),
                self.lp_resource,
                "Bucket is not this pool's LP token"
            );
            let lp_amount = lp_in.amount();
            assert!(!lp_amount.is_zero(), "Cannot burn zero LP");
            assert!(
                lp_amount <= self.lp_total_supply,
                "LP burn exceeds tracked supply"
            );

            let total_lp = self.lp_total_supply;
            let reserve_a = self.vault_a.balance();
            let reserve_b = self.vault_b.balance();

            let amount_a_out = mul_div(lp_amount, reserve_a, total_lp);
            let amount_b_out = mul_div(lp_amount, reserve_b, total_lp);
            assert!(
                !amount_a_out.is_zero() && !amount_b_out.is_zero(),
                "Burn amount too small — would withdraw zero"
            );

            // Burn LP tokens. Same RAII binding requirement as in add_liquidity.
            let _auth = self.admin_badge_vault.authorize();
            lp_in.burn();
            self.lp_total_supply = self
                .lp_total_supply
                .checked_sub(lp_amount)
                .expect("underflow in lp_total_supply");

            let bucket_a = self.vault_a.withdraw(amount_a_out);
            let bucket_b = self.vault_b.withdraw(amount_b_out);
            vec![bucket_a, bucket_b]
        }

        // --- Swap ---

        /// Swap `input` for the other side of the pool. Direction is determined by
        /// the input bucket's resource address. Returns the output bucket.
        pub fn swap(&mut self, input: Bucket) -> Bucket {
            input.assert_contains_no_confidential_funds();
            let in_resource = input.resource_address();
            let amount_in = input.amount();
            assert!(!amount_in.is_zero(), "Cannot swap zero");

            let a_addr = self.vault_a.resource_address();
            let b_addr = self.vault_b.resource_address();
            let a_to_b = in_resource == a_addr;
            let b_to_a = in_resource == b_addr;
            assert!(a_to_b || b_to_a, "Resource not in this pool");

            let (reserve_in, reserve_out) = if a_to_b {
                (self.vault_a.balance(), self.vault_b.balance())
            } else {
                (self.vault_b.balance(), self.vault_a.balance())
            };
            assert!(
                !reserve_in.is_zero() && !reserve_out.is_zero(),
                "Pool has not been bootstrapped yet"
            );

            let amount_out = compute_swap_output(amount_in, reserve_in, reserve_out);
            assert!(!amount_out.is_zero(), "Insufficient output amount");
            assert!(
                amount_out < reserve_out,
                "Cannot drain entire reserve in a single swap"
            );

            // Emit event for indexers/explorers.
            emit_event(
                "Swap",
                metadata![
                    "in_resource" => in_resource.to_string(),
                    "amount_in" => amount_in.to_string(),
                    "amount_out" => amount_out.to_string(),
                ],
            );

            if a_to_b {
                self.vault_a.deposit(input);
                self.vault_b.withdraw(amount_out)
            } else {
                self.vault_b.deposit(input);
                self.vault_a.withdraw(amount_out)
            }
        }

        /// Read-only swap quote. Useful for clients to preview output before submitting.
        pub fn quote_swap(&self, in_resource: ResourceAddress, amount_in: Amount) -> Amount {
            let a_addr = self.vault_a.resource_address();
            let b_addr = self.vault_b.resource_address();
            let (reserve_in, reserve_out) = if in_resource == a_addr {
                (self.vault_a.balance(), self.vault_b.balance())
            } else if in_resource == b_addr {
                (self.vault_b.balance(), self.vault_a.balance())
            } else {
                panic!("Resource not in this pool");
            };
            if reserve_in.is_zero() || reserve_out.is_zero() {
                return Amount::from(0u64);
            }
            compute_swap_output(amount_in, reserve_in, reserve_out)
        }
    }

    // --- Internal math helpers ---

    fn compute_swap_output(amount_in: Amount, reserve_in: Amount, reserve_out: Amount) -> Amount {
        // amount_out = (amount_in * 997 * reserve_out) / (reserve_in * 1000 + amount_in * 997)
        let fee_num = Amount::from(FEE_NUMERATOR);
        let fee_den = Amount::from(FEE_DENOMINATOR);

        let amount_in_with_fee = amount_in
            .checked_mul(fee_num)
            .expect("overflow: amount_in_with_fee");
        let numerator = amount_in_with_fee
            .checked_mul(reserve_out)
            .expect("overflow: swap numerator");
        let denom_term1 = reserve_in
            .checked_mul(fee_den)
            .expect("overflow: denom term");
        let denominator = denom_term1
            .checked_add(amount_in_with_fee)
            .expect("overflow: denom sum");
        numerator
            .checked_div(denominator)
            .expect("div by zero in swap")
    }

    fn mul_div(a: Amount, b: Amount, denom: Amount) -> Amount {
        let prod = a.checked_mul(b).expect("overflow in mul_div");
        prod.checked_div(denom).expect("div by zero in mul_div")
    }

    // Amount uses a private u128 field; round-trip via to_le_bytes/from_le_bytes
    // (both pub) to do u128-space math when needed. tari_template_lib_types' built-in
    // `checked_sqrt` is gated behind the `extra-arith` feature which we can't enable
    // through the transitive dep, so we implement our own.
    fn isqrt_amount(n: Amount) -> Amount {
        let value = u128::from_le_bytes(n.to_le_bytes());
        let root = isqrt_u128(value);
        Amount::from_le_bytes(root.to_le_bytes())
    }

    fn isqrt_u128(n: u128) -> u128 {
        if n < 2 {
            return n;
        }
        let mut x = n;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + n / x) / 2;
        }
        x
    }
}
