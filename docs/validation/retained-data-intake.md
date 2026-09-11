# Retained numerical source intake

Source checks on 2026-09-11 use temporary databases and local mock HTTP providers. They do not claim installed-app validation or scientific suitability.

The laboratory chat now accepts typed attachments, validates the whole batch before starting work, and retains original UTF-8 bytes in immutable `data` jobs. Session inputs and saved message chips contain file references instead of raw contents. File-only messages request inspection rather than authorizing an experiment. Older raw-attachment sessions admit their files on explicit Resume.

Each reference includes `job_id`, `path`, filename, actual byte count, SHA256 and canonical MIME. Source provenance records upload origin or requested/final public URL, retrieval time and available response validators. Declared units are explicitly unverified. CSV profiles retain missing, nonfinite and nonnumeric counts; JSON syntax and complexity are bounded. Other supported text formats remain text whose scientific syntax/engine compatibility requires checking. No raw rows are silently removed or treated as an inferred biological endpoint.

Models receive an attachment index and real `list_project_files`, `read_project_file`, `profile_dataset` and `acquire_data` tools. Reads verify project ownership and retained hashes. UTF-8-safe pages use explicit offsets and a continuation marker. File listing is concise; full profiles require a separate read. Specialists can read only their assigned evidence files.

The isolated execution import contract is `{job_id,path,destination,sha256}`. The source-copy implementation independently verifies project ownership, path safety and the optional SHA pin, then retains its own immutable input snapshot.

Limits are explicit: eight files and 8 MiB combined; 2 MiB per text file; CSV additionally uses the existing 1 MiB/20,000-row/128-column validator. Public acquisition accepts CSV, JSON or PDB on a fixed host list using HTTPS, no credentials/query/custom port, no redirects or system proxy, bounded streaming and cancellation. Unsupported or insufficient data is an error or unresolved prerequisite. The full response receipt is saved before interpretation; retry uses the saved snapshot instead of silently retrieving changed data.

Validated commands:

- `cargo test --lib laboratory::data::tests -- --nocapture`: **7 passed**. Original-byte/hash/unit retention, all-or-nothing invalid admission, replay, project isolation/tamper detection, Unicode paging, fixed-host admission, redirect/streaming/cancellation bounds, and saved public receipt recovery.
- `cargo test --lib laboratory -- --nocapture`: all **4 agent intake tests passed**, alongside the **7 source-intake tests**. File-only chat requests the actual reader/profile tools and receives exact CRLF source bytes plus measured statistics with the selected model/effort. Retry does not generate again; invalid batches create no job/message/provider call; legacy raw files migrate on Resume. An added recovery case verifies that a saved tool proposal receives its result before newly admitted file context. The broader command returned 71 successful test results, including seven environment-gated checks that printed skips; those skipped checks do not establish Python, Blender or process-isolation behavior.
- `node --test tests/attachmentIntake.test.mjs`: **3 passed**. UTF-8/BOM preservation, batch rejection and explicit path/type/size/binary errors.

No external source or paid provider was contacted by these checks. Installed upload/download demonstrations and engine-specific dataset validation remain separate acceptance steps.
