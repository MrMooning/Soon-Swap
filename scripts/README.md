# Example wallet manifests

These are ready-to-paste manifests for the Tari Ootle Wallet's **Manifest** view.

Before submitting any manifest, set the **Globals** in the wallet UI to point at
the live components on your network. Replace the values below with your own
addresses if you've deployed your own pool / token.

## Globals to set in the Wallet UI

| Key | Value (replace with your own) |
|---|---|
| `account` | `component_<your account component address>` |
| `soon` | `component_7c20414944194b905f9f63c73f479c80bf03627483276cf51f0f8a8c08a3b8fd` |
| `pool` | `component_3ab560338b91343b1a6ec1ccb21e47b23b3743ee72475603a0b0d1f41c147e40` |

## Manifests

- [`bootstrap.tari`](./bootstrap.tari) — bootstrap a fresh pool with 100 SOON + 10 tTARI
- [`add_liquidity.tari`](./add_liquidity.tari) — proportional add (50 SOON + 5 tTARI by default)
- [`swap_tari_for_soon.tari`](./swap_tari_for_soon.tari) — swap 1 tTARI → SOON
- [`swap_soon_for_tari.tari`](./swap_soon_for_tari.tari) — swap 10 SOON → tTARI
- [`remove_liquidity.tari`](./remove_liquidity.tari) — burn 5,000,000 LP, return shares of both reserves
- [`faucet_soon.tari`](./faucet_soon.tari) — get 100 SOON without doing anything else
- [`get_reserves.tari`](./get_reserves.tari) — read-only: show pool reserves

## Fee

Use a generous fee (e.g. `50000` microTARI) when submitting. Estimation can be
off by 1 unit and a too-tight estimate causes `InsufficientFeesPaid`.
