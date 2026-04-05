#!/usr/bin/env bash
set -euo pipefail
FUZZ_DIR="$(cd "$(dirname "$0")" && pwd)"
echo "== cargo fuzz run pipeline (smoke) =="
cargo +nightly fuzz run pipeline --fuzz-dir "$FUZZ_DIR" -- -runs=1
echo "OK"
