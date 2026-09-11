# Recorded field viewer and illustration integration

Implementation and component evidence, 2026-09-11. This does not replace the installed ordinary-chat acceptance gate.

The laboratory selector now includes numerical solver results and explicit scientific illustrations. Selecting an illustration passes its job ID as chat context, subject to the same project and visible-view checks as a numerical run. Its view loads the full immutable request, shows the actual Cycles PNG, offers the saved GLB for 3D inspection, and downloads PNG, GLB and editable BLEND artifacts. Preview edits are separate presentation revisions; a newly requested Blender still is a separate render attempt.

GLB camera conversion accounts for both Blender's scene normalization and glTF's Y-up export. Camera position, target and up convert back to authored source coordinates. Stable `phaseforge_node_id` extras identify highlighted components. A source-level check loaded the actual blue conceptual GLB from the illustration component acceptance job, changed color and contrast, highlighted its authored node, and returned camera position `[3,-5,2]`, target `[0,0,0]`, up `[0,0,1]` and focal length 50 mm. The preview uses viewer lighting; the stored PNG keeps Cycles lighting. They are explicitly separate views.

The diffusion viewer reads `fields/index.json` and committed JSON views, checks each view SHA-256, and preserves row-major `[y,x]` cell orientation with low y at the bottom of a front view. It uses an unlit nearest-cell texture and one fixed numerical scale across time. Selected cells report actual values, time, physical cell centers and units. Palette and contrast edits apply identically to the view and legend; they do not alter field values. Timeline endpoints use the exact retained binary values through the shared integer slider. Physical simulation seconds, wall-clock execution time and user-selected playback duration remain separate.

Field display memory uses a bounded cache and serialized file loads. Superseded queued scrubs are skipped. Committed numerical files remain on disk. The live viewer refreshes newly committed frames; completed results stop polling the field index. Presentation changes use the existing revision/patch/operation-ID writer and have undo/redo. Reconnect notices preserve an already loaded view.

The field MP4 worker reads both authoritative `.npy` arrays and their JSON views, checks committed hashes, and requires exact agreement between them before rendering. It runs no solver. Blender uses Standard color management, an emission texture, nearest-cell sampling, and a fixed quantitative legend. User resolution, frame rate, physical time window, playback duration, camera and palette are retained in the export receipt. A paused export restarts as an independent attempt through the existing durable export API. Only completed MP4 results are downloadable.

## Evidence

- `frontend/tests/laboratory-field.test.mjs`: physical orientation, exact recorded endpoints, fixed palette, checksum failures, cache limits and superseded-load behavior.
- `frontend/tests/illustration-coordinates.test.mjs`: source/GLB coordinate round trips and inherited stable node IDs.
- `frontend/tests/workbenchRequests.test.mjs`: illustration context cannot cross projects or leak from another view.
- `tools/test_field_render_contract.py`: independently checks exact retained-state selection, mismatched `.npy`/JSON rejection, digest rejection and explicit sRGB values without importing Blender or the numerical solver.
- `.local/viewer-acceptance/field-illustration-original.png`: actual saved asymmetric 32 × 48 field and actual authored Blender GLB, rendered through current Three.js modules in a read-only harness.
- `.local/viewer-acceptance/field-illustration-presentation.png`: saved final field with teal/magenta palette, contrast, tilt and pan; orange GLB with stable-node highlight and changed source camera.
- `.local/render-acceptance/field-fourier-32`: actual 32 × 32 diffusion record, 0–2.56 scientific seconds; 1280 × 720 H.264, 6 fps, six encoded frames, one playback second. Independent FFprobe frame count and full FFmpeg decode passed. First/final PNGs were visually inspected and show numerical mode decay with a fixed range.
- `.local/render-acceptance/field-asymmetric`: actual asymmetric numerical record, 0–0.08 seconds; 1280 × 720 H.264, 2 fps, two encoded frames, one playback second. The explicit teal/magenta palette with contrast 1.2 was checked independently at all 1,536 physical cell centers in each endpoint PNG. All 3,072 cells were within one sRGB byte per channel of the expected color, with low y displayed at the bottom. The independent projection/pixel comparison is saved as `independent-pixel-check.json`. Independent full video decode passed.

These short export checks establish rendering/encoding behavior for saved numerical states. They are not claims of additional scientific time, predictive biological accuracy, or a new solver validation. Full installed-chat checks and release distribution remain separate gates.

## Field viewport follow-up

Installed inspection found that an absolute full-panel canvas let the scientific field extend behind the playback controls. The field now uses a grid with separate rows for header, camera controls, numerical viewport, fixed legend and playback controls. The renderer measures the viewport row for camera fitting. The legend has an opaque background, readable endpoint values and explicit units. PNG capture appends its annotation strip below the complete rendered viewport instead of masking its lower cells; its metadata includes viewport dimensions and annotation placement.

A read-only harness using the actual 64 × 64 diffusion fixture, current CSS and current Three.js module checked 420-, 520- and 900-pixel panes. In all three layouts the canvas, legend and footer did not overlap, and the controls stayed inside the viewer. Evidence is `.local/viewer-acceptance/field-layout-report.json`, the three `field-layout-*.png` images and `field-capture-below.png`. The captured image was visually inspected. This source-level layout check does not claim the new layout was installed. The frontend production build passed after the change.
