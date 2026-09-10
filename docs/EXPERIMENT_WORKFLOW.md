# Direct experiment workflow (0.8)

These controls remain available in the Electron workbench and browser development
client. The **Agents** tab adds a separately authorized timed-session controller;
see [sessions](research-sessions.md). [Procedural scenes](SCIENTIFIC_SCENES.md)
represent the experiment without replacing solver measurements.

## What the buttons do

| Control | Artifact / work | Model call | Numerical consent |
|---|---|---|---|
| Build experiment | New complete validated setup, or exact capability/error response | Metered, existing bounded repairs | No numerical run |
| Build & run | New validated setup and one run within the selected build caps | Metered, existing bounded repairs | The click authorizes that accepted setup |
| Run saved setup | Exact selected immutable revision | None | Explicit click |
| Replay | Whole exact source experiment, including any seeded search | None | Explicit click/confirmation |
| Half-size steps & run | New revision with h/2, sufficient max_steps, approximately preserved capture grid | None | Explicit click/confirmation |
| Double horizon & run | New revision with twice the source interval from its original starting state | None | Explicit click/confirmation |
| Enter equations | Backend-validated user-authored ODE setup | None | Save only; run separately |
| Notes task analysis | Actual analysis, or executable setup for a simulation task | Metered | Build only unless a separate run is requested |

A plan is not an executable object. The repair loop now enforces this distinction:
`study_intent="experiment"` accepts only create/revise manifest or a non-running
precise `capability_gap` without a plan. `review` returns an explanation or gap, not a
new plan. Planning notes and legacy intent values remain supported for explicit uses.
An actual provider refusal is surfaced directly, not sent through a bypass/retry path.

## Assumptions and claims

The assumptions checkbox permits declared hypothetical/scaled inputs for exploratory
models. It is visible and carried as an explicit typed request field. It does not mean
measurement data exist, a model has been calibrated, an engine ran or a biological
candidate works. Unchecked permission means do not invent required empirical values.
The model must identify a meaningful supported computational subquestion or a precise
missing input/engine; it must not label an unrelated toy demonstration as the full goal.

Task checkboxes, earlier unreviewed literature, real laboratory results and publication
dossiers cannot become required application workflow stages before exploratory numerical
work. They remain necessary evidence for stronger claims in the domains where they apply.
Numerical schema/capability checks and provider safety rules are not relaxed.

## Budgets and consent

`experiment_options` contains `assumptions_allowed`, `max_wall_seconds`, `max_memory_mb`
and `max_candidates`. Defaults in the visible UI are 180 seconds, 1024 MiB and 64 total
screening candidate evaluations; these are editable within existing runtime ceilings.
The backend defaults assumption permission to false if the field is omitted. Search
population × generations and the native candidate allocation must fit the request cap.
Challenge replays are additional checks within the same wall-time/memory budget and the
runtime's existing step/work limits. There is no model call per numerical candidate.

The public SendMessageRequest defaults missing `auto_run` to false. In experiment mode,
the server uses the user's flag to authorize one accepted run; the generated should_run
field is not a second stage gate. In old modes, legacy user-plus-proposal behavior is
preserved. Direct responses do not authorize a future series; the separately started
session controller can iterate within its approved limits. A setup that fails queue admission
is retained, with `execution_error` exposed to the UI. Stop may not recover remote tokens
already processed. Cancellation and paid usage accounting remain the existing system.

## Saved-run operations and receipts

POST `/api/projects/:project/experiments/next`:

```json
{
  "request_id": "<new UUID>",
  "source_run_id": "<terminal run UUID in this project>",
  "operation": "finer_steps",
  "run": true
}
```

Operations: `replay`, `finer_steps`, `longer_horizon`. `run` is required; false saves a
prepared copy without enqueuing. Only a completed/failed/cancelled source run is accepted.
The exact source revision is used, even when the project has a newer active revision.
New copies retain model inputs, constraints, seed and compute caps. Same-solver refinement
is not independent verification; extending a horizon does not continue from the final frame
or guarantee the old acceptance rules are meaningful over the changed interval.

A synchronous project/action receipt is reserved before enqueueing. Same request ID and
payload returns the prior receipt; a different action with the same ID is rejected. A
process interruption after reservation can leave `reserved` with an unknown queue outcome:
inspect Runs, do not silently enqueue again. This conservative behavior prefers visible
ambiguity over duplicated work. Receipts are data objects in the existing database; no
external migration or destructive rebuild is required.

## Interface and compatibility

Experiment / Results / Setup are the primary tabs. More exposes optional notes, molecules,
evidence, equation editing and advanced batch/verification tools. Old `/discovery/` URLs
redirect into the main laboratory; known UUID project/study/manifest/run query values are
retained and arbitrary query strings are not promoted to trusted references. The old study
APIs and saved records remain available. The component used for advanced studies remains
in the source tree but is not a second top-level page or an experimentation prerequisite.

New versions must be built and old servers stopped; the scripts check the running backend
version. Sources and data are separate: extracting to a fresh directory does not erase or
replace the keyring, local database, saved plans, runs or exact frozen verification sources.
