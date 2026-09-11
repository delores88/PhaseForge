# Durable specialist and compaction checks

Source validation on 2026-09-11. These tests use temporary databases and local mock HTTP providers. They do not establish installed-app behavior or scientific predictive validity, and did not contact a paid provider.

## Specialist jobs

`delegate_specialist` creates a persisted child job with a concrete objective, exact evidence job IDs, parent/root IDs, and the parent's captured provider, model and reasoning effort. Its deadline is the parent's absolute deadline, including Off. Child cancellation tokens descend from the parent's token. `inspect_specialist` waits locally and returns the actual child's output, error and provenance. Assignment and result handoffs appear as recorded timeline events.

Specialists inspect assigned evidence and their own retained context. They cannot start experiments, change presentation, generate illustrations or retrieve unrelated conversation. Nested delegation cannot enlarge the evidence scope. Admission under a shared lock limits each parent to three children, the whole tree to eight specialists and nesting to two levels. Each specialist can make at most eight model turns and finishes earlier when it returns an answer. Identical assignments reuse their existing child. Existing account token/cost admission limits still apply.

Resume requires a running parent and inherits its remaining deadline. A child cannot independently extend its budget. Cached final delivery uses the same message and completion identity without another generation. Uncertain requests remain uncertain; resume does not purchase a replacement to hide missing receipts.

`cargo test --lib agent::laboratory::team::tests -- --nocapture`: **10 passed**.

- Actual child request and answer, captured model/effort, evidence and handoff records.
- Stable target recovery and repeated assignment reuse with one provider request.
- Foreign evidence, unsupported tool and unrelated child inspection rejection.
- Atomic depth, fanout and tree limits, including concurrent admission.
- Parent cancellation and rejection of late orphan delegation.
- Provider-slot contention without a request while waiting; exact deadline/Off preservation.
- Stop at eight model turns with an explicit incomplete outcome.
- Resume requires a live parent and commits a saved outbox exactly once.
- Resumed child still pauses with its parent.
- Shared account token limit rejects admission before a request or reservation.

## Compaction and response recovery

Compaction saves intent before reserving usage, and a dispatch marker before HTTP. The exact summary prompt, system instructions, chosen model/effort and output limit remain in a request artifact. Initial background and final provider receipts are retained. Saved remote IDs are retrieved instead of submitted again. Context replacement and its delivery outbox commit together; the full source transcript and tool receipt map remain available. A hash checks the source boundary being summarized.

`cargo test --lib compaction_recovery_tests -- --nocapture`: **9 passed**.

Checks cover synchronous/background completion, restart between usage reservation and journal identity, cached receipt without credentials, saved remote-ID GET, a queued receipt before ID extraction, uncertain dispatch, incomplete output, committed-context/outbox replay, legacy orphan reservations, and provider admission contention. Recovery assertions include request counts, preserved source/tool associations, metering, and deduplicated compaction events.

`cargo test --lib agent::laboratory:: -- --nocapture`: **26 passed** across seven response-recovery, nine compaction and ten specialist tests. `cargo test --lib usage:: -- --nocapture`: **8 passed**, including admission and preserved uncertain usage across restart.

`cargo test --lib laboratory::tests::pause_is_durable -- --nocapture`: **1 passed**. A cancellation waker observes the durable state of the whole tree at the instant cancellation is signalled. All active descendants are already paused, completed artifacts and unrelated jobs remain unchanged, and repeated pause does not add duplicate stop events.

Installed restart/compaction evidence is tracked separately by the main implementation ledger. Real integrated specialist delegation, long-duration work and scientific reviews remain to be demonstrated in the installed build.
