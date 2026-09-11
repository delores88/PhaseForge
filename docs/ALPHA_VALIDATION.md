# 0.8.0-alpha.1 engineering validation

Development checks were recorded on 2026-09-10. Package and local-upgrade
evidence was updated on 2026-09-11 for application source
`7471ff72ac2cba9a89e96e88235cda810ebf1c1d`. This supplements the historical
[0.8.0 baseline](RELEASE_VALIDATION.md); later documentation changes do not alter
the identity of those tested installer bytes.

## Completed locally

| Check | Result |
| --- | --- |
| Rust locked all-targets tests | 211 passed, 0 failed, 5 optional native/GPU tests intentionally ignored |
| Frontend tests, including actual GLTFLoader material cases | 45 passed |
| Desktop authentication, lifecycle, proxy and packaging tests | 26 passed |
| Production frontend export | Passed |
| Source contract audit | 122 passed; source inspection, not native acceptance |
| Candidate harness JavaScript tests | 18 passed; 1 Windows symlink-permission skip, required on Linux |
| Candidate harness Python tests | 6 passed |
| Actual backend dynamic-port smoke | Two child launches and restart passed with fresh endpoint and HMAC identity |
| Actual API/proxy admission smoke | Passed against the rebuilt backend |

The dynamic-port smoke used real child stdout discovery, a fresh launch secret,
the actual health endpoint and HTTP/WebSocket forwarding. The prior secret was
rejected after restart. Existing services already occupied local ports 3000 and
7331 and were left running. That observation is not a controlled zero-probe proof;
the isolated hosted acceptance harness owns inert listeners and counts all TCP
connections to those ports.

The saved 13,600,384-byte molecular GLB with SHA-256
`19f602c96779dc32955eebc7c22d0bf2a92d6590555b5f323711a3720a5643d3`
passed the import guard and rendered in the actual browser WebGL workbench with
1,091,365 expanded triangles. Its original source bytes remain unchanged. Dark,
light and compact navigation and the About view were visually inspected with the
supplied original branding. These browser checks do not establish native
installer appearance or prove all graphics-runtime vulnerabilities unreachable.

The final frontend export includes production source maps. Actual emitted mapping
segments identify 311 unique mapped sources across 40 of 54 JavaScript bundles,
with their content and bundle/map hashes retained. Fourteen bundles have explicit
coverage gaps (11 lack a map reference; 3 have empty mappings). The evidence is
unchanged when copied to an installed UI directory; it does not claim complete
dependency coverage or that every line in a source file executes.

Independent read-only review accepted the bounded GLB material-name and texture
coordinate fixes and the backend/desktop endpoint ownership implementation. Provider
redirect rejection, key redaction and SQLite preservation tests are included in
the full Rust result above. Runtime and dependency scanner findings remain
separate from application regression tests.

## Color branding and dark default

The current frontend uses the owner's corrected v1.1 blue color artwork in both
expanded and compact navigation; compact dimensional symbols are 48px wide.
Flat, all-black and all-white branding are excluded, including native icons.
The first-paint theme bootstrap defaults to dark regardless
of operating-system preference, while preserving an explicit saved light choice.
The production build and interactive browser checks passed in expanded/compact
navigation. Internal light-mode compatibility images are excluded from marketplace
media; marketplace screenshots must use dark mode.

The v1.1 integration verified all 10 packaged asset receipts against their source
or recorded ICO conversion, built the production frontend, and checked both
themes and sidebar states visually. The 22 relevant desktop server, authentication
and lifecycle tests passed after correcting stale public-asset caching. Revisioned
branding URLs also bypass responses already cached by the previous app version.

The native candidate harness additionally checks an untouched fresh profile,
reload under renderer light-preference emulation, and an explicit light preference
across normal Quit and reopen. Its 1008px native-window screenshots must show dark
mode and the exact original color assets in both navigation states. These added
assertions passed in both native jobs for the corrected candidate below. All 12
original hosted screenshots were visually reviewed: the shaded blue wordmark
and the 48-by-38.765625-pixel compact symbol were visible without clipping, and
all captures were dark.

## Corrected candidate and local Windows upgrade

