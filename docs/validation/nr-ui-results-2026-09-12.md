# Retained NR viewer and ordinary result interpretation

These observations used the source application at `http://127.0.0.1:7442`
with an isolated profile. They are not final installed-release acceptance.
The result was the actual completed CUDA job
`5ebd8820-43ef-4e91-8213-9d158a439809`, in project
`00ee87c4-a95e-4035-8da1-76df1bc98579`.

## Retained numerical viewer

The browser loaded the real gauge job
`027406c3-355b-4e75-b060-d83d5bbffe05` and the CUDA result. Field selection,
pan, zoom, fit, playback, and cell inspection used saved numerical arrays.
The CUDA view retained times `0`, `0.5126953125`, and `1 L/c`, and showed
136 leaf blocks and 8,704 actual cells. It states that AMR blocks have different
native z centres and that the display uses no interpolation or 3D spacetime
embedding. Selected chi, lapse and Hamiltonian values matched the pinned
JSON values at all three times, subject to the displayed seven-digit rounding.

Actual browser observations found and fixed two layout issues: flex shrinking
clipped the viewer footer, and a passive wheel handler zoomed the canvas while
also scrolling the workspace. The corrected viewer contained its footer and
wheel zoom left workspace scroll position unchanged. Six focused data tests
and the production build passed. Fullscreen entry and exit by the displayed
button worked; this check does not claim native Escape-key acceptance.

Evidence: `.local/validation/nr-viewer-acceptance-20260912/acceptance-report.json`,
SHA256 `17438b18a82b9f657683cd919d6bb5af46887a399698b373ccae388c4b5e9599`.
This observation did not create a partial-result fixture or repeat the solver.

## Ordinary chat requests

Both requests were entered through the normal composer with Auto intent,
OpenAI `gpt-5.6-sol`, provider-default reasoning, a 900-second maximum,
Research off, and the actual CUDA result selected. Backend binary SHA256 was
`9443dfe24ae08710f6b5ace797761a494950e1561a4a4221dc0eeaa11e5752cc`.

| Request | Actual semantic intent and completion |
| --- | --- |
| Explain what the saved diagnostic establishes, what remains uncertain, and what to test next; use existing results and run no experiment | Explanation; completed session `ff491999-0692-4705-9bf3-760406b934b4` at `17:12:46.0401933Z`; six model rounds, 13 tool calls, one context compaction |
| Calculate initial/final H/M RMS, absolute change and percentage change from saved measurements, with exact times and units; run no solver or study | Analysis; completed session `5731154d-827b-4efb-b712-ac36f8f3d959` at `17:16:14.651166Z`; five model rounds, four tool calls, no compaction |

The first intent is consistent with interpreting existing evidence. It does not
exercise the quantitative Analysis completion branch. Its answer separates
execution from numerical convergence, horizon validity, calibrated boost and
merger claims. It notes constraint growth and stale/failed horizon searches.
One wording limitation remains: “peak sampled device allocation” should read
sampled **device-wide memory growth**. The original answer is retained.

The second request exercised real `retained_numerical_analysis` completion with
required capability null. Evidence binds the original result, measurements,
slice index, and finite first/last native-pinned AMR samples. It does not certify
the model's arithmetic; the values below were checked independently from the
retained JSON using `final - initial` and `100 * change / initial`.

| RMS, in `L^-2` | Initial at `0 L/c` | Final at `1 L/c` | Change | Percentage change |
| --- | ---: | ---: | ---: | ---: |
| Hamiltonian | 0.0034214237540210373 | 0.0037715398920999286 | 0.00035011613807889135 | 10.23305393456208% |
| Momentum | 0.000033593644752653335 | 0.0017379497253540526 | 0.0017043560806013992 | 5073.447948712929% |

The answer's endpoint values matched exactly; percentages matched rounding to
six decimal places. These retained digits do not imply equivalent physical
accuracy. The answer explicitly declines an accuracy claim, identifies the
chi mask and engine-derived residuals, and explains that one resolution and
timestep do not establish convergence.

Both replies rendered headings, tables, math, lists and code in chat. Opening
Agents during the quantitative request took 366 ms from click through DOM
observation. Returning to the laboratory took 431 ms, just after completion.
These observations do not measure responsiveness during large native imports
or recovery; the earlier timeout remains a separate failure.

The project retained exactly one solver throughout both requests. Its input,
result and completion timestamp were unchanged. The only new jobs were the two
chat sessions. Four downloaded source artifacts matched their retained SHA256
pins. No new experiment was launched, no dedicated Results review action was
invoked in this checkpoint, and no exact 0.999c collision was validated.

Evidence:
- `.local/validation/nr-result-review-20260912/acceptance-report.json`, SHA256 `4476214c88dc3b6265866e0212f7b3d097a3ce625c0bfece2900e3a92cd4071d`.
- `.local/validation/nr-result-review-20260912/quantitative-followup/acceptance-report.json`, SHA256 `c4bb6270595bde8223661fee4ef2e7bf9a18a16d1c7c54eb4b4a8ddd98fe58b2`.
- The same directories retain raw API responses, exact prompts, source artifacts,
  independent arithmetic, DOM observations, screenshots and original answers.

## Selected-result navigation and failed evidence

