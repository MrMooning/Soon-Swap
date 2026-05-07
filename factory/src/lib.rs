//! OotleSwap pool factory + registry.
//!
//! Permissionless registry for OotleSwap pools. Anyone can create a pool the
//! usual way (calling `Pool::new` directly), then register it here. The factory
//! verifies the registered component is genuinely an instance of the configured
//! pool template before adding it.
//!
//! After registration, anyone can look up the canonical pool for a resource
//! pair via `get_pool(a, b)` — order-agnostic — or enumerate all pools via
//! `list_pools()`.

use tari_template_lib::prelude::*;

#[template]
mod factory {
    use super::*;

    pub struct Factory {
        pools: Vec<PoolEntry>,
        pool_template: TemplateAddress,
    }

    /// One registered pool. Resources are stored canonically sorted so lookups
    /// don't need to canonicalize at query time.
    pub struct PoolEntry {
        pub resource_a: ResourceAddress,
        pub resource_b: ResourceAddress,
        pub component: ComponentAddress,
    }

    impl Factory {
        /// Construct a new factory bound to a specific pool template.
        /// Only components instantiated from `pool_template` will be accepted
        /// for registration.
        pub fn new(pool_template: TemplateAddress) -> Component<Self> {
            Component::new(Self {
                pools: Vec::new(),
                pool_template,
            })
            .with_access_rules(ComponentAccessRules::allow_all())
            .create()
        }

        /// Register a pool component for a given resource pair.
        ///
        /// The factory:
        /// 1. Verifies the component's template matches `self.pool_template`
        ///    (i.e. it really is a Pool from our published template).
        /// 2. Trusts the caller-supplied `(resource_a, resource_b)` pair.
        ///    Verifying the pair against the pool itself would require the pool's
        ///    cross-component method call from within the factory, which in turn
        ///    needs the pool's vault-referenced resource substates pre-declared
        ///    as transaction inputs — chicken-and-egg for a registrant who's
        ///    just looking up the pool for the first time.
        /// 3. Sorts the pair canonically (lex on stringified address).
        /// 4. Rejects duplicate pairs and duplicate components.
        /// 5. Inserts and emits a `PoolRegistered` event.
        ///
        /// **Trust model:** the registry is a *hint*. Clients that need certainty
        /// can re-verify by submitting a transaction that calls
        /// `get_resource_a` / `get_resource_b` on the looked-up component.
        pub fn register_pool(
            &mut self,
            pool: ComponentAddress,
            resource_a: ResourceAddress,
            resource_b: ResourceAddress,
        ) {
            assert_ne!(resource_a, resource_b, "Resources must differ");

            // 1. Verify template (cheap, no vault access required).
            let pool_handle = ComponentManager::get(pool);
            let actual_template = pool_handle.get_template_address();
            assert_eq!(
                actual_template, self.pool_template,
                "Component is not from the configured pool template"
            );

            // 2 + 3. Canonicalize.
            let (canonical_a, canonical_b) = canonical_pair(resource_a, resource_b);

            // 4. Reject duplicate pair or duplicate component.
            for entry in &self.pools {
                if entry.resource_a == canonical_a && entry.resource_b == canonical_b {
                    panic!("A pool for this resource pair is already registered");
                }
                assert_ne!(
                    entry.component, pool,
                    "This pool component is already registered"
                );
            }

            // 5. Insert + emit.
            self.pools.push(PoolEntry {
                resource_a: canonical_a,
                resource_b: canonical_b,
                component: pool,
            });

            emit_event(
                "PoolRegistered",
                metadata![
                    "resource_a" => canonical_a.to_string(),
                    "resource_b" => canonical_b.to_string(),
                    "component" => pool.to_string(),
                    "index" => (self.pools.len() - 1).to_string(),
                ],
            );
        }

        /// Look up the registered pool for a resource pair. Order-agnostic.
        pub fn get_pool(
            &self,
            resource_a: ResourceAddress,
            resource_b: ResourceAddress,
        ) -> Option<ComponentAddress> {
            if resource_a == resource_b {
                return None;
            }
            let (a, b) = canonical_pair(resource_a, resource_b);
            self.pools
                .iter()
                .find(|e| e.resource_a == a && e.resource_b == b)
                .map(|e| e.component)
        }

        /// Number of registered pools.
        pub fn pool_count(&self) -> u32 {
            self.pools.len() as u32
        }

        /// All registered pools as `Vec<PoolEntry>`. With small N (testnet) this
        /// is fine to return wholesale; for thousands of pools you'd want
        /// pagination.
        pub fn list_pools(&self) -> Vec<PoolEntry> {
            // We need to clone since PoolEntry contains Copy types; explicit
            // construction sidesteps any derive ambiguity.
            self.pools
                .iter()
                .map(|e| PoolEntry {
                    resource_a: e.resource_a,
                    resource_b: e.resource_b,
                    component: e.component,
                })
                .collect()
        }

        /// Read the configured pool template address.
        pub fn get_pool_template(&self) -> TemplateAddress {
            self.pool_template
        }
    }

    // --- helpers ---

    /// Sort two resources into canonical (a, b) order using lex comparison
    /// on the address's string representation. Stable across networks.
    fn canonical_pair(
        x: ResourceAddress,
        y: ResourceAddress,
    ) -> (ResourceAddress, ResourceAddress) {
        if x.to_string() <= y.to_string() {
            (x, y)
        } else {
            (y, x)
        }
    }
}
