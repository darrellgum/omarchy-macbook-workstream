#!/usr/bin/env bash
# Build the renderer, run its tests, render previews, and run the fake T1HW smoke test.
# The Grok Bot integration is enabled only when assets/marks.json exists locally (see the docs).
set -euo pipefail
cd "$(dirname "$0")"
features=()
if [[ -f assets/marks.json ]]; then features=(--features grok-bot); fi
cargo build --release --locked "${features[@]}"
cargo test --release --locked "${features[@]}"
./target/release/t1-dash preview previews
if command -v python3 >/dev/null; then python3 tests/fake_t1hw.py target/release/t1-dash previews; fi
printf 'built: %s (%s)\n' "$PWD/target/release/t1-dash" "${features[*]:-generic}"
