# 0.7.0 — direct experimentation, not a planning prerequisite

Incremental release from the uploaded v0.6.0 complete project. The motivating export
showed repeated plans and task prompts asking for proposals only, leaving no executable
setup while the UI displayed the first stage. This release changes command routing
and the visible workflow rather than removing scientific limits or inventing results.

- Experiment-first workspace with explicit Build, Build & run and Run saved setup.
- Typed experiment/review intents; direct requests cannot return another research plan.
- Visible permission for hypothetical inputs, and server-checked per-build time/memory/
  total candidate-evaluation caps. Empirical values and claims remain evidence-bound.
- User run consent is authoritative for one accepted direct experiment; model
  `should_run=false` is no longer an extra veto. Model refusal/capability errors do not run.
- Optional plans/sources, not research-task or publication gates. Review gives analysis,
  not automatic replanning. Source retrieval still requires public-query consent.
- Standalone Discovery removed; existing advanced records and functionality preserved
  behind More in Laboratory; historical links redirect with scoped identifiers.
- Provider-free deterministic repeat, smaller timestep and longer-horizon operations;
  immutable source copies; preserved budgets; request receipts prevent duplicate enqueue.
- Manual ODE equation editor for saving/running user mathematics without an API key.
- Same-project concurrent model requests rejected; synchronous UI locks prevent common
  fast double-submit races. HTTP-success assistant failures are displayed as errors.
- Selected revision/run context is explicit; queued errors retain the saved setup; old
  visual evidence is not relabeled when a new revision is authored.
- Current-operation progress replaces the confusing five-stage ribbon. Stop and machine
  telemetry remain. Results expose direct numerical actions even without a provider key.
- Version labels and start-script server checks now use v0.7.0.

No scientific solver equations, verification source hashes, database/keyring locations
or application dependency versions were changed. No material contact/chemistry engine
was added. Exploratory calculation is permitted before empirical validation, but its
output does not establish physical truth, a cure, clinical safety, chaos or novelty.

Validation limits and actual check results: `docs/RELEASE_VALIDATION.md`.
