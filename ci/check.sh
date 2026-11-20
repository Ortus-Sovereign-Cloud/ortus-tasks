#!/usr/bin/env bash
# Every check CI runs, in the same order. Run it before each push.
# Storage and file tests also need PostgreSQL and S3 (ORTUS_TEST_POSTGRES,
# ORTUS_TEST_S3_*); CI requires them.
set -euo pipefail
cd "$(dirname "$0")/.."
ci/fetch.sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check
ci/licence-gate-selftest.sh
echo "all checks passed"
