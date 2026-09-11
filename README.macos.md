# PhaseForge on Apple Silicon

The macOS alpha target is **ARM64 / Apple Silicon only**. Intel and universal
packages are not included. Native qualification is recorded in each candidate's
build manifest and acceptance evidence; a successful Windows or Linux test does
not qualify macOS.

The package is a DMG containing `PhaseForge.app`. The native build uses an ad-hoc
code signature for executable integrity. It has no Apple Developer ID certificate
or notarization. A DeloresAI integrity/security review is separate from Apple's
Gatekeeper decision; a hosted direct launch does not establish that a downloaded,
quarantined application opens through Finder without user approval.

Use the verified marketplace download, open the disk image and copy PhaseForge to
your Applications folder. The app's standard menu includes Quit PhaseForge and
editing shortcuts. Closing its window can leave research running in the menu bar;
Quit stops the app and its local research engine.

Your research stays in local application storage and provider keys use the macOS
credential store. AI work uses your OpenAI or Anthropic API key and incurs that
provider's charges. Blender, CadQuery and KiCad are optional separate installations;
their presence and capabilities are checked before a job uses them.

Back up research before an upgrade. Preserve the matching data backup if you may
need to return to an earlier version. The native acceptance suite checks a fresh
install, an offline numerical experiment, normal quit/reopen, saved-data restore
and removal in an isolated hosted environment. It records actual OS, architecture,
memory and signature observations without claiming universal compatibility or
minimum-memory certification.
