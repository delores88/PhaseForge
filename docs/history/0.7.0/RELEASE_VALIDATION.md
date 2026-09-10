# PhaseForge 0.7.0 — observed validation record

Date: 2026-09-08. Complete incremental **source** release from the supplied
`PhaseForge-0.6.0-FULL.zip` (SHA-256
`f3f472efa86164914e4ae4b54c437ab560d31c5191df2a7bd276feac9779d732`).
All 197 baseline paths are retained. The standalone Discovery route is now a
compatibility redirect; its old data, APIs and advanced implementation are retained.
No application dependencies, database paths, OS keyring names or numerical solver
implementations were changed. The findings next-action descriptions did change.

Prior validation is saved in `docs/history/0.6.0/`; those old counts are not being
represented as current test execution. The final archive report separately records
its CRC, clean extraction, file counts, byte comparison and internal checksums.

## Checks executed in the packaging environment

| Check | Observed result | What this establishes |
|---|---:|---|
| Existing strict proposal-schema tests | 19 passed | JSON contract checks, not provider inference |
| Existing independent reference/integrity tests | 23 passed | Actual Python numerical replay and integrity checks |
| Existing independent verification-worker tests | 34 passed | Real numerical/control execution, isolated process and failure tests |
| Existing research/trajectory upgrade tests | 30 passed | Real independent measures plus schema/provenance regression checks |
| New manual-editor-to-numerics tests | 5 passed | Actual emitted JavaScript setup, strict shape, Python analytic decay and finer replay |
| Frontend API helpers | 15 passed | Real helpers with fetch doubles |
| Lab view-model helpers | 23 passed | Real helper calculations/contracts |
| Retained batch/discovery helpers | 29 passed | Real helper contracts and fetch doubles |
| Verification helpers/API | 41 passed | Real helpers with fetch doubles |
| Research/intent/radius/data helpers | 24 passed | Real helpers with the updated explicit intent list |
| New experiment workflow/API helpers | 33 passed | Consent, caps, selected context, error handling, single-flight, redirect, manual setup and request payloads |
| New actual JSX event-handler checks | 19 passed | Real component callbacks with deterministic hook doubles |
| New experiment static browser layouts | 56 passed | Chromium, seven states, dark/light, 1440/1024/540 px and a 560 px work pane |
| Updated optional-notes browser layouts | 24 passed | Actual notes JSX output in the existing themes/widths |
| Retained advanced-study browser layouts | 18 passed | Existing study/record/export static render functions |
| Retained verification browser layouts | 24 passed | Existing verification static render functions |
| Source/contract audit | 119 gates passed | Source integration, safety/consent plumbing and retained contracts; NOT compilation |
| Frontend syntax parsing | 51 files passed | TypeScript parser only; not npm resolution or a Next build |
| Rust module/include paths | 50 source files checked | Module and embedded fixture/source files exist |
| Rust/PowerShell lexical delimiter checks | 59 files passed | Balanced delimiters ignoring comments/strings; NOT native parsers |
| Python source compilation | 20 files passed | Syntax/byte compilation, not execution of all acceptance scripts |
| JSON/TOML/Bash checks | Passed | Source config parsing and `bash -n` for five shell scripts |
| Dependency/compatibility diff | Passed | Cargo/npm dependency sections, config paths, solver and frozen verifier bytes unchanged |

The unittest discovery run executed **111 tests**, with no failures or skips.
The JavaScript helper suites executed **165 checks**, plus **19 JSX handler checks**.
The browser checks covered **122 static layouts**. Counts are separated because they
are different kinds of evidence, not a single number of end-to-end tests.

The manual-editor test really emits a full ODE setup from the user-facing helper,
then executes its equations with the independent Python solver: for the analytic
fixture dx/dt=-x, x(0)=1, t=10, the expected endpoint is exp(-10). The normal and
finer-grid replays agree with that control. This is a software fixture, not a
scientific discovery or an embedded selectable demonstration.

JSX tests execute the actual Build, Build & run, Run saved setup, Stop, next-pass and
manual-editor callbacks. They verify that direct Results actions do not require an
API key and that failed/missing inputs do not silently run. They use hook doubles,
not React DOM reconciliation, real model responses or the native Rust server.

Static layout fixtures use the actual component render functions. The WebGL viewport
is an explicitly labeled placeholder, not a generated scientific scene. Screenshots
were visually inspected. Fixtures check expected content/action availability and
horizontal overflow; they do not establish full-page hydration, browser/server
integration, physical GPU rendering or provider model quality.

## Native / external checks supplied but not executed here

This environment has Python, Node, TypeScript and Chromium. It does not have
Rust/Cargo, Windows PowerShell/MSVC or resolved project npm packages. Registry DNS
was unavailable. Therefore none of these is claimed to have passed:

- Native Rust compilation, linking, unit tests or clippy;
- Full Next.js production build, React hydration or a live native frontend/backend session;
- Actual Windows installers, Windows keyring calls or physical GPU/driver execution;
- Live OpenAI/Anthropic generation or Europe PMC/Crossref requests;
- The newly supplied native-backend acceptance test.

There are **12 additional native Rust tests** in this release: seven direct
routing/budget/immutable-copy tests and five agent-service tests. The latter start
a local HTTP provider double and exercise the real proposal repair/accounting path,
provider-refusal handling, same-project request lock, user-authorized run with the
model's should_run=false, and build-only preservation. The numerical scheduling test
actually waits for the native ODE endpoint when run; it was not executable here.

`tests/experiment_runtime_smoke.py` starts a real built CPU backend in a temporary
data directory. It exercises local equation import/run, h/2 and doubled horizon,
exact source preservation, direct-action idempotency, conflicting request IDs,
wrong-project rejection, explicit run consent, no research-task prerequisites and
zero paid tokens. It is wired into Linux and Windows CI. A missing binary is not
a passing test. The CI workflow was not triggered from this conversation.

The root TXT installers retain real Cargo build/test and Next production-build
gates. Stop old servers, back up application data and keep the previous source
release until these native acceptance gates succeed on the target machine.

## Scientific boundaries

This is a workflow correction, not a new physical/biomedical engine. Hypothetical
parameters are allowed only as declared exploration; observations and clinical
claims must not be invented. Provider refusals and precise missing capabilities
remain visible. No chemistry/MD/QM, contact response, cure, efficacy, novelty or
physical truth is certified by this release or these tests. A finer or longer
same-solver run is not independent verification. Old results are not rewritten.
