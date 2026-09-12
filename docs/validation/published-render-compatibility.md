# Published numerical data: export and observation compatibility

Completed `published_simulation` jobs now have an explicit path through the
trusted field/particle observation and video renderers. The integration does not
execute incoming HTML, expressions, Python or Blender source. It does not run a
scientific solver or upgrade a generated model's scientific validity.

Before admission and again at launch, the backend verifies the original retained
data, execution manifest, code provenance, model declarations, numerical index,
registered chunks/arrays/views and particle topology against publication
receipts. A published job must be completed and inactive. Exact model/source
metadata is carried into render settings and compared on restart. Cross-project
observations, changed data, changed models and incomplete publications fail.

The renderer independently checks admitted source/index/topology hashes and
retains `source_metadata` with the original model scope and limitations. Visible
labels say **Generated numerical data; model validity not established**. Particle
units come from the source; isolated trajectories cannot acquire a periodic box
or wrapping. Field publication uses the existing explicit micrometre coordinate
schema, arbitrary named scalar units and declared time units. Native PNG
observations select an exact retained state. Optional video interpolation remains
a declared display operation; isolated interpolation is correctly labelled linear,
not minimum-image periodic interpolation.

Publication-time capability flags in old immutable result files are preserved
when a publication tool receipt replays. Enabling a later renderer does not
rewrite those historical files.

## Actual component acceptance

`actual_published_field_and_isolated_particle_capture_and_video` was explicitly
run, rather than left ignored. It passed in **8.79 seconds**, with zero skipped
tests. The fixture uses the real trusted publisher, then supervised Blender PNG
observations and H.264 video exports for both representations. Its input is
clearly labelled synthetic numerical contract data; it does not claim a real
physical solver produced these fixtures.

| Representation | Declared units | Source publication | PNG observation | Video export |
| --- | --- | --- | --- | --- |
| Isolated particles | AU, days | `3031f58e-0a2f-4d18-a8cd-bfa378b8c726` | `2dab03de-f818-4502-a06f-694a017d0696` | `be02cbee-b94b-4c1d-91fa-b4afe0f998c9` |
| Scalar field | K, um, s | `4b91bb68-283e-4d89-a87a-ece521ab3dab` | `004edef4-572e-459b-b993-72098f3b512c` | `13cfff9f-539a-4c80-ba7a-4c9914bda468` |

Both PNGs select the retained state at time 1. Both videos contain two decoded
640×360 frames over source times 0–2. Receipts confirm exact source/model pins,
unchanged source indices and `scientific_rerun:false`. The particle display
domain is explicitly nonperiodic and retains AU/days. Actual PNG inspection
confirmed readable labels, model-validity qualification, field units, physical
time and the expected isolated particles/scalar cells.

The persistent report is
`.local/validation/published-render-01/report.json`, SHA256
`832c7fb9cb7f884211637050a1eaf873adf46da2424a5748bb84092454e0080b`.
All publication, original source, observation, export, process and image/video
receipts remain under that evidence directory.

Two Rust admission/history tests passed. Twenty Python renderer contracts also
passed: three publication-provenance checks, eleven trajectory checks and six
field checks. The first admission run identified an actual format mismatch:
source JSON integer lengths become f64 lengths in the published field index.
Admission now compares exact numeric lengths after verifying both raw-file
hashes; it does not loosen units or apply a tolerance. An earlier unqualified
Cargo invocation could not replace the running development backend executable;
the successful checks used `--lib` without interrupting that application.

## Bounds and remaining acceptance

Existing trusted-renderer limits remain in force. Published field grids use
micrometre coordinates; no arbitrary geometry, HTML animation, hidden force law
or unsupported representation is substituted. Coordinates/radii outside the
renderer bounds or a degenerate isolated particle domain can still be rejected
with an explicit error. Publication format validity is separate from physical
model validity.

This component check did not submit a paid model request, inspect images through
an LLM, install a new application or exercise the frontend buttons. Normal chat,
UI capture/model dispatch and installed-release acceptance are separate parent
integration checks. The source PNG and video paths required by those workflows
have been exercised here.

