# Trusted retained-PDB import

Source implementation prepared on 2026-09-11. Installed execution and the
post-install CAR-T revision remain pending. This is a general structure import,
not a CAR-T preset or a new biological solver.

The durable `import_structure` tool accepts an existing completed same-project
data job's exact `job_id`, `path`, `sha256`, a structure `name`, and explicit
`units: "angstrom"`. `acquire_data` or an uploaded PDB supplies the original
bytes. The importer reuses `science::molecular::import_structure`, the same
trusted PDB parser used by research asset import, through its bounded entry point.
It does not generate a parser
with a model or run downloaded code.

The source file is limited to the existing 2 MiB intake bound and 1–12,000 raw
coordinate records to bound the trusted parser's pairwise diagnostics. A larger
source is rejected rather than silently clipped. Connectivity is capped at
100,000 bonds before inferred-bond allocation can expand further, and the atomic
receipt is checked against its 32 MiB read limit before publication. Conflicting declared units,
foreign/incomplete jobs, changed bytes, wrong paths/hashes, and invalid PDB
coordinates and non-ASCII fixed-width atom records fail. Supported file access uses the existing bounded plain-file
reader, including its Windows reparse/hardlink checks. The importer returns
author-chain/coordinate-residue/atom counts, parser warnings, source provenance,
source/header limitations and its registered structure ID.

All retained first-model chains remain in their original common PDB coordinate
frame. There is no new chain filter, per-chain recentering, normalization,
missing-residue reconstruction or silent unit conversion. Bind an interacting
complex as **one structure to one authored scene node**. The existing renderer
then applies one common fit to that complex and records its source-to-scene
mapping. Its smoothed van der Waals envelope is display geometry, not electron
density, atomistic dynamics, or a solvent-excluded surface.

Each tool target owns a durable `structure` child job and one deterministic
registered structure ID. The child inherits the current active parent's exact
deadline, including Off. Original source bytes and the parsed structure are
retained together in atomic `import-receipt.json`; the database pins the exact
receipt bytes before registering the structure. Recovery from receipt publication
or structure persistence reuses that identity and does not duplicate molecules
or completion events. If the receipt reached disk before its database pin,
recovery compares it against the same trusted parser locally. It never makes a
provider or network request. A changed parser without a committed parse requires
a new explicit import; it cannot silently reinterpret a pending source. A
transient terminal-write failure can reconcile its already-registered structure
only against the exact pinned receipt; it cannot retry an arbitrary failed parse.

The returned `registered_structure_sha256` hashes compact Rust serialization and
states that recipe. Blender's existing `structure_sha256` fingerprint uses sorted
Python JSON and has its own declared recipe. The original PDB hash and registered
structure ID connect these records; the different serialization hashes must not
be compared as if they named the same bytes.

Parsing happens outside the lifecycle gate. Publication rechecks cancellation,
the parent deadline and the child's state under the same gate as Stop. An
explicitly cancelled child cannot be revived by replay. Reading a completed
receipt does not change its original job or budget. Parser/I/O failures remain
retained attempts.

Eight deterministic source tests cover exact two-chain coordinates and identity,
ownership/hash/path/unit failures, Windows hardlinks, malformed/nonfinite PDB,
receipt-before-database and structure-before-terminal restart boundaries,
pause/cancel/deadline refusal, receipt/database tampering, execution reservation
completed replay, and dense-coordinate bond expansion. After the separate runtime
benchmark released compute, `cargo test --locked --no-default-features
retained_pdb_import -- --nocapture` passed all eight tests in 1.07 seconds, with
zero failures or ignored tests. The existing `science::molecular` filter passed
both tests, also with zero failures or ignored tests. Initial compilation took
58.39 seconds and reported existing no-default-feature warnings. No installed or
biological validation claim is made from these synthetic fixtures.

## Reference for the later CAR-T illustration

[PDB 7URV](https://www.rcsb.org/structure/7URV) is an experimentally determined
human CD19–FMC63 scFv complex at 3.05 Å cryo-EM resolution. CD19 is label chain
A / author chain C; the mouse-derived FMC63 scFv is B / author D. The archive
lists 3,460 atoms and 445 modeled of 499 deposited residues. A read-only in-memory
check of the [official PDB](https://files.rcsb.org/download/7URV.pdb) returned
308,691 bytes, with 1,701 atom records in C and 1,759 in D. These are source
feasibility observations, not a saved project import or a new render.

The [primary paper](https://pmc.ncbi.nlm.nih.gov/articles/PMC10228544/) describes
soluble CD19 ectodomain, without its transmembrane helix and cytosolic region.
This complex does not establish a whole-cell structure or the full CAR hinge,
membrane anchor and intracellular signaling arrangement. A later illustration
can combine an explicitly authored cellular contact scene with a labeled 7URV
inset, retaining exact acquired bytes, source identity and a declared magnification.
Unresolved domains and membrane placement must remain conceptual. Labels and
realistic lighting do not establish treatment efficacy or biological validation.
