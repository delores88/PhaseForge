# ML worker and coordinator contract

Implementation contract, 2026-09-11. The frozen scientific protocol is
[`ml-lab-proposal.md`](ml-lab-proposal.md), SHA-256
`b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737`.
This document specifies mechanics; it does not revise that protocol or claim a
fitted candidate, completed dataset, or installed acceptance.

## Execution and trust boundary

`tools/ml_surrogate_worker.py` is a self-contained standard-library/NumPy source
for the existing `python_numpy` generated job. As `experiment.py`, it reads
`input.json` in its working directory, reads only explicitly listed files under
`imports/`, and writes finite JSON and numeric NPZ (`allow_pickle=False`) into
the working directory. `result.json.artifacts` lists relative output paths.
An optional command-line working directory exists for fixed source fixtures;
production uses the existing LPAC runner without host-shell access.

The coordinator supplies an immutable common envelope:

```json
{
  "schema_version": 1,
  "phase": "fit",
  "study_id": "durable-study-uuid",
  "proposal_sha256": "b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737",
  "split": {"path": "split.json", "sha256": "64 hex characters"},
  "sources": [{"path": "train-0.json", "sha256": "64 hex characters", "role": "train"}],
  "seal": {"stage": "fit", "previous_job_id": "durable-job-uuid"}
}
```

Paths are relative to `imports/`; plain-file and hash checks apply before JSON or
NPZ decoding. The generated source rejects unexpected roles and malformed
membership too, but that is a consistency check, not access control. The trusted
coordinator must derive roles from its own frozen split and job registry, not
from caller-supplied `role`, `seal`, code, artifact names, or timestamps. Direct
generated imports of protected study labels must obey the same policy. Copying
or renaming a calibration/test file does not remove its protected provenance.

The coordinator pins the source hash for each admitted stage. It commits a
model-freeze receipt with fit job ID, `model.npz` hash, model-card hash, split
hash, protocol hash, and source hash **before** admitting any calibration/test
label imports. Failed or cancelled stage receipts remain in the ledger. A retry
reuses immutable imports and model identity; it cannot reopen model selection
after a held-out access.

## Phase inputs and outputs

| Phase | Admitted label roles | Required inputs | Retained outputs |
|---|---|---|---|
| `freeze` | None | Exact pinned protocol identity, study ID, proposal hash | `study.json`, `split.json`, `runs.json` with 99 immutable per-seed requests |
| `reduce` | One role per job | Up to 24 registered `measurements.json` files and one manifest bundling their solver metadata/provenance | `shard.json`, `shard.npz`, per-seed and pressure-block summaries |
| `fit` | `train`, `validation` only | Complete role shards plus frozen split | All 72 fitted candidate scores/failures; selected refit `model.npz`, `model-card.json`, `fit-data.npz` |
| `calibrate` | `calibration` only | Frozen model/card and complete calibration shards | `calibration.json`, residuals, constant interval half-width bound to the frozen model hash |
| `evaluate` | `test`, `regime_test`; fitting baselines from frozen `fit-data.npz` | Frozen model/card/calibration, complete held-out shards, measured cost receipt | Per-case predictions, baselines, coverage/support decisions, rejection gates, workload arithmetic, finite `evaluation.json` |
| `infer` | None | Frozen model/card/calibration/evaluation, complete query protocol and units | `prediction`, `invalid_input`, or `requires_solver`, with exact reason and identities |
| `finalize_costs` | None | Frozen model/card/calibration, pinned initial evaluation, complete measured cost receipt | New `evaluation.json` retaining initial predictions and scientific gates; only cost gates and overall availability change |

`reduce` uses one role per shard. Splitting 36 training seeds across two reduction
jobs is permitted; a fit aggregates only complete three-seed conditions and
rejects duplicate or missing replicas. Labels are never additional independent
rows merely because a pressure trajectory contains 201 frames. The three
baseline rules are scored separately from the 8 polynomial and 64 kernel fits.

Every run manifest entry contains `condition_index`, `replicate_index`, `seed`,
trusted `role`, `solver_job_id`, actual attempt lineage, exact parameters,
`measurements_path`, `measurements_sha256`, `manifest_sha256`, exact UTF-8
`manifest_text`, worker hash, and source artifact references. The bundling coordinator
must verify metadata against the corresponding immutable solver record and
artifacts before allowing a reduction. The manifest hash covers the original
UTF-8 bytes, not a JSON reserialization. A separate parsed `manifest` is optional
and should be omitted across serializer boundaries. Reduction validates all 201 integer
sample steps, finite values, time increments, units, protocol identity, box
volume and the exactly 100 pressure observations with
`10000 < step <= 20000`. It retains the original source references in its output.