Reproduce with an existing Blender installation and a fresh evidence directory:

```text
PHASEFORGE_PUBLISHED_RENDER_EVIDENCE=<new-absolute-directory>
cargo test --locked --no-default-features --lib published_render -- --nocapture
cargo test --locked --no-default-features --lib actual_published_field_and_isolated_particle_capture_and_video -- --ignored --nocapture
python -I -B tools/test_publication_render_contract.py
```

## Source-native UI acceptance, 2026-09-12

The later integration check exercised the rebuilt 0.10.0 source application on
the isolated localhost workbench, using the repository's Electron dependency
and a separate test profile. This was **not the installed final candidate**.
The host loaded the production permission and renderer-diagnostic modules with
`nodeIntegration:false`, `contextIsolation:true`, `sandbox:true` and
`webSecurity:true`. Its helper neither clicked nor requested fullscreen;
computer-use performed the interactions manually.

The retained 51-event log corroborates three HTML fullscreen entry/exit pairs:
the particle viewer, field viewer and scientific-image viewer. Each has a trusted
DOM `fullscreenchange` entry followed by a null fullscreen element on exit.
Manual inspection reported the particle view filling the screen, the field
playing through its retained 0.4-second endpoint, and the complete blue XYZ still
remaining visible; Escape returned each to the workbench. These visual claims
come from the manual inspection, not from the event log. The host closed
normally. Its renderer log contains only the attachment record, with no recorded
warnings, errors or renderer-process exits. The original native receipt remains
under `.local/validation/workbench-010/native-ui/runs/20260912-122708-657-9c6a4c5c/`.

The normal UI **Ask AI** action on published oscillator
`bdd69aca-4fe5-4231-b5e9-0a7488156f3e` completed session
`89a011fb-b7ab-42f3-a854-2eb7c5f2f020` with the selected
`open_ai / gpt-5.6-sol / medium` and a 300-second budget. The read-only checker
decoded the actual PNG bytes in both retained native model requests and verified
them against the captured file and both completed-response acknowledgements.
The PNG SHA256 is
`528ef4d0b9eb145f4d74e01ffe5b187ad0ed00ce5462614869ff5260cdae9e69`.
Tool receipts also show reads of the retained topology, numerical trajectory
chunk and capture metadata. The final explanation cross-checked the time-zero
position `(1, 0, 0) m`, distinguished the display radius from a measurement, and
explicitly declined to infer oscillation from one still image. Byte delivery and
response completion do not independently validate every model inference.

The UI video export `5ee5b37f-7246-4e29-989c-25fe9a226479` completed as a
1280×720 H.264 MP4 with 24 frames at 24 fps: one second of playback spanning the
retained physical times 0–6.28 seconds. The worker's completed decode receipt
confirms those dimensions and frame count. The independent read-only capture
verified that the API download and stored video are byte-identical, with SHA256
`d61ae0700f65590ec169ae01f3b32302f840109991d7f6d241f860fcf8583e8c`.
It did not run another decoder or external player.

Export metadata pins match the retained source code, original data, execution
manifest, topology, numerical index and every registered trajectory chunk. All
65 source times remain strictly increasing. The export holds the previous
recorded state between selected samples, retains metres/seconds and an isolated
nonperiodic domain, and records `scientific_rerun:false`. The inspection session's
seven completed tools contain only intent resolution, evidence reads, image
observation and deliverable checking; no solver or generated computation was
started by that session. The source remains an educational analytic harmonic
oscillator, not measured data or a numerical-integrator validation.

The compact report, exact API responses, downloaded artifacts and copied native
receipts are retained at
`.local/validation/workbench-010/native-published-receipt-20260912T123613Z/`.
The report SHA256 is
`54fe9ac8ea706fa48ea8be2a21d882ea11340892f02ae350dfeefa20e8d94518`.
The capture helper used GET requests and retained-file reads only. No new model
request, scientific job, render, GUI action, build or application mutation was
performed during this evidence capture. Installed-release and process-recovery
acceptance remain separate checks.
