# Security policy

OotleSwap is a testnet-only AMM. It has not been audited and should not be used
with funds you are unwilling to lose.

## Reporting a vulnerability

If you find a security issue, please report it privately by opening a
[GitHub security advisory](https://github.com/MrMooning/Soon-Swap/security/advisories/new)
on this repository. Do not open a public issue for security-relevant findings
until a fix is available.

When reporting, please include:

- A description of the issue and its impact
- Steps to reproduce (a failing integration test is ideal)
- Affected components/templates and versions

## Known properties of the design

The pool relies on these invariants — break any of them and reserves can desync,
funds can be lost, or the swap math becomes incorrect:

1. **No confidential deposits.** Every entry point that accepts a `Bucket`
   calls `bucket.assert_contains_no_confidential_funds()`. This is critical when
   trading against a stealth resource like tTARI — a confidential commitment
   would not appear in `Vault::balance()` and would silently desync the reserves.
2. **LP supply tracked in component state.** The pool does not query
   `ResourceManager::total_supply()` — it tracks LP issuance and burn manually
   in `lp_total_supply`. Mints and burns must always update this counter.
3. **Mint/burn authorization via internal admin badge.** The pool holds a
   single-NFT admin badge in its own vault and uses `vault.authorize()` to mint
   and burn LP tokens. The `ProofAuth` returned by `authorize()` MUST be bound
   to a `let _auth = ...` to keep the proof alive across the call —
   bare-statement use is a no-op and the burn will fail.
4. **`MINIMUM_LIQUIDITY=1000` burn on bootstrap.** Mitigates first-LP-attack
   share inflation. Removing this is a security regression.
5. **Cleartext math only on revealed reserves.** All swap and LP math operates
   on `Vault::balance()` which returns only the revealed sub-balance. Any
   change that introduces confidential math needs a complete redesign.

If a contribution touches any of these areas, call it out explicitly in the PR.