The next source-UI checkpoint used frontend build
`3WDbnHm8j-kMZY1OVU5OT`; its combined 162 frontend tests and production build
passed. Real history/Results actions selected an older labelled illustration,
preserved that selection across Notes, and explicitly returned to the latest
completed heat result. Completed Results and history counts differ deliberately:
history includes failed attempts. No new job was started by these actions.

An actual failed gauge result (`293a446d-f4b9-42ab-820b-f6acee1f8f2d`) exposed
a presentation defect: retained numerical playback appeared above the only
visible failure marker. The rebuilt workbench now displays **Analytic checks
failed**, the original failed state/error, and a qualification of the retained
data above the viewer. A screenshot confirms the notice is visible. Its
Agents/evidence action opened that project's actual job history; explicitly
opening the completed grid32 result cleared the notice and selected its exact
ID. Neither attempt nor its thresholds were changed.

The same check confirmed that an equation workspace without a selected model
asks for a conversation model, keeps local equation entry available, and does
not misdiagnose missing credentials. The new direction-action evidence pane
has five focused helper tests; a real server attention record has not yet been
used to exercise that new pane in the browser.

The CUDA dedicated review buttons were then prepared with their exact saved
source ID, Sol/provider-default reasoning, 15-minute limit and Research off.
They were enabled after loading the retained result. No dedicated review was
submitted during this navigation checkpoint.

Raw before/after DOM, the warning screenshot, callback map and test log are in
`.local/validation/nr-result-review-20260912/navigation-actions/`. The separate
installed profile's older equation workflows were inspected by another agent;
this source profile has no equation manifests/runs, so no such acceptance is
claimed here. Large native import/recovery responsiveness remains separate
from these ordinary navigation observations.

## Final stopped-attempt actions and dedicated review

The final source UI used build `OwHJ3IJwEWTrjPSjKTWA_`. Its frozen receipt
records 194 output files, the earlier 162-test suite separately from the final
three React title regressions, and the successful final production build.
These tests and the build were not repeated during the walkthrough. The five
attention/recovery UI sources still match their earlier tested identities.

The actual historical paused capability-gap session
`1495d57a-bc9b-40dd-bdf7-0e7a441d8d8e` now shows **Needs direction**. Its card
offers Revise the request, Inspect capabilities and Inspect saved evidence,
with no Resume action. Capabilities shows the current engines and explicit
scope/requirements. Evidence opens that exact attempt, its original request,
18 recorded events and registered-reference state; it does not substitute the
unrelated selected viewer output. Revise focuses the composer and preserves
an unsent draft. The original empty draft was restored afterward. Original
input, result, deadline, completion, state and events were unchanged, and these
actions made no provider or solver request. A separate historically interrupted
provider session retains its own Resume control; it was not used.

After the separate asynchronous recovery acceptance completed, the CUDA job's
Agents card visibly reported **Recovery completed** and retained the original
experiment. Opening its exact record through **Results**, then clicking
**Explain with AI** once, created session
`b434de80-ade2-4da5-b08b-8e6c7fe2a0c6`. The saved request binds source job
`5ebd8820-43ef-4e91-8213-9d158a439809`, action `explain`, OpenAI Sol,
provider-default reasoning, 900 seconds and 161 registered evidence files.
The conversation visibly had Research off.

The review completed at `2026-09-12T19:04:56.0943472Z`, after 156.3 seconds,
nine model rounds, 20 tool calls and two context compactions, with no recorded
tool error. It cites retained artifact paths and distinguishes successful
execution, constraint measurements, unresolved horizons and unvalidated
physics. The final reply rendered a table, headings, lists and math. It is
lengthy—1,050 whitespace-delimited words—so this observation does not establish
that the default explanation is concise. It also reproduces historical raw
horizon time labels and ambiguous separation wording; completion does not
certify every scientific sentence or independently validate numerical accuracy.
The answer and original data were retained without revision.

While the review was running, click-through-DOM observations were 366 ms for
Agents, 334 ms for Notes and 343 ms for a different project. The sidebar kept
its running indicator. Completion while the other project was selected showed
an unread marker; clicking the CUDA project cleared it and persisted
`seen_at == completed_at`. No console errors were observed. Only the review
session was added: the original solver's input, result, deadline, completion,
state and events remained equal, with the solver count unchanged at one.

Evidence directory: `.local/validation/nr-result-review-20260912/final-ui-20260912/`.

- `frontend-report.json`: SHA256 `4502da681894261088356f08523d7328f05d4164ce058222ef25b2ce160396bd`.
- `attention-report.json`: SHA256 `aff9f8a21bd6f712e3bafb3a86733fe8fd7b8a75a5d29d1bb38c8929d9e6fe3d`.
- `review-report.json`: SHA256 `f9b6b998ae07ace486164f8cb757841c0f6cffd2d055d1c643ef69051d0d38bc`.

The directory retains original API responses, UI observations, screenshots,
the exact final answer and identity/read-acknowledgement comparisons. A
transient screenshot caught loading plus the previous chat scroll position;
the later `review-completed-latest.png` shows the loaded field and final reply.
This completes the ordinary source-UI actions above. Installed-package and
marketplace acceptance remain separate release work.
