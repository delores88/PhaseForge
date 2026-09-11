# Delores marketplace preparation

`marketplace/manifest.template.json` is a preparation template, not a release
manifest. Its null identifiers and empty artifact list are intentional. Do not
upload it as `deloresai.manifest.json`. Existing marketplace releases do not
establish admission for a new installer or source commit.

The Delores marketplace task supplied the contract from
`DeloresAI/packages/contracts/src/index.ts` on 2026-09-10. For each authorized
submission, resolve the PhaseForge app and publisher IDs,
GitHub repository/release IDs, exact source commit, tested system requirements,
and actual release asset IDs. Bind exact filenames to assets in the same release.
Use genuine screenshots from the installed app and its actual icon.

The current 0.9 release targets Windows x64 only:

| Platform | Asset | Adapter |
| --- | --- | --- |
| Windows x64 | `PhaseForge_VERSION_x64-setup.exe` | `windows-nsis` |

Each asset declares exactly one architecture. Build configuration
does not establish native install/runtime validation. Populate only targets that
have passed that validation; keep other targets out of release declarations.
The marketplace requires all intended artifacts on one stable draft release
before publishing/submission. No release is created by any PhaseForge workflow.

Required release steps include independent marketplace scanning, exact final
source/runtime profiles, authentic artwork and a recorded data backup. Prepare
the complete stable draft and obtain the marketplace owner's review before
publishing, because publication can trigger ingestion and scans. Preserve the
existing accepted download until the new artifact passes ordinary admission.

Use the PhaseForge logo in discovery. Put exactly two genuine application
screenshots on the download/detail page, with captions distinguishing numerical
playback from scientific illustration. Expose the accepted Windows download
directly and remove obsolete alpha wording and unsupported platform choices.
