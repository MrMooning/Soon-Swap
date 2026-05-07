#!/usr/bin/env bash
# Build both templates to WASM. Each package builds independently because
# tari_template_test_tooling expects per-package target directories.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

for pkg in pool soon_token; do
    echo ">>> Building $pkg"
    (cd "$SCRIPT_DIR/$pkg" && cargo build --target wasm32-unknown-unknown --release)
done

echo
echo "Artifacts:"
ls -lh "$SCRIPT_DIR/pool/target/wasm32-unknown-unknown/release/ootleswap_pool.wasm"
ls -lh "$SCRIPT_DIR/soon_token/target/wasm32-unknown-unknown/release/soon_token.wasm"
