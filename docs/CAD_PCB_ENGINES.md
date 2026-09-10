# Native CAD and PCB engines

PhaseForge runs the fixed `tools/fabrication_worker.py` against a validated
declarative design. CAD jobs use CadQuery/OpenCascade Boolean solids and retain
STEP and STL files. PCB jobs use KiCad's board parser, footprint serializer,
design-rule checker and native exporters. Generated Python is never executed.

These native programs are optional local installations, not bundled into the
PhaseForge desktop installer. Studio design generation uses the chosen AI provider;
building engineering files from a saved design runs locally without another model
call. The engine badges report discovery; a job's output records its actual checks.

## Supported designs and files

CAD specifications use millimetres: boxes, cylinders, spheres, cones and extruded
polygons, with rotations, translations, Boolean addition/subtraction and an optional
global fillet. CadQuery/OpenCascade must produce a valid positive-volume solid.
The job retains `model.step`, `model.stl`, bounds, volume and solid count. These are
geometry checks, not structural analysis, material qualification or proof of fit.

PCB specifications support rectangular two-copper-layer boards, explicit nets,
custom component pads, tracks, mounting holes and silkscreen. Outputs include the
editable `board.kicad_pcb`, a local footprint library and BOM. When KiCad is available,
the worker adds native DRC, an SVG preview and a board-only GLB. It is a board-layout
exporter, not a SPICE simulator or schematic-authoring/electrical-validation engine.
The full data contract is [the fabrication schema](fabrication.schema.json).

## Local CAD runtime

Use a standalone CPython 3.12 environment and install the pinned requirements:

```powershell
python -m venv "$env:LOCALAPPDATA/PhaseForge/engines/cadquery-cpython"
& "$env:LOCALAPPDATA/PhaseForge/engines/cadquery-cpython/Scripts/python.exe" -m pip install -r tools/requirements-cad.txt
& "$env:LOCALAPPDATA/PhaseForge/engines/cadquery-cpython/Scripts/python.exe" tests/test_fabrication_worker.py
```

The first `python` must be the intended standalone interpreter. Discovery prefers
`cadquery-cpython` over the older `cadquery` environment. Linux uses the equivalent
`~/.local/share/PhaseForge/engines/<environment>/bin/python` paths.
`PHASEFORGE_CAD_PYTHON` explicitly overrides discovery. Optional engine wheel
availability must be checked on each operating system and architecture; the
Windows x64 validation below does not establish ARM or Linux native execution.

Run these commands from the source checkout; the requirements file is
[tools/requirements-cad.txt](../tools/requirements-cad.txt). Do not substitute an
unverified wheel set just because imports succeed: the tests also verify native
process exit and exported geometry. General installation choices are described in
[CadQuery's official guide](https://cadquery.readthedocs.io/en/latest/installation.html).

The Windows CasADi 3.8.0 and NLopt 2.11.0 wheels can crash during native teardown
when loaded together. A fresh interpreter alone did not fix this. This was
reproduced independently of CAD geometry with `import casadi; import nlopt`.
The pinned CasADi 3.6.7 wheel uses a different SWIG runtime table and exits cleanly
alongside NLopt 2.11.0 in the tested environment. See the upstream
[CadQuery issue](https://github.com/CadQuery/cadquery/issues/1911).
The worker does not alter native destructors or turn a failed process into a
successful job.

## KiCad footprints and manufacturing gates

Install [KiCad from its official distribution](https://www.kicad.org/download/).
Set `KICAD_CLI` to `kicad-cli` if it is not found on `PATH` or in supported Windows
installation directories. Restart PhaseForge after changing the environment.
The Python runtime above needs `jsonschema` even for PCB-only work. KiCad's matching
Python bindings are also required for footprint normalization; finding only the
CLI does not establish a working export pipeline.

Generated boards carry a local `PhaseForge.pretty` library and `fp-lib-table`.
KiCad's Python bindings parse the board and save normalized footprint copies,
including rotated components and mounting holes. The matching library avoids
format-default differences between hand-written board and library text.
Footprint names are bounded identifiers and library files stay within the job
folder. The bundled KiCad `bin/python.exe` is used on Windows; other platforms
need Python with KiCad's `pcbnew` bindings available.

Native DRC runs without suppressed checks. Any reported violation, warning,
unconnected item or schematic-parity issue blocks the manufacturing archive.
Editable boards, DRC reports and previews remain inspectable. A passing board
gets a ZIP containing native Gerbers and drill files. Passing these checks does
not validate electrical behavior or replace review of component ratings and
schematic parity.

When no KiCad CLI is found, editable board data can still be generated,
but DRC is recorded as not run and no manufacturing archive is produced. With
KiCad, `editable-project.zip` retains the board and library even when DRC reports
issues; `manufacturing.zip` is gated on the actual zero-issue result. A native
process failure remains a failed job even if some files were written.

## API and job limits

`POST /api/studio/fabrications` accepts `project_id`, `design` (the fabrication
object, not the outer Studio design record), and optional `max_seconds` (default
300, range 5–1800). It immediately saves a queued job. One fabrication job runs at
a time, with at most eight active/queued jobs; queueing consumes the original
deadline. Native processes have a 4 GiB host-memory budget and memory-pressure
monitoring, with Windows process-tree enforcement.

Use `GET /api/studio/fabrications?project_id=UUID` to list jobs,
`GET /api/studio/fabrications/UUID` to read status/results, and
`POST /api/studio/fabrications/UUID/cancel` to stop queued or active work.
`GET /api/studio/fabrications/UUID/artifacts/NAME` streams an allowed artifact.
Jobs retain `request.json` and `result.json`; inputs and available files are stored
under the app data directory at `artifacts/fabrication/JOB_UUID/`.

Saved states are `queued`, `running`, `completed`, `failed`, `cancelled` and
`interrupted`; a cancellation response can briefly report `cancelling`.
After restart, active jobs become interrupted when loaded. Start a new job to
rebuild the retained specification; native partial-work continuation is not
implemented. Revisions through [Studio](public-research-and-studio.md) can include
the saved diagnostic report without overwriting the original design.

## Validation on this development machine

Validated on Windows x64 using standalone CPython 3.12.14, CadQuery 2.6.1,
OpenCascade bindings 7.8.1.1.post1, CasADi 3.6.7, NLopt 2.11.0 and KiCad 10.0.6:

- Seven fabrication tests passed; the suite process exited with code 0.
- A 40 × 30 × 3 mm plate with four 3 mm holes exported STEP/STL with volume
  3515.176998353075 mm³. STEP reimport preserved the solid and expected volume.
- A separate real worker invocation produced those artifacts and exited 0.
- Connected PCB, 37° rotated footprint and mounting-hole cases passed native
  DRC and produced Gerber/drill archives with matching editable libraries.
- An intentionally disconnected board retained its DRC findings and produced
  no manufacturing archive. Unsafe footprint names were rejected.

The process-exit regression specifically rejects a native shutdown crash even
when the output files and geometry assertions look correct.
