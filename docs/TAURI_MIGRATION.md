# Historical Tauri proposal — superseded by the 0.8 desktop implementation

PhaseForge 0.8 uses **Electron**, a bundled static Next.js interface and a managed
native Rust engine. Tauri is not a current dependency, packaging target or required
migration step. This file remains to keep older documentation links meaningful.

The current desktop server uses a stable loopback UI origin, proxies requests to
the local Rust API, and keeps Node privileges out of the sandboxed renderer.
The app manages its owned engine, preserves background work through its tray menu
where available, and bounds unexpected engine restart attempts.

The original proposal's useful architectural requirements remain:

- Keep window lifecycle code separate from scientific execution and persistence.
- Preserve immutable manifests, run evidence and the OS credential abstraction.
- Keep SQLite authoritative; browser preferences are not the research database.
- Never grant model output arbitrary shell, filesystem or process authority.
- Test packaged credential storage, native file operations and lifecycle behavior
  on every claimed platform.
- Treat signed updates and any alternative desktop framework as future work,
  without rewriting stored scientific provenance during updates.

Use [architecture](ARCHITECTURE.md), the [Windows guide](../README.windows.md),
the [Linux guide](../README.linux.md) and [roadmap](ROADMAP.md) for current plans.
