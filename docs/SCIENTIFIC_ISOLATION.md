# Generated experiment execution on Windows

This boundary is separate from the process supervisor for trusted shipped
scientific adapters. A virtual environment is not a security sandbox.

**Current runtime integration:** application service launches now require the
compiled, offline `python-numpy-v4` outer inventory and its pinned inner manifest.
See [runtime v2 validation](validation/runtime-v2.md) for the source standard
library, exact inventory checks, atomic copy and current acceptance evidence.
The v1 provisioning commands and results below are retained historical evidence;
the production service no longer invokes their network provisioner or accepts a
legacy self-described inner manifest in place of the compiled v2 outer.

## Chosen boundary and local prerequisites

The implementation uses a Windows less-privileged AppContainer (LPAC)
with the single non-network `registryRead` capability needed for Python startup.
It explicitly opts out of the broad All Application
Packages policy and verifies both the AppContainer and LPAC flags on the actual
process token before letting Python execute. Windows supplies filesystem,
registry, process and network access restrictions. A Windows Job Object adds
aggregate and per-process memory limits, a child-process count limit, a wall-clock
deadline, cancellation and termination of remaining children when the root exits.
The process starts suspended, is assigned to its Job Object, and only then resumes.

The caller must provide a dedicated, provisioned embedded Python runtime and a
separate experiment working directory. Only the fresh package SID receives read
and execute access to that runtime and read/write/execute access to the work
directory. No network capabilities are granted, no host handles are inherited,
and the environment is built from an explicit allowlist. Temporary/home variables
point into the assigned working directory. Windows may also give the AppContainer
its own private profile storage and access to required operating-system resources;
this is an OS policy boundary, not a claim that every visible path is a virtual
filesystem containing exactly two directories.

Directory grants and profile creation are scoped application operations. They do
not enable an OS feature, change firewall/loopback exemptions, modify machine
security policy, or require a reboot. Grants are removed and the temporary profile
is deleted when the process supervisor is dropped. A host crash can leave a stale
profile and ACL entry; reuse of that profile identity fails closed until explicit
cleanup. Runtime/work trees containing directory links are rejected before grants.

The local machine reports Windows build 26200, x64, Pro. Docker and Windows Sandbox
were absent. `wsl --status` reported WSL2 with Ubuntu 24.04; it is a possible
alternative, not used by this implementation. No Windows feature was enabled.

## Pinned independent runtime

Initial development probe root: `.local/science/isolation`. The reproducibly
provisioned and fully hashed runtime is `.local/science/isolation-v1/runtime`.

| Dependency | Origin | SHA-256 |
| --- | --- | --- |
| CPython 3.13.15 embedded x64 | `https://www.python.org/ftp/python/3.13.15/python-3.13.15-embed-amd64.zip` | `d1f04d990aee1253d8569e8e5104e30fa9f5fa830899f14843448872d936a2cf` |
| NumPy 2.4.6, CPython 3.13 Windows x64 wheel | PyPI, `numpy-2.4.6-cp313-cp313-win_amd64.whl` | `c4fc99836233ea196540b17ab0983aff60ed07941751930f5f4d05bc3b3b7359` |

The downloaded Python executable has a valid Authenticode signature from Python
Software Foundation. NumPy was obtained with binary-only, no-dependency pip download
against `https://pypi.org/simple`. Licenses remain in the unmodified distributions.
The dedicated runtime does not reference the user's Anaconda installation.
`tools/isolation_probe.py --provision <fresh-absolute-runtime-directory>` provisions
this exact runtime as a trusted host operation. It verifies both download/cache
hashes, safely extracts into a staging directory, retains licenses, writes 967
content hashes, and atomically renames the completed directory. It never executes
generated installation code and refuses to overwrite an existing runtime.

Every isolated launch verifies the version/source receipts and all runtime file
hashes, rejecting missing, changed, or extra files. Package modifications require
an explicitly supported new runtime version; generated code cannot install into
this runtime. Provisioning uses the host network, while experiment execution has
no network capability.

## Frozen acceptance before restricted execution

`tools/isolation_probe.py` is a fixed benign fixture. It receives only synthetic
canary paths and a temporary listening port. It must:

