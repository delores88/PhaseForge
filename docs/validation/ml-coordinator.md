# Durable ML coordinator: integration evidence

The scientific contract remains the frozen [ML proposal](ml-lab-proposal.md).
This document records implementation checks; it does not claim a fitted
scientific model, completed 99-run study, installed workflow or useful speedup.

On 2026-09-11 the actual service test
`laboratory::ml_study::tests::actual_ml_protocol_freeze_pilot_and_reduction`
completed with the five deterministic coordinator tests: six passed, zero
failed or ignored, 56.09 seconds. The actual test was explicitly enabled with
`--include-ignored`, an absolute `.local/ml` acceptance root and the local
scientific Python bootstrap. It provisioned the managed runtimes and used the
production Windows LPAC and supervised OpenMM paths.

The retained report is `.local/ml/latest-report.json`; data remains in
`.local/ml/9141a8c2-03dd-43d3-94ac-3f437a321482`.

| Receipt | Identity |
| --- | --- |
| Component study | `c30df59c-f6a0-46a2-b391-b17aa908bad6` |
| Isolated protocol/split freeze | `22baa51a-1204-4ccf-8fbb-105442cde9cf` |
| Actual pilot sweep | `d83f4107-9940-44c5-ac38-939e8851ea5e` |
| Actual OpenMM seed | `0c8b0733-5843-42c2-abe3-80228888cd02` |
| Isolated pressure-window reduction | `a6d84f40-66e7-47dd-b11c-e076f52b4f38` |
| ML worker SHA256 | `74475774261c19d6e0bc0315789b3a79940effcbf7f79d8f7b3fed473deac892` |

The first frozen training case was condition 1, replicate 0, seed 111000,
220 K and 0.45 g/cm³. It retained 201 scalar samples from 20,000 integration
steps. The reducer used exactly the prescribed 100 observations at steps
10,100–20,000 and reported mean pressure 165.39684255441952 bar,
P0 204.14293793528185 bar. This single correlated finite-window endpoint is
neither an equilibrium estimate nor empirical validation. Its five block means,
source manifest, raw measurement hashes and attempt lineage are in the report.
The component study was paused afterward; it did not fit a model or generate
the other 98 seeds. The installed acceptance must have its own ordinary-chat
study and retain every actual run.

The deterministic checks cover exact stage admission, forged role/source/code
rejection, copied and monitor-derived protected provenance, unrelated malformed
requests not poisoning storage accounting, reserved partial files counting
toward storage, completed-byte mutation rejection, idempotent metadata replay,
and Stop preventing later stage admission. Solver receipt hashes are verified
again before reduction, so a changed file cannot be accepted merely by assigning
it a new hash. Model-freeze receipt replay repairs a missing derivative file
from its already committed ledger identity and rejects changed identities.

The production workflow freezes membership, reuses its measured pilot, executes
the remaining fixed dataset, admits only training/validation shards to fitting,
commits model identity, then admits calibration and held-out roles. It retains
the original held-out predictions before the OOD query launches three fresh
solvers. Cost finalization cannot reopen labels, refit or recompute predictions.
The final numeric dataset export is a separate isolated read-only stage.

Actual installed full-study execution, pause/recovery, frozen-model reviewer
challenge, numerical/visual independent checks, and the usefulness verdict
remain pending. A completed workflow may validly reject the surrogate.

The final pre-installation targeted Rust batch passed 15 checks (two explicit
live components excluded from that batch and tested separately). Independent
review found and repaired a crash window between a cost-stage reservation and
its database record: the full cost snapshot is now committed to the durable
ledger first and reused lazily. A regression recreates that missing-record
window and a later reserved stage without recalculating the frozen costs.

The setup charge conservatively uses the larger of summed child elapsed costs
and study creation-to-snapshot elapsed time, retaining both values and the
nonnegative residual. It includes coordinator work, queueing, pauses and recovery
and can overcharge overlapping work. It is not active CPU time. Failed-attempt
timings come from retained terminal events rather than mutable last-viewed or
updated timestamps. Final archival export, plotting and report publication are
outside the pre-finalization snapshot and are reported separately.

The registered plot subprocess actually executed against exact copied synthetic
source bytes: `.local/plt/latest-report.json`, plot job
`3bab1294-9d9c-48d9-a55b-5808c22d6616`. It verified all three source pins and the
PNG hash. The preceding attempt correctly rejected a synthetic fixture whose
JSON was parsed and reserialized after hashing; its failed output remains in
`.local/plt/d09f75ab-fc44-4d6e-9f5e-567576567131`. Only the fixture changed to
copy original bytes. Neither attempt is scientific ML acceptance.
