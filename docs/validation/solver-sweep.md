# Durable solver sweep acceptance

Before actual execution, the lifecycle fixture is fixed to three sequential real
periodic diffusion cases: 16-square/8 steps/record 8; 32-square/20,000 steps/
dt 0.001 seconds/record 100; 16-square/16 steps/record 8. Other parameters retain
the declared diffusion defaults. Output budget is 512 MiB and deadline six minutes.

Pause only after the second solver exposes a strictly positive computed step.
Require the first completed case and all its pinned files to remain byte-identical,
the third case to remain unscheduled until explicit resume, the interrupted second
case to retain its paused attempt and receive a new immutable attempt, and all
three requested cases to complete with valid stored hashes afterward. Failure of
a case stops further scheduling. No model or scientific label is simulated by
this fixture. The earlier independent diffusion references cover its solver;
this test establishes batch execution/recovery, not additional physical validity.

The initial component had four deterministic checks for admission, frozen input identity,
changed artifacts, and budget/case validation. The real service test is explicitly
ignored in ordinary unit runs because it provisions a managed numerical runtime;
run it separately with `--ignored` and an absolute
`PHASEFORGE_SWEEP_ACCEPTANCE_ROOT`. Do not count the ignored test as passed.

## Actual component acceptance on 2026-09-11

The real service test passed after two earlier failures described below. The
test was `laboratory::sweep::tests::actual_sweep_pauses_between_cases_preserves_completed_bytes_and_resumes`.
Its explicit acceptance report is
[latest-report.json](../../.local/science/sweep-acceptance/latest-report.json),
SHA-256 `e3eee96c40537233bc07b4480295273e0844bcb97eee045ee0520400f28e2d89`.
That filename can be replaced by a later run; this digest identifies the report
reviewed here. The unique retained run directory is
`.local/science/sweep-acceptance/2e13d6a8-b721-4919-a921-5a799d3ccd1c/`.

The sweep is `91a5c361-80b2-4b94-9e42-f66954cb7a43`, in project
`3b593525-c04e-4fb8-833c-b297d83b124a`. Its durable
[case ledger](../../.local/science/sweep-acceptance/2e13d6a8-b721-4919-a921-5a799d3ccd1c/artifacts/laboratory/91a5c361-80b2-4b94-9e42-f66954cb7a43/sweep-ledger.json)
retains the original ordered inputs, attempt IDs and completed artifact hashes.
The run's `phaseforge.sqlite3` stores the sweep/child states and timestamped
events in its `objects` table, as rows with laboratory job records.

| Case | Completed solver job | Attempts | Pinned files | Pinned bytes | Solver lifecycle wall time |
| --- | --- | ---: | ---: | ---: | ---: |
| control | `c6672f8b-58b1-4cc2-b40c-d0134983b523` | 1 | 47 | 207,779 | 28.182 s |
| long | `c8ac96e6-fbbc-48a7-b652-d3392335c1d7` | 2 | 444 | 5,971,594 | 26.161 s |
| last | `258592fa-116b-46e5-a9c8-02454447d0ed` | 1 | 46 | 239,628 | 3.490 s |

The lifecycle times above include preparation/readiness overhead. The completed
workers separately report compute wall times of 0.2830583, 24.0487332 and
0.4562987 seconds. Their simulated durations are respectively 0.04, 20 and
0.08 physical seconds; these are not playback or computation durations.
Each completed child retains its `worker-input.json`, `manifest.json`, numeric
fields, measurements, checkpoint, observations, process logs and `result.json`
under `artifacts/laboratory/<completed solver job>/` in that unique run directory.
The retained diffusion worker SHA-256 is
`ee88e854487ca98bf124bf41a70fbdc992d62b88c912124dce4ca3523a5e95d7`.

The database records creation at `2026-09-11T17:00:18.764326700Z` and completion
at `2026-09-11T17:01:20.681100700Z`, approximately 61.917 elapsed seconds.
The selected deadline remained `2026-09-11T17:06:18.763444800Z` across pause
and resume. No additional deadline was silently granted.

