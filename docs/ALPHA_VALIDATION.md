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

## Pending exact installer evidence

The candidate workflow must build from a clean pushed commit and pass actual
Windows x64 NSIS and Linux x64 AppImage installation/launch, offline computation,
normal quit, persistence, closed-database backup restoration and removal. The same
Windows candidate installer must then be installed and inspected on the developer's
Windows 11 machine. Original installer hashes, source identity, compiler-bound
dependency metadata, native runtime observations, raw scans and detached build
provenance are required before any package acceptance claim.

Electron 43.7.0 is an engineering candidate with verified upstream backports, but
the reviewed ANGLE CVE-2026-87500 remains an unresolved release blocker. The GLB
guards address two demonstrated application input paths; they do not constitute
a vendor patch or complete shader reachability proof. Marketplace profile
activation and admission remain held. No security exception, published release
or completed marketplace validation is claimed by this document.
