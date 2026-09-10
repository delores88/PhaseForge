# Read-only telemetry and workflow progress

CPU/RAM samples are collected in a retained sysinfo 0.30 System every approximately
2 seconds. CPU is a between-sample value; the first snapshot is warming_up. RAM
pressure is total minus available memory. Backend process memory/CPU is separately
labeled, excluding the browser and external engines. The CPU percentage is normalized
to total logical CPU capacity. Counters are observational, not per-run accounting.

GPU probing runs independently at about 6 seconds plus driver-query duration. A
probe has a 4-second timeout and kill-on-drop. Commands and fields are fixed,
read-only and contain no model/user shell fragments. No installation or elevation
is performed by the monitor. A bounded 150-sample history stays in memory.

NVIDIA: query device UUID/name/utilization/dedicated-memory usage+capacity/temperature
through nvidia-smi. MiB are converted to bytes. N/A remains null. This provider does
not claim other-vendor or per-process VRAM coverage, especially under Windows WDDM.

Windows fallback: fixed PowerShell CIM query for WDDM engine and adapter-memory
counters. It reports a busiest-engine device aggregation, not a fabricated overall
percentage, and labels adapters by LUID. Reliable total dedicated capacity is not
inferred; unknown capacity means unknown VRAM percentage. Counter support varies.

Linux AMD fallback: DRM sysfs gpu_busy_percent, mem_info_vram_used and
mem_info_vram_total where the driver exports readable values. Detection is not proof
that a particular solver is using that GPU. Multi-vendor coverage is best effort:
a successful NVIDIA probe currently owns that sample; it does not merge AMD data.

The browser polls lightweight workflow/telemetry snapshots without overlapping
requests. Timeout/stale samples are labeled, not shown as live zeros. Progress
updates do not repeatedly download full trajectories. Completion fetches the
finished evidence package. Workflow stage percentages are supplied only for the
numerical work plan. No fake overall generation ETA is shown.

## Sources used in implementation review

- NVIDIA System Management Interface reference:
  https://docs.nvidia.com/deploy/nvidia-smi/index.html
- Microsoft GPU-process counter accuracy caveat:
  https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/gpu-process-memory-counters-report-wrong-value
- sysinfo changelog (0.30 API retained; 0.31 changed refresh/process APIs):
  https://github.com/GuillaumeGomez/sysinfo/blob/main/CHANGELOG.md
- Three.js OrbitControls:
  https://threejs.org/docs/pages/OrbitControls.html
- Three.js GPU resource cleanup:
  https://threejs.org/manual/en/cleanup.html

No Windows-driver, Linux-AMD or native GPU test was performed by the packaging
environment. Device readings must be verified on the target machine. Telemetry
is never included in a provider prompt by these endpoints.
