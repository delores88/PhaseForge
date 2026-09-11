# Windows release coordination, 2026-09-11

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
