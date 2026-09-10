# PhaseForge 0.8.0 on Windows

PhaseForge now has an Electron desktop shell bundling the native Rust engine and
exported interface. Windows x64 is being tested locally, including numerical GPU
execution on an RTX 4090 Laptop GPU. Windows ARM64 has a native CI packaging target
prepared; it is not yet verified. No release is published. Consult the
[validation record](docs/RELEASE_VALIDATION.md) for completed acceptance checks.

## Packaged application

Local packaging produces `desktop/dist/PhaseForge_0.8.0_x64-setup.exe` on x64, or
the corresponding ARM64 installer when built on ARM64. Launch the installer,
choose the installation folder, and open PhaseForge. A packaged install includes
the core Rust engine; Rust, Node.js and a separate browser are unnecessary to run it.
Blender, CadQuery and KiCad are optional, separately installed Studio engines and
are not included in this installer.

When the tray icon is available, closing the window keeps research running. Use
the tray menu's **Quit PhaseForge** to exit. The desktop attempts at most three
engine restarts after unexpected exits. Interrupted research sessions return
paused, with saved work available for explicit resume.

Stop older backend/frontend processes before opening the new desktop app. The
engine uses loopback port 7331; the desktop UI uses stable loopback port 7332.
The desktop refuses to reuse a different engine version.
Keep existing data and credentials; an app upgrade does not require deleting them.
Run only one backend against a given database.

## Build from source

Use native tools for the intended architecture:

- Git and current stable Rust/Cargo; the crate declares Rust 1.85 as its minimum.
- Visual Studio C++ Build Tools with the MSVC compiler and Windows SDK.
- Node.js 22 and npm 10 or newer. The CI package workflow uses Node 22.
- Current graphics drivers for GPU computation and rendering.

From the repository root in PowerShell:

```powershell
cargo test --manifest-path backend/Cargo.toml --locked --all-targets
cargo build --manifest-path backend/Cargo.toml --locked --release
npm --prefix frontend ci --no-audit --no-fund
npm --prefix frontend run build
npm --prefix desktop ci --no-audit --no-fund
npm --prefix desktop test
node scripts/stage-desktop.mjs
npm --prefix desktop start
```

The staging script checks the engine version and frontend export, then copies the
native binary into the matching runtime directory. Run it from a Git checkout
with a commit; its build record includes that source commit. x64 and ARM64 builds
require matching tools and binaries; renaming an x64 executable is not ARM64 support.

To create a local installer after staging:

```powershell
npm --prefix desktop run package:win:x64
```

On an ARM64 build machine use `package:win:arm64`. These commands use
`--publish never`. The manually dispatched
[desktop CI workflow](.github/workflows/desktop.yml) uploads build artifacts and
does not create a GitHub release.

## Browser development mode

Run these in separate terminals from the repository root:

```powershell
cargo run --manifest-path backend/Cargo.toml
```

```powershell
npm --prefix frontend run dev -- --hostname 127.0.0.1 --port 3000
```

Open [the browser client](http://127.0.0.1:3000).
[Backend health](http://127.0.0.1:7331/api/health) identifies the engine.
This is separate from the packaged app. Legacy `INSTALL_ALL.txt` and component
text helpers remain available for source setup; read their contents before use.

## Models, compute and saved data

Save a provider API key in **Settings**, load the account's models and choose a
default. The workbench picker can override provider, model and supported reasoning
for each request. Session choices apply to its calls and can change on resume.
Provider API charges apply independently of the app. OpenAI is the live-test
provider for this development environment.

Windows GPU inventory uses wgpu; eligible ODE search scoring can use DX12 or Vulkan.
Particle integration and final ODE replay use CPU f64. Procedural 3D geometry is a
visual representation, not proof that a matching scientific solver executed.
NPU devices may be inventoried, but no NPU numerical backend, local LLM inference
pool or cluster orchestrator is installed.

The default data location is resolved by the Rust application's platform directory
configuration. `PHASEFORGE_DATA_DIR` selects a different store. Keys are kept
separately by the OS credential service. Engine logs are written to
`logs/engine.log` under Electron's application user-data directory. Preserve these
locations and any custom configuration during upgrades.

The engine supports `--config PATH`, `--cpu-only`, `--version` and `--help`.
Use [the example configuration](configs/phaseforge.example.toml).
For a desktop launch, `PHASEFORGE_CONFIG` and `PHASEFORGE_DISABLE_GPU=1` are
inherited by the child engine.

## Optional tools and troubleshooting

Studio can run local Blender Cycles renders and native CAD/PCB exports. Install
[Blender](https://www.blender.org/download/) and
[KiCad](https://www.kicad.org/download/), and set up the isolated CAD Python runtime
using [the pinned native-engine guide](docs/CAD_PCB_ENGINES.md). This development
machine has local Windows x64 installations; installing PhaseForge on another
machine does not install these engines automatically.

`BLENDER_PATH`, `PHASEFORGE_CAD_PYTHON` and `KICAD_CLI` can point to the respective
executables. Set them in the environment that starts PhaseForge and restart the
app after changing them. Discovery also checks supported local installation paths.
Studio's engine badges indicate discovery; each completed job records actual
execution and checks. Blender selects a compatible Cycles device independently
of the core wgpu numerical backend. See [render setup and limits](docs/BLENDER_RENDERING.md).

Molecular files can be imported without external chemistry software. OpenMM,
GROMACS, CP2K, xTB, LAMMPS and Open Babel can be inventoried when available in the
engine's environment. Discovery alone does not execute their scientific workflows.
The independent ODE verifier requires Python 3.10+ and uses the standard library;
`INSTALL_VERIFIER.txt` remains its optional setup helper.

For a port conflict, stop the identified older app before retrying. For unavailable
GPU computation, inspect Hardware and the engine log; CPU execution remains
supported. After exhausted engine restart attempts, inspect the log and saved run
error before restarting. Source build failures should be diagnosed from the first
compiler or frontend error rather than by deleting saved data.

See [compute/recovery](docs/COMPUTE_AND_RECOVERY.md),
[sessions](docs/research-sessions.md), [scenes](docs/SCIENTIFIC_SCENES.md),
[scientific scope](docs/SCIENTIFIC_SCOPE.md), and
[verification methods](docs/VERIFICATION_METHODS.md).
