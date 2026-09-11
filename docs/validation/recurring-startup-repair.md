# Recurring desktop startup exception: reproduction and repair

Verified 2026-09-11, 19:41–19:50 UTC. The original installed desktop application was left running and its profile was not reset. This acceptance uses the actual production frontend in a separate Electron diagnostic host, copied desktop state, and read-only access to the existing backend. It precedes installation of the corrected package.

## Cause

The copied desktop profile restores project `6adabf78-2ee6-472d-9b5d-e0727e4a9aa5`, **visualize an electron's wave state collapsing**. Its completed legacy run `9ad950eb-b44d-415c-ae12-ac3bb5a5687c` retains 251 frames from 0 to 4.999999999999938 model-time units. Rendering that run enters GenericViewport's conditional playback footer.

That footer referenced `TIMELINE_SLIDER_MAX`, `timelineSliderValue`, and `timelineSliderTime` without importing the existing timeline helpers. The old installed production bundle throws:

```text
ReferenceError: TIMELINE_SLIDER_MAX is not defined
    at D (http://127.0.0.1:7332/_next/static/chunks/1rwp99fdas5na.js:1:16630)
    at l8 (http://127.0.0.1:7332/_next/static/chunks/2it4nvh-zn7an.js:1:84171)
```

Next then displays its application-wide client exception page. The previously prepared 0.9.0 source and bundle also contained this defect, so that installer was held for replacement. A fresh browser profile opening CAR-T did not enter the same saved-frame footer and therefore did not reproduce the failure. The earlier live CAR-T incident had no original stack; this evidence specifically identifies the recurring saved-project startup failure.

## Fix and regression evidence

GenericViewport now imports all three helpers from `@/lib/timeline-slider.mjs`. The numerical run, scene geometry, solver state and saved profile are unchanged. No dependency was added.

`node --test tests/generic-viewport.test.cjs tests/timeline-slider.test.mjs` passed four tests. The component regression executes the actual JSX through React with retained frames so its conditional footer runs; it also checks the empty state. The endpoint tests preserve exact retained endpoints despite floating-point roundoff. The production frontend 0.9.0 build exported all eight routes successfully.

Two fresh diagnostic hosts each copied the original profile snapshot and loaded a frozen copy of the rebuilt production UI. Both performed this sequence:

1. Restore the electron-wave project and its actual 251-frame viewer.
2. Press End in the real simulation range input: its value becomes 10000 and the final saved time displays as 5.
3. Use the actual sidebar to open T Car therapy, then Blackhole Simulation and its separate legacy trajectory.
4. Return to the electron-wave project and its working viewer.

Both runs completed with no JavaScript exceptions, console errors, error-boundary fallback, attempted API mutation, or watchdog expiry. All 193 frozen rebuilt UI files match the production output by SHA-256. The retained legacy run, read again afterward, is unchanged. Both test hosts exited normally; original installed main PID53444 and backend PID7396 remained alive with their original start times. No model request, numerical run, render job, or new backend was started.

## Preserved receipts and limitations

Evidence is in `.local/viewer-acceptance/startup-crash-20260911-1934/`:

- `snapshot.json`: exact old installed UI/app.asar plus local/session storage and preferences, file hashes and original-read stability checks; 202 files copied with zero unstable reads. Only runtime lock files were excluded.
- `copied-profile03/events.json`, `page-state.json`, and `page.png`: successful old-bundle reproduction with complete exception stacks and the generic error page.
- `repaired-profile/` and `repaired-profile02/`: two successful fresh-copy replays, five DOM/image checkpoints each, read-only API logs and process identities.
- `repair-assessment.json`: source/build hashes, both acceptance results and retained-run equality check; `pass: true`.
- `copied-profile/setup-failure.json` and `cleanup.json`: the first diagnostic host stalled during debugger setup before navigation. Only that separately created fixture process was cleaned up after checking its exact command and creation time. The corrected host has an unconditional watchdog before debugger startup.
- `copied-profile02/`: an invalid setup attempt omitted the desktop HTML flag and hit the wrong API-origin CSP. It is preserved and excluded from application conclusions.

The corrected package still requires normal installation and a native startup check. These isolated-host results do not establish native installed performance. The original application's profile was not cleared, its processes were not stopped, and no rejected installed-process action was retried.
