# Extended installed research: preregistered workflow

Before dispatch, 2026-09-11. Use the installed third scientific checkpoint
(`2026-09-11T16:21:32.328Z`) through its normal project/chat API. Native navigation
checks remain separate; an API call is not counted as a native click.

The first actual request asks for 32 argon-like atoms, 120 K, 0.8 g/cm³,
Langevin friction 1/ps, seed 314159, CPU one thread, timestep 1 fs, 500,000
integration steps, sample interval 20, chunks of 100. This is 500 ps and
25,001 calculated states. Timer Off is explicit. The prior source-level run
supports feasibility only; it is not substituted for this installed execution.

During calculation the app should monitor a retained checkpoint at which
`msd_nm2 > 0.1`, supply its actual image to the model, compare its appearance
with the simultaneous numerical instrument, and request an additional camera
view when it helps distinguish periodic wrapping or occlusion from transport.
Record whether the source was still running when the observation was acquired.
After completion, an isolated instrument should compare early and late
mean-square-displacement behavior with units and uncertainty limitations. A
specialist should challenge the interpretation using the exact run and
instrument receipts. Its advice must inform a further test rather than being
reported as independent empirical evidence.

The following cycles will be selected after inspecting these measured outputs.
Preserve failed attempts and corrections. Conversation compaction must retain
the original goal, constraints, job IDs, measured values/units and unresolved
questions; the full original transcript remains retrievable. Do not insert
fabricated context or solver data merely to trigger a threshold.

Acceptance requires the genuine 25,001-state final trajectory and immutable
hashes, provider image-delivery receipts, a numerical/visual comparison,
additional-view provenance, a real review handoff, later evidence-driven
experiments, and observed compaction/recovery where actually exercised. This
document contains no execution result yet.

## First installed study: independently audited 2026-09-11

The first finite dispatch completed through the installed chat API using
`open_ai`, `gpt-5.6-sol`, medium effort and Timer Off. The actual request explicitly
said to stop after one measured study and one proposed next experiment. That
scope completed; the broader sequence of later evidence-driven experiments in
this preregistration has **not** completed. This audit performed no solver/model
execution and makes no native-navigation claim.

Session `de06fabe-1ade-4ac9-be9d-48d2b2835a33` belongs to project
`6989338a-79a8-46f5-9f49-9f4bbf8366f3`. It retained 17 model rounds, 18 tool calls
and one actual provider-generated continuation. The complete original request,
including its stop condition, is preserved in
[dispatch.json](../../.local/science/installed-extended/dispatch.json) and the
[captured session](../../.local/science/installed-extended/audit-20260911/de06fabe-1ade-4ac9-be9d-48d2b2835a33.json).
An initial invalid provider identifier was rejected before scientific action;
[its dispatch receipt](../../.local/science/installed-extended/dispatch-invalid-provider.json)
remains separate from the corrected `open_ai` request.

### Retained numerical evidence

Solver `62bdf4b7-039b-48b5-8024-6f6feabf695e` completed the exact parameters above:
500,000 integration steps, 25,001 states, 251 immutable trajectory chunks,
251 checkpoint images and 499.99999999508844 ps. Its 1,022 files occupy
146,109,262 logical bytes. The actual retained worker has SHA-256
`9a7c3167f164a6e4f8ab27988584b66492657ebf2ec87fa1a3ac541fc76e88d5`;
it records OpenMM `8.5.2.dev-36a30cb`, NumPy 2.4.6, Python 3.12.7 and one CPU thread.

[The independent existing-output audit](../../.local/science/installed-extended/audit-20260911/numerical-attempt2/report.json)
validated every saved frame, array/chunk hash, step/time range, image/source binding
and finite numeric array, including states beyond the former 20,000-state limit.
It independently recomputed Lennard-Jones quantities on 69 declared sample frames
using the frozen pair-sum reference, without importing or executing worker methods.

| Quantity | Maximum absolute error | Frozen tolerance |
| --- | ---: | ---: |
| Potential energy | 3.0922245813e-5 kJ/mol | 2e-4 kJ/mol |
| Force component | 9.3479910618e-4 kJ/(mol nm) | 3e-3 kJ/(mol nm) |
| Pressure | 2.6147972676e-12 bar | 1e-8 bar |
| Unwrapped MSD | 0 nm² | 1e-12 nm² |

The numerical gate passed. Worker wall time was **496.203 s**. The copied component
checker carries a **480 s process-launch cap**, which the existing-output
verification function does not apply. This installed job explicitly had Timer Off;
it therefore completed correctly under its actual time policy, but it did **not**
demonstrate completion within the component 480 s cap. No numerical tolerance or
trajectory length was changed to obtain the pass.

The read-only entry is [check_installed_extended.py](../../tools/check_installed_extended.py).
An initial audit-wrapper failure tried to hash the module cache directory as a file,
before verification began. It is retained in
[preflight-failure.json](../../.local/science/installed-extended/audit-20260911/numerical/preflight-failure.json).
The successful second audit copied the actual installed worker and frozen checker
sources, and wrote its own frame/file hash inventories; it did not copy or rerun
the 146 MB scientific result.

### Monitoring, actual image delivery and visual limits

