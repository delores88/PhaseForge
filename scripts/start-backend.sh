#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export RUST_LOG="${RUST_LOG:-phaseforge_backend=info,tower_http=info}"
cd "$ROOT/backend"
cargo run "$@"
