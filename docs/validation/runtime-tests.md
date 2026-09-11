# Laboratory runtime acceptance

These checks exercise the public durable-job service and the trusted-worker
supervisor. They do not call a model, install packages, use the user's live
database, or run an experiment. All job databases and process receipts use
temporary directories. The Python subprocesses are fixed, benign test fixtures.

Run on Windows in PowerShell from the repository root:

```powershell
$env:PHASEFORGE_TEST_PYTHON = (Resolve-Path .local/science/venv/Scripts/python.exe).Path
cargo test --locked --manifest-path backend/Cargo.toml --test laboratory_execution -- --nocapture
```

`PHASEFORGE_TEST_PYTHON` must point to an existing absolute Python executable.
Process cases explicitly print `SKIP` if it is not configured; that is not a
Windows acceptance pass. An invalid configured executable fails. If Windows
denies symlink creation with privilege error 1314, the directory escape case
creates an actual directory junction instead. Failure to create either fixture
fails the test. These Windows-specific checks do not establish equivalent
process-tree enforcement on another operating system.

## Frozen behavioral requirements

- Eight simultaneous submissions with the same immutable request identity create
  one saved job and one initial event. Reusing that ID for another payload,
  project, kind, or parent is rejected. A replay does not resurrect paused work.
- Reopening an actual temporary SQLite database pauses every queued, running,
  provisioning, or waiting job, preserves terminal jobs and artifact bytes, and
  does not append another interruption to an already-paused job. A new attempt
  has a separate directory and an explicit source ID, preserving the old record.
- Stopping a parent cancels active descendants' actual cancellation tokens;
  completed descendants and their evidence remain intact. One job cannot acquire
  two concurrent execution leases.
- Artifact reads reject traversal, absolute/UNC paths, Windows alternate streams,
  another job's files, directories, and JSON larger than 16 MiB. Inventory and
  reads reject an actual directory symlink or junction pointing outside the run.
- A Python parent immediately spawns a Python child. Both must first be observed
  alive, and retained Windows process handles (including the separate venv Python
  launcher when present) must signal exit after token
  cancellation and after supervisor drop. Cooperative cancellation must write
  `cancel.request`; an uncooperative fixture is killed after the three-second
  grace period, including its child.
- A cooperative fixture must actually observe the cancellation file, save a
  checkpoint receipt, and exit within the grace period. It must not be reported
  as successful completion.
- Under a 128 MiB Windows process-memory cap, a 16 MiB allocation succeeds and a
  512 MiB allocation raises Python `MemoryError`. This checks an actual allocation,
  not only configured flags. This is a per-process cap, not a total tree cap.
- A helper test process receives synthetic credential and Python-path sentinels.
  A worker launched by `clean_command` must observe none of them, and must have
  isolated Python mode and disabled user-site imports. The main test process's
  real environment is never modified and credential values are never read.

These checks cover service-level persistence and process behavior. They do not
prove HTTP admission, provider-resume reconciliation, numerical accuracy, model
vision, UI playback, package installation, predictive validity, or marketplace
release acceptance. Molecular numerical acceptance is documented separately in
`molecular-lab.md`. New-attempt creation here checks the public service boundary;
the HTTP resume handler needs its own installed application acceptance.

## Execution evidence — Windows, 2026-09-11

The command above completed with **10 passed, 0 failed, 0 ignored**, in 3.49 s
after compilation. The explicit interpreter was the development venv's CPython
3.12.7. All Windows process cases executed; no final case was skipped. Windows
denied symlink creation, so the directory escape check exercised a real junction.

The first run returned eight passing cases and two failing process-tree cases:
the fixture incorrectly assumed the venv launcher's PID equaled `os.getpid()`
inside Python. The fixture was corrected to keep handles for the launcher, the
actual Python parent, and its Python child. All three must stop. The initial
symlink privilege skip was also replaced by the junction fixture. No behavioral
requirement was relaxed.

Observed outcomes:

| Case | Evidence |
| --- | --- |
| Concurrent identity replay | Eight submissions, one record/event; all conflicting immutable fields refused |
| Restart and new attempt | Four active states paused after reopening SQLite; terminal states, old record, event sequence, and artifact bytes retained |
| Parent stop | Active child/grandchild tokens cancelled; completed child's evidence retained |
| Path and size bounds | Cross-job/traversal/ADS/directory/oversized JSON refused |
| Reparse-point escape | Actual external directory junction excluded from reads and inventory |
| Process cancellation | Live launcher/parent/child handles all signaled exit, for immediate and forced-after-grace cancellation |
| Supervisor drop | Live launcher/parent/child handles all signaled exit |
| Cooperative checkpoint | Fixture observed the signal and saved step 42 before exiting; cancellation remained an error outcome |
| Memory enforcement | 16 MiB control succeeded; 512 MiB allocation raised `MemoryError` under the same 128 MiB process cap |
| Environment isolation | Actual child observed zero synthetic credential/path sentinels, isolated mode 1, and disabled user-site imports |

The build reported existing dead-code warnings for the legacy ODE and particle
`execute` functions. This task ran only the new integration target, not the full
backend suite.
