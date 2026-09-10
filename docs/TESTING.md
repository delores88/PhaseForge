# PhaseForge testing

For current 0.5.0 additions and native end-to-end acceptance, see `TESTING_0.5.md`.
The laboratory acceptance below is retained from 0.3.3.

## Checks that can run without Rust or npm dependencies

From the project root:

```text
python tests/test_proposal_schema.py
node tests/frontend-api.mjs
node tests/lab-model.mjs
node scripts/release-audit.mjs
```

The Python suite needs Python 3 and `jsonschema` (development testing only, not a
runtime dependency). It executes 19 checks against the shipped provider schema
and generator. The Node API suite executes 15 checks against the actual API
helper with a fetch double, not live provider requests. The lab view-model suite
executes 23 checks against the shipped pure helpers: missing/zero counters,
legacy challenge statuses, phase selection, timestamp interpolation, finite
frames, and display memory bounds. The 61 source-audit gates are structural
checks, not compilation or runtime validation.

## Native Rust compilation and numerical tests

On a machine with Rust, linker/build tools, and registry access:

```text
cd backend
cargo generate-lockfile
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo build --locked
```

The Windows `.txt` installers already resolve a lockfile when needed, build all
targets, and execute all Rust tests. This release adds 13 native Rust tests:
seven metric-adjudication/constraint/perturbation tests, three GPU CSV-parser
regressions, and three ODE execution/visual-mapping regressions. Prior native
schema, accounting, and numerical tests remain. They were **not executed in the
packaging environment**, which has no Rust/Cargo toolchain or registry access.

## Real isolated-backend integration test

After a successful native build, from the project root:

```text
python tests/runtime_smoke.py --binary backend/target/debug/phaseforge-backend
```

On Windows append `.exe` to the binary name. Python and `jsonschema` are required
for this developer-only integration test. It starts a CPU-only backend on port
17433 using a temporary, isolated data_directory. It imports a test-only generic
constant-state experiment and checks actual API responses for completed numerical
execution, constraints, three distinct perturbations, inconclusive no-rule
challenges, multiple rendered entity records, findings, lightweight workflow
status, and resource response shape. No API credential or model call is used.
The packaging environment could not execute this integration test.

## Frontend integration

```text
cd frontend
npm install --no-audit --no-fund
npm run build
```

The dependency versions are unchanged from 0.3.2. No npm packages were fetched and
no full Next.js build ran in the packaging environment. JavaScript/JSX parsing
and local-import resolution were checked separately. The packaging layout
fixtures use actual component markup/styles with static hook snapshots: they
exercise Chromium layout but not React hydration, backend traffic, or WebGL.

## CI and manual acceptance

`.github/workflows/ci.yml` defines Windows/Linux Cargo checks/tests, the isolated
backend smoke test, and a separate frontend install/build job. Adding the workflow
does not mean CI has run or passed.

Before treating the release as validated on a target machine, verify:

- Install and start in a fresh folder; confirm backend version 0.5.0.
- Existing history/key/model settings still load; no paid call is made on opening
  Findings. Old green score-only challenge flags are inconclusive.
- Replay an old manifest and inspect actual baseline/trial values. A missing
  comparison rule stays inconclusive; a failed check cannot turn green.
- Request one AI explanation; confirm one usage-ledger record and cached reuse.
- Prepare a next-step proposal; confirm no run appears until explicitly approved.
- Stop during model generation, CPU work, GPU work, and queued compute.
- Inspect CPU/RAM and GPU/VRAM alongside native OS tools. Unsupported counters
  must remain unavailable; GPU detection must not imply per-run GPU utilization.
- Play/pause/scrub, inspect all mapped entities, switch XY/3D, resize, change theme,
  reload a lost WebGL context, and export the research bundle.

The current execution and packaging record is `docs/RELEASE_VALIDATION.md`.


## v0.7 regression tests

- `node tests/experiment-model.mjs`: actual UI/API helper contracts with fetch doubles.
- `node tests/experiment-ui.cjs`: actual JSX event handlers with deterministic hook doubles
  (requires developer TypeScript; not React DOM hydration).
- `python tests/test_experiment_editor.py`: the emitted JS manual setup, strict schema
  and real independent numerical replay against an analytic control.
- `node tests/render_experiment_fixtures.cjs <folder>` then
  `python tests/check_experiment_layout.py --folder <folder>`: optional developer
  TypeScript, Playwright and Chromium static-layout checks. The viewport is a labeled
  placeholder; these tests do not establish native solver or physical GPU behavior.
- `cargo test --all-targets`: includes native direct intent/budget/copy tests and real
  agent-service tests with a local HTTP provider double, actual scheduler execution,
  refusal handling and repair. No real model key is used by these fixtures.
- `python tests/experiment_runtime_smoke.py --binary <built-backend>`: real Rust HTTP
  acceptance test covering direct runs, h/2, doubled horizon, immutable source,
  request idempotency, project scope, absence of task gates and zero paid tokens.

A missing native binary is a missing prerequisite, not a passed or mocked test.
