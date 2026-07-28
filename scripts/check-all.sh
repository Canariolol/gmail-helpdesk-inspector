#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Tracked secret patterns"
./scripts/check-secrets.sh

echo "==> Rust fmt"
cargo fmt --manifest-path apps/api/Cargo.toml -- --check

echo "==> Rust tests"
cargo test --manifest-path apps/api/Cargo.toml

echo "==> Rust clippy"
cargo clippy --manifest-path apps/api/Cargo.toml --all-targets -- -D warnings

echo "==> Rust release build"
cargo build --release --manifest-path apps/api/Cargo.toml

echo "==> AI worker tests"
uv run --directory apps/ai-worker --frozen --all-extras pytest tests

echo "==> Web dependency audit"
npm --prefix apps/web audit --omit=dev

echo "==> Web build"
npm --prefix apps/web run build

echo "==> All checks passed"