Both jobs in [candidate run 34545849872](https://github.com/delores88/PhaseForge/actions/runs/34545849872)
passed at exact clean source `7471ff72ac2cba9a89e96e88235cda810ebf1c1d`.
[Standard CI 34545836683](https://github.com/delores88/PhaseForge/actions/runs/34545836683)
also passed its browser, Windows backend and Ubuntu backend jobs.

| Current distributed package | Bytes | SHA-256 |
| --- | ---: | --- |
| `PhaseForge_0.8.0-alpha.1_x64-setup.exe` | 112,934,603 | `c919b026929256b74ad27cb8a29c3f5c44e492153aadc022fb1d3dfba42f0b06` |
| `PhaseForge_0.8.0-alpha.1_x86_64.AppImage` | 142,969,129 | `511c04d1ce37c146bf4d5575ceabe2849b726d00af4aaf7aedd2a2f5807f580f` |

Hosted acceptance on Windows Server 2025 x64 and Ubuntu 24.04.5 x64 with glibc
2.39 covered three installed-app launches, offline numerical results, preserved
data, normal Quit, backup restoration and removal. Both deterministic runs
produced final `x = 1`, retained 101 frames and used zero provider tokens. Fresh
profiles remained dark under a light color-scheme preference; an explicit saved
light preference survived reopening. These runs used debugger instrumentation
and disabled GPU acceleration. Ubuntu's restriction on unprivileged user
namespaces was relaxed to permit Chromium's sandbox, which remained enabled;
this does not certify stock AppArmor compatibility or local GPU operation.

The corrected Windows installer was subsequently installed locally through its
visible per-user installer. The user explicitly authorized closing the old app.
After verifying no active research objects or native jobs, the exact idle
PhaseForge process tree was terminated with `Stop-Process`; this was not the
normal application Quit lifecycle. A closed-app backup of 308 files totaling
339,515,320 bytes was verified before installation. OS-managed provider
credentials stayed in their existing credential store and were not exported.

All 218 installed payload files matched the hosted inventory, with no missing,
additional or changed entries. The actual uninstrumented local app displayed
the corrected dark interface, seven existing projects, the saved OpenAI
configuration and original color branding. A saved model with 1,091,365 expanded
triangles was observed in the workbench, and the blue application icon was seen
on the taskbar. Existing database counts and the SQLite integrity check remained
unchanged, and no new provider-usage records were created. The user later
confirmed that the corrected installation worked.

Local compact-navigation inspection and a local normal-Quit/reopen cycle were
not completed while the user was active and the app was minimized. The hosted
three-launch results are separate coverage and do not substitute for those
unperformed local checks. This local upgrade acceptance does not resolve the
security admission hold below.

## Superseded candidate packages

The packages below are retained engineering evidence. They were superseded by a
branding correction: the full wordmark previously disappeared at 1050 pixels
while the sidebar stayed expanded until 950 pixels. The corrected CSS aligns
those breakpoints. Original artwork and application theme colors are unchanged.
The subsequent `63d6807` candidate is also superseded: the owner requires original
blue color symbols in compact navigation, dark mode as the default, and dark-only
marketplace screenshots. Those requirements replace the earlier monochrome
compact treatment and operating-system theme fallback.
The subsequent `0f3d43c` native candidate run `34544797712` was cancelled before
artifact intake when the owner supplied corrected v1.1 artwork. Its v1.0 assets
and earlier visual approvals are historical. The corrected source and fresh v1.1
visual/native evidence are identified above; the hashes below identify only the
superseded packages.

Both native jobs in [candidate run 34540332559](https://github.com/delores88/PhaseForge/actions/runs/34540332559)
passed at clean source `b868e93b4a5ae4c356263684a515a8787ef5b4b7`.
The regular browser, Windows and Linux CI also passed at this commit.

| Distributed package | Bytes | SHA-256 |
| --- | ---: | --- |
| `PhaseForge_0.8.0-alpha.1_x64-setup.exe` | 111,709,894 | `0fcce03cb07eef6c050f99f4ba83dc5b8f368a70f82c47dea4d0188ff40de33c` |
| `PhaseForge_0.8.0-alpha.1_x86_64.AppImage` | 142,784,759 | `4f6a3f1373dd19614f44a4d663014bb406eb0da15f0849015dcf025b4587b581` |

Actual Windows Server 2025 x64 and Ubuntu 24.04.5 x64 package acceptance covered
installation, three launches, offline numerical computation, persistence, normal
quit, closed-database backup restoration and removal. The deterministic fixture
and its refined run both produced final `x = 1`, retained 101 frames and used zero
provider tokens. All four inert listeners on IPv4/IPv6 loopback ports 3000 and
7331 received zero connections and zero bytes. The owned dynamic backend port
closed after normal Quit. These fixtures do not certify minimum memory, GPU
workloads, ARM64 acceptance or an upgrade from an earlier version.

The hosted harness launches the installed executable through local debugger
instrumentation, with Chromium's sandbox enabled and GPU acceleration disabled.
It invokes the normal application Quit lifecycle and tracks process creation
identities. This is not an uninstrumented launch or GPU-rendering certification.

The Ubuntu runner explicitly disabled its restriction on unprivileged user
namespaces to permit Chromium's sandbox; Chromium's sandbox itself remained
enabled. This result therefore does not establish out-of-box compatibility with
stock Ubuntu 24.04 AppArmor settings. No developer-machine security setting was
changed by these checks.

Each retained candidate includes its original installer, compiler-bound backend
dependency metadata, installed payload inventory, observed runtime versions,
frontend module evidence, raw scans, checksums and detached GitHub OIDC provenance.
The workflow verified all six attested subjects against a freshly obtained trust
root. Subsequent local checksum/receipt consistency checks are not a separate
cryptographic replay. Neither package has been published by the candidate workflow.

## Apple Silicon packaging preparation

The requested release also includes macOS ARM64 DMG; Intel and universal Mac
packages are excluded. The native workflow now builds all three selected targets
from one source commit. Its Mac path checks the real host and every Mach-O member,
copies the app from a read-only DMG, and exercises the same offline, restart and
data-preservation fixture. Native Mac execution remains pending for this source.

The Mac backend is ad-hoc signed before its materials are bound; the packaging
step preserves those exact backend bytes while sealing the app. Evidence retains
the compiler output and signed hashes, unchanged dependency section, raw signature
verification and Gatekeeper/quarantine observations. No Apple Developer ID or
notarization is configured. Direct hosted launch does not certify Finder approval
of an internet download. The previous `7471ff7` Windows/Linux receipts and local
Windows upgrade above remain their own historical source bindings; they cannot be
relabelled as artifacts of this new packaging source.

## Security review status

The historical Electron 43.7.0 candidates retain 413 High/Critical scanner candidates and eight source
secret findings. Counts alone do not establish exploitability. The source secret
findings are six public WebSocket handshake fixtures and two documented native-job
UUIDs; their individual dispositions leave the raw reports intact. Payload and
expanded application-archive secret scans reported no findings. Runtime
applicability review remains separate from these secret dispositions.

Those Electron 43.7.0 packages remain superseded: ANGLE CVE-2026-87500 was not
patched in that runtime. The GLB guards address application input paths and are
not a vendor patch or a complete shader reachability proof.

The current source pins Electron 45.0.0-alpha.6. The exact vendor source contains
both the reviewed ANGLE CVE-2026-87500 and V8 CVE-2026-87491 fixes; the official
dependency pins, fix ancestry and applied patch checks are recorded in
`release-coordination/ALTERNATE_RUNTIME_FEASIBILITY.md`. The local Windows runtime
probe observed Electron 45.0.0-alpha.6, Chromium 155.0.8038.2, Node 24.21.0,
V8 15.4.80-electron.0 and Node SQLite 3.53.4. The separate backend retains SQLite
3.53.2. These observations do not waive other vulnerability findings or certify
the stability of an alpha runtime. New packages require their own native
acceptance, exact materials and marketplace admission; historical receipts and
dispositions cannot be relabelled as evidence for this source.

Native builds may run concurrently. Marketplace publication must proceed Windows
x64 first, then macOS ARM64, then Linux x64, without waiting for all three before
the first eligible platform can be submitted. No security exception, published
release or completed marketplace validation is claimed by this document.
