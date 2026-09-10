# Research sessions and per-turn models

PhaseForge sessions coordinate specialist briefs, a validated experiment build, a local numerical run, an evidence review, and a concrete next step. The reviewer can request another cycle, within the researcher's time and cycle limits. All artifacts, stage receipts, model usage, accepted manifests, and numerical results are stored locally.

User-authored turns remain attributed to the researcher. Internally orchestrated builder turns appear as **Research team · cycle N** with the session objective, and their replies retain the same task attribution. The full builder instruction still reaches the provider and is retained in local message audit metadata (`internal_prompt`), rather than filling the visible conversation with the internal briefing.

Generated compute context sent to providers uses a capability whitelist: CPU cores and workers, available RAM, the selected GPU's name, backend and device limits, one numerical execution slot, and the fact that NPU numerical execution is unavailable. Raw hardware identifiers, driver details and redundant adapter inventories remain in the local Compute page.

The sessions do not install new scientific solvers or claim that procedural geometry establishes scientific accuracy. Provider calls in this loop reason over the supplied evidence and native solver measurements; they do not independently browse or execute arbitrary code. GPU availability affects native solver allocation, not the remote language model's compute.

Optional `research_mode:true` authorizes bounded public-catalog intake before the
specialists work in each cycle. It is off by default. Intake saves source receipts
and treats retrieved material as untrusted evidence; it does not give individual
agents unrestricted browsing. See [public research and Studio](public-research-and-studio.md).

## Work budget and control

`POST /api/projects/{project_id}/tasks` starts a background local session:

```json
{
  "objective": "Compare two bounded dynamical hypotheses and identify a discriminating control",
  "duration_minutes": 60,
  "max_cycles": 3,
  "specialist_count": 3,
  "auto_run": true,
  "provider": "open_ai",
  "model": "gpt-6-astra",
  "reasoning_effort": "high",
  "experiment_options": {
    "assumptions_allowed": false,
    "max_wall_seconds": 600,
    "max_memory_mb": 4096,
    "max_candidates": 128
  }
}
```

Select a model actually returned by the provider's authenticated catalog. The example model is not a claim of availability for a particular account.

- Work budgets span 1–10,080 minutes, with 1–1,000 cycles and 1–3 specialists. Default values are 30 minutes, 3 cycles, and 2 specialists.
- The default API value of `auto_run` is false. The workbench's **Start research session** action explicitly authorizes local experiments and passes true.
- Each specialist sees the shared deadline and saved peer briefs. Calls run concurrently only up to the existing **Usage & cost** parallel-call limit, which defaults to one. Other usage controls continue to apply.
- Model selection and reasoning are saved on the session and applied to every call. On resume, these can change without changing saved evidence.
- Solver submission occurs after model work, under the same state gate as Pause, and its wall-time cap is reduced to the parent's actual remaining time. The immutable scientific manifest is retained.
- A session may finish before the time limit when its bounded question is answered, the cycle ceiling is reached, or researcher input is needed. A completed computational session does not establish empirical or clinical validity.

`GET /api/projects/{project_id}/tasks` lists sessions; `GET /api/tasks/{id}` returns one session. Records include `state`, `stage`, `cycle`, `deadline_at`, `remaining_seconds`, specialist `children`, `artifacts`, and `run_ids`.

`POST /api/tasks/{id}/control` accepts `{"action":"pause"}`, `{"action":"cancel"}`, or `{"action":"resume"}`. Resume can additionally carry `provider`, `model`, `reasoning_effort`, `research_mode`, and `duration_minutes`. Omitting `research_mode` preserves the previous choice. Supplying `duration_minutes` grants that new remaining work budget. An expired session requires a new positive budget before resuming.

Pause freezes the remaining time, stops active model work and owned solvers, and preserves accepted designs and completed artifacts. Cancel is terminal. On app restart, running sessions become paused, owned interrupted solvers are stopped, and completed stages are reused after explicit resume. Native numerical recovery handles its own checkpoint compatibility; resuming a paused session whose numerical run was cancelled starts a new run of the exact immutable experiment from its declared initial conditions and records that fact.

With `auto_run:false`, the builder saves an experiment and the session waits. Run that exact setup from the workbench and resume the session to review its measurements; the completed experiment is reused without another builder call.

## Model controls and long provider requests

`POST /api/projects/{id}/messages` supports optional `provider`, `model`, and `reasoning_effort` on each turn, independently of the saved default model. The selected model and reasoning are recorded in conversation metadata.

`GET /api/providers/{provider}/models` obtains account-visible models from the provider. Each entry includes `capability_rank`, `reasoning_efforts`, `recommended_reasoning`, and a short `description`. Ordering is a maintained product heuristic, not a benchmark score or guarantee of access. Only the strongest model in the returned text-model catalog is recommended; unrelated media and embedding models are excluded. No replacement model is silently selected after a provider failure.

Current model-specific effort controls are based on official documentation: [GPT-6 Astra](https://developers.openai.com/api/docs/models/gpt-6-astra) supports low through max; [GPT-5.5 Pro](https://developers.openai.com/api/docs/models/gpt-5.5-pro) supports medium, high, and xhigh; [GPT-5.6 Luna](https://developers.openai.com/api/docs/models/gpt-5.6-luna) also supports none. Unsupported effort values are rejected before sending a call. Anthropic controls follow its [effort compatibility guidance](https://platform.claude.com/docs/en/build-with-claude/effort); this development workflow uses OpenAI for live tests.

OpenAI Pro requests, and requests using high/xhigh/max reasoning, use Responses background mode. The initial response ID is persisted to the usage ledger before polling. PhaseForge polls that response, sends remote cancellation when stopped, and retains reported token usage. Poll failures do not issue another generation. A single provider operation has a 30-minute ceiling; the parent session deadline can stop it sooner. Synchronous calls have a 30-minute HTTP timeout.

Requests retain `store:false`. OpenAI's current [background-mode documentation](https://developers.openai.com/api/docs/guides/background) permits this setting but states that response data is temporarily retained for roughly ten minutes to support polling. A provider call interrupted before its response ID reaches the app can still be billable. On app restart, unknown or interrupted usage is retained conservatively; an unfinished provider call is not silently reissued. Resume retries an interrupted stage explicitly and may incur another call.

The default maximum output is 12,000 tokens per call, adjustable from 512 to 64,000 in **Usage & cost**. Reasoning consumes that output budget, so demanding Pro/max work may need a larger user-selected limit even when its visible report is short. All session calls honor the selected output limit. Experiment builders and Studio designs can recover from explicit output-limit exhaustion within the configured repair allowance by requesting a complete compact result under the same ceiling. Both calls retain their usage; zero repairs disables recovery. Specialist and review calls do not gain an extra retry from this mechanism. Exhausted or partial output is never treated as a validated experiment or artifact.
