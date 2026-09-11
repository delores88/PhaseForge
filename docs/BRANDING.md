# PhaseForge artwork

Production artwork comes from the project owner's supplied
`PhaseForge-Brand-System-v1.1-Static-Assets.zip` (13,219,417 bytes), verified against
SHA-256 `f251de1f29c29ad49f5fd80fafb014fa25ff10caacd1213354ec1d9e115c6e43`.
All 87 original archive checksum entries were verified before integration.
This replaces v1.0 with corrected transparent letter openings and app-tile corners.
The packaged [provenance receipt](../frontend/public/brand/provenance.json) records
the source filename and SHA-256 of each selected asset. Original SVG and PNG copies
are byte-identical; the source correspondence was checked again when the receipt
was produced. No email content, account identifiers or source archive is bundled.

The sidebar uses original dark/light color horizontal wordmarks, with the original
shaded blue symbol at compact widths. Flat, all-black and all-white branding are
prohibited throughout the app, native shell and marketplace media. Browser and
native icons use the shaded app tile, including at small sizes. The About view
uses original 512-pixel app tiles. The application icon, installer/uninstaller, window, taskbar identity,
shortcut, tray and browser favicon refer to the supplied artwork. Native shell
appearance must be checked on the exact installed candidate; source paths alone
do not prove Windows cache or shortcut behavior.

The source package contains no ICO. [The icon conversion script](../scripts/brand-icons.py)
assembles PNG-backed ICO entries using Lanczos reductions of the original
512-pixel shaded app tile at every size. The browser SVG is the original shaded
dark app tile. This follows the owner's shaded/gradient-only preference instead
of the package's small-size flat-favicon recommendation. The conversion does not
redraw, recolor or retype the logo.
Pillow 10.4.0 was used for the recorded conversion.

The existing mint/charcoal and light application themes are retained. The brand
package's theme tokens and illustrative product claims are not imported. At full
sidebar widths the horizontal logo is at least 160 pixels wide; compact symbols
are 48 pixels wide with automatic height. Preserve the artwork's aspect ratio, traced shape and clear space
when changing layouts.
The package's prose specifies 48px high, while its implementation example uses
48 by 39 pixels. The compact sidebar follows that example at 48px wide without
distorting the artwork or cropping its viewBox.

Asset URLs include the artwork revision so upgrades bypass old cached artwork.
Unversioned public assets are served without persistent caching; content-addressed
Next.js assets retain caching.

The responsive symbol switch and sidebar collapse share the 950-pixel breakpoint.
An expanded sidebar between 951 and 1050 pixels must retain the full wordmark.

Dark mode is the first-launch default, independent of the operating system's
theme. A researcher can explicitly choose light mode and retain that preference.
Marketplace screenshots use dark mode exclusively; light-mode images are internal
compatibility checks and are not marketplace media.
