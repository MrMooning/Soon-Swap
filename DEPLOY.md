# OotleSwap — Esmeralda testnet deploy runbook

This is the end-to-end procedure to publish both templates and create your first $SOON / tTARI pool.

## Prerequisites

- Tari Ootle Wallet Daemon (`tari_ootle_walletd`) running locally
- Wallet Web UI accessible at `http://127.0.0.1:5100`
- A wallet account funded with tTARI (use the in-UI faucet)

## 1. Build the WASM artifacts

The two templates are independent packages (no Cargo workspace — `tari_template_test_tooling`
expects per-package `target/` directories). Build them with:

```bash
cd ootleswap
./build.sh
```

Or build each manually:

```bash
(cd pool && cargo build --target wasm32-unknown-unknown --release)
(cd soon_token && cargo build --target wasm32-unknown-unknown --release)
```

Outputs:

- `pool/target/wasm32-unknown-unknown/release/ootleswap_pool.wasm` (~360 KB)
- `soon_token/target/wasm32-unknown-unknown/release/soon_token.wasm` (~285 KB)

## 2. Publish $SOON token template

In the Wallet Web UI:

1. **Home → Publish Template**
2. **Fee account:** select your funded account
3. **Upload:** `target/wasm32-unknown-unknown/release/soon_token.wasm`
4. **Estimate Fee** → **Publish Template**
5. Copy the **template address** from the Templates sidebar — this is `SOON_TEMPLATE_ADDRESS`

## 3. Publish the Pool template

Same flow with `target/wasm32-unknown-unknown/release/ootleswap_pool.wasm`.
Copy the resulting address — this is `POOL_TEMPLATE_ADDRESS`.

## 4. Create the $SOON token component

In the Wallet Web UI:

1. **Templates → SOON_TEMPLATE_ADDRESS → Call Function**
2. Select function `new`, no arguments
3. Pay fee from your funded account
4. Submit

From the receipt, extract:
- **Component address** of the `SoonToken` component → `SOON_COMPONENT`
- **Resource address** of the SOON token (the new public-fungible resource minted on creation) → `SOON_RESOURCE`

You can find both by clicking the transaction receipt in the UI. The resource is the
non-LP-admin resource that appeared in the upped substates.

## 5. Create the Pool component

The pool's `new` takes two `ResourceAddress` arguments. Use:

- `resource_a = SOON_RESOURCE`
- `resource_b = STEALTH_TARI_RESOURCE_ADDRESS` (a.k.a. `TARI_TOKEN`, the constant `resource_010101...01`)

> The native tTARI resource address is the same on every Tari network — it's the
> well-known constant `resource_010101010101010101010101010101010101010101010101010101010101010101`.

In the Web UI:

1. **Templates → POOL_TEMPLATE_ADDRESS → Call Function**
2. Function: `new`
3. Args: paste the two resource addresses above
4. Submit. Note the new component address → `POOL_COMPONENT`

## 6. Bootstrap initial liquidity

> ⚠️ **Critical:** when depositing tTARI, ensure you withdraw it from your account
> as **revealed funds**, not confidential. The pool will reject confidential
> deposits via `assert_contains_no_confidential_funds()`.

Build a manifest (or use the wallet CLI) that:

1. Calls `faucet()` on `SOON_COMPONENT` enough times to get the desired SOON amount onto the workspace
2. Withdraws the desired amount of revealed tTARI from your account onto the workspace
3. Calls `add_liquidity(soon_bucket, tari_bucket)` on `POOL_COMPONENT`
4. Deposits the returned LP bucket into your account

Example wallet-CLI call (rough — adjust syntax to your CLI version):

```bash
# 100 SOON + 1 tTARI → bootstraps pool, mints sqrt(100e6 * 1e6) = 1e7 LP
# (with MINIMUM_LIQUIDITY=1000 burned)
tari_ootle_wallet_cli transactions submit-manifest bootstrap.tari \
    --fee-account my-account
```

## 7. Test a swap

```bash
# Swap 0.1 tTARI for SOON
tari_ootle_wallet_cli transactions submit call-method \
    POOL_COMPONENT swap \
    -a "Bucket(account_withdraw(tari, 100000))" \
    --fee-account my-account
```

Or use the Web UI's component-method UI on `POOL_COMPONENT`.

## Reading pool state

Read-only methods (free to call from any explorer/indexer):

- `get_reserves() -> [Amount, Amount]` — `[reserve_soon, reserve_tari]`
- `get_total_lp_supply() -> Amount`
- `get_lp_resource() -> ResourceAddress`
- `quote_swap(in_resource, amount_in) -> Amount` — preview swap output

## Events

Every successful swap emits `OotleSwap.Pool.Swap` with `in_resource`, `amount_in`,
and `amount_out` — useful for indexers / price feeds.

## Known limitations of v0.1

- One pool per (resource_a, resource_b) instance — no pool factory yet.
- Pools only work with **public-fungible** and **revealed-only** stealth deposits.
- Add-liquidity does NOT refund excess: deposit in proportion or accept rounding loss.
- No multi-hop routing.
- No flash swaps.
