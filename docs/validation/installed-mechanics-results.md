# Installed mechanics acceptance: execution ledger

The scientific criteria remain in
[the frozen proposal](mechanics-lab-proposal.md) and
[installed preregistration](installed-mechanics-preregistration.md).
The separate [23-group component acceptance](mechanics-lab-results.md) passed.
The installed gate has **not passed** as of the attempt recorded below.

## Fourth checkpoint: input preservation failure before integration

On 2026-09-11 the authorized Stage A request was submitted through the normal
installed project/chat API at port 58021. It created project
`74f134cd-d2c7-455c-8d30-669b25268fab`, session
`c1df3457-c4cc-4d80-93b3-5b525cbc4fe7`, and proposed control solver
`5053252b-ac84-459d-a8a3-eadf8aeabbb0`.
The model was `open_ai` / `gpt-5.6-sol`, medium effort, Timer Off for the finite
request. Backend SHA-256 was
`62aaac8fbf4495aef9329888bc82041ea4c7f262c6208cf35be3d2c3d3693139`;
mechanics worker SHA-256 remained
`455056ac9bdf12cba80a9b86699970cf92939432e1177841b936838913c1d125`.
The acceptance plan was
[preregistration-v2/plan.json](../../.local/science/installed-mechanics/preregistration-v2/plan.json),
SHA-256 `e554fd449a38370741a8a2a368929794711ee38f54aa862a4e26ea91dc321997`.

The model's raw provider function arguments contained the correct frozen decimal
`timestep: 0.0015707963267948967`. The server's saved `input.json` instead contained
`0.0015707963267948969`, one binary floating-point increment higher. The external
acceptance observer rejected that input difference and paused the parent and child
during environment preparation. This was a JSON parsing/storage defect, not a model
copying error, measured trajectory failure or justified timestep adjustment.

[The retained comparison](../../.local/science/installed-mechanics/stage-a-20260911/parameter-roundtrip-failure.json)
contains the exact raw argument string, saved input and both byte hashes. The
provider receipt hash is
`b5eb31f3149531e62f304c7cc6fc815d888fc552f0ec235e71fcaf28a984c7c4`;
saved input hash is
`e907992f7d3424a0e9c6692269f75b2ac979f22afe214ae53160c09d0e2e675e`.
At this checkpoint `serde_json` lacked its `float_roundtrip` feature. A repaired
build must preserve the requested float before a new immutable scientific attempt;
the original paused input must not be rewritten.

No `solver_started` event, numerical trajectory, checkpoint, or scientific state
was produced for this control. This attempt therefore does not count as the
planned intermediate scientific pause/resume test. Environment readiness activity
is separate. The numerical checker was not run against nonexistent results.

The session initially waited locally for the configured single model-call slot;
its queue receipt explicitly stated that no provider request had yet been sent.
It later obtained the slot and completed two provider responses. The parent had
started another model request by the time the observer paused it; the final receipt
says that request was interrupted and remote usage may remain billable. No
zero-cost claim is made.

[The final drain snapshot](../../.local/science/installed-mechanics/stage-a-20260911/drain-snapshot.json)
shows both session and solver paused. The solver has a `worker_stopped` event;
the external controller exited and retained its
[failure receipt](../../.local/science/installed-mechanics/stage-a-20260911/controller-failure.json).
No replacement solver or reviewer variant was launched.

## Native observation limit

A read-only Windows UI Automation capture found the PhaseForge window titled
“Application error: a client-side exception has occurred.” Its accessible document
reported that error while loading `127.0.0.1`, with no workspace controls exposed.
The screenshot capture appeared to show the foreground occluding application,
so it is not credited as a PhaseForge image. No native clicks were performed.
Navigation-away/back and numeric playback acceptance remain pending after UI
recovery; API availability alone does not pass them.

## Preparatory checks, separate from installed success

The read-only installed checker was exercised against existing component results:
[controlled pair](../../.local/science/installed-mechanics/component-fixture-audit2/report.json)
and [previous tilted/translated fixture](../../.local/science/installed-mechanics/reference-geometry-component.json).
Both passed without new solver calls. The checker also rejected a changed running
checker against its older frozen plan, preserving
[that expected refusal](../../.local/science/installed-mechanics/expected-changed-checker-refusal/report.json).
These checks validate the auditing path; they are not installed execution evidence.

After a separate OpenMM startup problem was discovered, actual field/mechanics
two-step processes were run in backend-like directories containing full provenance
input, narrow worker input, adjacent pinned requirements, logs and completed
readiness subdirectories. All four readiness/study subprocesses passed and the
preseeded source/input bytes remained unchanged. The
[preseed contract report](../../.local/science/supervisor-compat/field-mechanics-preseed-20260911/report.json)
records field Fourier error `2.22e-16`, zero integral drift, and mechanics circular
position/velocity errors `1.67e-10` / `8.33e-11` at two steps. No field or mechanics
worker edit was needed. This did not test native JSON parameter preservation, which
the fourth-checkpoint attempt then correctly caught.
