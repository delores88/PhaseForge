# Local Blender rendering

PhaseForge can turn an imported molecular structure or a validated procedural scene into a Cycles image, an editable Blender project, and a GLB model. Blender runs locally as a separate, supervised process. This is an appearance and geometry pipeline; the ODE and particle solvers still supply numerical experiment evidence.

Install a current Blender build for your operating system and architecture. Set `BLENDER_PATH` to its executable before starting PhaseForge, or place Blender on `PATH`. The app also checks common Blender Foundation installation directories and `%LOCALAPPDATA%/PhaseForge/engines/blender/`, including an extracted version subdirectory, or an `engines/blender-VERSION/` directory. Blender is not downloaded by an API request. There is no cloud rendering account or model-provider charge for this operation.

## What is rendered

- **Imported structures:** select a project-owned `MolecularStructure` ID. The fixed worker builds a joined atomic mesh using element van der Waals radii, then produces a bounded voxel-remeshed surface with two smoothing passes. Polymer chains and heteroatom ligand groups are separate surfaces; the studio style colors them separately. Water residues are hidden in the surface view, with the omitted count recorded. All source atom coordinates, including water, remain in `input.json`; a SHA-256 fingerprint identifies that saved structure, and a recorded center and uniform scale map those coordinates into the display scene. Large structures receive a proportional triangle budget per surface, with mesh decimation recorded explicitly. This reduces display tessellation without discarding source atoms. The interpolated surface is an approximate van der Waals envelope. It is not a solvent-excluded surface, electron-density reconstruction, or a force field.
- **Authored scenes:** the same data-only scene schema used by the interactive viewer supplies geometry, transforms and provenance. Explicit triangle meshes, molecular atoms, protein backbones, DNA curves, sampled field curves and procedural objects are supported. A protein without atoms or a backbone fails with an actionable message. Scene labels such as `measured` remain the author's provenance assertion; rendering does not independently verify it.
- **Lighting:** `microscopy` uses neutral grey surfaces and white lighting; `studio` uses chain colors and colored separation lights. Both use Cycles lighting, denoising, depth of field and surface materials. The microscopy style is an illustration style, not an acquired microscope image. Conceptual virus envelopes and astronomical scenes remain conceptual.
- **Whole-virion reconstruction:** a `virus` node produces a textured membrane with finite bilayer thickness, a clean cutaway, a conical capsid lattice and packaged RNA curves. `parameters.cutaway=false` closes the envelope. These components are explicitly morphological reconstruction. If `parameters.atoms` supplies sourced envelope-protein coordinates, a bounded surface is built once and instanced around the envelope, with at most 16 instances and a simplified mesh per instance. The author must identify the source in the scene provenance; placement, orientation and assembly are still illustrative. Without supplied atoms, the envelope proteins are morphological placeholders. The renderer never invents an atomic coordinate source or labels the whole assembly as cryo-EM data.

Blender exports `render.png`, `scene.blend`, and `scene.glb`. The `.blend` retains Cycles materials and area lights. GLB contains geometry, base materials, camera and provenance, and uses the interactive viewer's lighting; Cycles area lights and the fine procedural bump shader are not baked into the GLB. Every completed job also exposes `input.json` and `renderer.json`, which record the source, coordinate mapping, Blender version, actual render device, any GPU fallback, elapsed time, geometry count and limits.

## API

`GET /api/studio/capabilities` reports whether a local executable was found. Device execution is established by the actual job and reported in its metadata, rather than inferred from executable discovery.

Create a render with `POST /api/studio/renders`:

```json
{
  "project_id": "PROJECT_UUID",
  "structure_id": "STRUCTURE_UUID",
  "style": "microscopy",
  "width": 1024,
  "height": 1024,
  "samples": 64,
  "max_seconds": 300,
  "max_memory_mb": 4096
}
```

Supply exactly one of `structure_id` or `scene`. The scene must satisfy [the scene schema](scene.schema.json) and the backend scene validator. An optional camera has `position` and `target` vectors in source coordinates, with `focal_length_mm` between 20 and 120. Without an explicit camera, the worker frames the geometry automatically.

For measured geometry inside an authored scene, send `scene` with `bindings: [{"node_id":"protein-target","structure_id":"STRUCTURE_UUID"}]`. The backend resolves the saved structure directly; the model does not need to copy its coordinates into scene JSON. Bindings support `molecule`, `protein`, and `virus` nodes, with at most 16 bindings and 50,000 source atoms shared across the scene. Reusing a source in separate bindings counts its atoms again. The server rejects missing nodes, duplicate bindings, sources from another project, and a bound node that also supplies inline atoms, bonds, backbone points or mesh geometry. Sources also retain the 12,000-atom limit per chain/group surface. Bindings cannot accompany a standalone `structure_id`.

Bound molecular/protein surfaces are centered and uniformly fitted to the target node's declared diameter, followed by that node's authored transforms. A bound virus node uses the saved structure as its envelope-protein template with bounded repeated instances. In both cases `renderer.json` records each source ID, full saved-structure fingerprint, atom counts, fitting transform and scene node; `input.json` retains the full source. The overall scene placement is authored. For example, 4NCO contains Env with Fab fragments; using that complex as a template does not turn it into a measured native virion assembly or silently remove its Fab atoms.

