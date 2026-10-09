#!/usr/bin/env bash
# Build the renderer, run its tests, render previews, and run the fake T1HW smoke test.
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --locked
cargo test --release --locked
./target/release/t1-dash preview previews
if command -v python3 >/dev/null; then python3 tests/fake_t1hw.py target/release/t1-dash previews; fi
printf 'built: %s\n' "$PWD/target/release/t1-dash"
