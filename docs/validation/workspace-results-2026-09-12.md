# Completed-result navigation verification

The Results control now indexes completed equation runs and completed laboratory outputs from the current project. Its latest-result action and history cards open exact retained IDs. Background completion and polling preserve the currently inspected result. Generated numerical JSON remains an accessible output, and completed data publications replace their generated source card rather than duplicating it.

The investigation found retained outputs in the backend: 12 completed legacy runs and 88 laboratory job summaries. The frontend gaps were separate: Scientific lab excluded the legacy run store, the chat findings action followed an older selected run, and run deep links loaded data without selecting the findings view. The fix adds visible result availability, correct findings targets, run/laboratory deep-link project resolution, and guards against stale result callbacks or reconnects reapplying a consumed link.

Evidence retained locally on 2026-09-12:

- Read-only source snapshots: `.local/validation/workbench-010/b8-results-audit-20260912T063308930/legacy-runs.json` and `laboratory-summaries.json`. Original records were not modified. The snapshots remain private and ignored by Git.
- **27 passing actual React interaction assertions** against those retained records: `.local/viewer-acceptance/results-20260912-01/runs/2026-09-12T12-40-48-883Z/interaction-report.json`. The test exercised completion availability, passive selection preservation, exact latest/history selection, project switching, a stale callback, component remount, and control layout at 320/520/1000 px. Completion/navigation events were simulated UI events; the retained scientific outputs were not rerun or changed. An earlier harness syntax failure remains in its separate attempt directory.
- **130/130 frontend tests** and successful production build: `.local/validation/results-final-20260912-02/frontend-tests.log` and `frontend-build.log`.
- **122/122 source release-audit checks**: `.local/validation/results-final-20260912-01/release-audit.log`. The functional Tools → Equation measurements route remains available.
- Root-agent full-page verification opened `/?run=9ad950eb-b44d-415c-ae12-ac3bb5a5687c`, showed its actual findings for the retained 251-frame electron experiment, and verified the Results control and the legacy-only Scientific lab direct-result action. Further full-page history/blackhole checks are recorded by the root agent separately.

The full-page verification host at port 7443 serves the rebuilt frontend and forwards only GET/HEAD requests to the installed application's port 7332. It blocks all mutations and can return explicitly synthetic acknowledgement responses. Its receipt is `.local/validation/readonly-results-ui-20260912-01/receipt.json`. This verifies original-record availability and navigation with the new frontend; it does **not** establish installed-bundle acceptance or persisted read acknowledgements. No provider calls, new studies, simulations, or renders were initiated by this result-navigation verification.

## Registered numerical output follow-up

The full-page blackhole checks exposed another availability bug. Job `78ec44da-cdcd-4a7a-ac3b-83e0849c03da` registered `work/result.json`, but its nested advertised artifact list omitted that file. The viewer consequently opened a large scene-geometry JSON. Job `925a7791-0009-4026-a4eb-d9c9d894392c` contained both a PNG and numerical results, but the image branch hid its numerical preview.

The artifact viewer now includes the exact registered canonical `work/result.json` even when the nested artifact list omits itself. It does not construct a substitute from `reported_result`. Mixed outputs have explicit Image and Numerical result buttons. Switching views preserves the retained PNG object and its pan/zoom; JSON size limits, original numeric tokens, registered byte counts, and SHA-256 checks remain enforced. Long display titles are clamped with their full text retained in the title attribute.

Another **27 actual React interaction assertions** passed against exact read-only files in `.local/viewer-acceptance/artifact-modes-20260912-01/runs/2026-09-12T12-55-16-399Z/`. They checked canonical default selection, verbatim numerical display, direct downloads, image/data switching, retained image transformation, and reachable controls/downloads at 320 and 520 px. The original scientific artifacts were:

| Job | Registered file | Bytes | SHA-256 |
| --- | --- | ---: | --- |
| `78ec44da…` | `work/result.json` | 5,154 | `a09438f650325c7e73558f9812377af8ab049d742b272080a83f2bb8b084e482` |
| `925a7791…` | `work/result.json` | 1,899 | `b39e06e5a72828b1afae1922b60b8a732d44c5dcbaae61f19d6303f2b24f8a00` |
| `925a7791…` | `work/preview.png` | 34,432 | `3592e340e2e96581d32913f7d2a4f445a59d56cac14df26fcbdf735af7921067` |