The interrupted long-case child is `b70e9a45-99a5-4fdd-ba7f-7302536ee4df`.
Its stored job progress had reached step 200 before stopping; its final retained
[checkpoint](../../.local/science/sweep-acceptance/2e13d6a8-b721-4919-a921-5a799d3ccd1c/artifacts/laboratory/b70e9a45-99a5-4fdd-ba7f-7302536ee4df/checkpoint.json)
is step 300 at 0.3 physical seconds. The job remains paused. Explicit sweep
resume created the distinct completed long-case attempt listed above; it did
not mutate or relabel the paused attempt as completed. This checks batch
replacement-attempt recovery, rather than same-worker checkpoint continuation.

All six report assertions are true: three measured cases, pause after real
progress, unchanged completed-case hashes, no third-case scheduling while
paused, new-attempt lineage, and verified final hashes. A separate read-only
documentation audit reread all 537 listed files and compared both lengths and
SHA-256 values: zero mismatches. Their 6,419,001 bytes are the completed-case
receipt total, not a total of every interrupted or failed attempt on disk.

## Preserved failures and correction

1. The coordinating execution reported an initial missing-fixture failure.
   This was a test setup failure, not a successful service or solver run.
   Its standalone stderr/fixture-path receipt was not present in the acceptance
   directory reviewed for this document; the exact missing fixture is therefore
   not independently reconstructed here. This failure is not counted as a pass.
2. The next service attempt is preserved at
   `.local/science/sweep-acceptance/560633b4-b1e6-46b5-ba41-52aa73bed77e/`,
   including its database, child artifacts and
   [failed sweep ledger](../../.local/science/sweep-acceptance/560633b4-b1e6-46b5-ba41-52aa73bed77e/artifacts/laboratory/dd7a395c-f4b4-4a6a-8114-855105c2802d/sweep-ledger.json).
   Sweep `dd7a395c-f4b4-4a6a-8114-855105c2802d` completed the control,
   paused its first long attempt, then failed during the replacement long
   attempt `f7894c2f-20e0-4f40-83a7-69c4d66bc29c`.
   Its `sweep_stopped` event at `2026-09-11T16:58:11.436860300Z` records
   `The system cannot find the file specified. (os error 2)`.
   The third case remained unscheduled. The coordinating investigation identified
   an `artifact_inventory` race: enumeration saw an atomic-publication temporary
   file that had disappeared by the subsequent metadata read. The retained
   database confirms the NotFound failure but does not identify that filename.
   The generic inventory traversal was changed to continue on a disappeared
   entry (`ErrorKind::NotFound`) while preserving other error paths. The passing
   run above followed that correction; the failed directory was not overwritten.

## Scope and remaining evidence

This is actual Rust service scheduling plus managed Python diffusion execution,
with no model request between cases. It is not installed ordinary-chat/UI
acceptance, an ML dataset-generation run, or new numerical validation of the
diffusion equations. The earlier independent
[diffusion acceptance](diffusion-lab-results.md) supplies that separate evidence.

The original small sweep stayed far below its 512 MiB budget and completed
before its deadline; that run alone does not establish storage-exhaustion or
deadline-expiry behavior. Subsequent accounting and lineage evidence is recorded
separately below; the original report does not certify those source changes.
The report pins solver source and artifacts, but does not contain the Rust test
binary hash or a complete backend source snapshot. Default unit runs still
ignore the real provisioning test; an ignored test is not an additional pass.

## Final accounting and lineage acceptance on 2026-09-11

After the accounting and lineage changes, the coordinating test execution
reported **7 passed, 0 failed, 0 ignored in 63.83 seconds**, including the
explicitly enabled real service test. This count and duration come from the
coordinator's test output; they are not fields in the numerical run report.
The deterministic checks cover completed-child project/parent identity and
reservation membership, changed science/input rejection, presentation addition,
detached continuation input identity, all-attempt storage accounting, duplicate
attempt rejection, and refusal to admit another solver when already over budget.

