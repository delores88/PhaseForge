# Delores marketplace preparation

PhaseForge is not submitted or published. `marketplace/manifest.template.json`
is a preparation template, not a release manifest. Its null identifiers and empty
artifact list are intentional. Do not upload it as `deloresai.manifest.json`.

The Delores marketplace task supplied the contract from
`DeloresAI/packages/contracts/src/index.ts` on 2026-09-10. Before a later,
explicitly authorized submission, resolve the PhaseForge app and publisher IDs,
GitHub repository/release IDs, exact source commit, tested system requirements,
and actual release asset IDs. Bind exact filenames to assets in the same release.
Use genuine screenshots from the installed app and its actual icon.

Planned package names match the desktop builder:

| Platform | Asset | Adapter |
| --- | --- | --- |
| Windows x64 | `PhaseForge_VERSION_x64-setup.exe` | `windows-nsis` |
| Windows ARM64 | `PhaseForge_VERSION_arm64-setup.exe` | `windows-nsis` |
| Linux x64 | `PhaseForge_VERSION_x64.AppImage` | `linux-appimage` |
| Linux ARM64 | `PhaseForge_VERSION_arm64.AppImage` | `linux-appimage` |

Each Windows/Linux asset declares exactly one architecture. Build configuration
does not establish native install/runtime validation. Populate only targets that
have passed that validation; keep other targets out of release declarations.
The marketplace requires all intended artifacts on one stable draft release
before publishing/submission. No release is created by any PhaseForge workflow.

Future requirements: independent marketplace scanning, authentic artwork,
repository access/onboarding, and a recorded rollback/data-backup procedure.
No marketplace validation or upload was performed during this workbench overhaul.
