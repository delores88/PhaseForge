# Windows release coordination, 2026-09-11

## Current status, September 12 UTC

Version 0.9.1, source `27f5c91ac6482df16c34012c7f56501a1f494f32`,
was installed with 88 saved jobs preserved and accepted through ordinary
marketplace validation at **03:58:56.667 UTC**. The exact Windows installer
SHA-256 is `a631ba9f7ce0c211376a31b1bdf33edae715069ea590db45d6e7d07a89e268c1`.
Submission `lvzzXIwbE8kvXsOK9LEVfdAI`, artifact `ypf467Kft7sInAt5lvpTTbU3`,
and validation job `dPyhbsAQBEp2oHCk313sQ5cQ` identify that accepted release.
Its local receipt is `.local/stable-release-state.json`; these identities do
not certify the subsequent 0.10.0 source changes.

The 0.10.0 candidate, local installation and marketplace admission are pending.
Discovery artwork is being changed to the exact original transparent PF symbol,
SHA-256 `9e80652ae7851e9f1baf79a45cd4a43666404182b267b9bd76c7a3cbc66da9ec`.
The two genuine app screenshots remain detail/download-page media. The prepared
marketplace artwork patch has not yet been deployed. Release remains Windows
x64 only; future-platform design work does not add release targets.

## Earlier September 12 repair and admission preparation

Version 0.9.0 at commit `3cd794e4e869d319b958a91f3ccb636e404b006f` was
installed locally with saved data preserved and published as an immutable GitHub
release. Ordinary marketplace validation completed at 02:10:21 UTC and rejected
the bundled OpenSSL 3.0.21 DLLs: CVE-2026-75803, CVE-2026-54874,
CVE-2026-63072 and CVE-2026-63076. Malware, SBOM and secrets stages passed.
The complete report was retained with SHA-256
`f2c9ed3a471c85e0bc7ab9deef4458c6963ffa97cd3d28a956d36602aad41587`.
The private full-report upload now succeeds; this is a dependency rejection,
not the earlier report-transport failures.

Version 0.9.1 prepares new immutable runtimes with the official CPython OpenSSL
3.0.22 Windows DLL pair. It requires its own final hosted build, installed
compatibility checks and exact marketplace profile/admission. The rejected
0.9.0 assets and report remain unchanged. No exemption or automatic retry
establishes acceptance. Logo discovery art and the two detail-page screenshots
are already hosted; the historical discovery defect described below is fixed.

## Earlier coordination record

Delores owner reconfirmed the live state at 16:20:29 UTC by comparing deployed
source hashes. The clean deployed worktree is
`<DeloresAI repository-root>/.local/phaseforge-alpha`, branch
`feature/phaseforge-alpha-validation`, commit
`d606d5cf64ba775e2d5702c0d2feb4a9e6157ecd`. Main is older.
Deployment `d3aaf16c-ffbf-433e-a422-82f7fc3709c6` remains active.

Current accepted Windows download `SE-JkTVyViJeOL0nFyMUEgDR` is the old
0.8.0-alpha.1 beta-channel release. Preserve its evidence, files and availability
while preparing the replacement. Its acceptance expires September 18 at
03:48:24.666 UTC; it does not cover new bytes.

After scientific and final local installation acceptance, prepare a new clean
commit, non-prerelease version, exact NSIS x64 artifact and workflow/materials,
and current manifest with `release.channel: stable`. The GitHub release must have
`prerelease:false`; this is not an artifact field. Keep it draft until the Delores
owner reviews the new Windows runtime profile and complete bindings. Publishing
triggers automatic ingestion and a paid scan. Use the normal unchanged checks and
deduplication, then verify acceptance and final served-file hash. No Mac/Linux
artifact is required. Do not reuse old signatures or runtime-profile evidence.

The Delores owner will handle marketplace hosting/mapping, presentation and
admission after receiving final materials. PhaseForge must supply established-logo
icon and landscape discovery art plus exactly two genuine installed-app
screenshots/captions, with manifest identities/hashes. Current discovery explicitly
uses `molecular-workbench.jpg`. Both live discovery image elements were checked
through the browser and use that screenshot. The primary fix is the manifest's
logo thumbnail; if needed, Delores will make the PhaseForge spotlight fallback
use its icon/name rather than a detail screenshot, preserving other apps.

Stable acceptance defaults to the stable download but does not remove the old
optional Alpha chooser: channel-specific eligible artifacts coexist. The owner
will make a narrow PhaseForge presentation change after stable Windows qualifies
to omit the obsolete chooser/alpha marketing, preserving old acceptance records
and policy. Do not relabel or revoke the old release to hide the interface.

No deployment, publication, ingestion, download or scan was performed for this
handoff. Relevant thread: `01a086d7-002c-7642-a8fa-9a77edb823dc`.
