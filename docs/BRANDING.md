# PhaseForge artwork

Production artwork comes from the project owner's supplied
`PhaseForge-Brand-System-v1.0-Static-Assets.zip` (13,209,294 bytes), verified against
SHA-256 `5dea0fc7072eae507ce77f6f18ac10632935cd7f3519354c1afed5ff9fa8b29a`.
The original archive's staged checksum entries were verified before integration.
The packaged [provenance receipt](../frontend/public/brand/provenance.json) records
the source filename and SHA-256 of each selected asset. Original SVG and PNG copies
are byte-identical; the source correspondence was checked again when the receipt
was produced. No email content, account identifiers or source archive is bundled.

The sidebar uses original dark/light horizontal wordmarks, with the original
monochrome symbol at compact widths. The About view uses original 512-pixel app
tiles. The application icon, installer/uninstaller, window, taskbar identity,
shortcut, tray and browser favicon refer to the supplied artwork. Native shell
appearance must be checked on the exact installed candidate; source paths alone
do not prove Windows cache or shortcut behavior.

The source package contains no ICO. [The icon conversion script](../scripts/brand-icons.py)
assembles PNG-backed ICO entries using the supplied optimized 16/32/64-pixel
favicons and Lanczos reductions of the original 512-pixel app tile for larger
Windows entries. It does not redraw, recolor, retype or otherwise alter the logo.
Pillow 10.4.0 was used for the recorded conversion.

The existing mint/charcoal and light application themes are retained. The brand
package's theme tokens and illustrative product claims are not imported. At full
sidebar widths the horizontal logo is at least 160 pixels wide; compact symbols
are 28 pixels. Preserve the artwork's aspect ratio, traced shape and clear space
when changing layouts.

The responsive symbol switch and sidebar collapse share the 950-pixel breakpoint.
An expanded sidebar between 951 and 1050 pixels must retain the full wordmark.
