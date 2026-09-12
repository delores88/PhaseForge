# PhaseForge 0.10.0 on Windows

The current scientific package targets **Windows x64**. It bundles the desktop
interface, Rust backend, and verified scientific Python runtimes. Running the
packaged app does not require Rust, Node.js, a separate Python installation or a
browser. Current execution evidence was collected on Windows 11 x64; each final
installer still needs its own installation checks and marketplace review.

## Use the packaged application

The local package command produces
`desktop/dist/PhaseForge_0.10.0_x64-setup.exe`. Install it, open PhaseForge, and add
your OpenAI or Anthropic API key in **Settings**. Choose the provider, model and
reasoning effort for the next chat request. API usage is charged by that provider.

Start in ordinary project chat. Set a timer or choose **Off**, then describe a
finite study, its measurements and the comparison you want. Follow individual
agent/tool and solver jobs, inspect the saved results and play back the numerical
states. Pause/Resume uses durable job records and compatible checkpoints; source
changes can require a fresh attempt. See the [workflow and scientific scope](README.md).

The first scientific use verifies and copies the bundled runtimes into the app's
managed environment. The supported stack is CPython 3.13.15, NumPy 2.4.6,
OpenMM 8.5.2 and Pillow 12.3.0. Generated instruments run with Python and NumPy
inside the Windows LPAC boundary. A missing or changed runtime is reported;
PhaseForge does not substitute a host Python or install dependencies online.

When a tray icon is available, closing the window can leave work running. Use
**Quit PhaseForge** to exit before upgrading or switching builds. Interrupted work
returns paused for explicit continuation. Keep the existing data and credentials;
reinstallation is not a request to erase research history.

The backend uses an assigned loopback port and the desktop authenticates its owned
child. The desktop interface uses port 7332. Do not run two backends against the
same database or attach a development client to an unidentified service.

## Build the required runtime seeds

Source builds need Git, Rust/Cargo (minimum Rust 1.88), Visual Studio C++ Build
Tools with the Windows SDK, Node.js 22/npm, and a host Python build tool. Python
3.13 is suitable for the seed builder; it is not the interpreter shipped by
copying your local installation.

From the repository root in PowerShell, create a **new** seed output directory:

```powershell
python -I -B scripts/release/build_seeds.py --output .local/runtime-seeds-v2 --cache .local/runtime-seed-cache
$env:PHASEFORGE_RUNTIME_SEED_ROOT = (Resolve-Path .local/runtime-seeds-v2).Path
```

The builder obtains pinned upstream archives, checks their hashes and reconstructs
the exact committed manifests under `tools/runtime-seeds/`. Use `--offline` when
the verified archives are already cached. It refuses an existing output directory
or a mismatch; ordinary builds do not use `--freeze` to accept different bytes.
These downloads happen during package preparation, not experiment execution.

The same explicit seed root is needed by development-mode runtime provisioning
and Windows desktop staging. The staged installer contains both seeds; release
binaries use their packaged resources rather than this development override.

## Build and package locally

With the seed root set in the same PowerShell session:

```powershell
cargo test --manifest-path backend/Cargo.toml --locked --no-default-features
cargo build --manifest-path backend/Cargo.toml --locked --release
npm --prefix frontend ci --no-audit --no-fund
npm --prefix frontend run build
npm --prefix desktop ci --no-audit --no-fund
npm --prefix desktop test
node scripts/stage-desktop.mjs
npm --prefix desktop run package:win:x64
```

Staging verifies the backend version, exported interface and both runtime seed
inventories. It requires a Git checkout with a commit for its build record.
Packaging uses `--publish never`; producing an installer does not publish it or
establish marketplace acceptance. See [release evidence tooling](scripts/release/README.md)
for the additional source/materials and exact-artifact checks.

Real scientific, isolation and rendering acceptance tests have their own explicit
fixtures and opt-in settings. The normal test suite does not replace those checks;
see the [runtime validation record](docs/validation/runtime-v2.md).

To run the staged desktop against a separate development store:

```powershell
$env:PHASEFORGE_DATA_DIR = Join-Path (Get-Location).Path '.local/desktop-dev-data'
npm --prefix desktop start
```

Keep the installed app closed while using its desktop port. Do not use your live
research database for parallel development instances.

## Browser development mode

In the backend terminal, retain the explicit seed root and choose a development
store:

```powershell
$env:PHASEFORGE_RUNTIME_SEED_ROOT = (Resolve-Path .local/runtime-seeds-v2).Path
$env:PHASEFORGE_DATA_DIR = Join-Path (Get-Location).Path '.local/browser-dev-data'
cargo run --manifest-path backend/Cargo.toml
```

In another terminal:

```powershell
npm --prefix frontend run dev -- --hostname 127.0.0.1 --port 3000
```

Open [the development client](http://127.0.0.1:3000).
[Development backend health](http://127.0.0.1:7331/api/health) identifies that
backend. These addresses are separate from the packaged desktop's assigned
backend port.

## Optional rendering and saved data

Blender is installed separately for additional views, images and video exports.
The numerical OpenMM, diffusion and mechanics workers run without Blender.
`BLENDER_PATH` can select an installed executable; see
[Blender setup](docs/BLENDER_RENDERING.md). Existing CAD/PCB workflows also have
separate optional dependencies described in [their setup guide](docs/CAD_PCB_ENGINES.md).
An engine-discovery badge alone is not proof of scientific execution.

The default research store on this Windows setup is
`%LOCALAPPDATA%\PhaseForge\PhaseForge\data`; `PHASEFORGE_DATA_DIR` selects another
store. Provider keys stay separately in the OS credential store. The app also
keeps desktop engine logs under its user-data directory. Preserve data, logs and
custom configuration when diagnosing failures or upgrading.

For a port conflict, use normal Quit on the identified older app. For a runtime
inventory error, retain the error and restore the matching package rather than
editing the managed environment. An unavailable optional renderer does not mean
the numerical solver failed. Check each job's execution receipt and saved error.

The [main evidence ledger](README.md#current-evidence) links the completed scoped
studies and the pending installed mechanics/ML gates. Older
[0.8.0 validation](docs/RELEASE_VALIDATION.md) remains historical evidence, not
acceptance of the 0.9.1 installer.
