# OpenSSL correction for the Windows 0.9.1 candidate

The ordinary marketplace scan of published 0.9.0 rejected the original CPython
3.13.15 OpenSSL 3.0.21 pair in both bundled runtimes. The four blocking advisory
identities were CVE-2026-75803, CVE-2026-54874, CVE-2026-63072 and CVE-2026-63076.
The [OpenSSL advisory](https://openssl-library.org/news/secadv/20260825.txt)
identifies 3.0.22 as the fixed release on this branch. No exception is used.

The replacement is the unmodified AMD64 pair from CPython's official
`openssl-bin-3.0.22` binary-dependency archive, SHA-256
`f41c05d4e91ca5d687dcea3a5cc54b8692c86f5e7e8eb51d7b0bf537c7bcaddb`
(24,671,379 bytes). Its tag resolves to commit
`8898e94682675b969ef09c0be4cdf46bf764fa9e`, and the
[CPython 3.13 update](https://github.com/python/cpython/commit/87c9dfbf06de955e79c606d3163d9a40160d04cf)
selects this binary tag. No interpreter upgrade, DLL renaming or local OpenSSL
build is involved.

| Retained file | Bytes | SHA-256 |
| --- | ---: | --- |
| libcrypto-3.dll | 5,243,128 | fce8b678990ce3266fd9a86fbda19f232afa7ba76de81d73145dc35cdb19513a |
| libssl-3.dll | 794,872 | 55913d1e5287ee969c0dab64b101d880a7fd0d95a974c112d925579be097afbd |
| licenses/OpenSSL-3.0.22-LICENSE.txt | 10,352 | ed72ce2b51ee58f117e5a021e2e04af158857f40269fbc03491f0b2a99dbcc96 |

New frozen manifests record every file and both original/replacement archive
members. Earlier manifests and numerical artifacts remain unchanged:

- `science-v5`: SHA-256
  `81d70a3d337bded463bf37fa71c32e49c2b860e5e3ae12cce4aa0c4107c94dec`,
  2,160 files and 76 native members.
- `python-numpy-v4`: SHA-256
  `d5cfc619397e636aca0444a8967e496b54e0669b04e586208fecdf0980ca5543`,
  1,511 files and 51 native members.

The prior SQLite 3.53.4 correction remains. NumPy, OpenMM, Pillow and the
interpreter are unchanged. Runtime-identity checks require a new attempt when
saved work names an older runtime; they do not rewrite previous experiments.

The release acceptance now requires the actual copied Python to load both fixed
DLLs from its own directory, import `_ssl` and `_hashlib`, compute a known digest
and exchange data over a certificate-verified TLS 1.3 MemoryBIO connection.
An ephemeral localhost key/certificate is generated for that bounded check;
there is no external networking or committed private key. The observed pair and
its independently replayed archive provenance also feed explicit OpenSSL
components into candidate vulnerability screening.

Both new seeds passed that actual local copied-runtime check: OpenSSL reported
`3.0.22 25 Aug 2026`, the exact `_ssl`, `_hashlib` and DLL paths matched, and the
certificate-verified TLS 1.3 connection negotiated `TLS_AES_256_GCM_SHA384`.
The source-level receipt is
`.local/validation/runtime-v5-openssl-01/managed-openssl-runtime.json`, SHA-256
`4f4764a1fcacaf0d40f1fbbb6250e8ad7417f2d28e04a4e8775c9e1ce2c493a4`.
Its source commit is explicitly null because it precedes the frozen release
commit. This check used disposable local copies and did not touch app data.

The backend's copied-runtime/LPAC check and all three existing solver service
entry checks also passed using these seeds. Receipts in
`.local/validation/runtime-v5-backend-01/` retain runtime paths, access-denial
results and the small OpenMM, diffusion and mechanics jobs. This verifies the
existing app entry points after the dependency change; it adds no claim about
broader scientific validity or longer experiments.

These contracts and source/member checks do not certify an installer by
themselves. The exact 0.9.1 candidate still needs its own hosted native execution,
local installation verification and ordinary marketplace admission. Published
0.9.0 assets and its rejection evidence are immutable.
