#!/usr/bin/env bash
# Proves the licence gate works: a throwaway crate that depends on an
# MPL-2.0 crate must fail `cargo deny check licenses` under our deny.toml.
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src"
cat > "$tmp/Cargo.toml" <<TOML
[package]
name = "licence-gate-selftest"
version = "0.0.0"
edition = "2021"
license = "MIT OR Apache-2.0"
publish = false

[dependencies]
option-ext = "0.2"   # MPL-2.0: must be refused
TOML
echo '' > "$tmp/src/lib.rs"
cp "$root/deny.toml" "$tmp/deny.toml"
cd "$tmp"
cargo generate-lockfile --quiet
if cargo deny check licenses >"$tmp/out.txt" 2>&1; then
    echo "licence gate FAILED: an MPL-2.0 dependency was accepted" >&2
    exit 1
fi
grep -q "MPL-2.0" "$tmp/out.txt" || { echo "licence gate failed for an unexpected reason:" >&2; cat "$tmp/out.txt" >&2; exit 1; }
echo "licence gate works: MPL-2.0 dependency refused"
