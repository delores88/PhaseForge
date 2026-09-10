# Usage, cost estimates, budgets and cancellation

## Scope of accounting

The local SQLite database records provider calls made by PhaseForge 0.3.1 onward.
It cannot reconstruct earlier calls or charges incurred by other programs. Every
proposal, Studio design, specialist brief, review, repair and connection test has
a separate record under its parent request ID. Records contain provider/model, project ID, purpose, attempt, status,
timestamps, response ID, token counts, a price snapshot and diagnostic errors.
They do not contain API keys or copies of request prompts. Conversations and
attachments are separately stored in the pre-existing research database.
Internal session instructions are retained in local conversation audit metadata;
Studio revisions also retain their design brief, lineage and supplied failure report.

The page shows complete-database totals and the latest 500 ledger rows. JSON export
contains the displayed report, not an unlimited historical export. Counts are not
silently reset on application restart. Budgets use this lifetime local scope;
raise the threshold deliberately to allocate additional budget.

## Tokens

OpenAI input tokens already include cached input, and output tokens already include
reported reasoning tokens. They are displayed as subsets, never added twice.
Anthropic raw `input_tokens` excludes cache-read and cache-creation tokens; those
are added to normalized input. Five-minute and one-hour cache writes have separate
rate-card entries when reported. Missing or malformed final usage is unknown, not
a zero-token success. The request reservation is retained for budget accounting.

If the response arrives but the proposal or Studio artifact fails validation,
reported token usage is still recorded. Restart marks unfinished records as
interrupted and preserves known usage or an unsettled reservation.

## Dollar estimates

Enter exact requested model IDs and current/contracted USD-per-million rates in
Usage & cost. There are no bundled pricing guesses. Historical unpriced records
can receive their first matching rate card; an existing captured rate is not
silently replaced when a new price is entered later.

Cost = uncached input × input rate + cached reads × cached rate + cache writes ×
their rate + output × output rate, divided by 1,000,000. Reasoning is included in
output rather than charged a second time. Taxes, provider minimums, special modes,
non-token tool charges, discounts and billing adjustments are outside this estimate.
Unpriced entries keep the total explicitly incomplete. Provider billing is authoritative.

## Admission controls

Defaults: 12,000 maximum output tokens per call, one automatic proposal-repair
attempt, one simultaneous provider call, no lifetime token/USD limit, 80% warning
threshold. Users may select zero repairs; the hard maximum is two. Every repair
rechecks pause and admission budgets before network I/O.

An explicit provider output-limit failure can use that same repair allowance to
request a complete, more compact experiment or Studio artifact. PhaseForge keeps
the user-selected output ceiling and records usage from both attempts. Zero repairs
disables this retry; lowering the allowance or stopping work is honored before the
next call. Partial JSON is never saved as a validated design. Other incomplete
responses, including content filtering, do not trigger this output-limit recovery.
Reasoning consumes the output budget, so a demanding design may still need a larger
user-selected limit or a narrower brief.

Public catalog retrieval and local Blender/CadQuery/KiCad execution do not make
model-provider calls. AI design and revision do, even when their eventual render
or engineering export runs locally. Network access, local resource use and optional
engine installation are separate from this provider token ledger.

Preflight reserves prompt/system/schema UTF-8 byte lengths plus overhead as a
conservative input estimate and the full output-token cap. This is not a tokenizer
or provider-side monetary cap. Completed calls replace the reservation with reported
counts. Failed, cancelled or interrupted calls without final usage keep theirs.
Known prices are needed for a hard local USD limit: an unpriced current or historical
model blocks admission rather than being assumed free.

Concurrent admission is serialized by a process-wide usage lock. Each provider call
checks the parallel-call setting and records its reservation before sending HTTP.
Model response-shape validation, provider rejection, timeout, transport failure,
and invalid experiments remain distinct recorded outcomes.

## Stop controls

The chat Stop button and per-request Usage button signal the backend cancellation
token. Stop all agents signals all active parent requests. Pause paid calls persists
the paused setting and signals active requests; model discovery and local diagnostics
remain available. Stop handles registration races via short-lived cancellation IDs.

Stopping does not undo tokens processed remotely. After a simulation is committed,
use its Cancel control on Runs. It is a separate numerical job, not an agent call.
Cancelling or killing the whole server can prevent receipt of final usage; that is
why reservations remain counted. Close the old launch windows before upgrading.

## Provider reference contracts

Implementation references (reviewed for this release):
- OpenAI Structured Outputs: https://developers.openai.com/api/docs/guides/structured-outputs
- Anthropic structured outputs: https://platform.claude.com/docs/en/build-with-claude/structured-outputs
- Anthropic cache usage: https://platform.claude.com/docs/en/build-with-claude/prompt-caching

A schema-constrained response is still not scientific validation. PhaseForge also
checks the returned manifest or Studio design against the trusted runtime before
persistence/execution. Native geometry checks and PCB DRC have their own reports
and do not certify scientific claims or electrical behavior.
