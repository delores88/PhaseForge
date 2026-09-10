#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MINIMUM_NODE="20.9.0"

if ! command -v node >/dev/null; then
  echo "Install Node.js ${MINIMUM_NODE} or newer (Node.js 22+ recommended), then rerun this script." >&2
  exit 1
fi

if ! node -e 'const [M,m,p]=process.versions.node.split(".").map(Number); process.exit(M>20 || (M===20 && (m>9 || (m===9 && p>=0))) ? 0 : 1)'; then
  echo "Node.js ${MINIMUM_NODE} or newer is required; detected $(node --version)." >&2
  exit 1
fi

cd "$ROOT/frontend"
[ -f .env.local ] || cp .env.example .env.local
if [ -f package-lock.json ]; then npm ci --no-audit --no-fund; else npm install --no-audit --no-fund; fi
npm run build

echo "Frontend installed. Start with ./scripts/start-frontend.sh"
