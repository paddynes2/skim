#!/usr/bin/env bash
# All fork gates in one go. Exit non-zero on the first failure.
# Usage: bash scripts/fork/gates.sh [--no-build]
set -u
cd "$(dirname "$0")/../.."
M=src-tauri/Cargo.toml
run() { echo "== $*"; "$@" || { echo "FAILED: $*"; exit 1; }; }
run npm run check --silent
if [ "${1:-}" != "--no-build" ]; then run npm run build --silent; fi
run cargo fmt --check --manifest-path $M
run cargo clippy --all-targets --manifest-path $M -- -D warnings
run cargo test --manifest-path $M
if [ -f scripts/fork/contrast.mjs ]; then run node scripts/fork/contrast.mjs; fi
echo "ALL GATES GREEN"