- `GET /api/studio/renders?project_id=UUID` returns recent jobs for the project.
- `GET /api/studio/renders/UUID` returns status, settings, errors, renderer metadata and artifact links.
- `POST /api/studio/renders/UUID/cancel` requests cancellation, including while queued.
- `GET /api/studio/renders/UUID/artifacts/NAME` streams one of the five completed artifacts named above. Arbitrary paths, scripts and logs are not downloadable through this route.

States are `queued`, `rendering`, `completed`, `failed`, `cancelled`, and `interrupted`. The response uses `artifacts[].url` for downloads. The time budget includes queueing, building geometry, rendering and exports.

## Resource control and recovery

One Blender job runs at a time; at most eight jobs can be active or queued. Requests allow 256–2048 pixels per dimension, 16–256 samples, 15–1800 seconds, and 512–16384 MiB of RAM. The actual RAM grant is reduced to at most 60% of currently available system memory when the job starts. A grant below 512 MiB is rejected. Imported structures allow at most 50,000 atoms in total and 12,000 atoms per chain/group surface. The data-only scene validator retains its separate, smaller geometry limits. Voxel resolution is coarsened for large structures to bound the grid; structure surfaces share 1.5 million triangles through declared mesh decimation. Total scene geometry is capped at 1.6 million vertices and 1.8 million triangles. GLB files must fit within 128 MiB for the interactive viewer; other exported files must fit within 256 MiB.

The worker respects the app's GPU-enabled setting, and otherwise attempts compatible Cycles devices in the order OptiX, CUDA, HIP, oneAPI, Metal, then CPU. If an initialized GPU render raises a recoverable Blender error, it retries once on CPU within the original deadline. A native crash or exhausted host RAM ends the job with its diagnostic; it does not silently start unlimited retries. Windows uses a Job Object to bound process-tree memory and close the tree when the supervisor exits. The supervisor also monitors resident RAM and available host memory, enforces cancellation and elapsed time, and isolates the Unix process group. These controls cannot reserve GPU VRAM against other applications.

Jobs and inputs are stored under the configured app data directory at `artifacts/studio/JOB_UUID/`. After an application restart, a previously active job becomes `interrupted` when loaded. Existing files remain on disk. Start a new job to rerender; partial Cycles sample-state continuation is not implemented.

The request cannot supply Python, shell commands, external asset paths, arbitrary `.blend` files, or network URLs. PhaseForge launches an embedded fixed worker using Blender's factory startup and disabled automatic script execution. NPU rendering, distributed render farms, molecular dynamics, and therapeutic efficacy assessment are outside this renderer's scope.

See [Blender's Cycles rendering overview](https://www.blender.org/features/rendering/), the [Remesh modifier API](https://docs.blender.org/api/current/bpy.types.RemeshModifier.html), and the [glTF export API](https://docs.blender.org/api/current/bpy.ops.export_scene.html). Broader runtime limits are documented in [GPU and scheduling](GPU_AND_SCHEDULING.md) and [scientific scope](SCIENTIFIC_SCOPE.md).

## Native render verification

The ignored Rust test `studio::render::tests::real_local_blender_writes_all_artifacts` imports a real PDB structure with the native parser and runs the actual supervised Blender process. Set `PHASEFORGE_RENDER_TEST_PDB` to a local copy of RCSB 1HSG and `PHASEFORGE_RENDER_TEST_OUT` to an isolated output directory, then run the test explicitly with `--ignored --nocapture`. It checks PNG and GLB signatures, the Blender project, imported-source metadata and the actual execution device. Outputs remain available for visual inspection. Routine tests cover request limits, artifact path restrictions, interrupted-job recovery and Windows process-tree ownership.

On the development Windows x64 machine, Blender 4.5.9 LTS rendered the imported 1,686-atom 1HSG structure at 768 × 768 and 32 samples using OptiX on the RTX 4090 Laptop GPU. A separate 1024 × 1024, 48-sample whole-virion morphological cutaway also produced all three files using OptiX. Both GLBs passed the frontend's actual asset-size, geometry and embedded-resource checks. The whole-virion validation used generated morphological envelope proteins, not a sourced Env trimer. Native Blender execution on other architectures is not established by these Windows results.

The same machine also rendered the full native-imported [RCSB 4NCO Env–PGT122 Fab structure](https://www.rcsb.org/structure/4NCO): all 22,014 atoms were represented, with no solvent or atom omissions. At 1024 × 1024 and 48 samples, the worker took about 39 seconds with OptiX. Its declared surface-mesh detail reductions produced a roughly 30 MiB GLB below the viewer's 2-million-triangle ceiling. Source coordinates and their fingerprint stayed unchanged. This validates rendering that imported complex, not a therapeutic effect or a whole-virion molecular simulation.

A further 768 × 768, 32-sample worker validation bound that same full 4NCO source to a cutaway virus node. It completed in about 55 seconds with OptiX, retained the identical source fingerprint and all 22,014 source atoms, and exported a 2.7 MiB GLB accepted by the frontend's real import checks. The display used 13 instances after the geometry cap and cutaway; their placements and the membrane/capsid remain morphological reconstruction. Routine binding tests cover exact source preservation, ownership, missing/unsupported nodes, mixed geometry, duplicate bindings and aggregate/surface admission limits.

Blender is separately installed free software. Its binaries and Python API have GPL licensing terms; the original fixed worker is offered under `MIT OR GPL-3.0-or-later`. See [third-party notices](../THIRD_PARTY.md), [the worker's GPL option](../tools/BLENDER_WORKER_LICENSE.txt), and [Blender's license statement](https://www.blender.org/about/license/). Blender does not require an engine royalty for the files it creates.
