# Detailed scientific model viewer

`ScientificModelViewer` is the local inspection surface for Blender GLB output,
scientific triangle meshes, and imported molecular coordinates. Its public React
interface is:

```jsx
<ScientificModelViewer
  url={completedRenderGlbUrl}
  style="microscopy"
  provenance={renderProvenance}
  onInspect={setInspection}
  onCameraChange={setCamera}
/>
```

Alternatively pass a browser `File` through `file`, or an existing
`MolecularStructure` through `structure`. An explicit URL takes precedence over
structure coordinates; a selected local file takes precedence over both. Files
can also be dropped into the viewer. GLB and STL remain local to the viewer;
opening one does not upload it or convert it into a scientific result.

`MoleculeViewport` uses the same renderer. Molecular coordinates default to a
continuous union of atom-radius envelopes extracted with marching cubes. This is a smooth
display representation of the supplied atomic coordinates and element display
radii. It is **not** a measured electron-density map, solvent-excluded surface,
force-field calculation, or molecular-dynamics result. The source coordinates
and atom count are preserved. Chain identity determines display colors; color
does not encode an inferred biological function.

The viewer accepts Å/angstrom (the native molecular import convention), nm, pm,
and m coordinate units. The displayed grid spacing identifies the surface's
sampling resolution. Balanced detail uses a 104³ sampling grid, Economy 64³,
and High 128³; generation is capped at 50,000 atoms, 300,000 surface triangles,
and a bounded number of voxel evaluations. Work yields periodically and responds
to cancellation. Larger assemblies should use the Blender renderer or a selected
subassembly instead of silently dropping atoms.

Surface picking reports the nearest supplied atomic coordinate to the picked
surface point. Atomic volume or atom-and-bond views report the picked atom
directly. `selectedAtomIds` displays a bounded selection overlay; selecting atoms
does not reconstruct the molecular surface. Mesh picking reports the named
surface and source-space coordinates. Inspection and camera callbacks contain
serializable records suitable for the research agent's context.

Microscopy and Studio lighting share physically based materials, a procedural
studio environment, soft shadows and optional screen-space ambient occlusion.
These styles are presentation choices. Orbit, pan, zoom, camera presets, section
clipping, fullscreen and PNG capture remain available in both. Geometry is
recentered and scaled internally for graphics precision; reported coordinates
are transformed back to the source coordinate system.

## Asset limits and external resources

GLB version 2 and binary/ASCII STL files are validated before loader execution.
The input limit is 128 MiB, 2 million source triangles, 6 million vertices and
12,000 nodes. Decoded accessor allocations, expanded drawing counts and embedded
texture dimensions are bounded separately. Cyclic/deep scene hierarchies and
nonfinite coordinates are rejected. Compressed geometry/Basis textures currently
require an uncompressed GLB export.

GLB buffers and PNG/JPEG textures must be embedded. External buffers, texture
URLs, and data-URI resources are rejected; the loader is also restricted to its
own embedded-image blob URLs. Root downloads are limited to this app or its
loopback backend. Embedded cameras/lights are disabled in favor of the viewer's
controlled presentation. glTF animation playback and arbitrary shader scripts
are not executed.

Rendering is bounded to 60 updates per second during interaction and lower-rate
idle checks. A static scene is drawn only when it changes, and hidden windows
do not draw. Unmounting or replacing a model cancels loading, stops rendering,
disconnects input/resize observers, and disposes geometry, materials, textures,
environment maps and postprocessing buffers. Context loss offers an Economy
detail recovery action.

Run data, geometry, unit-conversion and asset-intake checks from `frontend`:

```text
node --test tests/model-assets.test.mjs tests/scene.test.mjs
```
