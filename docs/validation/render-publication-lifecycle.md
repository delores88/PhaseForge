# Render publication and retry lifecycle checks

Source checks on Windows, 2026-09-11. These checks use temporary databases and
retained PNG fixtures. No installed app, renderer, solver, external provider,
release build, or installer was started by this batch.

HTTP artifact publication now classifies the safely resolved canonical path,
with Windows case and separators normalized, before testing completion. Native
image acquisition uses the same classifier and additionally requires any
observation or illustration job to have completed validation before supplying
pixels. `RENDER.PNG`, `FIRST-FRAME.PNG`, uppercase scene names, and incomplete
export videos cannot bypass publication by changing spelling. Existing unsafe
relative paths remain rejected. Accepted repeated separators/interior dot
components resolve to the same canonical file. Native completion tests compare
the delivered PNG data and SHA-256 to the exact retained bytes.

Illustration admission captures the scene and presentation while holding the
same parent gate used by stop and presentation edits. A retained request identity
never re-resolves a later camera or scene. Stopped/expired parents cannot admit a
new child, and a completion/budget change between admission and launch durably
pauses or times out the queued attempt instead of leaving an orphan spinner.

Retries preserve the original input, scene, presentation and deadline. A separate
attempt retains its original attempt and parent lineage. A live parent supplies
its exact absolute deadline, including Off. An inactive/completed/expired parent
is detached for an independent retry; expired independent budgets require an
explicit new budget. HTTP render retry handling occurs before generic old-record
budget mutation and rejects a child-specific budget override under an active
parent. A replacement saved before its backlink event is recovered by immutable
lineage, and simultaneous retry admission does not create duplicate replacements.

Executed checks:

- `cargo test --manifest-path backend/Cargo.toml --no-default-features lifecycle_ -- --nocapture`: five illustration lifecycle tests passed.
- The resulting test binary, `laboratory::api::tests`: seven API/presentation/budget tests passed.
- The same binary, `native_pixels_require` and `detector_uses_numeric`: one test each passed.

Total: **14 passed, zero failed, zero ignored**. An initial monitor module filter
matched no tests; corrected exact-name filters were executed and passed. No
environment-gated renderer test was counted or selected. Compilation emitted
existing warnings from disabling the GPU feature. The same snapshot included the
new sweep resume API branch; sweep behavior has its own owner's checks.

## Solver and coordinator continuation follow-up

The solver resume path now reserves execution under the parent admission gate.
An active parent supplies its exact absolute deadline. A paused child of an
inactive parent needs an explicit timer choice (including null for Off) to
continue independently, after the previous parent execution tree has drained.
The original solver input and checkpoint identity remain unchanged; a durable
lineage event records the detachment. Sweep and ML coordinator resumes are
dispatched before generic deadline mutation so failed admission cannot alter a
retained budget.

Five `solver_lifecycle::tests` and eight `laboratory::api::tests` passed, with zero
failures or ignored cases. These checks cover concurrent reservation, inherited
Off and finite deadlines, elapsed-budget rejection, drained descendants,
unchanged input bytes, protected study lineage and failed API admission leaving
the old deadline intact. No actual solver or installed runtime was started by
these deterministic checks.
