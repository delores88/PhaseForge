#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/frontend"
npm run dev -- --hostname 127.0.0.1 --port "${PHASEFORGE_FRONTEND_PORT:-3000}"