After this follow-up, **132/132 frontend tests** and the production build passed in `.local/validation/results-final-20260912-03/`. This establishes output availability and byte fidelity, not validity of the underlying scientific approximation. The provider-readiness guidance also correctly directs provider configuration to Settings and model selection to the current conversation.

## Laboratory explanation and follow-up actions

Completed numerical jobs and plot results now expose **Explain with AI**, **Suggest next steps**, and **Review numerical checks** below the selected result. These actions fetch and match the completed full job, capture the current conversation's model, reasoning setting and work limit, and submit a new session with `result_review: {source_job_id, action}` and explicit explanation intent. They do not use the steering route. Illustrations retain their separate controls, and the legacy Findings view retains its existing explanation and suggested-action controls.

The frontend rejects stale source/project selections, changed completion identities, missing models, duplicate submissions, and active same-project sessions, including model-slot waiting. A locally admitted-session record covers the interval before the next global poll. Requests and replies remain bound to their original project if the user navigates elsewhere. Read-only tool enforcement and source pinning are separate backend requirements verified by the root agent; frontend labels or client checks alone do not establish those properties.

**30 actual React interaction assertions** passed in `.local/viewer-acceptance/result-review-20260912-01/runs/2026-09-12T13-06-33-782Z/`. The fixture used a real retained source record and intercepted every review POST. Four recorded requests covered all three actions and Timer Off. It verified exact source/action/model/reasoning/time fields, submission disablement, stale callbacks after navigation, same-project busy gating, other-project independence, and reachable controls at 320/520 px. No paid call or backend session was created by this fixture.

The resulting frontend passed **137/137 tests**, the production build, and **122/122 source release-audit checks**, with logs in `.local/validation/results-review-final-20260912-01/`.

## Dedicated review admission and uncertain delivery

The final client uses only `POST /api/laboratory/jobs/:source_id/review`. This is a compatibility boundary: an older backend returns 404 rather than ignoring an optional review field in ordinary chat. The client never retries through ordinary chat or steering. The backend independently enforces matching source IDs, read-only tools, source pins, and idempotent request identities.

Before delivery, the client stores the exact review payload and request ID for that project. Network/server failures retain this record. Only an explicit retry of the same source/action is offered; it reuses the original model, reasoning, work limit, and request ID even after a component reload or subsequent model selection. Other review actions remain blocked until delivery is resolved. There is no automatic resubmission. A matching session receipt, including one found by ordinary job polling, clears the pending delivery. Definite admission rejections such as 404 clear the unaccepted request. Requests and delayed responses preserve project identity during navigation.

**41 actual React/API interaction assertions** passed in `.local/viewer-acceptance/result-review-dedicated-20260912-01/runs/2026-09-12T13-19-34-959Z/`, with seven intercepted POSTs and no renderer errors. This includes all three review actions, Timer Off, source/model binding, active-session and stale-callback guards, uncertain delivery, an explicit byte-identical retry after remount/model/timer changes, the older-backend 404 case, and 320/520 px control layout. The final screenshot intentionally retains the simulated 404 error. Frozen fixture inputs and source hashes are stored with that attempt. No real provider call or backend review session was created by these frontend tests.

The final frontend passed **140/140 tests** and the production build, with logs in `.local/validation/results-review-dedicated-final-20260912-01/`. Earlier receipts remain unchanged. Read-only backend enforcement and actual full-app acceptance are separate evidence supplied by the root agent.

## Full-page sibling-key regression

The root agent's subsequent full-page check exposed a real integration regression: switching from result `78ec44da…` to `925a7791…` left two visible Saved output files sections. `LaboratoryViewer` and its sibling `LaboratoryResultActions` shared the same React key, allowing an old viewer to remain after reconciliation. The root agent corrected those sibling keys to distinct `viewer:<id>` and `review:<id>` identities. The isolated component fixtures did not cover that shared parent and must not be treated as proof against this failure.

After the correction, **140/140 frontend tests** and the production build passed again; logs are retained separately in `.local/validation/results-review-sibling-key-final-20260912-01/`. Final full-page verification of exactly one viewer across historical/latest-result switches remains the root agent's integration check, rather than a source assertion that merely repeats the chosen key strings.

## Delayed inspection, visible timer drafts, and read timing

A subsequent bounded source review found three gaps beyond those isolated component checks:

