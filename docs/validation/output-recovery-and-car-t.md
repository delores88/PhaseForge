# Output-cap recovery and the user's CAR-T illustration

The installed fourth checkpoint retained the user's original question in project
`8967071e-7425-4c8e-b283-09e76cb34338`: a detailed molecular/cellular illustration
of “tcar therapy” for medical communications. The model explicitly interpreted
this as CAR-T. No solver ran; it used Blender for the conceptual scene.

Sessions `d3e94f40-d5a6-4787-bdf3-8131a28cdeb6` and
`a5002c95-487d-4ac1-ab85-9c78db712a27` paused after exhausting the 12,000-token
output cap. The first failure had 4,817 input tokens and 12,000 output tokens,
including 7,768 reasoning tokens. Its render call arguments were truncated.
This is output exhaustion, not context overflow. The old recovery path retained
the pending receipt but repeatedly rejected it when resumed.

The third session `59607b5d-b5e4-4f2e-9b05-c486d1d021ab` completed at
2026-09-11T18:32:17Z, using the user's selected OpenAI/Astra/max settings.
Blender job `60f4d832-cc44-4ea5-af5a-b40f56796e7f` produced a 2048×1152 image,
an editable Blender scene and GLB. Raw PNG SHA256:
`51568dd8e885a0623924114ff77a5586f5755d5035f15bf0d7ab6516f3ccd727`.
The subsequent isolated annotation job
`93c5db8e-a6c7-4771-badf-7becc237981e` saved:

- `work/car-t-therapy-labeled.png`, SHA256
  `22f3eb23126796a9ead23c794d75fe218ab16a87d03f3d975bc6f5caffe789b0`.
- `work/car-t-therapy-labeled.svg`, SHA256
  `5fa5e490d9c98fadae6884dba6940de5f7f0ab6bfe68c03774fcf5c17ca268d3`.
- `work/figure-caption.txt`, SHA256
  `2e19c04971d5e7665bc6b1209a25a4b258620fcf0fbf23dc1ab7d2ca04ec304d`.

Root visually inspected the final PNG: four mechanism steps, cellular labels and
a receptor close-up are present. The graphic identifies itself as conceptual,
simplified and not to scale. This observation checks the saved output, not its
suitability for clinical communication or predictive biological validity.

The user also reported a native client exception. The original installed window
showed the generic Next error while the backend stayed healthy. The old desktop
did not retain a renderer stack. A fresh diagnostic browser loaded the exact
installed bundle, CAR-T project, PNG, GLB and activity without that exception.
The underlying crash is not yet reproduced; new error containment/diagnostics
must not be described as a confirmed root-cause fix. Completed generated images
were also excluded from the normal viewer: their discoverability is being fixed.

## Source repair, awaiting installed acceptance

Output-cap responses are identified by the provider's actual reason, not every
incomplete status. They are consumed once into a durable recovery journal; no
tool item from an incomplete response is executed. Bounded retries use the
configured repair allowance and preserve model, reasoning effort, output cap,
remaining time and spend controls. Exhaustion pauses for explicit continuation.
An execution reservation protects journal updates from concurrent Resume clicks.
Inspecting a paused specialist cannot reset its repair allowance.

For a large authored scene, the model can construct data-only JSON with concise
code inside the existing isolated runtime, then supply a registered
`scene_source` artifact to Blender. The renderer checks project ownership,
completed state, registered bytes, SHA256, size and scene schema before copying
the resolved scene into immutable render input. Generated Blender Python is not
executed by this route. This supports detail without a huge inline tool payload.

Eight output-related Rust tests passed, including actual local HTTP receipt
recovery, explicit continuation, concurrent late resumptions, repeated specialist
inspection and contradictory incomplete tool items. The scene-source ownership/
hash/schema test and standalone API lineage/budget test also passed. The first
manual-recovery test fixture initially left a usage reservation open and waited
for its own provider slot; its harness was corrected to finish the saved usage
receipt, matching the real acceptance path. No installed recovery claim is made
from these source tests alone.

## User clarification after inspecting the plate

The user clarified that the desired deliverable is realistic scientific 3D
CAR-T imagery at the appropriate cellular/molecular scale, with useful labels.
The communications audience did not request the headline and step-card layout.
The third-session graphic remains retained evidence of actual Blender and
annotation execution, but does not fulfill that clarified scientific 3D intent.
Realistic appearance alone is not structural or biological validation; any
structure-specific detail needs verified retained data, and authored geometry
must remain identified as conceptual. The ordinary-chat revision and intent
switching checks are preregistered in
[installed-intent-routing.md](installed-intent-routing.md); they have not yet
been executed against the next installed checkpoint.
