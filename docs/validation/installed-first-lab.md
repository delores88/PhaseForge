# Installed molecular lab acceptance

The first checkpoint installer is a local development build, not a marketplace release.
Installer SHA-256: `db5e322463c4cb28f838a89c60b072153b3898308775e54e50d17d8d7b8a7664`.
Installed build receipt: Windows x64, source c3843dc1 plus working changes, built 2026-09-11T14:18:02.603Z.
It must not be presented as an immutable clean-source release.

## Frozen ordinary-chat study

Through the installed app's normal chat, request a real OpenMM periodic argon-like
Lennard-Jones fluid comparison: 108 atoms, density 0.8 g/cm³, CPU platform,
Langevin friction 5/ps, timestep 1 fs, 10,000 steps (10 ps), temperatures 90 K and
180 K, seeds 812 and 813 respectively. Record at least 100 frames for each run.
Request late-half mean temperature, pressure, MSD and RDF, an actual image
observation cross-checked against numerical output, and clear limitations.

Independent numerical tolerances are those frozen in molecular-lab.md:
each final-half mean temperature within 15% of target and the hot mean exceeds
the cold mean by at least 60 K. Recompute these directly from the app's recorded
measurements. Compare numeric coordinates with recorded/rendered frames.
The worker's separate independent force/energy checker remains the numerical
engine reference; these app runs do not establish empirical argon validity.

Observe navigation to other project/views during the request, durable activity,
completion/read state, detailed playback and camera controls. Demonstrate
interruption at a named in-flight stage and compatible checkpoint/receipt
recovery. Preserve failed attempts; fix defects before declaring the checkpoint passed.

Status: ordinary-chat numerical execution and actual image observation passed;
interactive visual quality and interruption recovery are still pending.
Existing closed-app backup: `%LOCALAPPDATA%/PhaseForge/backups/before-alpha-20260911T142435Z`.
It contains 397 verified files (441,152,415 bytes), including the desktop profile.
The previous installed program is separately retained under
`.local/backups/installed-7471ff72`.

## Observed installed execution

Project `bd5f2ac7-9f57-4eb2-9960-da96695b66fb`; session
`3293f233-7dba-41a2-b638-77ecad4fa9ab` used the visibly selected
`open_ai / gpt-5.6-sol / medium`, with a 900-second maximum. The question was
entered and submitted through the installed app's normal chat. It automatically
created the managed Python environment, installed the pinned packages, performed
an integration smoke test, and executed two fresh OpenMM jobs.

- 90 K: `4f1f5a10-46b8-47fc-a80b-ccca34bd52c4`, late mean 88.342795 K.
- 180 K: `16fd7fbe-05bc-40b9-aebf-8bc40dfed372`, late mean 183.768614 K.
- Each retained 201 states over 10 ps. Actual solver time was about 3.8/3.6 s;
  chat planning, provisioning and interpretation have separate durations.
- The read-only checker `tools/check_installed_molecular_lab.py` passed the frozen
  temperature comparison and independently recalculated pressure, MSD and RDF
  from every retained state. Maximum pressure disagreement was <1.8e-11 bar;
  MSD matched exactly, as did RDF pair counts. Numerical arrays and wrapped
  viewer coordinates match exactly, with valid chunk/image hashes.
- Full report: `.local/science/installed-first-lab/numerical-report.json`.
- The model called `observe_frame` twice for the actual final PNGs. Native image
  inputs and SHA-linked evidence were retained in its journal. Its final answer
  explicitly separated projected appearance from trajectory-derived displacement
  and did not claim empirical or biological validation.
- A context compaction occurred before those observations; the continuation
  preserved both run identities, constraints and the pending image review. It
  did not launch a duplicate solver.
- Navigation to Settings succeeded during model/solver work. The job spinner
  remained visible. Two completed solver dots appeared without forcing navigation.
  Opening the hot run restored the study and numerical viewer. Playback traversed
  the actual retained states to 10 ps in the selected 30-second presentation.

The visual inspection failed the quality gate: the installed checkpoint's initial
camera crops the box in a narrow pane and sphere surfaces show triangular rendering
artifacts. Preserve this failure; rerun visual acceptance after fixing the viewer.
This first workflow is **not yet fully passed**.

## Recovery extension, frozen before submission

Use the same ordinary chat and request one fresh 108-atom run at 120 K, density
0.8 g/cm³, CPU, Langevin friction 5/ps, seed 9217, 1 fs, 100,000 steps (100 ps),
sample interval 20 and chunk size 50. This retains 5,001 computed states.
Interrupt the installed backend after a committed solver checkpoint exists and
before completion. Its process-owned child must terminate. Reopen/reconnect, then
resume the same session through Activity. The same solver ID/input must recover
its compatible checkpoint, keep earlier committed chunk hashes unchanged, produce
100,000 final steps and a strictly increasing 5,001-frame timeline without duplicate
states. The absolute session deadline must remain unchanged. If the provider was
in flight, its stored remote ID must be retrieved rather than resubmitted.

This checks execution recovery and fresh numerical extent, not predictive validity.
Any restart/checkpoint mismatch or duplicate run fails the recovery criterion.

The second checkpoint was installed from SHA-256
`a5803c66523b8a2556b905a0433eec919332c50349131d72726ef6c2db21866f`
(417,864,918 bytes, uncompressed local NSIS build). Build receipt:
2026-09-11T14:52:29.138Z, source c3843dc1 plus working changes, dirty=true.
Backup before installation: `before-alpha-20260911T145425Z`, 3,561 verified files.
The corrected installed viewer now renders smooth particle surfaces and the full
cell, with the same saved numerical runs reopened. Final release packaging still
requires clean-source/exact-material evidence; these checkpoints are not releases.

## First workflow passed, 2026-09-11 15:10 UTC

Recovery session `28d19529-b4a8-49ed-bb2b-d43af0af685b` completed the same solver
`cdff7bda-8a88-480a-9d40-0afe2f2cd67d` after interruption at committed step 1,000.
The original backend and its OpenMM child terminated; Electron restarted the host,
which persisted both jobs as paused. Resume was clicked on the parent in Activity.
The saved provider response was retrieved without another generation, and the
unchanged absolute deadline remained 15:13:11.603455100Z.

`tools/check_installed_recovery.py` verifies all 13 criteria in
`.local/science/installed-recovery/report.json`: immutable inputs and original
checkpoint, identical 51-frame committed prefix, all JSON/NPZ hashes, exactly
5,001 distinct steps from 0 through 100,000, monotone 0–100 ps times, final
checkpoint, and the exact final PNG bytes in the provider's native image input.
The final late temperature was 120.354332 K and MSD 1.546687 nm². These are
numerical outputs, not empirical validation.

The user interface remained in the Blackhole Simulation workspace throughout
completion, showed blue unread dots, and reopened the recovered numerical viewer
when its sidebar job was clicked. The retained trajectory scrubs and renders
smoothly. A small final-slider rounding defect (End selected 99.98 instead of
100 ps) and a transient export-status reconnect overlay are retained as interface
follow-ups; neither changes the computed data. Camera fit and surface rendering
are corrected in the second installed checkpoint. The first integrated workflow
passes; the fresh reviewer variant is the next required gate.