`shard.json` records the common study/split/protocol identity, one role, and seed
records. Each seed record retains its condition and replica identity, source
job/hash references, temperature, density, volume, P0, mean pressure, five block
means, descriptive block correlation, and the late-minus-early window difference.
NPZ arrays contain numeric data only. Constant-block correlation is explicitly
undefined in finite JSON (`null`), rather than emitted as NaN.

## Frozen protocol and model identity

The identity includes the proposal hash, engine, wheel version, actual engine
manifest version, solver worker hash, platform and properties, fixed integrator
parameters, particle/potential/boundary conventions, target integer-step window,
feature names and units, input normalization, and label/P0 definitions. The
accepted wheel is OpenMM 8.5.2; its observed manifest version is
`8.5.2.dev-36a30cb`. Preserve both values. A different source or actual engine
version is a protocol mismatch, not a compatible-model guess.

`model-card.json` includes schema/study/split/proposal/protocol/source identities,
the complete selected lexical configuration, fitted condition indices, feature
schema (`temperature_kelvin: K`, `density_g_cm3: g/cm^3`), normalization bounds,
target center/scale fitted from the admitted rows, array names/shapes, numeric
model file SHA-256, and scope limitations. Polynomial arrays contain coefficients;
kernel arrays contain normalized fitting coordinates and Cholesky-solved weights.
The model is frozen before calibration and remains unchanged after evaluation.

Inference performs validation before any prediction: malformed/nonfinite values,
wrong units, and values outside the solver's own numeric limits produce
`invalid_input`. Valid solver requests with unsupported surrogate parameters,
different protocol/hash, inadequate support, wide calibration intervals, or a
rejected candidate produce `requires_solver`. An explanation-only request returns
that decision without starting work. A scientific request requires the trusted
coordinator to launch fresh solver children and return their actual IDs, measured
endpoint and replicate lineage. A worker cannot claim it scheduled a solver.

The frozen OOD condition is index 32, T=180 K, density=0.55 g/cm^3, seeds 142000,
142001, 142002. It belongs to role `ood`; its three actual runs are charged to
study setup and cannot enter fitting, calibration, or the eight initial held-out
scores. A later reviewer challenge has separate job lineage and disclosed cost.

## Independent checks and timing

`tools/check_ml_surrogate.py` will not import worker helpers. It calls worker
entry points as a subprocess with explicit fixed fixtures, and independently
solves small systems, computes expected split membership, validates calibration,
checks label reductions and recomputes evaluation gates. It retains raw worker
outputs and failure receipts. Synthetic fixtures are labeled as matrix fixtures;
they never become scientific study labels.

The final study checker additionally reads registered original solver arrays to
verify the prescribed pair-virial pressure samples. No actual label generation,
fitting on scientific labels, or 99-run study begins merely by passing component
fixtures. Measured cost records must separately identify dataset/solver wall
times, startup, model selection, calibration/evaluation, cold loading, warm
single-query and batch timings, and fallback cost. Missing timing or labels
blocks the applicable comparison; it is never zero cost or a passed threshold.

## Exact file descriptor keys

Every imported descriptor is `{ "path": "relative-name", "sha256": "..." }`.
Descriptor names can differ from output names; the worker uses the supplied path.
The common envelope contains `study_id`, `proposal_sha256`, and `split` except
for `freeze`, which creates the split. `schema_version` is 1.

- `freeze`: `solver_worker_sha256`, `solver_engine_version` (the exact observed
  `8.5.2.dev-36a30cb`). `runs.json` contains common study/protocol identities,
  `split_sha256`, and `runs`. Each run has `case_id`, `condition_index`,
  `replicate_index`, `role`, `seed`, `engine`, and complete `parameters`. The pilot
  is the first training entry. Schedule that entry once, the other 95 dataset
  entries, and the three `ood` entries through the actual fallback.
- `reduce`: `role` and `manifest`. The manifest bundle has common identity,
  `role`, and `runs`; each run has the frozen fields plus original manifest text
  and scalar hashes described above. The scalar paths name separate imports.
