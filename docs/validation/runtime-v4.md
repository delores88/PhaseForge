# Fixed SQLite in immutable Windows runtimes

The first hosted 0.9 candidate passed native execution but its Python runtimes
loaded SQLite 3.50.4 with FTS5 enabled. Raw vulnerability findings were retained;
that package was not installed as the final update or admitted to the marketplace.
The [SQLite advisory](https://www.sqlite.org/cves.html) identifies the FTS5 fixes
in 3.53.2. Both Python runtimes now use the official 3.53.4 Windows x64 DLL.
The separate Rust database engine remains SQLite 3.53.2.

The replacement archive is
`https://www.sqlite.org/2026/sqlite-dll-win-x64-3530400.zip` (1,370,147 bytes),
SHA-256 `8b959b7eff4a81f6a62fc3468f9273e5cfe78d4a927e62215aed231b654fb104`.
Its official published SHA3-256 is
`deddee963c810d1eeac3ce5e15c7c41da21a1c54d7a39cf54fbf577d2f50de3a`.
The unmodified `sqlite3.dll` member is 3,285,504 bytes, SHA-256
`ab57d0437795ecc757cb693f32ea224173fa9856594d95cfa6b5033e645cd1ec`.
The manifests explicitly record the replaced CPython archive member, the new
member, and the retained companion definition file; the new DLL is attributed
to SQLite rather than to CPython. Original archives and older frozen manifests
remain available for evidence and reproduction.

New immutable contracts:

- `science-v4`: SHA-256
  `fc3bded856ffa58137f900a1fe8d5fe31531d7206f25d1d7e1ef95029fda57a9`,
  2,159 files, 76 native members, six source archives.
- `python-numpy-v3`: SHA-256
  `e842d9a76e9215978f4163a9aabedb4eac905e0ff5a23b9f8a6dc55e28324915`,
  1,510 files, 51 native members, four source archives, including the separately
  hashed inner isolation manifest.

The offline builder reproduced both contracts exactly. Actual copied-runtime
acceptance on Windows passed: CPython 3.13.15 source-module imports, NumPy 2.4.6,
OpenMM 8.5.2 integration, SQLite 3.53.4 and ordinary FTS5 queries, actual loaded
DLL paths, and unchanged complete file inventories. The generated runtime also
ran under LPAC; outside-file access, runtime writes and network connections were
denied while a retained NumPy calculation succeeded. Existing science-v1/v2/v3
and generated-v2 preservation sentinels remained unchanged. The three trusted
solver service entries (OpenMM, diffusion and mechanics) subsequently completed
against the new copied science runtime.

Receipts are retained locally in
`.local/validation/runtime-v4-acceptance-01/report.json` and
`service-entry-report.json`. These are source-build compatibility and isolation
checks. They do not replace exact final hosted installer authentication, installed
acceptance, or the independent marketplace security decision. No predictive
biology validation or additional ML study is claimed by this runtime update.
