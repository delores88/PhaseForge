# Reproduce the release checks

The app's main Windows installer does not require Python or JavaScript test tools.
These are optional developer checks. Existing dependency versions are unchanged.

## Native gates (run on a provisioned developer machine)

```text
cd backend
cargo check --all-targets
cargo test --all-targets
cargo build
```

Then, from the project root with Python 3.10+ and the optional `jsonschema` test dependency:

```text
python tests/runtime_smoke.py --binary backend/target/debug/phaseforge-backend.exe
python tests/discovery_runtime_smoke.py --binary backend/target/debug/phaseforge-backend.exe
```

Use the same paths without `.exe` on Linux. The tests launch isolated temporary
backends with GPU disabled; do not point them at a production database.
They do not call an AI provider or Crossref. Windows/Linux CI contains both tests.

## Independent Python checks (standard library)

```text
python tests/test_reference_tools.py
```

## Schema and UI helper checks

```text
python tests/test_proposal_schema.py
node tests/frontend-api.mjs
node tests/lab-model.mjs
node tests/discovery-model.mjs
node scripts/release-audit.mjs
```

These are real helper/schema tests but use fetch doubles where noted. They do not
replace native compilation or a full application integration test.

## Actual frontend build

```text
cd frontend
npm install --no-audit --no-fund
npm run build
```

The supplied self-contained Windows installer performs this build. Source exports
lack generated lockfiles when registries are unavailable; the first install
resolves them. Commit those generated lockfiles for the tested deployment.

## Optional static-layout fixtures

With a TypeScript compiler available (set `TYPESCRIPT_PATH` to its `typescript.js`
when global module resolution cannot find it), Playwright, and Chromium:

```text
node tests/render_discovery_fixtures.cjs build/layout-fixtures
python tests/check_discovery_layout.py --folder build/layout-fixtures --chromium PATH_TO_CHROMIUM
```

These execute new JSX render paths with synthetic data and minimal hook doubles,
then check layout in Chromium. They are not a React hydration, model-provider,
WebGL or real-backend test. Synthetic fixture output is not scientific evidence.
