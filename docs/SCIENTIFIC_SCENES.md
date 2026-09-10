# Scientific scenes

PhaseForge uses Three.js (MIT) for a local, GPU-rendered scientific scene graph.
Rendering is independent of the numerical solver: an attractive surface does not
establish a physical mechanism, an experimentally determined structure, or a cure.

The scene lives in `manifest.visualization.scene`. Each run saves its own scene
snapshot in `result.visualization.scene`. A historical run never borrows geometry
from an unrelated current manifest. The full provider-compatible JSON schema is
[`scene.schema.json`](scene.schema.json).

## What agents can author

- Atomic records with element colors, explicit bonds, multiple-bond cylinders,
  space-filling representations and translucent visual envelopes.
- DNA helices, protein ribbons from backbone coordinates, and procedural membrane
  envelopes with surface proteins and optional cutaways.
- Planetary surfaces and rings, stars, and illustrative accretion disks.
- Curves, supplied streamlines, mathematical surfaces, boxes, spheres, and arbitrary
  indexed triangle meshes. Remote URLs, scripts and executable expressions are
  never evaluated by the renderer.

A molecular node without `parameters.atoms` shows a conceptual scaffold. A protein
without backbone `parameters.points` shows a conceptual fold. Procedural DNA,
viral, planetary and accretion-disk detail is illustrative. These are useful scene
construction primitives, not molecular-dynamics, fluid, quantum or relativity
solvers. Real structure coordinates should be supplied when available.

The Flow scene study integrates the analytic steady, inviscid, incompressible
potential-flow velocity field around a sphere. It deliberately makes no claims
about viscous boundary layers, wakes or turbulence.

## Bind geometry to numerical evidence

Each node has a stable `id`. Set `entity_id` to the identifier emitted by
`visualization.entities`. The renderer adds the node's `position` offset to the
retained entity position at each frame. The scene's spatial units and geometry
dimensions must match that numerical mapping. Static geometry leaves `entity_id`
empty or null. Radius and surface detail do not introduce contact dynamics.

```json
{
  "schema_version": "1.0",
  "title": "A measured trajectory with authored geometry",
  "units": "m",
  "provenance": {
    "kind": "conceptual",
    "description": "Surface detail is illustrative; center motion is linked to retained numerical states."
  },
  "nodes": [{
    "id": "body_surface",
    "type": "planet",
    "label": "Body A",
    "entity_id": "body_a",
    "position": [0, 0, 0],
    "color": "#91b6cf",
    "parameters": {"radius": 0.3, "rings": false, "seed": 7}
  }]
}
```

Native imports can omit optional fields. Structured provider output includes all
schema fields and uses null for unused optional values. `scale` is a three-number
vector in the strict schema; the local renderer additionally accepts a scalar.
Rotation uses radians about X, Y and Z.

For a molecular node, `parameters.radius` is the base atom display radius in the
same units as its atom coordinates; hydrogen glyphs use 60% of that radius.
`thickness` sets the requested bond radius, bounded by each bond's length and atom
radius so an overlarge request cannot obscure the structure. Multiple-bond offsets
and optional visual envelopes use the same scale. An omitted atom radius is derived
from median positive bond spacing, or sampled nearest-neighbor coordinate spacing
when bonds are absent. This display sizing never changes coordinates or bonding
records and does not infer physical atomic radii.

## Shared inspection

The viewport provides unrestricted orbit, pan and zoom, front/top/side views,
object focus, a reference grid, section clipping, image capture, and scene/camera
JSON export. Playback can step through retained frames or interpolate positions;
the inspector always identifies the recorded frame behind numerical values.

`GenericViewport` accepts `onInspect`, `onCameraChange`, and `cameraCommand`.
Camera positions and targets are reported in original world coordinates despite
internal recentering. Commands accept `view` (`perspective`, `xy`, `xz`, `yz`),
`node_id`, `position` and `target`. Inspection includes a compact object summary,
the picked atom when available, retained numerical entity, frame time, camera and
provenance. These callbacks support giving the research agent the same context
the researcher is inspecting without sending the entire geometry payload.

The reusable `ScenePreview` component and `exampleScene` helper provide four
explicitly labeled scene studies: molecular architecture, structural biology,
flow, and orbital systems. Example geometry never substitutes for a run's missing
numerical evidence.

## Resource behavior

Scene imports are bounded on both server and client. The renderer caps node,
vertex, atom, bond, curve and retained-frame allocations and reduces procedural
tessellation for large scenes. Instances share sphere/cylinder buffers. Camera,
playback, settings and inspector updates do not rebuild the scene. Rendering is
capped at 60 frames per second during interaction/playback and 15 while idle;
hidden windows suspend drawing. Playback advances from monotonic elapsed time,
independent of display refresh rate. A sustained
low frame rate lowers pixel resolution; the user can choose Economy, Balanced or
High geometry detail. A lost graphics context exposes a recovery action that
rebuilds at Economy detail.

Unmounting disposes geometries and materials, disconnects observers and input
listeners, stops animation and releases the renderer. Display limits never alter
the exported numerical evidence.

Run the independent data/geometry checks from `frontend`:

```text
node --test tests/scene.test.mjs
```
