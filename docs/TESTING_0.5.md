# Verification and discovery acceptance (0.5.0)

## Runnable without a Rust build

From the project root:

```text
python tests/test_proposal_schema.py
node tests/frontend-api.mjs
node tests/lab-model.mjs
node tests/discovery-model.mjs
python tests/test_reference_tools.py
node tests/verification-model.mjs
python tests/test_verification_worker.py
node scripts/release-audit.mjs
```

The proposal-schema suite needs the developer-only `jsonschema` Python package.
The standalone verifier and its tests need only Python 3.10+ standard library;
an additional SciPy DOP853 comparison runs when SciPy happens to be installed.
Neither package is a new runtime dependency of the Rust backend/frontend.

The verifier tests actually execute both independent implementations, analytic
controls, the isolated process CLI, checkpoints, source/input hashes, evaluation
budgets and deliberately unsuccessful comparisons. They do not compile or test
the Rust orchestration service. View-model/API tests use local fetch doubles.

## Native acceptance after building

```text
cd backend
cargo check --all-targets
cargo test --all-targets
cargo build
cd ..
python tests/runtime_smoke.py --binary backend/target/debug/phaseforge-backend
python tests/discovery_runtime_smoke.py --binary backend/target/debug/phaseforge-backend
python tests/verification_runtime_smoke.py --binary backend/target/debug/phaseforge-backend
```

Use `phaseforge-backend.exe` on Windows. The integration suites isolate app data,
choose local test ports and make no paid calls. The new verification test runs a
real novelty-directed campaign, performs an actual separate control run, freezes
and approves a verification dossier, checks independent solver/control/holdout
results, records an inconclusive human review, requests a recorded data window,
and validates the actual backend-generated research ZIP and its checksums. It
also confirms that missing literature/review evidence never becomes a novelty
certificate. A native binary and Python with `jsonschema` are prerequisites.

26 new native Rust unit tests cover the verification service/comparison contracts
and novelty-directed campaign selection; these are included, not represented as
executed in the packaging container. The Windows/Linux CI jobs execute them and
the three real integration suites when run in an appropriate environment.

## Browser acceptance

After `npm install` and `npm run build`, exercise the actual application. The
optional layout fixtures render real JSX with minimal hook/network doubles;
they measure document overflow and important headings in Chromium, not React
hydration, user interaction, network behavior, WebGL, or server execution.

The original laboratory must still open without a forced question modal. Check
single-sidebar navigation, both themes, provider key/model separation, chat
cancellation, usage limits, old experiment loading and Findings before testing
the new Verify & compare workflow. Cancel verification during interpreter
probing and during each numerical phase; completed checkpoints may survive,
but incomplete results must remain visibly incomplete. Start again by freezing
a new dossier, not overwriting a previous attempt. Confirm data persistence
after a backend restart and that no compute automatically resumes.

## Required pre-publication acceptance

Compare against domain-relevant independently established controls, test
scientifically justified feature scales/tolerances and preparation assumptions,
examine relevant source texts, preserve negative findings and have a named
scientist record limitations. Source audits and software tests cannot establish
scientific novelty, correctness of a physical model or publication acceptance.
