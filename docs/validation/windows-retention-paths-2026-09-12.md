# Windows retained-output path compatibility

## Original release failures

At source `725f14b56c0287b8701fcec3f28afbad0b0c07ae`, Windows candidate `34713311972` and CI `34713311973` each stopped in backend tests with **479 passed, 17 failed, 12 ignored**. Both sets of failures originate from retained-file handle validation rejecting an unexpected ancestor, or follow-on assertions when that refusal prevented a retained file from being written. CI's Linux backend and browser jobs passed. The Windows candidate stopped before packaging; neither failed run was retried or relabeled successful.

The guard compared the normalized name returned by `GetFinalPathNameByHandleW` with the original requested spelling. A native, read-only local probe reproduced a false rejection for the real `C:\PROGRA~1` alias: the handle reports `\\?\C:\Program Files`. Expanding the requested spelling with `GetLongPathNameW` makes those names agree. The hosted failure logs did not print their requested path, so the exact runner alias is not independently recorded; the native alias reproduction demonstrates the compatibility bug.

Original evidence remains in `.local/validation/delivery-watch-725f14b-20260912/`:

- `candidate-34713311972-api-job.log`, SHA-256 `ea874667e19667ed07801a3f64071c217020545be19c67d67192934ef081245b`.
- `ci-34713311973-windows-api-job.log`, SHA-256 `d3b6e61a1c5f47e0db3dc0728cd305362a4df69d93d222622dba0cc818d822b5`.
- `windows-short-alias-probe.json`, the native before/after comparison, plus timestamped exact-source run snapshots and the candidate failure artifact. Two initial `gh run view --log` attempts returned empty logs; the subsequent API job-log downloads contain the failure evidence.

## Narrow fix

The Windows guard expands only the requested path spelling before comparing it with the opened handle's final path. Absolute DOS and UNC paths receive the corresponding verbatim namespace for this API; existing extended prefixes remain intact. Buffers remain bounded to 32,768 UTF-16 code units and conversion failures fail closed. No path canonicalization, registry changes, or tilde-based shortcut is used.

Every ancestor is still inspected for reparse points and held open without `FILE_SHARE_DELETE`. The opened file still must satisfy the original reparse, hardlink, size, timestamp and hash checks. A different file handle cannot become valid through name expansion.

Microsoft documents that [GetLongPathNameW expands short names and has explicit long-path and buffer rules](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getlongpathnamew), while [GetFinalPathNameByHandleW returns a normalized final name by default](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-getfinalpathnamebyhandlew).

## Local affected validation

Final source passed **34 retention tests and 6 read-only intent tests**, using one Cargo worker and one test thread. No provider request or new scientific computation was made. Logs and command receipts are in `.local/validation/windows-short-alias-20260912/`.

- `retention-03.log`: 34 passed. Both new Windows tests printed `EXERCISED`: actual short directory/file aliases and paths exceeding 260 UTF-16 code units supported exact retained reads and immutable streaming. The short-alias test explicitly reports skipped alias coverage if a host's filesystem supplies no short spelling; this host exercised it.
- Existing junction/ancestor redirection, hardlink, malformed path, tampered bytes and recovery ownership checks remained green. The new alias test also rejects an unrelated file handle and a hard-linked source reached through its real short name.
- `readonly-intent-01.log`: 6 passed, including the read-only evidence test that failed in both hosted Windows runs.
- `retention-01.log`: the first alias-only patch passed 33 checks. `retention-02.log` is preserved as a failed intermediate run: 33 passed, and the new long-path fixture failed during directory creation because its component strings accidentally ended in spaces. Correcting those fixture names produced the final 34-pass run; the failed receipt has not been overwritten.

The rebuilt binary is recorded separately in `build-01-receipt.json`: 55,157,248 bytes, SHA-256 `a80cd8b79878ffcd8fee05f92342d83130f3fce006616c3a4c9857119a9a5b27`.

The actual rebuilt application then recovered the same already-completed CUDA job `5ebd8820-43ef-4e91-8213-9d158a439809` through its Windows/WSL UNC paths. Operation `208d6953-a60a-4c29-bcc4-417bb9ea43f6` returned HTTP 202 within the unchanged one-second bound and completed in approximately 42.46 seconds. All 197 concurrent GETs succeeded, with maximum 63 ms and p95 47 ms. Original result/input/deadline/completion and job identities remained equal; no scientific execution or duplicate retention event was created.

This changed-path integration receipt is `.local/validation/nr-background-recovery-20260912-01/actual-async-02-winpath/report.json` (`passed: true`), with original before/after records and every request measurement. It is separate from the prior successful recovery and earlier failed receipts. Unchanged UI/provider workflows were not rerun. Local checks do not substitute for the fresh exact-source Windows installer acceptance or marketplace admission.
