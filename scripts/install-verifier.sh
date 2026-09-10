#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PYTHON="${PHASEFORGE_VERIFIER_PYTHON:-python3}"
"$PYTHON" -I -c 'import sys; assert sys.version_info >= (3,10), "Python 3.10+ required"'
if [[ ! -f "$ROOT/.phaseforge-verifier/bin/python" ]]; then
  "$PYTHON" -m venv --without-pip "$ROOT/.phaseforge-verifier"
fi
"$ROOT/.phaseforge-verifier/bin/python" "$ROOT/tests/test_verification_worker.py"
printf '\nIndependent verifier installed. Restart PhaseForge and check the worker in Verify & compare.\n'
