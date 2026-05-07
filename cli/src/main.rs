//! soonswap-cli — demo binary that exercises the live $SOON / tTARI pool on Esmeralda.
//!
//! This is intentionally single-shot: it generates a fresh wallet, claims tTARI
//! from the faucet, swaps 1 tTARI for SOON, and prints the receipt. No state is
//! persisted between runs.
//!
//! For programmatic interaction beyond a swap, see the manifests in `scripts/`
//! and load them through the Wallet UI.

use std::str::FromStr;

use anyhow::{Context, Result};
use ootle_rs::{
    Network,
    ToAccountAddress,
    TransactionRequest,
    builtin_templates::{
        UnsignedTransactionBuilder,
        component::{IComponent, TransactionBuildable},
        faucet::IFaucet,
    },
    default_indexer_url,
    key_provider::PrivateKeyProvider,
    provider::{Provider, ProviderBuilder, WalletProvider},
    wallet::OotleWallet,
};
use tari_ootle_transaction::args;
use tari_template_lib_types::{Amount, ComponentAddress, constants::TARI_TOKEN};

const NETWORK: Network = Network::Esmeralda;

/// Live pool component on Esmeralda (deployed at v0.1.0 of OotleSwap).
const POOL_COMPONENT: &str =
    "component_3ab560338b91343b1a6ec1ccb21e47b23b3743ee72475603a0b0d1f41c147e40";

/// Amount of tTARI to swap (in micro-units; divisibility 6).
/// 1_000_000 = 1 tTARI.
const SWAP_AMOUNT_MICRO: u64 = 1_000_000;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== OotleSwap CLI demo ===");
    println!("Network: {NETWORK:?}");
    println!();

    // 1. Fresh wallet
    let signer = PrivateKeyProvider::random(NETWORK);
    let wallet = OotleWallet::from(signer);
    let mut provider = ProviderBuilder::new()
        .wallet(wallet)
        .connect(default_indexer_url(NETWORK))
        .await
        .context("connecting to Esmeralda indexer")?;

    let address = provider.default_signer_address().clone();
    println!("Wallet address:");
    println!("  {address}");
    println!();

    // 2. Faucet — gives us 1000 tTARI and creates the account if missing.
    println!("Step 1/2: claiming faucet funds...");
    let unsigned_tx = IFaucet::new(&provider)
        .take_faucet_funds()
        .pay_fee(2_000u64)
        .prepare()
        .await
        .context("preparing faucet transaction")?;

    let tx = TransactionRequest::default()
        .with_transaction(unsigned_tx)
        .build(provider.wallet())
        .await
        .context("signing faucet transaction")?;

    let pending = provider
        .send_transaction(tx)
        .await
        .context("submitting faucet transaction")?;
    let outcome = pending
        .watch()
        .await
        .context("waiting for faucet transaction to commit")?;
    println!("  ✓ funded. tx outcome: {outcome:?}");
    println!();

    // 3. Swap 1 tTARI → SOON via the live pool.
    println!("Step 2/2: swapping {} micro-tTARI for SOON via {POOL_COMPONENT}...",
        SWAP_AMOUNT_MICRO);
    let pool: ComponentAddress =
        ComponentAddress::from_str(POOL_COMPONENT).context("parsing pool component")?;
    let account = address.to_account_address();

    let unsigned_tx = IComponent::new(&provider)
        // Withdraw revealed tTARI from our account
        .call_method(
            account,
            "withdraw",
            args![TARI_TOKEN, Amount::from(SWAP_AMOUNT_MICRO)],
        )
        .put_last_instruction_output_on_workspace("input")
        // Swap via the pool
        .call_method(pool, "swap", args![Workspace("input")])
        .put_last_instruction_output_on_workspace("output")
        // Deposit the received SOON back to our account
        .call_method(account, "deposit", args![Workspace("output")])
        .pay_fee(5_000u64)
        .prepare()
        .await
        .context("preparing swap transaction")?;

    let tx = TransactionRequest::default()
        .with_transaction(unsigned_tx)
        .build(provider.wallet())
        .await
        .context("signing swap transaction")?;

    let pending = provider
        .send_transaction(tx)
        .await
        .context("submitting swap transaction")?;
    let outcome = pending
        .watch()
        .await
        .context("waiting for swap transaction to commit")?;
    println!("  ✓ swap committed. tx outcome: {outcome:?}");

    // Pull the receipt for swap event details.
    let receipt = pending
        .get_receipt()
        .await
        .context("fetching swap receipt")?;
    println!("  fees paid: {}", receipt.fee_receipt.total_fees_paid());
    if !receipt.events.is_empty() {
        println!("  events:");
        for event in receipt.events.iter() {
            println!("    {} {}", event.topic(), event.payload());
        }
    }
    println!();
    println!("Done. Your fresh wallet now holds whatever SOON the pool gave you for 1 tTARI.");
    Ok(())
}

// `Provider` and `WalletProvider` are imported but only their methods used implicitly.
#[allow(dead_code)]
fn _silence_unused_provider_imports(p: &impl Provider) {
    let _ = p.network();
}

#[allow(dead_code)]
fn _silence_unused_walletprovider_imports<P: WalletProvider>(p: &P) {
    let _ = p.wallet();
}
