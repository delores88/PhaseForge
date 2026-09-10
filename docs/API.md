# Loopback API

The standalone backend defaults to `http://127.0.0.1:7331`. The packaged desktop
app asks the operating system for an available loopback backend port and discovers
it through its owned child process, then verifies the launch identity before
forwarding requests. Its browser interface stays at `http://127.0.0.1:7332` and
proxies the API to that verified endpoint; an occupied UI port prevents startup.

All routes, including WebSocket upgrades and OPTIONS requests, require an exact
loopback Host with the configured backend port. If an Origin is present, it must
match `allowed_frontend_origins`; configured origins must be canonical HTTP(S)
loopback origins without paths or credentials. Foreign/null/malformed origins
and duplicate authority headers are rejected before handlers. No-Origin local CLI
requests remain supported. See [the security boundary](SECURITY.md).

## Service and compute

```text
GET  /api/health
GET  /api/hardware
GET  /api/capabilities
GET  /api/scientific/engines
```

Health includes `sqlite` with the linked library's version, numeric version,
source ID and bundled linkage for installed-component inspection. It contains no
database path or records; unavailable metadata is `null`.

`GET /api/desktop/ready?nonce=HEX` is reserved for desktop startup. A launch with
`PHASEFORGE_DESKTOP_SECRET` (64 hexadecimal characters representing 32 raw bytes)
returns `{algorithm:"HMAC-SHA256",nonce,proof,version}` for a 64-character hex nonce.
The proof is lowercase hexadecimal HMAC-SHA256 of the nonce's UTF-8 bytes, using
the decoded secret as key. The response sets `Cache-Control: no-store`. The secret
is never an HTTP parameter. Missing launch secret returns 404; invalid nonce returns
400. Public `/api/health` does not establish ownership of the responding process.

## Research projects

```text
GET     /api/projects
POST    /api/projects
GET     /api/projects/:id
PATCH   /api/projects/:id
DELETE  /api/projects/:id
GET     /api/projects/:id/messages
POST    /api/projects/:id/messages
POST    /api/chat/requests/:id/cancel
```

Messages may contain bounded text attachments and an optional molecular-structure context ID. Supported molecular attachments can be imported into the same research world.

## Manifests and runs

```text
GET   /api/projects/:id/manifests
POST  /api/projects/:id/manifests
GET   /api/manifests/:id
POST  /api/manifests/:id/run
GET   /api/runs
POST  /api/runs
GET   /api/runs/:id
POST  /api/runs/:id/cancel
```

## Providers

```text
GET     /api/providers
PUT     /api/providers/:provider/key
DELETE  /api/providers/:provider/key
GET     /api/providers/:provider/models
PUT     /api/providers/:provider/model
POST    /api/providers/:provider/test
DELETE  /api/providers/:provider
```

The key route does not require a model. The models route requires a stored key and returns account-visible model metadata. The model route stores only the selected ID.

## Molecular discovery

```text
GET     /api/molecules?project_id=:id
POST    /api/molecules
GET     /api/molecules/:id
DELETE  /api/molecules/:id
GET     /api/molecules/:id/qmmm-plans
POST    /api/molecules/:id/qmmm-plans
GET     /api/molecules/:id/campaigns
POST    /api/molecules/:id/campaigns
```

Molecular APIs return parsed and derived computational records. They do not imply that docking, dynamics, quantum chemistry, synthesis, safety, or efficacy work was performed.

## Events

```text
GET /api/events/ws
```

The WebSocket carries run lifecycle events. Provider chat cancellation uses its dedicated request route.

## v0.3.1 additions

- `GET /api/usage`: lifetime totals, per-model summaries, last 500 call records,
  warnings, settings, active calls, and active parent requests with phase.
- `GET /api/usage/settings`: current local spend controls and rate cards.
- `PUT /api/usage/settings`: validated complete controls/rates; pausing also signals
  active requests. No keys accepted or returned here.
- `POST /api/chat/cancel-all`: signals all active parent agent requests.
- Existing `POST /api/chat/requests/:id/cancel`: signals the request through validation
  and repair, including a short-lived pre-start cancellation tombstone.

Chat output proposals use `manifest` (nested object or null), never `manifest_json`.
Each call is metered before its output is parsed. Schema and scientific validity
are both prerequisites for execution. Final failures are assistant error messages.


## v0.3.3 findings and live monitoring

GET /api/telemetry — read-only machine/process/driver counters and bounded history.
GET /api/workflow — lightweight active agent requests and run-status rows; no result frames.
GET /api/runs/:id/findings — deterministic report tied to the run's source manifest,
with a stored ai_analysis when available. This GET never calls an AI provider.
POST /api/runs/:id/explain — {request_id: UUID, provider: optional provider kind}.
One metered prose explanation or a cached result. Cannot launch another experiment.
POST /api/projects/:id/messages additionally accepts source_run_id for explicit
continuation from a particular run. Proposal UI sends auto_run=false.

New authoring shape: visualization.entities = [{id,x,y,z,vx,vy,vz}] and
falsification[].checks = [{metric,expectation,absolute_tolerance,relative_tolerance}].
Defaults allow reading old manual manifests, but the provider schema requires the
arrays. Empty checks have no pass criteria and remain inconclusive.
Execution result evidence_version=2 includes constraint_results and per-trial metric
comparisons; a missing survived flag/value is never equivalent to a successful test.


## Discovery and research records (0.4.0)

