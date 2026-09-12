# Windows 0.10 delivery handoff

Prepared September 12 for the final dependable-workbench changes. This is a
delivery plan, not an installation or admission receipt. Installed and accepted
public version remains **0.9.1**. The failed `112bb0b` candidate does not cover
the current source. Exact 0.999c collision success is not a release gate.

## Freeze, build and authenticate the exact candidate

Complete the current ordinary-user acceptance, review the intended changes,
commit them and push `scientific-workbench-0.9`. Do not modify source while its
candidate is being assembled. The push normally starts `ci.yml`; use its exact
source run, and do not start duplicate CI. The candidate workflow only builds
Windows x64 and does not publish. It already includes the actual NSIS install,
native numerical jobs, lifecycle checks, payload inventories and provenance.

```powershell
$pfCommit = (git rev-parse HEAD).Trim()
gh workflow run marketplace-candidates.yml --repo delores88/PhaseForge --ref scientific-workbench-0.9 -f platform=windows
gh run list --repo delores88/PhaseForge --workflow marketplace-candidates.yml --commit $pfCommit --limit 5 --json databaseId,headSha,status,conclusion,url
gh run list --repo delores88/PhaseForge --workflow ci.yml --commit $pfCommit --limit 5 --json databaseId,headSha,status,conclusion,url
```

Set `$pfRun` only from the matching newly dispatched run. Require its `headSha`
to equal the full `$pfCommit` and require both workflows to pass. A failure means
preserving its evidence and fixing the actual cause, not reusing an earlier
candidate. Create a fresh absolute `$pfRelease` evidence directory, then:

```powershell
$pfCandidate = Join-Path $pfRelease 'candidate'
gh run download $pfRun --repo delores88/PhaseForge --name "PhaseForge-windows-x64-$pfCommit" --dir $pfCandidate
python -I -B .local/verify_candidate_origin_010.py --candidate $pfCandidate --output (Join-Path $pfRelease 'origin-verification') --source-commit $pfCommit --run-id $pfRun
```

Check each exit code before the dependent step. The new `010` origin helper is
the existing strict 0.9.1 verifier with only the exact version/prefix changed;
fresh GitHub trust roots and exact source/workflow verification remain required.
The old `verify_candidate_origin_091.py` and `audit_091_hosted_handoff.py` contain
0.9.1 identities and must not be run as if they certify 0.10. Candidate metadata,
original checks archive, installed-resource comparison and independent origin
receipt are the handoff materials; do not rewrite the candidate files.

## Preserve and install locally

Before closing the installed app, retain a fresh
`.local/stable-0.10.0-pre-stop.json` with every laboratory job's `id`, `state`,
`kind`, and `project_id`, plus project identities and the active-job check. Use
the installed app's actual API, not the isolated test profile. Stop owned test
hosts and the installed PhaseForge app after verifying no active jobs/recovery;
the existing backup helpers deliberately refuse any PhaseForge/backend process.

Alternatively, when the installed app is already closed, use a private,
byte-verified DB/WAL/SHM copy and query only that copy. This baseline was captured
at **2026-09-12 19:07:00 UTC** by
`.local/capture_010_offline_baseline.py`: 88 laboratory jobs, 18 project IDs,
no stored active jobs, and SQLite `quick_check` passed. The installed main and
backend executable paths were absent before and after capture; isolated source
backend PID 45496 was left running. Original database bytes were rehashed and
unchanged. Receipt `.local/stable-0.10.0-pre-stop.json` is ready for the existing
preservation helper; evidence directory
`.local/stable-0.10.0-offline-baseline-20260912T190700Z` grants access only to the
current Windows user and SYSTEM. No provider configuration was queried/exported.
Do not relaunch the old app merely to obtain another baseline, and do not
overwrite this receipt. If installed data changes before backup, reassess it
explicitly rather than treating the old receipt as current.

```powershell
python -I -B .local/backup_stable_010_closed_app.py
python -I -B .local/preflight_stable_010.py --source-commit $pfCommit --candidate-run-id $pfRun
$pfInstaller = Join-Path $pfCandidate 'PhaseForge_0.10.0_x64-setup.exe'
$pfInstall = Start-Process -FilePath $pfInstaller -ArgumentList '/S','/ACCEPT_MSVC_TERMS=MSVC-2026-09-11' -WindowStyle Hidden -Wait -PassThru
if ($pfInstall.ExitCode -ne 0) { throw 'Installer failed; retain the installer exit receipt' }
$pfResources = Join-Path $env:LOCALAPPDATA 'Programs\PhaseForge\resources'
python -I -B .local/verify_hosted_candidate_install_091.py --candidate $pfCandidate --source-commit $pfCommit --resources $pfResources --output (Join-Path $pfRelease 'local-installed-verification.json')
```

