#!/usr/bin/env bash
# Every check CI runs, in the same order. Run it before each push.
# Storage and file tests also need PostgreSQL and S3 (ORTUS_TEST_POSTGRES,
# ORTUS_TEST_S3_*); CI requires them.
set -euo pipefail
cd "$(dirname "$0")/.."
ci/fetch.sh
# The web client: type-checked against the pinned core's kit, then built
# into web/dist, which the crate embeds. Logical CSS only (right to left
# works), and no style attributes (the CSP forbids them).
if grep -nE '(margin|padding|border)-(left|right)|^[[:space:]]+(left|right):|text-align: *(left|right)|float: *(left|right)|[[:space:]]style=' web/src/*.ts; then
  echo "physical CSS or a style attribute in web/src" >&2
  exit 1
fi
(cd web && npm ci --silent && npm audit --omit=dev --audit-level=moderate && npx tsc --noEmit && npm run build --silent)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check
ci/licence-gate-selftest.sh
# The screens in a real browser, against the core with this app.
cargo build --examples
(cd web && npx playwright test)
echo "all checks passed"