1. Observe AppContainer and LPAC token flags equal to one.
2. Import the pinned NumPy build, solve a nontrivial 3-by-3 linear system, save and
   reread its numerical arrays, and produce a maximum residual below 1e-12.
3. Fail with access denial when reading a synthetic secret outside its work root,
   creating a file outside that root, creating a hardlink to the outside canary,
   or writing to the runtime directory.
4. Fail with access denial when connecting to a known reachable host loopback
   listener and a reserved external documentation IP. Timeouts or connection
   refusals do not count as policy denial.
5. Observe no provider credential or Python injection environment variables.
6. Return exit zero only if all the above checks pass. The host independently
   verifies the output hash, numerical tolerance, unchanged read canary and
   absence of both forbidden output files.

The report and calculation arrays are retained in a uniquely named `probe-*`
directory. The synthetic canary remains beside it for inspection. Real credential
files are never used as test targets.

```powershell
$env:PHASEFORGE_ISOLATION_TEST_ROOT = (Resolve-Path .local/science/isolation-v1).Path
cargo test --locked --manifest-path backend/Cargo.toml --lib laboratory::isolation::windows::tests -- --nocapture
```

Absence of `PHASEFORGE_ISOLATION_TEST_ROOT` explicitly skips the opt-in case; a skip
is not acceptance. A missing module or zero discovered tests is not acceptance.

## Verification status

The initial actual restricted-process canary test passed on 2026-09-11 in 0.57 s.
Its report is retained at
`.local/science/isolation/probe-8b0567f9-7e31-4ca1-92ca-cf7fbf8e01df/isolation-report.json`.
It observed AppContainer=1 and LPAC=1, NumPy 2.4.6, a linear residual of
4.440892098500626e-16, and retained array SHA-256
`ed1d9dacd3bfc767ff6a6b08e13b8b571d5666682db927072862c18a4a5c9fef`.
Outside read/write and runtime-write checks returned permission error 13; both
network attempts returned Windows access-denied error 10013. No provider or
Python-path environment variable was present.

Failed attempts were retained. The first failed before execution because this
Windows installation rejects the SDK's token-information class 46 with error 87.
The implementation now queries the underlying `WIN://NOALLAPPPKG` security
attribute when that convenience query is unavailable; both the parent and fixture
verify it. Subsequent attempts with zero capabilities reached a confirmed LPAC
token but Windows refused Python startup with status `0xc0000022`, including from
a fresh copy under Temp. The runtime needs `registryRead`; adding that capability
enabled startup without granting network access or outside-file permissions.
The numerical and access-denial criteria were preserved.

The expanded suite passed on 2026-09-11: **4 passed, 0 failed, 0 ignored, no skips**,
in 9.40 s. It used the fresh reproducibly provisioned runtime and verified every
runtime file on each launch. Final numerical/access report:
`.local/science/isolation-v1/probe-ecd343ee-8608-4689-aafb-7d8fedaf956a/isolation-report.json`.

| Actual check | Observed outcome |
| --- | --- |
| Numerical calculation and storage | NumPy solved the system with residual below 1e-12; saved arrays were reread and their hash matched the host check |
| Filesystem negatives | Outside read, outside write, outside-canary hardlink, and runtime write all received permission denial |
| Network negatives | Reachable host loopback and external documentation IP both returned WSAEACCES 10013 |
| Cancellation and lifetime | Both live parent/child OS handles signaled exit after token cancellation, deadline expiry, supervisor drop, and normal root exit |
| Per-process memory cap | 16 MiB allocation succeeded; 512 MiB allocation raised `MemoryError` under a 128 MiB cap |
| Aggregate job memory cap | Parent/child allocations of 112 MiB each hit a 192 MiB combined cap although each was below its individual cap |
| Child-count limit | Limit 1 denied child creation; limit 2 allowed the same child fixture to execute normally |

This proves the bounded Windows execution entry point with benign negative
fixtures. It is not an independent penetration test or an installed-chat admission
test. No arbitrary user/agent code path was exposed during these checks.

## Reusable entry point and integration responsibilities

