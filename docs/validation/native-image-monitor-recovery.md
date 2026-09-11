# Native image ordering and monitor recovery

Source checks on 2026-09-11 use temporary databases, retained PNG fixtures and loopback HTTP. No installed app, external provider, scientific worker or renderer was started by this batch.

Canonical tool input now places every result for a model response before its user image or text blocks. The same normalization handles older journals with interleaved images, partially completed tools and steering that supersedes outstanding proposals. It preserves image bytes and evidence text, keeps separate model turns separate, and does not change archived compaction source. Missing results or conflicting call identities stop dispatch instead of producing a malformed native request. Completed receipts remain completed; normalization does not re-execute their tools.

Newly acquired images also enter a durable image outbox. Compaction preserves that outbox and carries its exact native image/provenance inputs after the summary until a native model response acknowledges them. A summary is not image delivery. Native dispatch persists `native-request-<usage_id>.json` with exact image input blocks, selected provider/model/effort, source IDs, paths, SHA256 and image count. Completed native response acceptance writes `native-vision-ack-<usage_id>.json` and a deduplicated `vision_input_acknowledged` event with the matching request receipt and provider response identity. OpenAI background polling retains those pending identities. Cancelled or uncertain responses do not clear pending images. These records prove input delivery; they do not establish scientific validity or the quality of an image inference.

The scheduled monitor checks durable user updates both before its first wait and while waiting. It returns a saved `yielded_for_user_update` result with an unknown detection outcome, leaving the numerical source unchanged. A model can then address the update or request a new observation policy. Parent state, cancellation and deadline checks apply to admission and publication under the same service gate used by Stop. Saved acquisitions recover their exact frame and matching hash only while the session and monitor are active; paused, cancelled and timed-out records are not automatically revived. A completed acquisition is returned without resampling or adding another completion event.

The new `render_observation` tool requests a separate durable camera view of an exact retained scientific state. The model must wait for the observation job, inspect its renderer receipt and pass `first-frame.png` through `observe_frame`. Additional-view pixels are unavailable to the tool until the observation completes validation. The tool instructions distinguish this additional view from the original checkpoint image and a scientific rerun. Actual rendering validation belongs to the observation worker's separate checks.

`cargo test --lib agent::laboratory -- --nocapture`: **46 passed, no skips**. New checks include:

- Native OpenAI and Anthropic request bodies with two tool results preceding both actual image content blocks; exact content retained.
- Multi-image legacy replay, partial recovery, steering ordering, stable serialization and refusal of incomplete/conflicting dispatch.
- Monitor updates saved before and during waiting, with unchanged source job and no fabricated frame.
- Paused, cancelled and timed-out monitor/parent combinations remaining unchanged, plus refusal of expired admission.
- Active saved image recovery exactly once, without resampling; a cancelled token cannot continue it.
- Full Anthropic cached native-tool replay, two retained PNG acquisitions, context compaction and final continuation: both exact PNG byte strings reach the native image body after compaction, with the selected model/effort and matching request/acknowledgement hashes. Replaying the saved final response without credentials adds no generation or acknowledgement event.
- Anthropic summary dispatch receipt recovery without credentials or another summary. The original source and pending image outbox remain intact; only a later native vision response may acknowledge the images.

Earlier receipt, compaction, team, attachment and steering tests also passed in that command. Installed UI and live-provider behavior remain separate acceptance evidence.

`cargo test --lib agent::model_provenance_tests -- --nocapture`: **7 passed**. The legacy chat endpoint now rejects a missing, empty or whitespace-only model before creating messages, usage or provider calls, even when former Settings defaults exist for both providers. The separate provider credential-test endpoint retains its explicitly scoped connection-test semantics. The suite also covers selected-model repair, unavailable-model refusal, Explain cache provenance, verification, Discovery, legacy research tasks and Studio. Actual picker/navigation request interception remains an installed UI acceptance step.