The first durability rerun failed before control readiness finished. Its
[preserved error.json](../../.local/science/sweep-durability-acceptance/a1b5bc77-25d9-4831-918b-d89a2b55a0aa/artifacts/laboratory/68e94d44-715f-4ead-9cb7-8c19b6831683/field-readiness/b321a073-85ee-4739-998c-2e2fcbb07675/error.json)
records `FileNotFoundError` opening `fields/view-00000000.json.tmp`.
The absolute target is 263 characters long; a read-only check confirmed that
its parent directory exists and already contains `field-00000000.npy`.
The coordinating investigation attributed this failure to the Windows long-path
environment limit introduced by the longer evidence-root name. No OS setting
was changed. This is a separate failure from the earlier inventory race.
The same scientific and solver-source parameters were rerun under the shorter
`.local/science/sweep-durable/` root, preserving the failed directory.

The final [durability report](../../.local/science/sweep-durable/latest-report.json)
has `passed: true` and all six lifecycle checks true. Its SHA-256 is
`88f0552f27e8c7807109f3cc228f2c0ead8479dcf8b34e9984e999380b4a3fb6`.
The retained run directory is
`.local/science/sweep-durable/fa126f3c-af0d-48ba-8658-a57e6637e894/`,
project `52be5ff4-74a9-4a16-af3b-a3ea5426eade`, sweep
`5da23524-6b62-4314-8e60-b0d43cd34115`. Its
[case ledger](../../.local/science/sweep-durable/fa126f3c-af0d-48ba-8658-a57e6637e894/artifacts/laboratory/5da23524-6b62-4314-8e60-b0d43cd34115/sweep-ledger.json)
preserves the paused long attempt `00d8d134-3429-4189-86e3-07a19c969916`
and the distinct replacement listed below.

| Case | Completed solver job | Attempts | Pinned files | Pinned bytes | Solver lifecycle wall time |
| --- | --- | ---: | ---: | ---: | ---: |
| control | `0f9a5ee5-180c-423d-8381-52968d296797` | 1 | 47 | 207,763 | 28.914 s |
| long | `663cb27f-51eb-4429-85ac-d0da049d48e3` | 2 | 444 | 5,971,590 | 26.233 s |
| last | `911dbb69-cc7b-47a3-b352-2d0e304e095e` | 1 | 46 | 239,621 | 2.898 s |

A fresh read-only audit of this new report reread its 537 pinned files and
verified every length and SHA-256: zero mismatches, 6,418,974 pinned bytes.
This is a distinct audit from the original run's 537-file audit above. The
paused attempt adds 314,953 bytes across 48 files; independently summing all
four attempt directories gives 6,733,927 bytes, exactly the final stored
`result.retained_bytes`. The database records creation at
`2026-09-11T17:19:52.347430700Z`, completion at
`2026-09-11T17:20:55.524948800Z` (about 63.178 seconds), and deadline
`2026-09-11T17:25:52.347287400Z`. These lifecycle times are wall time,
separate from the unchanged physical simulation durations.

The storage budget is a **soft logical-file-length limit**, covering every
reserved attempt directory: failed, paused, completed, active, and pending
reservations even before their job record exists. Temporary publication files
and added presentation files count toward it. It is not a measurement of
allocated disk blocks; concurrent growth can overshoot between checks, and
an atomic temporary file that disappears during enumeration is skipped.
The sweep's own ledger/database are outside this attempt-directory total.
The deterministic accounting test uses real files in failed, paused, completed
and pending directories. The separate admission test uses `File::set_len` to
create a real retained failed-attempt file of **268,435,457 logical bytes
(256 MiB + 1 byte)** against a 256 MiB limit, then verifies refusal before
any new attempt is created or solver launched. This tests the logical budget;
it does not claim physically filling the disk or exhausting free space.
The passing lifecycle run still does not exercise deadline expiry or installed
ordinary-chat acceptance, and no new numerical reference claim is made.
