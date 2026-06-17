#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Rust fmt"
cargo fmt --manifest-path apps/api/Cargo.toml -- --check

echo "==> Rust tests"
cargo test --manifest-path apps/api/Cargo.toml

echo "==> Rust clippy"
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings

echo "==> AI worker tests"
python3 -m pytest apps/ai-worker/tests

echo "==> Web build"
npm --prefix apps/web run build

echo "==> All checks passed"