Monitor `209669b7-90e0-4bf6-b99e-c86cf0b4132b` detected the first retained image
checkpoint above `msd_nm2=0.1` at step 4,000, time
3.9999999999996705 ps, MSD 0.14694635007449575 nm². The preceding retained image at
step 2,000 had MSD 0.06534053352405317 nm². Its receipt and timestamps show that
the solver was still running. This establishes exceedance at that saved checkpoint;
it does not identify the exact continuous first-passage time.

The monitor's PNG matches its solver source byte-for-byte, SHA-256
`f49f6ff0d0a6546995796310c239b0119f29d3009e3bf5d16c4db0f527e13d46`.
Its registered chunk, exact frame hash, source-array data and simultaneous numerical
instruments were checked independently. One temperature value changed by exactly
one floating-point ULP during JSON processing: source 115.53058487056117 K versus
monitor receipt 115.53058487056116 K, a 1.4210854715e-14 K difference. The initial
[exact-identity audit failure](../../.local/science/installed-extended/audit-20260911/workflow-exact-identity-attempt1.json)
is preserved. The subsequent metadata consistency audit reports this difference
explicitly and allows one ULP only for that comparison; frozen solver tolerances
remain unchanged. MSD, time and byte hashes agree exactly.

Additional view `83e32af9-8008-48e1-8b1a-08cac9fc46a3` rendered the same step-4,000
state, held without interpolation and without a scientific rerun. It used Blender
4.5.9 LTS Eevee, 1024×1024 PNG, looking along the x axis with z up. The PNG hash is
`89fc46237275cde5009b012054df1c543ec69730c4c546d9485a346b8333f6a4`.
Both actual saved images were opened during this audit: the numerical xy projection
has labeled nm axes and overlapping markers; the additional sphere view has heavy
occlusion and objects cropped by the viewport. The latter is a limited spatial
inspection, not a useful displacement or transport-rate measurement.

All 11 retained provider image acknowledgements were cross-checked by decoding the
actual PNG bytes in their saved requests and matching them to the two source image
hashes and completed provider-response IDs. This supports actual image delivery;
it does not make the model's visual interpretation an independent physical test.
The extra view's loaded chunks, topology, worker and image hashes remain verifiable.
This installed checkpoint saved a hash of its then-current trajectory index but no
raw historical index snapshot. The index subsequently grew, so the full original
index byte object cannot now be independently verified. Later source-pin support
must be tested on a newer run; it is not retroactively credited here.

### Authored instruments, specialist and context continuation

Initial isolated instrument `9db7069a-8cae-45aa-a4cc-fdeaf6d0cb24` completed its
execution but omitted the nominal 450 ps sample from the late window. The stored
timestamp is 3.72921249e-9 ps below 450, outside the script's 1e-9 ps endpoint
tolerance. Its late OLS/secant analysis used 2,500 samples while its block descriptors
used the nominal endpoint, producing inconsistent selection. This attempt and code
remain retained; it is superseded for the equal-window comparison.

Corrected instrument `4522297c-c635-4b59-95e9-521d64e7d1fb` used nearest retained
indices `[0,2500]` and `[22500,25000]`, exactly 2,501 samples per window.
[The independent instrument audit](../../.local/science/installed-extended/audit-20260911/instrument-audit.json)
checked 109 descriptive statistics with scalar `math.fsum` references, maximum
discrepancy 5.3290705182e-15. It also recomputed MSD directly from 23 retained array
frames at window/block endpoints, maximum discrepancy 3.5527136788e-15 nm², and
verified original source, code, import and output hashes.

Early and late OLS slopes were respectively 0.04645384434073179 and
0.02902217034188047 nm²/ps; the ratio was 0.6247528219410071. The secant ratio was
0.5146564239111182. Three of ten late 5 ps block slopes were negative. These are
descriptive statistics of a single-origin MSD in one realization, not diffusion
coefficients, confidence intervals or evidence that stationary transport changed.

Specialist `31d8e5ff-9a61-4a6e-830d-4c98b2465a08` actually completed three rounds
and eight tools against the exact source/analysis evidence. Its response identified
correlated samples, startup relaxation, one-origin MSD, one seed, N=32, periodic
finite-size effects and Langevin dependence. The parent incorporated those limits
and proposed a replicated stationarity experiment with multiple seeds, a declared
relaxation period and multi-time-origin MSD at equal lag times. That experiment was
**proposed only**, as the finite dispatch required.

One trace status is incorrect: event 72 says `inspect_specialist returned an error`
although the actual output has `state: completed`, `error: null` and the complete
specialist answer. The answer was received and used. The audit preserves this UI/
trace classification defect separately from scientific execution.

The genuine compaction `3abe5992-5454-476f-a4c1-a910e2aca869` received a completed
provider response, retained all 61 journal items and advanced context start to
item 52. Its factual ledger preserves the exact original request, six child IDs,
parameters, Timer Off and stop constraint, both image hashes, the failed initial
analysis, corrected values/units, limitations and proposed-only next experiment.
The parent then recalled job IDs and explicitly saved a continuation memory matching
the durable journal. No subsequent solver was launched. This demonstrates one
successful real compaction/continuation, not arbitrary-duration reliability or the
remaining multi-cycle study.

[Workflow audit](../../.local/science/installed-extended/audit-20260911/workflow-audit.json):
79 consistency checks passed after separating the documented one-ULP serialization
finding; the report separately records the unmet 480 s component cap, misleading
trace status, historical-index limitation and incomplete broader study. All saved
source/evidence bytes read by that audit have retained SHA-256 entries.
