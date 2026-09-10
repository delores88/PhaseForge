# PhaseForge 0.6.0 — validation record

Date: 2026-09-08. Complete incremental **source** release from the uploaded
`PhaseForge-0.5.0-FULL(1).zip`. All177 baseline paths are retained. No Cargo/npm
dependency versions, backend/frontend frameworks, keyring locations or database
paths were changed. Prior validation is preserved under `docs/history/0.5.0/`;
it is not being represented as a new test run.

## Checks executed in this packaging environment

| Check | Observed result | Boundary |
|---|---:|---|
| Existing strict proposal schema |19 tests passed|JSON shape and contracts, not model quality|
| Existing independent reference tools |23 tests passed|Real Python numerical/integrity controls|
| Existing independent verification worker |34 tests passed|Real Euler/RK4, adaptive DP54, optional installed SciPy comparison, actual isolated subprocess and failure cases|
| New research/trajectory schema and numerical tests |30 tests passed|Real independent per-step reducers, alias/constraint use, first-passage censoring, finer grids, adaptive reference, source-version byte hashes|
| Frontend API helpers |15 checks passed|Actual helper code with fetch doubles|
| Existing lab helpers |23 checks passed|Real view-model/telemetry/renderer data helpers|
| Existing Discovery helpers |29 checks passed|Recipe/data/fetch helper contracts|
| Existing verification helpers |41 checks passed|Protocol, scope and API helpers|
| New research/intent/radius/data helpers |24 checks passed|Actual helper code and request payloads with fetch doubles|
| Chromium Research UI fixtures |24 layouts passed|Actual JSX render functions; light/dark, empty/populated/error,1440/1024/540px and560px-host narrow pane|
| Existing Discovery UI fixtures |18 layouts passed|Actual JSX static fixtures across themes/widths|
| Existing verification UI fixtures |24 layouts passed|Actual JSX static fixtures across themes/widths|
| JavaScript/JSX syntax |48 files passed|TypeScript parser, not framework compilation|
| Rust modules/embedded source paths |48 source files checked|Paths exist, not Rust type/borrow checking|
| Rust and PowerShell lexical checks |57 files passed|Balanced delimiters ignoring comments/strings, not native parsing/execution|
| Python/JSON/TOML/Bash source checks |Passed|Python compilation, configuration parsing and bash -n on5 scripts|
| Source/contract audit |See package report|Retained old gates plus additive integration checks; not a compiler|

The Python unittest discovery run executed **106 tests** with no failures or skips.
The optional SciPy comparison was available here; the application and verifier do
not require SciPy. New numerical tests actually integrate analytic controls and
check transient extrema, residence, entry counting, hysteresis, censoring, scoring
inputs, display-stride independence and finer grids. These controls are tests,
not scientific findings about the user's experiment.

Browser fixtures use actual render functions with minimal hook/network doubles.
React fragments, full task cards, preflight details and both states are rendered;
expected content is asserted as well as horizontal-overflow checks. Screenshots
were inspected. They are NOT a running Next application and do not establish
hydration, live form submission, Rust routes or WebGL/physical GPU correctness.

## Native and service checks not executed here

This container has Python, Node and Chromium but no Rust/Cargo, Windows PowerShell
or MSVC. Registry DNS access is unavailable, and project npm dependencies are not
resolved here. Therefore **no native Rust compile/test, full Next production build,
Windows install, live provider/source request or physical GPU run is claimed to pass**.
A source audit cannot replace those tests.

33 new native unit tests are supplied for reducers, actual ODE scoring/capture,
resolution levels, radius metadata,128-state capacity, legacy field serialization,
plan validation/provenance, CSV parsing and compute budgets. They have NOT run here.
`tests/research_runtime_smoke.py` launches an actual built backend, not an API double,
and checks numerical results, exact h/2+h/4 grids, radii, recorded compute admission,
CSV provenance, public-query consent and zero paid tokens. It is wired into existing
Windows/Linux CI alongside prior native smoke tests; that CI was not run here.

The supplied self-contained PowerShell TXT installers retain their actual Cargo
build/test and Next production-build gates. Keep the previous version and a data
backup until installation passes. Unknown frozen verifier hashes are rejected;
original0.5.0 sources are retained verbatim for supported existing protocols.

## Archive acceptance

The final source ZIP is CRC-tested, clean-extracted, byte-compared to staged source,
and verified against regenerated internal SHA-256 entries. Source files and all
baseline paths are checked; runtime data, credentials, dependency/build directories,
bytecode, logs and private user run exports are excluded. Executable Python/helper
and source suites are repeated from the clean extraction. The external package
report records exact counts, bytes, hashes and outcomes from the completed archive.

## Scientific limits

No experiment outcome or novelty certificate follows from these engineering tests.
New renderer radii are not material collision physics. Reductions are sampled, not
continuous root finding; missing first events are censored, not absent forever.
A programme, abstract, profile or green numerical check does not demonstrate a cure,
clinical safety, causal explanation, physical truth, convergence order or chaos.
The new workflow enables useful scoped work while keeping missing engines, evidence
and external validation visible instead of pretending the whole goal was achieved.