- The older ResearchSessions **Inspect latest run** callback inserted and selected an awaited response unconditionally. It now verifies the returned run ID/project and requires the original active project, view epoch, and project-load generation before inserting or opening that run. Leaving and returning also invalidates the old response.
- An invalid or blank custom timer remained visible in chat while only its last valid value was stored. Results reviews and frame observations previously read that stored value. The composer now shares its raw per-project timer draft synchronously, and these actions validate the visible draft before admission. An explicit retry of an uncertain review intentionally retains its original request's time limit, as its UI states.
- Project read acknowledgements captured their timestamp after project loading. A completion during that delay could lose its unread marker. The workbench now captures the opening time before asynchronous work and passes it to the peer agent's read-state helper. Later completions stay unread until a subsequent explicit acknowledgement.

Focused regression tests cover delayed inspection after switching project, leaving/returning, selecting another result, or starting a newer load; returned ID/project mismatches; invalid/valid custom timer drafts and Timer Off; and delayed first/cached job snapshots with later completions. The combined source passed **145/145 frontend tests**, `git diff --check`, and the production build. Logs and source hashes are in `.local/validation/results-navigation-timer-final-20260912-01/`. No study, provider request, original record mutation, or installed-app operation was performed for these checks. Full-page checks of the affected interactions remain separately attributed to the root agent.

Root's subsequent full-page review check verified that a visible zero-minute
value and an actual blank input each produce the duration validation message
before another review POST. The browser tool's first empty `fill` operation had
left the input at 30; that valid request reached the read-only proxy and was
blocked with 403. Keyboard select-all/backspace then cleared it visibly, and
the blank rejection passed. This failed test action is recorded, not classified
as a product fix. The proxy recorded exactly that one blocked review request
and forwarded no mutation or model call. Receipt:
`.local/validation/workbench-010/fullpage-review-timer-20260912-01.json`, SHA-256
`ddc81c400f7a8474ae63b6e9e9e383c43f49e0b8def107526f4890b19d475467`.

The root agent subsequently completed that full-page check. Switching from the
older `78ec44da…` result to the latest `925a7791…` leaves exactly one saved-output
viewer and one review source, both bound to the selected ID. Numerical result
shows the actual 311-state precontact result; switching back restores the original
analytic result. A full deep-link reload keeps the exact latest job. A subsequent
cross-project legacy deep link opens run `9ad950eb…` and its 251 retained samples.
The 15-case receipt preserves earlier failures separately from the final fixed
observations: `.local/validation/workbench-010/fullpage-results-20260912-01.json`,
SHA-256 `cafb67f14e08f8189f1afcd8479c1c19c165ddf3244d086aeacb448963295295`.
This was still the source frontend on the read-only 7443 host, not an installed
0.10.0 bundle, a numerical-relativity result, or a persisted-read-state check.

## Server-enforced saved-result review

The dedicated review route independently requires an inactive completed result,
the matching project and source ID, and an immutable request ID. Its saved
request preserves the explicitly selected chat model, reasoning effort and work
limit, including Timer Off. The server forces explanation intent and supplies
only bounded reading, observation, memory and catalog tools. Dispatch guards
also reject hidden or steered compute, rendering, editing and delegation calls.

The review pins both the source database input/result and actual admitted files.
Every model/tool round and final delivery checks those pins. Text/JSON readers
hash the same byte buffer they parse; PNG observation checks the hash of the
same payload sent to the model. Added or unpinned files cannot enter a review.
At most 512 files, 64 MiB total and 16 MiB per file are admitted; the selected
coverage and omitted count remain explicit. Existing core result/manifest files
that cannot be pinned cause admission to fail before a provider request. Binary
arrays are not decoded by this review path. A successful exact-source
`inspect_result` receipt is required before the review can claim completion.

Eight focused backend tests include real localhost HTTP review admission through
the native tool loop with a fixture provider, changed-file/record rejection,
read-only dispatch and same-ID replay without duplicate work. No paid provider
call was made. The complete backend library suite passed **408 tests, 0 failed,
11 ignored**; log `.local/validation/workbench-010/backend-full-with-result-review.log`,
SHA-256 `4b60cd9f2168cc274a6430634d7050806f615c3797dfc391bef0f58f5d843227`.
An independent source review found no blocking issue in these boundaries. This
is evidence handling and workflow validation, not independent validation of the
underlying scientific model or every shared orchestrator crash boundary.