- `fit`: `sources`, a list of role shard descriptors with a `role` field. All
  training and validation conditions must have exactly three replicas. Outputs
  also include `selection.json` and `fit-labels.json`.
- `calibrate`: `model`, `model_card`, and calibration `sources`. Additional output
  `calibration-labels.json` retains per-condition uncertainty/provenance.
- `evaluate`: `model`, `model_card`, `fit_data`, `calibration`, and test/regime
  `sources`. Optional `costs` is a pinned measured cost receipt. Before OOD
  completion it is normally absent, so the cost decision is pending.
- `infer`: `model`, `model_card`, `calibration`, `evaluation`, and `query`.
  `query.features` has exactly `temperature_kelvin` and `density_g_cm3`;
  `query.units` maps them to `K` and `g/cm^3`; `query.protocol` is the complete
  frozen protocol; `query.intent` is `scientific` or `explanation`.
- `finalize_costs`: `model`, `model_card`, `calibration`, the initial `evaluation`,
  and `costs`. `sources` must be absent or empty. This stage retains the previous
  cases, predictions, errors, coverage, support and benchmark samples; it records
  `cost_finalization.prior_evaluation_sha256` and updates only cost gates and
  overall availability. There is no selection, refit, prediction recomputation,
  or label access. A candidate with no admitted query gets a completed rejection
  with no invented speed measurement once all costs are known.

The cost receipt contains `solver_runs`: 99 distinct rows with `case_id`,
`solver_job_id`, `status: "completed"`, and positive `wall_seconds`. It also
requires measured nonnegative `environment_startup_seconds`,
`model_selection_seconds`, `calibration_evaluation_seconds`,
`cold_model_load_seconds`, and `failed_attempt_seconds`. These are disjoint
accounting increments. If cold loading is already included in generated job wall
time, its additional accounting increment can be zero with an explicit
explanation; report the actual cold load benchmark separately. Cost finalization
is a frozen accounting snapshot, and any excluded publication overhead must be
disclosed.

## Independent real-study export

`tools/check_ml_study.py --manifest <audit.json> --output <report.json>` reads
evidence only. Its manifest has an optional `root` relative to the audit file,
descriptors `split`, `model`, `model_card`, `calibration`, `evaluation`, `costs`, `worker`,
`dataset`, `dataset_metadata`, and `role_shards`. Each role shard descriptor points
to a registered `shard.json`; the dataset descriptors point to the registered
`dataset.npz` and `dataset.json` archival export.
The `runs` list contains all 99 frozen case/role/replica/seed/parameter identities,
`solver_job_id`, a solver `directory` relative to root, its `artifacts` inventory
(`path`, `sha256`, `bytes` relative to that directory), and `job_record`, a pinned
export of the actual completed `LabJob` relative to root. The checker derives
solver elapsed costs from that record's creation/completion timestamps.
The `worker` descriptor points to the exact surrogate worker source pinned by the
model card, not a later checkout version.

The checker streams every registered artifact hash, recomputes all 99 seed
means and all 33 condition means, verifies reduction provenance, recomputes 16
pair-virial pressure samples from retained arrays, and independently checks
retained-model predictions, calibration, baselines, coverage/support gates and
workload arithmetic. Its `--self-test` runs only fixed pair algebra. It does not
replace the coordinator's access-control/recovery tests or the installed UI and
actual-model-image acceptance. An absent or failed run stops a full-evaluation
claim; it is not imputed.

`tools/export_ml_check_manifest.py --data-directory <saved-data-directory>
--study-id <completed-study-id> --output <fresh-audit-directory>` assembles this
manifest from the study and sweep journals. It opens SQLite in read-only mode,
checks stage and solver ownership, verifies the original serialized input bytes
without re-encoding floating point values, and streams every completion artifact
hash. It writes only explicit audit output: exact job snapshots and `audit.json`.
It refuses incomplete studies, missing cases and changed receipts. Run the
independent checker on the resulting manifest to perform the numerical audit;
successful assembly alone makes no scientific acceptance claim.

The independent checker also verifies all 33×3 exported seed pressures, condition
means, standard deviations, standard errors, units, model-normalized values,
role codes and source-job ordering against independently reduced scalar records.
Its source fixtures are `tools/test_ml_check_manifest.py`.
