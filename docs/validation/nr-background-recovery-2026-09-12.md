# Retained-output recovery and API responsiveness

This work changes recovery of existing numerical-relativity files. It does not execute a solver, rerun a model request, or validate the accuracy of the retained puncture diagnostic.

## Preserved initial failure

The original CUDA pilot's HTTP GET timed out during first output retention. That remains recorded in `.local/validation/workbench-010/nr-cuda-api-pilot-01.json`; source inspection identified synchronous large-file copying/hashing on an async worker as a responsiveness risk, without establishing it as the cause of that particular timeout.

After moving the large reads to one bounded blocking-worker queue, a data-only recovery of completed job `5ebd8820-43ef-4e91-8213-9d158a439809` produced 141 successful concurrent GETs, maximum 54.4 ms and p95 37.5 ms. The recovery POST itself still timed out at the unchanged 30-second limit. The overall check therefore failed. The operation eventually appended one `nr_retained` event at `2026-09-12T17:05:04.928384Z`, about 50.315 seconds after the request began.

Raw-JSON follow-up GETs verified exact equality of the original job ID, project, parent, input, deadline, scientific state, result, and completion time. No request was resubmitted. The first follow-up serialized through PowerShell and altered floating-point serialization; its comparison is retained separately and superseded by the raw-response comparison, not silently corrected.

Evidence is under `.local/validation/nr-bh-evidence-integration-20260912-01/latency-attempt-01/`: `report.json` preserves the failed POST, `after-disconnect-02.raw.json` holds the raw eventual response, and `eventual-identity-check-02.json` records equality. These are API measurements, not native viewer frame-rate measurements.

## Background operation contract

`POST /api/laboratory/jobs/:id/control` with `{"action":"reconcile"}` now reserves the exact inactive job, writes an operation event, and returns HTTP 202 before importing or hashing the retained files. A background worker owns the reservation through completion and durable status recording. The original scientific job remains available.

`nr_recovery` events record the operation UUID, queued/verifying phase, and started/completed/failed/cancelled status. Compact job summaries expose the same information through `recovery`; full records retain the complete history. Repeated in-flight requests coalesce. A new explicit request after terminal status performs fresh verification, so an older successful operation cannot conceal subsequently changed artifacts.

The Activity view shows recovery progress separately from the solver's status and keeps result access available. Cancellation names the displayed operation UUID through `cancel_recovery`; a stale button cannot cancel a newer operation or rewrite a completed solver as cancelled. File readers check cancellation between bounded reads, unpublished partial imports are cleaned up, and terminal cancellation is recorded after the worker drains. On backend startup an interrupted recovery receives a cancelled/restart event and is not restarted automatically. Recovery does not alter the original solver timer.

## Verification scope

The initial implementation passed 32 retention tests and 11 existing API tests in an actual Windows test binary. Eight new tests exercise prompt admission and duplicate suppression, changed-file verification after success, cancellation while queued and reading, exact-operation cancellation, interruption on restart, worker panic, and safe cancellation of an unpublished copy. Nine frontend checks cover full and compact recovery state, unchanged unread/completion/runtime values, actual Activity controls, failures, and the supported attention actions.

Evidence is under `.local/validation/nr-background-recovery-20260912-01/`. `test-report.json`, `retention.log`, `api.log`, and `frontend-focused-02.log` identify this stage. A subsequent small guard prevents a late verifying event from replacing a cancellation request; the combined final source build will include it and the separately owned agent lifecycle hooks.

The combined frontend passed all 162 tests and its production build. The build ID is `3WDbnHm8j-kMZY1OVU5OT`, with 194 exported files; `frontend-report.json` pins its output inventory and source identities. This includes the separate selected-output failure warning and supported attention/evidence navigation. It is source-build evidence; the updated backend and integrated UI acceptance are separate checks.

The final combined backend passed 508 tests with 12 explicitly ignored and built successfully; all 123 inventoried backend source files remained unchanged through test/build. This includes all 22 progress/lifecycle, 3 legacy attention projection, 32 retention and 11 API checks. Exact binary SHA-256 is `2c3783296b810d5272e8f4620576cd3ecbbdc84fb7f1463d6d58b57718fab0eb`; the receipt is `.local/validation/backend-final-20260912/report.json`. The earlier interrupted focused run had no final result and remains preserved.

## Actual final application acceptance

The final source application passed the prepared asynchronous HTTP check at 19:00 UTC on September 12. The same completed CUDA job was recovered exactly once, with operation `3ad5d8de-2701-4169-a3cc-243dd730a5db`. HTTP 202 arrived within the preregistered one-second bound; background verification completed in approximately 46.04 seconds. All 212 concurrent navigation/data GETs succeeded, with maximum 63 ms and p95 47 ms. The ordinary request timeout remained 30 seconds.

The original scientific result, input, job/project/parent identities, deadline, completion time and job inventory remained equal. Recovery added its own queued/verifying/completed events, with no new execution events and no duplicate `nr_retained` event. No solver or model request ran for this check. Raw before/after records and every measured request are in `.local/validation/nr-background-recovery-20260912-01/actual-async-01/`; `report.json` records `passed: true`. The two earlier timeout failures remain separate.

The actual capability endpoint exposes five bundled numerical engines and the separately registered CUDA diagnostic. It correctly rejects the old gauge registration because its staging helper differs from the current application; the unavailable CPU puncture registration is also explicit. Resource observations are labeled proposals rather than reservations or scientific feasibility. Exact endpoint output is retained at `.local/validation/backend-final-20260912/runtime-capabilities.json`. No additional calculation was launched to inspect availability.
