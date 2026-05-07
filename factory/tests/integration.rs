//! Integration tests for the OotleSwap pool factory + registry.

use std::path::PathBuf;
use tari_template_lib::prelude::{
    Amount, ComponentAddress, ResourceAddress, TemplateAddress,
};
use tari_template_test_tooling::TemplateTest;
use tari_template_test_tooling::transaction::args;

fn workspace_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .join(rel)
}

fn setup() -> (TemplateTest, ComponentAddress, ComponentAddress, ComponentAddress, ResourceAddress, ResourceAddress) {
    // Compile factory + pool + soon_token. soon_token gives us two distinct
    // public-fungible resources cheaply (one per instance).
    let mut test = TemplateTest::new(
        workspace_path(""),
        [
            workspace_path("factory"),
            workspace_path("pool"),
            workspace_path("soon_token"),
        ],
    );
    let (account, _, _) = test.create_funded_account();
    let proof = test.owner_proof();

    let pool_template: TemplateAddress = test.get_template_address("Pool");

    // Create two SoonToken instances → two distinct $SOON resource addresses.
    let soon_a: ComponentAddress =
        test.call_function("SoonToken", "new", args![], vec![proof.clone()]);
    let soon_b: ComponentAddress =
        test.call_function("SoonToken", "new", args![], vec![proof.clone()]);
    let res_a: ResourceAddress =
        test.call_method(soon_a, "resource_address", args![], vec![proof.clone()]);
    let res_b: ResourceAddress =
        test.call_method(soon_b, "resource_address", args![], vec![proof.clone()]);
    assert_ne!(res_a, res_b, "two SoonToken instances must produce distinct resources");

    // Create a pool for (res_a, res_b).
    let pool: ComponentAddress = test.call_function(
        "Pool",
        "new",
        args![res_a, res_b],
        vec![proof.clone()],
    );

    // Create the factory bound to the pool template.
    let factory: ComponentAddress = test.call_function(
        "Factory",
        "new",
        args![pool_template],
        vec![proof],
    );

    let _ = account; // silence warning — kept for tests that need to extend
    (test, factory, pool, soon_a, res_a, res_b)
}

#[test]
fn register_and_lookup_pool() {
    let (mut test, factory, pool, _soon_a, res_a, res_b) = setup();
    let proof = test.owner_proof();

    // Pre-state: empty registry.
    let count_before: u32 =
        test.call_method(factory, "pool_count", args![], vec![proof.clone()]);
    assert_eq!(count_before, 0);

    // Register.
    test.call_method::<()>(
        factory,
        "register_pool",
        args![pool, res_a, res_b],
        vec![proof.clone()],
    );

    // Count is now 1.
    let count_after: u32 =
        test.call_method(factory, "pool_count", args![], vec![proof.clone()]);
    assert_eq!(count_after, 1);

    // Lookup in original order returns the pool.
    let found: Option<ComponentAddress> = test.call_method(
        factory,
        "get_pool",
        args![res_a, res_b],
        vec![proof.clone()],
    );
    assert_eq!(found, Some(pool));

    // Lookup in reverse order also returns the pool (canonicalized internally).
    let found_rev: Option<ComponentAddress> = test.call_method(
        factory,
        "get_pool",
        args![res_b, res_a],
        vec![proof],
    );
    assert_eq!(found_rev, Some(pool));
}

#[test]
fn lookup_unknown_pair_returns_none() {
    let (mut test, factory, _pool, soon_a, _res_a, _res_b) = setup();
    let proof = test.owner_proof();

    // A third resource not in any registered pool.
    let soon_c: ComponentAddress =
        test.call_function("SoonToken", "new", args![], vec![proof.clone()]);
    let res_c: ResourceAddress =
        test.call_method(soon_c, "resource_address", args![], vec![proof.clone()]);
    let res_a: ResourceAddress =
        test.call_method(soon_a, "resource_address", args![], vec![proof.clone()]);

    let found: Option<ComponentAddress> = test.call_method(
        factory,
        "get_pool",
        args![res_a, res_c],
        vec![proof],
    );
    assert_eq!(found, None);
}

#[test]
fn rejects_duplicate_registration() {
    let (mut test, factory, pool, _, res_a, res_b) = setup();
    let proof = test.owner_proof();
    let sk = test.secret_key().clone();

    // First registration succeeds.
    test.call_method::<()>(
        factory,
        "register_pool",
        args![pool, res_a, res_b],
        vec![proof.clone()],
    );

    // Second registration of the same pool must fail.
    let tx = test
        .transaction()
        .call_method(factory, "register_pool", args![pool, res_a, res_b])
        .build_and_seal(&sk);
    test.execute_expect_failure(tx, vec![proof]);
}

#[test]
fn rejects_wrong_template_component() {
    let (mut test, factory, _pool, soon_a, res_a, res_b) = setup();
    let proof = test.owner_proof();
    let sk = test.secret_key().clone();

    // soon_a is a SoonToken, not a Pool. Factory must reject it on the
    // template-address check (the resource pair we pass is irrelevant — the
    // template check fails first).
    let tx = test
        .transaction()
        .call_method(factory, "register_pool", args![soon_a, res_a, res_b])
        .build_and_seal(&sk);
    test.execute_expect_failure(tx, vec![proof]);
}

#[test]
fn list_pools_returns_all_registered() {
    let (mut test, factory, pool, soon_a, res_a, res_b) = setup();
    let proof = test.owner_proof();

    // Register the first pool.
    test.call_method::<()>(
        factory,
        "register_pool",
        args![pool, res_a, res_b],
        vec![proof.clone()],
    );

    // Create a second pool (res_a, res_c).
    let soon_c: ComponentAddress =
        test.call_function("SoonToken", "new", args![], vec![proof.clone()]);
    let res_c: ResourceAddress =
        test.call_method(soon_c, "resource_address", args![], vec![proof.clone()]);
    let pool2: ComponentAddress = test.call_function(
        "Pool",
        "new",
        args![res_a, res_c],
        vec![proof.clone()],
    );
    test.call_method::<()>(
        factory,
        "register_pool",
        args![pool2, res_a, res_c],
        vec![proof.clone()],
    );

    let count: u32 =
        test.call_method(factory, "pool_count", args![], vec![proof.clone()]);
    assert_eq!(count, 2);

    // Both lookups succeed.
    let p1: Option<ComponentAddress> = test.call_method(
        factory,
        "get_pool",
        args![res_a, res_b],
        vec![proof.clone()],
    );
    assert_eq!(p1, Some(pool));

    let p2: Option<ComponentAddress> = test.call_method(
        factory,
        "get_pool",
        args![res_a, res_c],
        vec![proof],
    );
    assert_eq!(p2, Some(pool2));

    let _ = soon_a; // unused alias
    let _: Amount = Amount::from(0u64); // silence unused import warning
}