The installed-resource verifier derives the version from the candidate and is
usable for 0.10 despite its filename. Capture the installer SHA/size, timestamps,
arguments, exit code and exact candidate/source in a new receipt. Compare every
saved job/project identity after launch, then verify the actual installed
workbench, relevant saved results, navigation, cameras and capability/error
states. Source UI checks do not replace this installed-build check.

## This machine's optional NR runtime

The installer does not bundle AthenaK, WSL, CUDA or the machine-specific runtime
registrations. Do not advertise those as available on every fresh Windows
installation. The app has an optional gauge adapter and optional puncture
adapters; current hardware admission and scientific validity remain separate.

The bounded helper `.local/prepare_nr_registration_010.py` only reads pinned
existing runtime files and stages exact registration bytes in a new local
evidence directory. It never writes installed data, mutates WSL files, starts a
solver or reprovisions anything. Run it again against the final installed tools:

```powershell
$pfNRStage = Join-Path $pfRelease 'nr-registration-ready'
python -I -B .local/prepare_nr_registration_010.py --engine athenak_two_punctures_cuda --tools-root (Join-Path $pfResources 'tools') --output $pfNRStage
```

Only after that report passes, while the app remains closed and its verified
backup exists, copy its one staged `athenak_two_punctures_cuda/runtime.json` to
`%LOCALAPPDATA%\PhaseForge\PhaseForge\data\environments\numerical-relativity\athenak_two_punctures_cuda\runtime.json`.
Resolve the destination within that exact installed data root, reject linked or
reparse ancestors, and refuse an existing different registration. If an existing
file is byte-identical, leave it alone. Record both source/destination SHA-256
and this narrow added file separately from the preserved research-data inventory.
After launch inspect actual `/api/laboratory/capabilities` and the resource plan;
registration pin checks do not establish current execution admission.

Preparation evidence: CUDA-only validation passed in
`.local/nr-registration-010-preparation-02/report.json`, with seven pinned runtime
files and CUDA aliases checked; no solver was executed. Registration SHA-256 is
`f9bfe4218845dbfef7096c00038e6031c832da2ebcabaf8bbeae62f6b06aea0d`.

**Do not transfer the old gauge registration.** Its unchanged original fixture
has obsolete staging/supervisor bytes and is unavailable under the current app
contract. The first failed preparation receipt remains in
`.local/nr-registration-010-preparation-01/report.json`. The old stage hash begins
`4e44cc87` versus current `20aa58c4`, and supervisor `4aa2650e` versus current
`d3a27f0a`. Preserve that honest limitation; no new compatibility runtime or
collision research is necessary for this release.

## Marketplace handoff and ordinary acceptance

After local installation acceptance, create the new stable `v0.10.0` draft with
`--target $pfCommit`, `--prerelease=false`, and `RELEASE_NOTES_0.10.0.md` as its
notes file. Upload the untouched candidate files. Prepare a fresh manifest using
the existing accepted application/repository/publisher identities, new release
ID/tag/version/source and actual installer asset ID. Its description and
functional-test statement must describe the final verified evidence, including
optional separately registered NR transport; old native receipts cannot be
carried forward as tests of the new installer.

Give the Delores owner the exact draft asset identities, source, original checks,
fresh origin receipt, installed comparison/behavior receipts and manifest for
the normal Windows x64 runtime profile and validity checks. Keep the draft
unpublished until the owner confirms the exact new profile/registry is ready.
Publish once; use its normal webhook ingestion and ordinary admission, without
duplicate submissions or overlapping scans. Record the resulting submission,
artifact, validation job, report hash, signed acceptance and served-file hash.

Discovery `iconUrl` and `thumbnailUrl` must use the transparent existing PF
symbol, SHA-256 `9e80652ae7851e9f1baf79a45cd4a43666404182b267b9bd76c7a3cbc66da9ec`,
at the owner's deployed URL (prepared URL:
`https://deloresai.com/media/phaseforge/pf-symbol-9e80652ae785.png`). Exactly two
existing desktop screenshots stay on the detail/download page: CAR-T
`car-t-workbench-25846c576214.jpg` and `dna-workbench.jpg`, with their conceptual
illustration captions. The prepared artwork is not yet deployed.

The owner's latest account requirement applies to every website/desktop
installer route, including `/artifacts/:id/file` GET, HEAD and ranges:
anonymous requests must return **401**, and the positive served-file check uses
an authorized verified account. Coordinate that check with the owner; do not
reuse the old anonymous-download expectation. Scanner service-ticket access is
unchanged. Confirm no obsolete alpha marketing/chooser, one Windows x64 stable
download, the transparent discovery symbol and two detail-only screenshots.

Only then mark B9 complete and update the 0.10 release receipt. Preserve
`.local/stable-release-state.json` as the completed 0.9.1 record.