| Method | Route | Behavior |
|---|---|---|
| GET/POST | `/api/discovery/studies` | List by optional project_id / validate and freeze a local draft |
| GET | `/api/discovery/studies/:id` | Frozen recipe, all trials, summary and author-review gaps |
| POST | `.../:id/start` | Explicit bounded compute approval or remaining-budget resume |
| POST | `.../:id/pause` or `.../:id/cancel` | Stop active trial and retain evidence |
| POST | `.../:id/review` | Metered skeptical interpretation, never a new run |
| POST | `.../:id/next-proposal` | Metered next-manifest proposal with auto_run=false |
| POST | `.../:id/fork` | New unexecuted draft with retained bounds and a new seed |
| GET | `.../:id/export` | Offline research ZIP, paused/terminal only |
| GET/PUT | `/api/research/notebooks/:project_id` | Versioned author record; stale writes return 409 |
| POST | `/api/research/literature` | Explicit public Crossref query; allow_public_query=true required |
| GET | `/api/runs/:id/signals` | Bounded diagnostics of saved samples, no paid model call |

Study creation has no provider call. AI study design uses the existing audited
chat route with the exact selected source context and auto_run=false. New
follow-up manifests are validated by the same trusted runtime as other proposals.

## Verification API (0.5.0)

- `GET/POST /api/verification/dossiers`: summaries / freeze a protocol and snapshots.
- `GET /api/verification/dossiers/:id`: state, outputs and assessment. Large source/control
  trajectories are omitted from polling responses but remain frozen in storage/export.
- `POST /api/verification/dossiers/:id/start` and `/cancel`: explicit worker admission/stop.
- `POST /api/verification/probe`: probe compatible Python, never run a simulation.
- `GET/POST /api/verification/catalog`: list or add immutable numerical comparisons.
- `POST /api/verification/dossiers/:id/search`: query plus `allow_public_query: true`;
  retained metadata/failure record, no full-text inference.
- `POST /api/verification/dossiers/:id/review`: named human disposition and evidence hash.
- `POST /api/verification/dossiers/:id/agent-review`: one read-only metered advisory.
- `GET /api/verification/runs/:id/window`: optional `series`, `start`, `end`, `max_points`
  (2–2048). At most 32 retained series, actual samples only, no invented interpolation.

See Rust `assurance/types.rs` for exact request shapes. Same loopback/CORS/data rules
apply. The API does not expose an arbitrary executable command or code parameter.


## v0.7 direct experimentation

`SendMessageRequest` additionally accepts `study_intent="experiment"|"review"`,
`context_manifest_id` (must belong to this project) and `experiment_options`
(`assumptions_allowed`, `max_wall_seconds`, `max_memory_mb`, `max_candidates`).
Missing `auto_run` is false. Experiment mode enforces a runnable manifest or exact
non-running gap, not a research plan. The user run flag authorizes one accepted run.

`POST /api/projects/:id/experiments/next` accepts a required `request_id`,
`source_run_id`, `operation` (`replay`, `finer_steps`, `longer_horizon`) and required
boolean `run`. Returns the prepared immutable manifest and optional queued run,
status, request hash, and execution error. Same-ID requests are idempotent. See
`EXPERIMENT_WORKFLOW.md` for reservation ambiguity, project scoping and restart semantics.

## Timed sessions, public research and Studio (0.8.0)

Chat turns accept per-request `provider`, `model`, `reasoning_effort` and
`research_mode` (default false). Timed sessions retain these settings and share a
deadline across specialist, builder and review stages. See [session request and
control contracts](research-sessions.md).

| Method | Route | Behavior |
|---|---|---|
| GET/POST | `/api/projects/:id/tasks` | List sessions / start a bounded session |
| GET | `/api/tasks/:id` | Saved state, stage, budget, artifacts and owned runs |
| POST | `/api/tasks/:id/control` | Pause, cancel or explicitly resume |
| GET | `/api/projects/:id/assets` | Saved assets and public-search receipts |
| POST | `/api/projects/:id/assets/search` | Bounded RCSB, literature or NASA search |
| POST | `/api/projects/:id/assets/import` | Import a catalog accession or allowed `public_file` URL |
| GET | `/api/assets/:id/content` | Original validated source bytes |
| POST | `/api/projects/:id/studio/design` | Metered declarative scene, CAD or PCB design/revision |
| GET | `/api/projects/:id/studio/designs` | Saved designs and lineage |
| GET | `/api/studio/capabilities` | Local Blender executable discovery |
| GET/POST | `/api/studio/renders` | List by `project_id` / queue a bounded local render |
| GET | `/api/studio/renders/:id` | Status, device report and artifact URLs |
| POST | `/api/studio/renders/:id/cancel` | Stop a queued or active render |
| GET | `/api/studio/renders/:id/artifacts/:name` | Fixed render artifact filenames |
| GET | `/api/studio/fabrication-engines` | Local CAD/Python and KiCad discovery |
| GET/POST | `/api/studio/fabrications` | List by `project_id` / queue a bounded CAD or PCB job |
| GET | `/api/studio/fabrications/:id` | Job status, geometry/DRC report and output filenames |
| POST | `/api/studio/fabrications/:id/cancel` | Stop a queued or active engineering job |
| GET | `/api/studio/fabrications/:id/artifacts/:name` | Fixed engineering artifact filenames |

Studio design accepts `prompt`, `kind`, model overrides, `research_mode`, selected
`asset_ids`, and optional `parent_design_id` and `failure_report`. Parent and source
ownership are checked before external work. Design generation uses the existing
`/api/chat/requests/:id/cancel` route when a `request_id` is supplied. Rendering and
fabrication have separate job cancellation routes and do not call a model provider.

See [public intake and design contracts](public-research-and-studio.md),
[render settings and artifacts](BLENDER_RENDERING.md), and
[fabrication inputs and checks](CAD_PCB_ENGINES.md). Executable discovery does not
establish successful execution, and a saved artifact does not certify scientific
or engineering correctness.
