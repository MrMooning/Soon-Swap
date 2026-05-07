//! Integration tests for the OotleSwap pool.
//!
//! Two distinct SoonToken instances (each mints its own public-fungible
//! resource, both with the symbol "SOON" but distinct addresses) stand in
//! for "token A" and "token B" so we can exercise the AMM core on real
//! public-fungible primitives.

use tari_template_lib::prelude::{Amount, ComponentAddress, ResourceAddress};
use tari_template_test_tooling::TemplateTest;
use tari_template_test_tooling::transaction::args;

fn amount_to_u128(a: Amount) -> u128 {
    u128::from_le_bytes(a.to_le_bytes())
}

/// Spin up a fresh SoonToken component and return (component_addr, resource_addr).
fn create_token(test: &mut TemplateTest) -> (ComponentAddress, ResourceAddress) {
    let proof = test.owner_proof();
    let comp: ComponentAddress =
        test.call_function("SoonToken", "new", args![], vec![proof.clone()]);
    let res: ResourceAddress =
        test.call_method(comp, "resource_address", args![], vec![proof]);
    (comp, res)
}

/// Bootstrap a pool with one faucet-drip (100 SOON, i.e. 100_000_000 micro)
/// of each side. Returns (pool_addr, token_a_comp, res_a, token_b_comp, res_b).
fn create_pool_with_initial_liquidity(
    test: &mut TemplateTest,
    account: ComponentAddress,
) -> (
    ComponentAddress,
    ComponentAddress,
    ResourceAddress,
    ComponentAddress,
    ResourceAddress,
) {
    let proof = test.owner_proof();
    let sk = test.secret_key().clone();
    let (a_comp, res_a) = create_token(test);
    let (b_comp, res_b) = create_token(test);
    let pool: ComponentAddress =
        test.call_function("Pool", "new", args![res_a, res_b], vec![proof.clone()]);

    let tx = test
        .transaction()
        .call_method(a_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("a_bucket")
        .call_method(b_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("b_bucket")
        .call_method(
            pool,
            "add_liquidity",
            args![Workspace("a_bucket"), Workspace("b_bucket")],
        )
        .put_last_instruction_output_on_workspace("lp")
        .call_method(account, "deposit", args![Workspace("lp")])
        .build_and_seal(&sk);
    test.execute_expect_success(tx, vec![proof]);

    (pool, a_comp, res_a, b_comp, res_b)
}

#[test]
fn bootstrap_sets_reserves_and_lp_supply() {
    let mut test = TemplateTest::new(".", [".", "../soon_token"]);
    let (account, _, _) = test.create_funded_account();
    let (pool, _, _, _, _) = create_pool_with_initial_liquidity(&mut test, account);

    let proof = test.owner_proof();
    let reserves: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof.clone()]);
    let lp_supply: Amount =
        test.call_method(pool, "get_total_lp_supply", args![], vec![proof]);

    assert_eq!(reserves[0], Amount::from(100_000_000u64));
    assert_eq!(reserves[1], Amount::from(100_000_000u64));
    // sqrt(100e6 * 100e6) = 100e6, minus MINIMUM_LIQUIDITY=1000 burned
    assert_eq!(lp_supply, Amount::from(100_000_000u64 - 1000));
}

#[test]
fn swap_preserves_k_with_fee() {
    let mut test = TemplateTest::new(".", [".", "../soon_token"]);
    let (account, _, _) = test.create_funded_account();
    let (pool, a_comp, _res_a, _b_comp, _res_b) =
        create_pool_with_initial_liquidity(&mut test, account);

    let proof = test.owner_proof();
    let sk = test.secret_key().clone();

    let reserves_before: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof.clone()]);
    let k_before = amount_to_u128(reserves_before[0])
        .checked_mul(amount_to_u128(reserves_before[1]))
        .unwrap();

    // One faucet drip of A (100 SOON) → swap for B.
    let tx = test
        .transaction()
        .call_method(a_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("input")
        .call_method(pool, "swap", args![Workspace("input")])
        .put_last_instruction_output_on_workspace("output")
        .call_method(account, "deposit", args![Workspace("output")])
        .build_and_seal(&sk);
    test.execute_expect_success(tx, vec![proof.clone()]);

    let reserves_after: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof]);
    let k_after = amount_to_u128(reserves_after[0])
        .checked_mul(amount_to_u128(reserves_after[1]))
        .unwrap();

    // Reserve A grew by exactly the input amount.
    assert_eq!(
        reserves_after[0],
        reserves_before[0].checked_add(Amount::from(100_000_000u64)).unwrap(),
        "reserve A should increase by input amount"
    );
    // Reserve B shrank by less than the input (price impact + fee).
    assert!(reserves_after[1] < reserves_before[1]);
    // K must be >= K_before (fee accrues to LPs by staying in the pool).
    assert!(
        k_after >= k_before,
        "k must not decrease: before={} after={}",
        k_before,
        k_after
    );
}

#[test]
fn quote_swap_matches_actual_output() {
    let mut test = TemplateTest::new(".", [".", "../soon_token"]);
    let (account, _, _) = test.create_funded_account();
    let (pool, a_comp, res_a, _b_comp, _res_b) =
        create_pool_with_initial_liquidity(&mut test, account);

    let proof = test.owner_proof();
    let sk = test.secret_key().clone();

    let quoted: Amount = test.call_method(
        pool,
        "quote_swap",
        args![res_a, Amount::from(100_000_000u64)],
        vec![proof.clone()],
    );

    let reserves_before: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof.clone()]);

    let tx = test
        .transaction()
        .call_method(a_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("input")
        .call_method(pool, "swap", args![Workspace("input")])
        .put_last_instruction_output_on_workspace("output")
        .call_method(account, "deposit", args![Workspace("output")])
        .build_and_seal(&sk);
    test.execute_expect_success(tx, vec![proof.clone()]);

    let reserves_after: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof]);
    let actual_out = reserves_before[1].checked_sub(reserves_after[1]).unwrap();
    assert_eq!(
        quoted, actual_out,
        "quote_swap output must match actual swap output"
    );
}

