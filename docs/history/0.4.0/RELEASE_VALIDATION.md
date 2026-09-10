# PhaseForge 0.4.0 — validation record

This is an incremental **source** release from the supplied 0.3.3 full project.
All baseline paths remain in the new archive. No npm or Cargo dependency versions
were upgraded, and the known-working install architecture was retained.

## Executed checks in this packaging environment

| Check | Result | What it actually establishes |
|---|---|---|
| Existing strict proposal-schema suite | 19 tests passed | JSON Schema contracts; not provider inference or native deserialization |
| Existing frontend API helpers | 15 checks passed | Actual helper code with local fetch doubles; no live server |
| Existing lab view-model functions | 23 checks passed | Deterministic UI evidence/telemetry/interpolation helpers |
| New discovery view-model/API helpers | 29 checks passed | Protocol validation and HTTP helper contracts with fetch doubles |
| Independent replay/integrity tools | 23 tests passed | Real Python implementation versus analytic/test controls; provenance, safe AST and payload integrity cases |
| Source/contract audit | 79 gates passed | Source structure, integration tokens, imports, known regressions and secret scanning |
| Real-JSX static layout fixtures | 18 layouts passed | New components rendered with minimal hook doubles, in Chromium at 1440/1024/540 px, dark/light themes, across three sections; no document-width overflow |
| JavaScript/JSX parse | All frontend files parsed | TypeScript parser; not npm resolution, Next compilation, hydration, or a live API test |
| JSON/TOML parse | Passed | Source configuration syntax |
| Bash syntax | Passed | `bash -n` on the existing Linux scripts |
| Rust module resolution and delimiter scan | Passed | Static structural checks only; not Rust type/borrow checking |
| ZIP/increment integrity | See acceptance below | Physical payload integrity, not scientific validity |

The layout fixtures execute new render functions but substitute React hooks and
network state. They are not full application tests. Screenshots were inspected
for the new responsive styling. The original 3D renderer is retained; actual
WebGL integration was not exercised as part of those layout fixtures.

## Native gates supplied but NOT executed here

The packaging environment does not contain Rust, Cargo, Windows PowerShell/MSVC,
or resolved project npm dependencies. Registry downloads were unavailable.
Consequently, **no native Cargo build/test, full Next.js production build,
Windows installer execution, real provider call, Crossref network request,
GPU/driver test, real discovery-campaign API run, or native research-ZIP export
was represented as passed in this environment.**

20 new native Rust unit tests are included: stratified sampling, DE selection,
archive/Pareto behavior, parameter whitelisting, refinement adjudication,
atomic revision reservations, notebook conflict rejection, signal diagnostics,
and ZIP writer contracts. They run with the installer’s `cargo test --all-targets`.

`tests/discovery_runtime_smoke.py` is an actual native integration acceptance
test. Once the binary exists it launches an isolated backend (no credentials or
network), executes four exploration trials and two refinement trials, verifies
study persistence and notebook conflicts, downloads the actual backend-generated
research ZIP, checks ZIP CRC and every payload hash, and runs the independent
Python replay against an actual output. This test is included in Windows/Linux
CI; it was not run here because there was no compiled backend binary.

The normal Windows TXT installer still runs native Cargo build/tests and the
real frontend production build. It stops on a failure rather than claiming
success from a static audit. Keep the working 0.3.3 installation and a data backup
until those native gates pass on your machine.

## Final archive acceptance procedure

1. Stage the exact incremental source tree; exclude `.git`, dependency/build
   directories, bytecode caches, databases, local credentials and transient logs.
2. Regenerate the internal `MANIFEST.sha256` from every packaged file except the
   manifest itself. Every original 0.3.3 project path must remain present.
3. Create a full standalone ZIP with a single `PhaseForge/` root.
4. Open and CRC-test the actual ZIP; extract to a new directory.
5. Compare every extracted payload byte-for-byte with the staging tree.
6. Verify every internal checksum and rerun all runnable tests from extraction.
7. Create the external SHA-256 checksum only after the final ZIP exists.

`PhaseForge-0.4.0-PACKAGE_REPORT.json`, distributed beside the project ZIP, records the observed
archive counts and checksums. No packaging check validates an author’s scientific
conclusion, novelty, data rights, or fitness for publication.
