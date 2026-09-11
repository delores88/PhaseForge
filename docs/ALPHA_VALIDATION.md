# 0.8.0-alpha.1 engineering validation

Recorded on 2026-09-10 for the source snapshot containing this document.
This supplements the historical [0.8.0 baseline](RELEASE_VALIDATION.md); it does
not replace that record or certify an installer that has not yet been built.

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

The current frontend uses the original blue color artwork in both expanded and
compact navigation. The first-paint theme bootstrap defaults to dark regardless
of operating-system preference, while preserving an explicit saved light choice.
The production build and interactive browser checks passed in expanded/compact
navigation. Internal light-mode compatibility images are excluded from marketplace
media; marketplace screenshots must use dark mode.

The native candidate harness additionally checks an untouched fresh profile,
reload under renderer light-preference emulation, and an explicit light preference
across normal Quit and reopen. Its 1008px native-window screenshots must show dark
mode and the exact original color assets in both navigation states. These added
native assertions remain pending execution in a replacement candidate.

## Superseded candidate packages

The packages below are retained engineering evidence. They were superseded by a
branding correction: the full wordmark previously disappeared at 1050 pixels
while the sidebar stayed expanded until 950 pixels. The corrected CSS aligns
those breakpoints. Original artwork and application theme colors are unchanged.
The subsequent `63d6807` candidate is also superseded: the owner requires original
blue color symbols in compact navigation, dark mode as the default, and dark-only
marketplace screenshots. Those requirements replace the earlier monochrome
compact treatment and operating-system theme fallback.
Replacement packages must bind the corrected source before installation or
release; the hashes below do not identify that correction.

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

Local installation and visual inspection on the developer's Windows 11 machine
remain pending a replacement package from the corrected source.

## Security review status

Each candidate retains 413 High/Critical scanner candidates and eight source
secret findings. Counts alone do not establish exploitability. The source secret
findings are six public WebSocket handshake fixtures and two documented native-job
UUIDs; their individual dispositions leave the raw reports intact. Payload and
expanded application-archive secret scans reported no findings. Runtime
applicability review remains separate from these secret dispositions.

Electron 43.7.0 is an engineering candidate with verified upstream backports, but
the reviewed ANGLE CVE-2026-87500 remains an unresolved release blocker. The GLB
guards address two demonstrated application input paths; they do not constitute
a vendor patch or complete shader reachability proof. Marketplace profile
activation and admission remain held. No security exception, published release
or completed marketplace validation is claimed by this document.