#[test]
fn add_liquidity_proportional_doubles_reserves_and_lp() {
    let mut test = TemplateTest::new(".", [".", "../soon_token"]);
    let (account, _, _) = test.create_funded_account();
    let (pool, a_comp, _, b_comp, _) =
        create_pool_with_initial_liquidity(&mut test, account);

    let proof = test.owner_proof();
    let sk = test.secret_key().clone();

    let lp_supply_before: Amount = test.call_method(
        pool,
        "get_total_lp_supply",
        args![],
        vec![proof.clone()],
    );
    // Bootstrap minted 100e6 - 1000.
    assert_eq!(lp_supply_before, Amount::from(100_000_000u64 - 1000));

    // Second LP deposits the same 100e6 + 100e6 → reserves double, LP supply
    // grows by min(in_a * total_lp / reserve_a, ...) = 99_999_000.
    let tx = test
        .transaction()
        .call_method(a_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("a2")
        .call_method(b_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("b2")
        .call_method(
            pool,
            "add_liquidity",
            args![Workspace("a2"), Workspace("b2")],
        )
        .put_last_instruction_output_on_workspace("lp2")
        .call_method(account, "deposit", args![Workspace("lp2")])
        .build_and_seal(&sk);
    test.execute_expect_success(tx, vec![proof.clone()]);

    let reserves: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof.clone()]);
    assert_eq!(reserves[0], Amount::from(200_000_000u64));
    assert_eq!(reserves[1], Amount::from(200_000_000u64));

    let lp_supply_after: Amount =
        test.call_method(pool, "get_total_lp_supply", args![], vec![proof]);
    // 99_999_000 (bootstrap) + 99_999_000 (proportional add) = 199_998_000
    assert_eq!(lp_supply_after, Amount::from(199_998_000u64));
}

#[test]
fn remove_liquidity_returns_proportional_shares() {
    let mut test = TemplateTest::new(".", [".", "../soon_token"]);
    // The funded account has its OWN owner proof + secret key (separate from the
    // test's default identity). Withdrawing from the account requires authority
    // proven by the account's keypair, so we keep them and use them below.
    let (account, account_proof, account_sk) = test.create_funded_account();
    let (pool, _a_comp, res_a, _b_comp, res_b) =
        create_pool_with_initial_liquidity(&mut test, account);

    let proof = test.owner_proof();

    let lp_resource: ResourceAddress =
        test.call_method(pool, "get_lp_resource", args![], vec![proof.clone()]);

    // Burn half of our LP. The user got bootstrap_lp = 99_999_000 LP minted.
    // Total supply when we burn is also 99_999_000 (no other LPs minted).
    // amount_x_out = 50_000_000 * 100_000_000 / 99_999_000 = 50_000_500 (integer floor).
    let burn_amount = Amount::from(50_000_000u64);
    let expected_out = Amount::from(50_000_500u64);

    let tx = test
        .transaction()
        .call_method(account, "withdraw", args![lp_resource, burn_amount])
        .put_last_instruction_output_on_workspace("lp_in")
        .call_method(pool, "remove_liquidity", args![Workspace("lp_in")])
        .put_last_instruction_output_on_workspace("withdrawn")
        // Vec<Bucket> indexed access via "name.N"
        .call_method(account, "deposit", args![Workspace("withdrawn.0")])
        .call_method(account, "deposit", args![Workspace("withdrawn.1")])
        .build_and_seal(&account_sk);
    test.execute_expect_success(tx, vec![account_proof]);

    let reserves: Vec<Amount> =
        test.call_method(pool, "get_reserves", args![], vec![proof.clone()]);
    assert_eq!(
        reserves[0],
        Amount::from(100_000_000u64).checked_sub(expected_out).unwrap()
    );
    assert_eq!(
        reserves[1],
        Amount::from(100_000_000u64).checked_sub(expected_out).unwrap()
    );

    let lp_supply: Amount =
        test.call_method(pool, "get_total_lp_supply", args![], vec![proof]);
    assert_eq!(
        lp_supply,
        Amount::from(100_000_000u64 - 1000).checked_sub(burn_amount).unwrap()
    );

    // Silence unused warnings for resource addresses (kept for documentation).
    let _ = (res_a, res_b);
}

#[test]
fn rejects_foreign_resource_on_swap() {
    let mut test = TemplateTest::new(".", [".", "../soon_token"]);
    let (account, _, _) = test.create_funded_account();
    let (pool, _a_comp, _res_a, _b_comp, _res_b) =
        create_pool_with_initial_liquidity(&mut test, account);

    let (foreign_comp, _) = create_token(&mut test);

    let proof = test.owner_proof();
    let sk = test.secret_key().clone();

    let tx = test
        .transaction()
        .call_method(foreign_comp, "faucet", args![])
        .put_last_instruction_output_on_workspace("foreign")
        .call_method(pool, "swap", args![Workspace("foreign")])
        .build_and_seal(&sk);
    test.execute_expect_failure(tx, vec![proof]);
}
