#!/usr/bin/env python3
"""Trusted, bounded OpenMM argon laboratory. No user code or model expressions.

The Python environment isolates dependencies; it is not a security sandbox.
Only data parameters enter this worker. Parent owns wall-time/process/OS limits.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import signal
import sys
import time
from datetime import datetime, timezone

import numpy as np
import openmm as mm
from openmm import unit
from PIL import Image, ImageDraw, ImageFont

SCHEMA = 1
MASS = 39.948
SIGMA = 0.3405
EPSILON = 0.997
R_GAS = 0.00831446261815324  # kJ/(mol K), exact SI constants
NA = 6.02214076e23
PRESSURE_BAR = 1e25 / NA  # kJ/mol/nm^3 -> bar
DEFAULTS = {
    "atom_count": 108, "temperature_kelvin": 120.0, "density_g_cm3": 1.0,
    "steps": 2000, "timestep_fs": 1.0, "seed": 20260911,
    "platform": "CPU", "thermostat": "langevin", "friction_per_ps": 1.0,
    "sample_interval": 20, "chunk_frames": 50, "cpu_threads": 1,
}
UNITS = {"time": "ps", "position": "nm", "velocity": "nm/ps",
         "energy": "kJ/mol", "force": "kJ/(mol nm)", "temperature": "K",
         "pressure": "bar", "density": "g/cm^3", "msd": "nm^2"}
STOP = False
ROOT: Path | None = None
REPORT_ERRORS = False


def canonical(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"),
                      allow_nan=False).encode("utf-8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def now():
    return datetime.now(timezone.utc).isoformat()


def checked(path: Path):
    if ROOT is None or not path.resolve().is_relative_to(ROOT):
        raise ValueError("Output path escapes the run directory")
    if path.is_symlink():
        raise ValueError("Output symlinks are not permitted")
    path.parent.mkdir(parents=True, exist_ok=True)
    return path


def atomic_bytes(path, data):
    path = checked(Path(path))
    tmp = checked(path.with_name(path.name + ".tmp"))
    with tmp.open("wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    # Windows readers (UI polling, virus scanners) can briefly hold a handle that
    # does not allow deletion. Preserve atomicity and retry; never truncate a live
    # progress/index file merely to work around a sharing violation.
    until = time.monotonic() + 2.0
    while True:
        try:
            os.replace(tmp, path)
            break
        except PermissionError:
            if time.monotonic() >= until:
                raise
            time.sleep(.01)


def atomic_json(path, value):
    atomic_bytes(path, canonical(value))


def load_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"),
                      parse_constant=lambda s: (_ for _ in ()).throw(ValueError("Nonfinite JSON")))


def validate_input(value):
    if not isinstance(value, dict) or set(value) - {"engine", "parameters"}:
        raise ValueError("Input must contain only engine and parameters")
    if value.get("engine") != "openmm_argon":
        raise ValueError("Only engine openmm_argon is supported")
    supplied = value.get("parameters", {})
    if not isinstance(supplied, dict) or set(supplied) - set(DEFAULTS):
        raise ValueError("Unknown parameter; supported: " + ", ".join(DEFAULTS))
    p = {**DEFAULTS, **supplied}
    integer_bounds = {"atom_count": (32, 512), "steps": (1, 5000000),
                      "seed": (1, 2147483647), "sample_interval": (1, 1000),
                      "chunk_frames": (1, 100), "cpu_threads": (1, 8)}
    real_bounds = {"temperature_kelvin": (20.0, 500.0), "density_g_cm3": (0.02, 2.0),
                   "timestep_fs": (0.1, 5.0), "friction_per_ps": (0.01, 100.0)}
    for key, (low, high) in integer_bounds.items():
        if type(p[key]) is not int or not low <= p[key] <= high:
            raise ValueError(f"{key} must be an integer in [{low}, {high}]")
    for key, (low, high) in real_bounds.items():
        if type(p[key]) not in (int, float) or not math.isfinite(p[key]) or not low <= p[key] <= high:
            raise ValueError(f"{key} must be finite in [{low}, {high}]")
        p[key] = float(p[key])
    if p["platform"] not in ("Reference", "CPU", "CUDA", "OpenCL"):
        raise ValueError("platform must be Reference, CPU, CUDA, or OpenCL")
    if p["thermostat"] not in ("langevin", "nve"):
        raise ValueError("thermostat must be langevin or nve")
    if math.ceil(p["steps"] / p["sample_interval"]) + 1 > 50001:
        raise ValueError("At most 50001 retained frames per run; choose an explicit sampling interval or split the study")
    if p["steps"] * p["atom_count"] > 50_000_000:
        raise ValueError("atom_count times steps exceeds the 50 million work budget")
    if p["sample_interval"] * p["timestep_fs"] > 1000:
        raise ValueError("Sampling interval must not exceed 1 ps")
    return p


def model_geometry(p):
    n = p["atom_count"]
    volume = n * MASS / NA / p["density_g_cm3"] * 1e21
    length = volume ** (1.0 / 3.0)
    cutoff = min(2.5 * SIGMA, 0.49 * length)
    cells = math.ceil((n / 4) ** (1 / 3) - 1e-12)
    basis = np.array([[0, 0, 0], [0, .5, .5], [.5, 0, .5], [.5, .5, 0]])
    sites = np.array([(np.array([i, j, k]) + b + .125) * length / cells
                      for i in range(cells) for j in range(cells)
                      for k in range(cells) for b in basis])
    # Even deterministic occupancy if the requested N is not a complete FCC cube.
    positions = sites[np.linspace(0, len(sites) - 1, n).astype(int)]
    return length, cutoff, positions


def create_context(p):
    length, cutoff, positions = model_geometry(p)
    system = mm.System()
    system.setDefaultPeriodicBoxVectors(mm.Vec3(length, 0, 0), mm.Vec3(0, length, 0),
                                        mm.Vec3(0, 0, length))
    force = mm.NonbondedForce()
    force.setNonbondedMethod(mm.NonbondedForce.CutoffPeriodic)
    force.setCutoffDistance(cutoff)
    force.setUseSwitchingFunction(True)
    force.setSwitchingDistance(0.8 * cutoff)
    force.setUseDispersionCorrection(False)
    for _ in range(p["atom_count"]):
        system.addParticle(MASS)
        force.addParticle(0.0, SIGMA, EPSILON)
    system.addForce(force)
    dt = p["timestep_fs"] * 0.001
    if p["thermostat"] == "langevin":
        integrator = mm.LangevinMiddleIntegrator(p["temperature_kelvin"],
                                                 p["friction_per_ps"], dt)
        integrator.setRandomNumberSeed(p["seed"])
        system.addForce(mm.CMMotionRemover(1))
    else:
        integrator = mm.VerletIntegrator(dt)
    available = [mm.Platform.getPlatform(i).getName() for i in range(mm.Platform.getNumPlatforms())]
    if p["platform"] not in available:
        raise ValueError(f"Requested platform {p['platform']} unavailable; installed: {available}")
    platform = mm.Platform.getPlatformByName(p["platform"])
    properties = {}
    if p["platform"] == "CPU":
        properties = {"Threads": str(p["cpu_threads"]), "DeterministicForces": "true"}
    elif p["platform"] in ("CUDA", "OpenCL"):
        properties = {"Precision": "mixed"}
    context = mm.Context(system, integrator, platform, properties)
    context.setPositions(positions)
    # Seeded, zero-center-of-mass Maxwell sample. The exact initial state is retained.
    rng = np.random.default_rng(p["seed"])
    velocities = rng.normal(size=(p["atom_count"], 3)) * math.sqrt(R_GAS * p["temperature_kelvin"] / MASS)
    velocities -= velocities.mean(axis=0)
    velocities *= math.sqrt((3 * p["atom_count"] - 3) * R_GAS * p["temperature_kelvin"] /
                            (MASS * np.sum(velocities * velocities)))
    context.setVelocities(velocities)
    return context, integrator, system, length, cutoff, positions


def pair_instruments(positions, length, cutoff, bins=60):
    """Minimum-image pair virial and RDF, including derivative of LJ switching."""
    i, j = np.triu_indices(len(positions), 1)
    delta = positions[i] - positions[j]
    delta -= length * np.rint(delta / length)
    r = np.linalg.norm(delta, axis=1)
    if not np.isfinite(r).all() or np.min(r) < 0.05 * SIGMA:
        raise ValueError("Nonfinite or overlapping coordinates; integration is not valid")
    selected = r < cutoff
    rr = r[selected]
    q6 = (SIGMA / rr) ** 6
    potential = 4 * EPSILON * (q6 * q6 - q6)
    # -r dU/dr = 24 epsilon (2 q^12 - q^6)
    virial_pair = 24 * EPSILON * (2 * q6 * q6 - q6)
    x = np.clip((rr - .8 * cutoff) / (.2 * cutoff), 0, 1)
    switch = 1 - 10 * x ** 3 + 15 * x ** 4 - 6 * x ** 5
    ds = (-30 * x ** 2 + 60 * x ** 3 - 30 * x ** 4) / (.2 * cutoff)
    virial = float(np.sum(switch * virial_pair - rr * potential * ds))
    counts, edges = np.histogram(r, bins=bins, range=(0, length / 2))
    return virial, counts, edges


def measure(context, initial_positions, p, length, cutoff):
    state = context.getState(getPositions=True, getVelocities=True, getForces=True,
                             getEnergy=True, enforcePeriodicBox=False)
    positions = np.asarray(state.getPositions(asNumpy=True).value_in_unit(unit.nanometer))
    velocities = np.asarray(state.getVelocities(asNumpy=True).value_in_unit(unit.nanometer / unit.picosecond))
    forces = np.asarray(state.getForces(asNumpy=True).value_in_unit(unit.kilojoule_per_mole / unit.nanometer))
    if not all(np.isfinite(a).all() for a in (positions, velocities, forces)):
        raise ValueError("Engine produced nonfinite state; no scientific result is accepted")
    potential = float(state.getPotentialEnergy().value_in_unit(unit.kilojoule_per_mole))
    kinetic = float(state.getKineticEnergy().value_in_unit(unit.kilojoule_per_mole))
    virial, counts, edges = pair_instruments(positions, length, cutoff)
    row = {"step": int(state.getStepCount()), "time_ps": float(state.getTime().value_in_unit(unit.picosecond)),
           "potential_energy_kj_mol": potential, "kinetic_energy_kj_mol": kinetic,
           "total_energy_kj_mol": potential + kinetic,
           "temperature_kelvin": 2 * kinetic / ((3 * p["atom_count"] - 3) * R_GAS),
           "pressure_bar": (2 * kinetic + virial) / (3 * length ** 3) * PRESSURE_BAR,
           "pair_virial_kj_mol": virial,
           "msd_nm2": float(np.mean(np.sum((positions - initial_positions) ** 2, axis=1)))}
    canonical(row)  # refuse any nonfinite scalar before it reaches disk
    return row, positions, velocities, forces, counts, edges


def projection(frame, length, path):
    """Fixed +z orthographic xy projection; Pillow draws exact stored frame data."""
    image = Image.new("RGB", (1024, 1024), "#10151e")
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default(size=20)
    large = ImageFont.load_default(size=27)
    left, top, extent = 106, 118, 804
    draw.text((42, 26), "OpenMM / Lennard-Jones argon", fill="#edf5fc", font=large)
    draw.text((42, 65), f"Measured frame | step {frame['step']} | t = {frame['time']:.6f} ps", fill="#becddd", font=font)
    draw.rectangle((left, top, left + extent, top + extent), outline="#74869c", width=2)
    for tick in range(5):
        fraction = tick / 4
        x = left + fraction * extent
        y = top + extent - fraction * extent
        draw.line((x, top + extent, x, top + extent + 8), fill="#b5c7d9", width=2)
        draw.text((x - 15, top + extent + 14), f"{length*fraction:.2f}", fill="#b5c7d9", font=font)
        draw.line((left - 8, y, left, y), fill="#b5c7d9", width=2)
        draw.text((20, y - 8), f"{length*fraction:.2f}", fill="#b5c7d9", font=font)
    radius = max(2.5, min(18, SIGMA / 2 / length * extent))
    for entity in sorted(frame["entities"], key=lambda e: e["position"][2]):
        px, py, pz = entity["position"]
        x, y = left + px / length * extent, top + extent - py / length * extent
        # Depth color is an explicitly declared numeric z coordinate, not density.
        t = pz / length
        color = (int(63 + 105*t), int(116 + 96*t), int(186 + 57*t))
        draw.ellipse((x - radius, y - radius, x + radius, y + radius), fill=color, outline="#cae5fb", width=1)
    draw.text((460, 966), "x (nm)", fill="#d4e5f6", font=font)
    draw.text((12, 458), "y (nm)", fill="#d4e5f6", font=font)
    draw.text((42, 998), "View +z; lighter = larger z. Wrapped periodic box; circles may overlap in projection.",
              fill="#b5c7d9", font=ImageFont.load_default(size=14))
    import io
    buffer = io.BytesIO()
    image.save(buffer, format="PNG")
    atomic_bytes(path, buffer.getvalue())


def measurements(rows, rdf_counts, rdf_edges, p, length):
    shells = 4 * math.pi / 3 * np.diff(np.asarray(rdf_edges) ** 3)
    denominator = len(rows) * p["atom_count"] * (p["atom_count"] - 1) / 2 * shells / length ** 3
    rdf = np.asarray(rdf_counts) / denominator
    # This is a late-window description, not an equilibrium detector or confidence interval.
    tail = rows[len(rows)//2:]
    summary = {"sample_count": len(rows), "late_window_start_ps": tail[0]["time_ps"],
               "late_mean_temperature_kelvin": float(np.mean([r["temperature_kelvin"] for r in tail])),
               "late_mean_pressure_bar": float(np.mean([r["pressure_bar"] for r in tail])),
               "initial_total_energy_kj_mol": rows[0]["total_energy_kj_mol"],
               "final_total_energy_kj_mol": rows[-1]["total_energy_kj_mol"],
               "max_energy_deviation_kj_mol": max(abs(r["total_energy_kj_mol"] - rows[0]["total_energy_kj_mol"]) for r in rows),
               "final_msd_nm2": rows[-1]["msd_nm2"]}
    return {"schema_version": SCHEMA, "units": UNITS, "series": rows, "summary": summary,
            "rdf": {"r_nm": ((np.asarray(rdf_edges)[1:] + np.asarray(rdf_edges)[:-1]) / 2).tolist(),
                    "g_r": rdf.tolist(), "pair_counts": np.asarray(rdf_counts).tolist(),
                    "edges_nm": list(rdf_edges), "sample_count": len(rows),
                    "normalization": "unordered pairs / (frames*N*(N-1)*shell_volume/(2*box_volume))"},
            "methods": {"temperature": "2*OpenMM kinetic energy / ((3N-3)*R); zero COM initialization, Langevin removes COM each step",
                        "pressure": "instantaneous (2*kinetic + switched minimum-image pair virial)/(3V); no tail correction",
                        "msd": "mean squared displacement from initial unwrapped positions; not a fitted diffusion coefficient",
                        "statistics": "Second-half sample averages include correlated samples and may be nonequilibrium; no confidence interval claimed"}}


def request_stop(*_):
    global STOP
    STOP = True


def run(input_path: Path, output: Path):
    global ROOT, REPORT_ERRORS
    if not input_path.is_absolute() or not output.is_absolute():
        raise ValueError("--input and --output must be absolute paths")
    if input_path.stat().st_size > 1_048_576:
        raise ValueError("Input exceeds 1 MiB")
    value = load_json(input_path)
    p = validate_input(value)
    output.mkdir(parents=True, exist_ok=True)
    if output.is_symlink():
        raise ValueError("Run directory must not be a symlink")
    ROOT = output.resolve()
    output = ROOT
    input_hash = digest(canonical({"engine": "openmm_argon", "parameters": p}))
    worker_hash = digest(Path(__file__).read_bytes())
    journal_path = output / "restart.json"
    old_manifest = load_json(output / "manifest.json") if (output / "manifest.json").exists() else None
    if old_manifest and (old_manifest.get("input_sha256") != input_hash or old_manifest.get("worker_sha256") != worker_hash):
        raise ValueError("Refusing resume: normalized input or trusted worker differs from saved manifest")
    if old_manifest and (output / "result.json").exists():
        saved = load_json(output / "result.json")
        if saved.get("status") == "completed":
            return saved
    elif not old_manifest and any(output.iterdir()):
        allowed = {"input.json", "worker-input.json", "cancel.request", "scientific_worker.py", "requirements-science.txt", "stdout.log", "stderr.log",
                   "environment.log", "environment-error.log", "install.log", "install-error.log",
                   "environment-smoke.log", "environment-smoke-error.log"}
        if any(x.name not in allowed for x in output.iterdir()):
            raise ValueError("Fresh output directory contains unrelated files")
    REPORT_ERRORS = True
    started = time.monotonic()
    atomic_json(output / "progress.json", {"fraction": 0, "step": 0, "message": "Initializing OpenMM", "status": "running"})
    context, integrator, system, length, cutoff, initial_positions = create_context(p)
    platform = context.getPlatform()
    properties = {key: platform.getPropertyValue(context, key) for key in platform.getPropertyNames()}
    manifest = old_manifest or {"schema_version": SCHEMA, "engine": "openmm_argon", "engine_name": "OpenMM",
        "engine_version": mm.version.version, "platform": platform.getName(), "platform_properties": properties,
        "python_version": sys.version.split()[0], "numpy_version": np.__version__, "units": UNITS,
        "created_at": now(), "parameters": p, "input_sha256": input_hash, "worker_sha256": worker_hash,
        "model": {"mass_dalton": MASS, "sigma_nm": SIGMA, "epsilon_kj_mol": EPSILON,
                  "box_nm": [length]*3, "cutoff_nm": cutoff, "switch_nm": .8*cutoff,
                  "dispersion_correction": False, "charges": 0, "boundary": "cubic periodic",
                  "equation": "4 epsilon [(sigma/r)^12-(sigma/r)^6], quintic-switched to zero",
                  "initialization": "seeded FCC sites and zero-COM Maxwell velocities, no minimization"},
        "scope": "Classical argon-like LJ model; no empirical phase/pressure calibration, chemical reactions, or biological claims",
        "checkpoint_portability": "Exact input, worker, OpenMM version, platform and compatible hardware; no portable RNG guarantee",
        "sources": ["https://docs.openmm.org/latest/userguide/theory/02_standard_forces.html",
                    "https://docs.openmm.org/latest/userguide/theory/04_integrators.html"]}
    if old_manifest and (old_manifest["engine_version"] != mm.version.version or old_manifest["platform"] != platform.getName()):
        raise ValueError("Saved checkpoint engine/platform is incompatible")
    atomic_json(output / "manifest.json", manifest)
    atomic_json(output / "topology.json", {"schema_version": SCHEMA, "position_unit": "nm",
        "entities": [{"id": f"ar-{i:04d}", "element": "Ar", "radius_nm": SIGMA/2, "mass_dalton": MASS}
                     for i in range(p["atom_count"])], "bonds": [], "box_nm": [length]*3,
        "radius_semantics": "display radius sigma/2, not a hard collision boundary"})
    atomic_bytes(output / "system.xml", mm.XmlSerializer.serialize(system).encode("utf-8"))
    atomic_bytes(output / "integrator.xml", mm.XmlSerializer.serialize(integrator).encode("utf-8"))
    rows, chunks, images = [], [], []
    rdf_counts = np.zeros(60, dtype=np.int64)
    rdf_edges = np.linspace(0, length/2, 61)
    step = 0
    if journal_path.exists():
        journal = load_json(journal_path)
        if journal["input_sha256"] != input_hash:
            raise ValueError("Checkpoint input hash mismatch")
        checkpoint_path = output / journal["checkpoint_path"]
        checked(checkpoint_path)
        data = checkpoint_path.read_bytes()
        if digest(data) != journal["checkpoint_sha256"]:
            raise ValueError("Checkpoint hash mismatch")
        context.loadCheckpoint(data)
        step = int(context.getState().getStepCount())
        if step != journal["step"]:
            raise ValueError("Checkpoint step mismatch")
        rows, chunks, images = journal["rows"], journal["chunks"], journal["images"]
        rdf_counts = np.asarray(journal["rdf_counts"], dtype=np.int64)
        initial_positions = np.asarray(journal["initial_positions_nm"])
        for chunk in chunks:
            for path_key, hash_key in [("path", "sha256"), ("arrays_path", "arrays_sha256")]:
                cp = checked(output / chunk[path_key])
                if digest(cp.read_bytes()) != chunk[hash_key]:
                    raise ValueError("Previously committed trajectory artifact hash mismatch")
        atomic_json(output / "progress.json", {"fraction": step/p["steps"], "step": step,
                    "message": "Resumed exact OpenMM checkpoint", "status": "running"})
    elif old_manifest:
        raise ValueError("Saved initialization has no committed checkpoint; use a fresh run directory")
    batch_frames, batch_positions, batch_velocities, batch_forces = [], [], [], []

    def commit():
        if not batch_frames:
            return
        number = len(chunks)
        relative = f"trajectory/chunk-{number:05d}.json"
        payload = canonical({"schema_version": SCHEMA, "frames": batch_frames})
        atomic_bytes(output / relative, payload)
        arrays_path = f"trajectory/arrays-{number:05d}.npz"
        import io
        buffer = io.BytesIO()
        np.savez_compressed(buffer, positions_unwrapped_nm=np.asarray(batch_positions),
                            velocities_nm_ps=np.asarray(batch_velocities), forces_kj_mol_nm=np.asarray(batch_forces),
                            steps=np.asarray([f["step"] for f in batch_frames], dtype=np.int64),
                            time_ps=np.asarray([f["time"] for f in batch_frames]))
        arrays_bytes = buffer.getvalue()
        atomic_bytes(output / arrays_path, arrays_bytes)
        start_frame = sum(c["frame_count"] for c in chunks)
        chunk = {"path": relative, "sha256": digest(payload), "arrays_path": arrays_path,
                 "arrays_sha256": digest(arrays_bytes), "start_frame": start_frame,
                 "end_frame": start_frame + len(batch_frames) - 1, "frame_count": len(batch_frames),
                 "start_time": batch_frames[0]["time"], "end_time": batch_frames[-1]["time"],
                 "start_step": batch_frames[0]["step"], "end_step": batch_frames[-1]["step"]}
        chunks.append(chunk)
        image_path = f"observations/frame-{batch_frames[-1]['step']:08d}.png"
        projection(batch_frames[-1], length, output / image_path)
        images.append({"path": image_path, "mime_type": "image/png", "width": 1024, "height": 1024,
                       "sha256": digest((output/image_path).read_bytes()), "source_chunk": relative,
                       "source_chunk_sha256": chunk["sha256"], "source_frame_index": len(batch_frames)-1,
                       "source_frame_sha256": digest(canonical(batch_frames[-1])), "step": batch_frames[-1]["step"],
                       "time_ps": batch_frames[-1]["time"], "camera": "orthographic +z toward xy, x right/y up",
                       "measurements": rows[-1],
                       "encoding": "position nm; depth brightness=z/L; radius=sigma/2 capped at18px; no interpolation",
                       "scope": "Numerical projection, not microscopy or new empirical evidence"})
        checkpoint_path = f"checkpoints/step-{step:08d}.chk"
        checkpoint_bytes = context.createCheckpoint()
        atomic_bytes(output / checkpoint_path, checkpoint_bytes)
        journal = {"schema_version": SCHEMA, "input_sha256": input_hash, "step": step,
                   "checkpoint_path": checkpoint_path, "checkpoint_sha256": digest(checkpoint_bytes),
                   "rows": rows, "chunks": chunks, "images": images,
                   "rdf_counts": rdf_counts.tolist(), "initial_positions_nm": initial_positions.tolist()}
        # The restart pointer is the commit boundary. Public indexes can be rebuilt from it.
        atomic_json(journal_path, journal)
        atomic_json(output / "trajectory/index.json", {"schema_version": SCHEMA, "time_unit": "ps",
                    "position_unit": "nm", "wrapping": "periodic [0,L)", "chunks": chunks,
                    "frame_count": len(rows), "start_time": rows[0]["time_ps"], "end_time": rows[-1]["time_ps"]})
        atomic_json(output / "observations/index.json", {"schema_version": SCHEMA, "images": images,
                    "scope": "Images are projected from retained numerical frame coordinates"})
        atomic_json(output / "measurements.json", measurements(rows, rdf_counts, rdf_edges, p, length))
        atomic_json(output / "checkpoint.json", {k: journal[k] for k in
                    ("schema_version", "input_sha256", "step", "checkpoint_path", "checkpoint_sha256")})
        batch_frames.clear(); batch_positions.clear(); batch_velocities.clear(); batch_forces.clear()

    def sample():
        nonlocal rdf_counts, rdf_edges
        row, positions, velocities, forces, counts, edges = measure(context, initial_positions, p, length, cutoff)
        rows.append(row)
        rdf_counts += counts
        rdf_edges = edges
        wrapped = np.mod(positions, length)
        batch_frames.append({"step": row["step"], "time": row["time_ps"], "entities": [
            {"id": f"ar-{i:04d}", "position": xyz.tolist()} for i, xyz in enumerate(wrapped)]})
        batch_positions.append(positions); batch_velocities.append(velocities); batch_forces.append(forces)

    if not rows:
        sample()
        commit()  # a usable checkpoint and observed initial frame exist before integration
    while step < p["steps"] and not STOP and not (output / "cancel.request").exists():
        count = min(p["sample_interval"], p["steps"] - step)
        integrator.step(count)
        step += count
        sample()
        if len(batch_frames) >= p["chunk_frames"] or step == p["steps"]:
            commit()
        atomic_json(output / "progress.json", {"fraction": step/p["steps"], "step": step,
                    "message": f"OpenMM integrating {p['atom_count']} atoms; retained instruments at {rows[-1]['time_ps']:.4f} ps",
                    "status": "running", "checkpoint_step": chunks[-1]["end_step"]})
    commit()
    status = "completed" if step == p["steps"] else "paused"
    final_measurements = measurements(rows, rdf_counts, rdf_edges, p, length)
    # Repair public views after a crash between journal commit and view publication.
    atomic_json(output / "trajectory/index.json", {"schema_version": SCHEMA, "time_unit": "ps",
                "position_unit": "nm", "wrapping": "periodic [0,L)", "chunks": chunks,
                "frame_count": len(rows), "start_time": rows[0]["time_ps"], "end_time": rows[-1]["time_ps"]})
    atomic_json(output / "observations/index.json", {"schema_version": SCHEMA, "images": images,
                "scope": "Images are projected from retained numerical frame coordinates"})
    atomic_json(output / "measurements.json", final_measurements)
    result = {"schema_version": SCHEMA, "engine": "openmm_argon", "engine_version": mm.version.version,
              "platform": platform.getName(), "status": status, "completed_steps": step,
              "requested_steps": p["steps"], "simulated_time_ps": rows[-1]["time_ps"],
              "elapsed_seconds": time.monotonic() - started, "atom_count": p["atom_count"],
              "frame_count": len(rows), "input_sha256": input_hash, "units": UNITS,
              "summary": final_measurements["summary"], "artifacts": {"manifest": "manifest.json",
              "topology": "topology.json", "trajectory": "trajectory/index.json",
              "measurements": "measurements.json", "observations": "observations/index.json",
              "checkpoint": "checkpoint.json"}, "completed_at": now(),
              "limitations": ["Small finite classical LJ system with an imposed cutoff, not calibrated physical argon",
              "Temperature/pressure averages are correlated finite samples; no equilibrium or precision claim",
              "Langevin total energy need not be conserved; NVE uses a separate conservation criterion",
              "RDF includes startup samples; MSD is not a diffusion-coefficient estimate",
              "No molecular binding, chemical reactions, biological efficacy, or empirical visual evidence"]}
    atomic_json(output / "result.json", result)
    atomic_json(output / "progress.json", {"fraction": step/p["steps"], "step": step,
                "status": status, "message": "Experiment completed" if status == "completed" else "Saved cooperative pause",
                "checkpoint_step": step})
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    signal.signal(signal.SIGTERM, request_stop)
    signal.signal(signal.SIGINT, request_stop)
    try:
        result = run(args.input, args.output)
        print(json.dumps({"status": result["status"], "steps": result["completed_steps"],
                          "result": str(args.output / "result.json")}, allow_nan=False))
        return 0 if result["status"] == "completed" else 3
    except Exception as error:
        # Bounded public diagnostic; no environment or credential values are inspected.
        message = str(error)[:2000]
        if ROOT is not None and REPORT_ERRORS:
            atomic_json(ROOT / "error.json", {"status": "failed", "error": message, "at": now()})
            previous = load_json(ROOT / "progress.json") if (ROOT / "progress.json").exists() else {"fraction": 0, "step": 0}
            atomic_json(ROOT / "progress.json", {**previous, "status": "failed", "message": message})
        print(json.dumps({"status": "failed", "error": message}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
