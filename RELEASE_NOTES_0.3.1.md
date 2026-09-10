# PhaseForge 0.3.1 — reliable proposals and controlled research

Complete, incremental source release based on the installed 0.3.0 project. The
Rust backend, Next.js frontend, generic simulations, molecular workbench and
self-contained Windows text installers remain in place. No overlay is required.

## Fixed model-output contract

Provider responses now contain a real nested `manifest` object, not escaped JSON
in a `manifest_json` string. Both provider requests use the same versioned schema.
All integration fields, including `start_time`, are required. A local schema check
runs before deserialization so optional defaults cannot mask missing fields. The
existing numerical validator then checks scientific representation, expressions,
units-as-authored, dimensions and resource constraints. Unsupported capabilities
remain unsupported; no named physics example is injected.

One repair call is allowed by default, with a configurable ceiling of two. Each
repair includes the actual rejection reason, uses the same strict schema, passes
through the same spend controls and is separately metered. HTTP errors, refusals
and token-truncated output do not trigger hidden retry loops. Nothing runs without
a valid proposal. Final failures stay in the conversation and usage ledger.

## Interface

- One collapsible navigation sidebar; no duplicate permanent activity rail.
- No forced first-run question modal. Creating a world is local and makes no model call.
- Readable split chat with full-width assistant responses, distinct user messages,
  role/model selectors, attachments, history, draft recovery, copy, edit-and-branch,
  retry/regenerate, and live request phase labels.
- Desktop chat resizes from 10% to 60%. Mobile uses a full-width collapsible chat.
- Branching stores parent-message links and gives the model that branch's history,
  rather than labeling a duplicate chronological message as a new branch.
- Light/dark theme controls, persisted locally with a pre-render theme bootstrap.
- Runs and other views no longer stretch controls across oversized empty rows.

## Usage and stopping

`Usage & cost` records provider-reported input, output, cache and reasoning tokens;
individual model calls, repair attempts and connection tests; response IDs; errors;
and user-rate-card-based dollar estimates. Missing prices are unpriced, not free.
Pricing is not downloaded, guessed, or silently updated.

Stop one request, stop all active agents, or pause paid calls. Cancellation signals
remain available through response parsing and validation. The local commit guard
prevents a stop accepted before commit from submitting a new simulation. Already
submitted simulations have separate Cancel controls on the Runs page.

Token and estimated-cost limits govern admission of every new call. Reservations
include estimated input and the full output allowance. Unreported cancelled calls
retain reservations; completed calls use actual reported counts. These are local
controls, not provider-enforced billing guarantees. Work already processed remotely
may remain billable. See `docs/USAGE_AND_COST.md`.

## Upgrade

Stop both old service windows with Ctrl+C. Extract the full archive to a new folder.
From its `PhaseForge` root, paste the entire `INSTALL_ALL.txt`, then the entire
`START_ALL.txt`. The launcher refuses an old backend or an already-running frontend
rather than silently displaying the previous version. Existing local data and OS
credentials use the same locations; keep any custom configuration/environment.

## Validation

See `docs/RELEASE_VALIDATION.md`. Source/schema/API-helper tests and Chromium
source-component layout fixtures are distinguished from actual native builds.
Windows compilation, actual provider calls and physical GPU tests were not run in
the packaging environment.
