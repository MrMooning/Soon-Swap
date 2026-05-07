# Contributing to OotleSwap

Thanks for your interest! This is a small project — contributions, issues, and
ideas are welcome.

## Development setup

You need:

- Rust stable + the `wasm32-unknown-unknown` target:
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- A working C/C++ toolchain (for transitive build deps of `tari_template_test_tooling`)
- For Esmeralda deployment testing: a running `tari_ootle_walletd` (linux/macOS prebuilts on the [tari-ootle releases page](https://github.com/tari-project/tari-ootle/releases))

The two templates are independent Cargo packages on purpose — `tari_template_test_tooling`
expects each template's WASM to live in its own `target/` directory, which a Cargo
workspace would prevent.

## Build

```bash
./build.sh
# or per-package:
(cd pool && cargo build --target wasm32-unknown-unknown --release)
(cd soon_token && cargo build --target wasm32-unknown-unknown --release)
```

## Test

```bash
(cd pool && cargo test --release --test integration)
(cd soon_token && cargo test --release)
```

The pool's `tests/integration.rs` covers bootstrap, proportional add-liquidity,
remove-liquidity, swap k-preservation, swap quote correctness, and foreign-resource
rejection. New behavior should ship with a corresponding integration test.

## Pull requests

- Branch off `main`
- Keep changes focused — one PR per concern
- Include integration test coverage for behavior changes
- CI must be green before merge

## Code style

Standard `rustfmt` defaults. No special conventions.

## Reporting bugs

Open an issue with:
- A description of expected vs actual behavior
- Steps to reproduce (manifest text + relevant addresses if testnet-related)
- Output of `cargo test --release` if a test is involved