The public API is `laboratory::isolation::{IsolationSpec, IsolatedProcess}`.
`IsolationSpec` takes a fresh UUID, an absolute dedicated runtime directory,
an absolute experiment directory, a script within that directory, argument strings,
memory limit (128–8192 MiB), process count (1–16), and optional time limit
(`Some` of 1 second–24 hours, or `None` for Off).
`IsolatedProcess::spawn(spec)` validates inputs and creates the native boundary;
`process.wait(&cancellation_token).await` returns the Windows exit code and execution
metadata or a cancellation/deadline error. Dropping it terminates its process tree.
`stop_and_reap()` terminates the job and confirms zero active processes before
generated files may be inspected; failure leaves those files unread.
No shell is used. Bootstrap redirects stdout/stderr to UTF-8 files in the work
directory and passes arguments to the script. There is no unrestricted fallback
on another OS or when isolation setup fails.

Call provisioning and the synchronous spawn/setup work outside a latency-sensitive
UI or async request thread. The application should persist the generated source,
input, source hash, budget, job identity, isolation outcome and artifacts. The
boundary verifies execution policy; an exit-zero script or model-generated report
does not establish numerical or predictive validity. The parent runtime still owns
the public tool/API admission, installed-environment management, experiment result
schema, and evidence validation.

This implementation does not supply a hard disk quota, a virtual machine, or a
fresh Windows account. It relies on the Windows kernel and AppContainer policy,
including system-defined LPAC-readable resources and registry-read grants. Add a
separate output-storage budget and crash cleanup to production job management;
neither is established by the canary calculation alone.

## Durable generated-code service

`laboratory::generated` supplies `LaboratoryService::start_generated(id)` for a
previously created `kind: "generated"` job. It does not expose an agent tool or
add an engine to the public catalog. The durable input contract is:

```json
{
  "engine": "python_numpy",
  "code": "import json\nopen('result.json', 'w').write(json.dumps({'value': 2 + 2}))\n",
  "inputs": {},
  "sources": [],
  "limits": {
    "memory_mb": 256,
    "process_limit": 2,
    "wall_seconds": null,
    "storage_mb": 64
  }
}
```

`wall_seconds` is mandatory and explicitly null means Off. The effective deadline
is the earliest explicit experiment, job, or parent deadline. Off introduces no
hidden deadline. Queueing and provisioning consume the explicit budget. Child
jobs inherit a live parent's cancellation token. The managed runtime is provisioned
automatically at `data_directory/environments/python-numpy-v1/runtime` using the
trusted shipped provisioner under `clean_command`. Generated code never runs the
installer. The first runtime supports pinned NumPy and Python's standard library;
arbitrary dependency installation is unavailable.

The service preserves original `source.py`, `source-input.json`, imported source
snapshots, their SHA-256 receipts, and the complete job input outside the LPAC
working-directory grant. Disposable copies are exposed as `work/experiment.py`,
`work/input.json`, and `work/imports/`. An import specifies `job_id`, `path`, and
`destination`; the source must be an inactive job in the same project. Imports
are limited to 32 files, 64 MiB each and 128 MiB combined. Directory links, reparse
points, hardlinks, traversal, device names and alternate data streams are refused.

The script writes a finite JSON object to `work/result.json`. Optional `artifacts`
must contain existing relative file paths. Stdout, stderr, files and hashes are
retained. Host ingestion never unpickles generated files. Exit zero means execution
completed; numerical and scientific validity still require independent checks.

`restart_generated(old_id, deadline)` creates a new job UUID and directory and
reruns the preserved code and input snapshots. It does not overwrite the original
attempt or silently interpret a generated pickle/checkpoint. A still-stopping
attempt cannot restart. Imported snapshots are hash checked and reused even if
the original source job's file subsequently changes.

Resume admission is idempotent for each original attempt. Concurrent requests and
later retries return the same saved replacement ID and preserve its first budget;
continuing that replacement again uses its own ID. Durable lineage also reconciles
a replacement created just before a restart-receipt interruption. A brief admission
reservation is released before the new worker can inspect its original snapshots.
If the old parent has ended, explicit continuation becomes an independent attempt
while preserving ancestry. An eight-concurrent-call test verified one replacement
and receipt, finished-parent detachment, released reservation and budget preservation.
The concurrent test passed in 0.14 s. The actual LPAC service regression then
passed in 22.63 s, retaining evidence at
`.local/science/isolation-v1/service-2e976263-c18c-4c47-b4e1-0b525990d40f`.

