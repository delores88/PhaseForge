# PhaseForge 0.3.3 — release validation

Recorded: 2026-09-06T23:19:52.243032+00:00

## Exact source and scope

This is an incremental release from the actual `PhaseForge-0.3.2-FULL.zip` archive
(SHA-256 `846a56122a213e2b609c9554a3eaa61e6b273e067f67bbe851d98c7560b24038`). All 106 original project files are
retained, with changes and additions described in RELEASE_NOTES_0.3.3.md. The
v0.3.2 `backend/src/agent/schema.rs` compiler-return correction is preserved
byte-for-byte. No frontend or backend dependency upgrades were introduced.

## Checks actually executed

| Check | Result | What was exercised |
|---|---:|---|
| Provider schema tests | 19 passed | Real schema/generator; nested manifests, complete integration fields, entity mappings and explicit challenge checks |
| Frontend API helper | 15 passed | Shipped JavaScript helper with local fetch doubles; not a live API |
| Lab view-model helper | 23 passed | Real resource, status, progress, interpolation, invalid-data and render-budget logic |
| Release source audit | 61 passed | Structural contracts and secret/named-case scans; not compilation |
| JavaScript/JSX syntax | 37 files, zero errors | TypeScript parser, not a Next.js build |
| Local frontend imports | 68 resolved | Local file paths; external package resolution was not tested |
| JSON / TOML | 4 / 3 parsed | Actual file syntax |
| Python / Bash syntax | 3 / 4 files passed | AST parsing / bash -n |
| Rust lexical/module checks | 28 sources / 26 modules | Balanced lexical delimiters and module paths, not Rust type checking |
| Windows helpers | 8 structurally inspected | Complete inline PowerShell programs; no native PowerShell execution |
| Chromium layout fixtures | 14 passed | Desktop dark/light findings, split chat, evidence, resources, approval, viewer controls; mobile findings |

The browser fixtures use actual component markup and styles with static hook
snapshots. They check layout, overflow, a usable stage, one sidebar, and approval
dialog placement. **They do not run React hydration, the actual backend, or the
Three.js/WebGL renderer.** Screenshots from those fixtures are not scientific
results and are not shipped as demonstrations of a running simulation.

## What was not executed

**Native Cargo compilation/tests, the full Next.js production build, live
provider calls, GPU driver telemetry, Windows PowerShell/MSVC, and actual WebGL
playback were not executed in the packaging environment.** It lacks Rust/Cargo,
Windows tools, and package-registry access. No claims of native compilation or
end-to-end numerical validity are made on the basis of static tests.

Thirteen new native Rust regression tests are included: seven evidence rules,
three telemetry-parser tests, and three ODE execution/visual-mapping tests. They
remain unexecuted here. An isolated CPU-backend integration test and Windows/Linux
CI workflow are included; neither has run here. Existing native tests remain.

The Windows installer performs the real backend all-target build/tests and the
frontend production build on the target machine and stops on failure. Keep the
working 0.3.2 folder until 0.3.3 has passed those target-machine checks.

## Archive verification

The packaging process checks ZIP member CRCs, extracts into a fresh directory,
compares every project byte, verifies every MANIFEST.sha256 entry, and reruns the
schema/API/view-model/source gates from that extracted copy. The external
`PhaseForge-0.3.3-FULL.sha256.txt` identifies the final ZIP; the ZIP hash is not
embedded in its own contents.

## Runtime boundaries worth testing first

An existing 0.3.2 run has legacy evidence, so its old green score-only challenge
flags become inconclusive. A confirmed local replay creates a new run, retains
the old result, and captures current comparisons and visual mappings. A challenge
without explicit comparison rules remains inconclusive after replay.

Findings can be read without a paid call. An AI explanation is user-requested or
explicitly opted into, cached, metered, advisory, and cannot run another experiment.
Preparing a next-step proposal forces auto-run off; review and approve the new
revision separately. These runtime contracts are covered by source/helper tests
but still require native integration validation.

CPU/RAM and GPU counters are machine/device-wide except the separately named
backend-process measurements. Driver limitations remain visible as unavailable.
No per-run GPU utilization, automatic resource throttling, novelty, quantum
execution, or proof of a scientific hypothesis is claimed.
