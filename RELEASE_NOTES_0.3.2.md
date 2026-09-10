# PhaseForge 0.3.2 — validator build correction

Complete incremental release based on the actual `PhaseForge-0.3.1-FULL.zip`.
No older archive or file-by-file patch is required.

## Fixed

`backend/src/agent/schema.rs::validate_draft` declares `anyhow::Result<()>` but
0.3.1 returned `anyhow::Result<Vec<String>>` from `validate_manifest`. This caused
the reported Rust E0308 error at schema.rs:66 and stopped the backend build.

The corrected function uses `?` to propagate a validation failure with its context,
then returns `Ok(())` on success. Nonfatal warnings do not become validation errors.
The underlying warning-producing validator and its existing callers are unchanged.

## Regression coverage added

Four Rust tests exercise:

- the function's exact `Result<()>` success contract;
- contextual propagation of a rejected integration time step;
- rejection of an undeclared expression variable;
- acceptance of a valid particle draft that produces a nonempty warning list.

These tests are compiled and run by the existing `cargo test --locked --all-targets`
installer step. They are not new user-facing scientific examples.

## Preserved

All 0.3.1 features and existing files remain: nested model-output schema, bounded
repair, separate key/model settings, one sidebar, explicit-only new-world dialog,
chat, cancellation, usage/cost controls, light/dark themes, numerical and molecular
workbenches, and self-contained PowerShell text installers. Dependencies and
scientific-engine behavior have not been changed.

## Install

Stop the old frontend/backend windows. Extract this complete ZIP into a fresh
folder. Open PowerShell inside the extracted `PhaseForge` directory and paste the
entire `INSTALL_ALL.txt`. After it succeeds, paste the entire `START_ALL.txt`.
The startup version checks expect 0.3.2, so an old running backend is not silently
accepted. Existing application data/credential locations are unchanged.

## Validation boundary

The correction matches the supplied compiler diagnostic. Static and fixture
checks are not a substitute for a native build. Rust/Cargo, Windows PowerShell,
MSVC, and package-registry connectivity are unavailable in this packaging
environment; see `docs/RELEASE_VALIDATION.md` for checks actually executed.