Imports can include an optional `sha256` pin from a retained-data receipt. The
service refuses changed source bytes before writing their snapshots. `kind: data`
jobs use the same same-project, inactive-job and bounded plain-file requirements.

Storage is a **soft working-directory limit**, sampled every 250 ms. The service
checks free volume space, reserves a private 64 MiB emergency file, and requires a
256 MiB free-space floor. It stops the entire isolated process tree when observed
output exceeds the selected 64–4096 MiB budget or space drops below that floor.
Receipts state observed peak usage and overshoot. Up to 10,000 filesystem entries
and depth 16 are accepted. Fast writes can exceed the limit between samples;
OS-created AppContainer profile storage is outside this monitor. These measures
are not a filesystem quota. Host-crash profile/ACL cleanup remains future work.

The generated-service acceptance command is:

```powershell
$env:PHASEFORGE_ISOLATION_TEST_ROOT = (Resolve-Path .local/science/isolation-v1).Path
cargo test --locked --manifest-path backend/Cargo.toml --lib laboratory::generated::tests -- --nocapture
```

The actual-process test requires the already provisioned pinned fixture runtime;
absence prints an explicit skip and is not acceptance. It exercises the service
with a fresh runtime installation from cached pinned archives, a separate database
and actual LPAC processes. Archive downloads and source hashes were checked separately.

After the optional-deadline and full-tree-reap changes, the four native isolation
tests passed again with no failures or skips in 10.46 s. The new canary report is
`.local/science/isolation-v1/probe-9cea2ac9-2a4b-470a-bca3-d1cc78aa6647/isolation-report.json`.
The generated-service suite passed with **4 tests, 0 failures, no skips in 22.07 s**.
It began with no installed runtime and invoked the production trusted provisioner
against cached pinned archives, then ran actual LPAC computations. Evidence is
retained at `.local/science/isolation-v1/service-7b824b52-061d-4132-9e93-19cd79f316e8`.
Its numerical job `9a7a62b5-20b9-4d5a-8590-4674284f8718` solved the linear system with
residual `4.440892098500626e-16`; its working-copy source modification left the
original retained source intact. Explicit cancellation, an immutable restart with
a two-second deadline, original imported snapshot reuse and NaN rejection passed.
The 64 MiB storage test stopped at 73,400,466 observed bytes, recording 6,291,602
bytes of overshoot. The initial failed provisioning evidence is retained under
`service-ad159128-0ddd-434b-bf21-a4fa6f319c34`; native Windows long-path handling fixed
the packaged NumPy filename failure without an OS policy change.

The API can use `read_generated_artifact(id, relative)` for bounded exports.
Working outputs are readable only after the attempt and its process tree stop,
must appear in the retained inventory, and are checked against their recorded
size and hash when a hash is available. Source and host-written receipts use an
explicit allowlist. Reads reject links and are capped at 64 MiB; larger artifacts
need a separately bounded range/stream API. The generic artifact route must call
this helper for generated jobs before any public generated-code tool is enabled.
Its regression passed with 4 tests, no failures or skips in 20.58 s, including
actual registered NPZ export and denial of an active working-output read. Evidence:
`.local/science/isolation-v1/service-1cd75966-dade-4025-8f4a-9ecb8f99a25c`.

## Primary references

- [Microsoft: launching an AppContainer or LPAC](https://learn.microsoft.com/en-us/windows/win32/secauthz/implementing-an-appcontainer)
- [Microsoft: AppContainer isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation)
- [Microsoft: scoped ACL updates and inheritance](https://learn.microsoft.com/en-us/windows/win32/api/aclapi/nf-aclapi-setnamedsecurityinfow)
- [Microsoft: sandboxing Python with Windows app isolation](https://blogs.windows.com/windowsdeveloper/2024/03/06/sandboxing-python-with-win32-app-isolation/)
- [Python: embedded Windows distributions](https://docs.python.org/3/using/windows.html#the-embeddable-package)
- [Python: current Windows release downloads](https://www.python.org/downloads/windows/)
- [Chromium: LPAC filesystem and registry prerequisites](https://chromium.googlesource.com/chromium/src/+/HEAD/docs/design/sandbox.md)
- [Google Project Zero: Windows token inspection implementation](https://github.com/googleprojectzero/sandbox-attacksurface-analysis-tools/blob/main/NtCoreLib/NtToken.cs)
