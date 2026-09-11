#!/usr/bin/env python3
"""Independent, preregistered mechanics inputs and numerical acceptance.

Default mode checks analytic references ONLY. --mode prepare writes fixed inputs;
--mode execute explicitly launches the supplied trusted worker. No worker code is
imported, and no integration algorithm supplies the expected trajectory. Keep the
proposal, this checker, inputs, source snapshots, failures and process receipts.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import time
import traceback
import zipfile

import numpy as np

ENGINE = "newtonian_nbody"
TAU = 2 * math.pi
PROPOSAL = Path(__file__).resolve().parents[1] / "docs/validation/mechanics-lab-proposal.md"
UNITS = {"time": "T0", "position": "L0", "velocity": "L0/T0",
         "acceleration": "L0/T0^2", "mass": "M0", "energy": "M0*L0^2/T0^2",
         "momentum": "M0*L0/T0", "angular_momentum": "M0*L0^2/T0"}
CONTRACT = {
    "schema_version": 1, "engine": ENGINE,
    "input": {"engine": ENGINE, "parameters": {
        "body_ids": "unique ordered strings", "masses": "float64-compatible [N]",
        "positions": "[N,3]", "velocities": "[N,3]", "timestep": "T0",
        "steps": "integer", "sample_interval": "integer", "chunk_frames": "integer",
        "min_separation": "L0", "boundary": "isolated", "unit_system": "scaled_G1",
        "initial_state": "Optional alternative to masses/positions/velocities: {path,sha256}; run-relative numeric NPZ with those three keys"}},
    "manifest.json": {"engine": ENGINE, "input": "normalized engine/parameters",
        "input_sha256": "canonical input", "worker_sha256": "worker source bytes",
        "engine_version": "versioned implementation", "numpy_version": "actual version",
        "initial_state_sha256": "canonical {body_ids,masses,positions,velocities}",
        "runtime": "{python_version,numpy_version,requirements_sha256}; descriptor, not a full runtime file inventory",
        "runtime_sha256": "canonical runtime descriptor",
        "platform": "CPU", "units": UNITS, "boundary": "isolated"},
    "topology.json": {"entities": [{"id": "input id", "mass": "M0",
        "display_radius": "presentation only, L0", "color": "#rrggbb"}],
        "bonds": [], "boundary": "isolated", "units": UNITS,
        "box_nm": "MUST be absent: there is no periodic box"},
    "trajectory/index.json": {"representation": "particle_trajectory",
        "boundary": "isolated", "time_unit": "T0", "position_unit": "L0",
        "units": UNITS, "frame_count": "integer", "start_time": 0,
        "end_time": "last physical time", "chunks": [{"path": "trajectory/chunk-00000.json",
        "sha256": "JSON bytes", "arrays_path": "trajectory/arrays-00000.npz",
        "arrays_sha256": "NPZ bytes", "start_frame": "zero based",
        "end_frame": "inclusive", "frame_count": "integer", "start_step": "integer",
        "end_step": "integer", "start_time": "T0", "end_time": "T0",
        "bounds": {"min": "[3] exact position minima across chunk, L0",
                   "max": "[3] exact position maxima across chunk, L0"}}]},
    "chunk_json": {"frames": [{"step": "integer", "time": "T0", "entities": [
        {"id": "stable input id", "position": "[x,y,z] in L0, Cartesian, unwrapped"}]}]},
    "chunk_npz": {"steps": "int64 [F]", "times": "float64 [F]",
        "positions": "float64 [F,N,3]", "velocities": "float64 [F,N,3]",
        "accelerations": "float64 [F,N,3], same integer-step time"},
    "measurements.json": {"units": UNITS, "series": [{"step": "integer", "time": "T0",
        "source_arrays_path": "registered array chunk", "source_arrays_sha256": "bytes",
        "source_frame_index": "chunk-local zero based index",
        "kinetic_energy": "scalar", "potential_energy": "scalar", "total_energy": "scalar",
        "momentum": "[3]", "angular_momentum": "[3]", "center_of_mass": "[3]",
        "min_separation": "scalar", "pairs": [{"ids": "[id_i,id_j], i<j",
        "distance": "scalar", "radial_velocity": "scalar",
        "relative_angle": "atan2(dy,dx) radians, declared xy plane"}]}]},
    "checkpoint.json": {"step": "integer", "input_sha256": "manifest hash",
        "initial_state_sha256": "manifest identity", "worker_sha256": "manifest identity",
        "runtime_sha256": "manifest identity", "engine_version": "manifest identity",
        "trajectory_index_sha256": "committed trajectory/index.json bytes",
        "measurements_sha256": "committed measurements.json bytes",
        "observations_index_sha256": "committed observations/index.json bytes",
        "checkpoint_path": "run-relative numeric npz", "checkpoint_sha256": "bytes"},
    "checkpoint_npz": {"step": "scalar int64", "time": "scalar float64",
        "positions": "float64 [N,3]", "velocities": "float64 [N,3]",
        "accelerations": "float64 [N,3]; exact final saved state"},
    "result.json": {"status": "completed or cancelled", "engine": ENGINE},
    "progress.json": {"step": "integer", "fraction": "[0,1]", "message": "string"},
    "observations/index.json": {"images": [{"step": "integer", "time": "T0",
        "time_unit": "T0", "path": "PNG relative path", "sha256": "image bytes",
        "source_chunk": "registered JSON chunk", "source_chunk_sha256": "bytes",
        "source_frame_index": "chunk-local zero based index",
        "source_frame_sha256": "canonical frame JSON", "measurements": "exact row snapshot",
        "camera": {"projection": "orthographic", "origin": "[3] in L0",
            "right": "unit vector [3]", "up": "unit vector [3]",
            "x_limits": "[low,high] L0", "y_limits": "[low,high] L0",
            "plot_bbox": "[left,top,right,bottom] exclusive right/bottom, image pixels",
            "marker_radius_px": "integer <=12; body colors distinct from axes/trails/text"}}]},
    "notes": ["All JSON finite; paths contained with no links; all hashes lowercase SHA256.",
        "NPZ archives are numerical only; no object dtype or pickle; <=16 MiB per chunk.",
        "Images 1024x1024, fixed camera across matching control/intervention runs.",
        "At least initial/quarter/half/three-quarter/final saved-frame images.",
        "Cancellation saves last valid state, exit 3; incompatible/corrupt resume exits 2.",
        "Same input/output resume appends without rewriting previously published chunks."]}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True,
                      allow_nan=False).encode("utf-8")


def write(path, value):
    Path(path).write_bytes(canonical(value))


def read(path):
    def finite_number(value):
        parsed = float(value)
        require(math.isfinite(parsed), "nonfinite JSON number")
        return parsed
    return json.loads(Path(path).read_text(encoding="utf-8"),
                      parse_float=finite_number,
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def sha(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def require(value, message):
    if not value:
        raise AssertionError(message)


def artifact(root, relative):
    require(isinstance(relative, str) and relative and "\\" not in relative,
            f"Invalid artifact name {relative!r}")
    parts = relative.split("/")
    require(all(part and part not in (".", "..") and ":" not in part for part in parts),
            f"Unsafe artifact name {relative!r}")
    path = root.resolve()
    for part in parts:
        path /= part
        require(not path.is_symlink() and not (hasattr(path, "is_junction") and path.is_junction()),
                f"Linked artifact {relative}")
    require(path.is_file() and path.resolve().is_relative_to(root.resolve()) and path.stat().st_nlink == 1,
            f"Artifact containment/link failure {relative}")
    return path


def numeric_archive(path):
    require(path.stat().st_size <= 16 * 1024 * 1024, "oversized array chunk")
    with zipfile.ZipFile(path) as archive:
        entries = archive.infolist()
        require(len(entries) <= 16 and sum(row.file_size for row in entries) <= 16 * 1024 * 1024,
                "oversized decompressed array chunk")
    with np.load(path, allow_pickle=False, max_header_size=10000) as arrays:
        result = {key: arrays[key].copy() for key in arrays.files}
    require(all(array.dtype.kind in "fiu" and np.isfinite(array).all()
                for array in result.values()), "nonfinite or nonnumeric archive")
    return result


def initial_state(kind, speed=1.0):
    if kind == "binary":
        masses = [0.5, 0.5]
        points = [[-.5, 0., 0.], [.5, 0., 0.]]
    elif kind == "triangle":
        masses = [1 / 3] * 3
        points = [[math.cos(TAU * k / 3) / math.sqrt(3),
                   math.sin(TAU * k / 3) / math.sqrt(3), 0.] for k in range(3)]
    else:
        raise ValueError(kind)
    velocities = [[-speed * y, speed * x, 0.] for x, y, _ in points]
    return masses, points, velocities


def parameters(kind="binary", speed=1.0, denominator=1000, period=False):
    masses, points, velocities = initial_state(kind, speed)
    return {"body_ids": [f"body-{i}" for i in range(len(masses))], "masses": masses,
            "positions": points, "velocities": velocities, "timestep": TAU / denominator,
            "steps": 4400 if period else denominator, "sample_interval": denominator // 1000,
            "chunk_frames": 50, "min_separation": .05, "boundary": "isolated",
            "unit_system": "scaled_G1"}


def force_reference(masses, points):
    """Scalar pair sum independent of any worker force implementation."""
    count = len(masses)
    accelerations = [[0., 0., 0.] for _ in masses]
    potentials, distances = [], []
    for i in range(count):
        for j in range(i + 1, count):
            delta = [points[j][axis] - points[i][axis] for axis in range(3)]
            separation = math.sqrt(math.fsum(component * component for component in delta))
            require(separation > 0, "reference singularity")
            potentials.append(-masses[i] * masses[j] / separation)
            distances.append((i, j, separation, delta))
            for axis in range(3):
                accelerations[i][axis] += masses[j] * delta[axis] / separation ** 3
                accelerations[j][axis] -= masses[i] * delta[axis] / separation ** 3
    return math.fsum(potentials), np.asarray(accelerations), distances


def measurements_reference(masses, points, velocities, ids):
    potential, acceleration, distances = force_reference(masses, points)
    total_mass = math.fsum(masses)
    kinetic = .5 * math.fsum(masses[i] * component ** 2
                            for i, velocity in enumerate(velocities) for component in velocity)
    momentum = [math.fsum(m * v[a] for m, v in zip(masses, velocities, strict=True)) for a in range(3)]
    center = [math.fsum(m * q[a] for m, q in zip(masses, points, strict=True)) / total_mass for a in range(3)]
    angular = [math.fsum(m * (q[(a + 1) % 3] * v[(a + 2) % 3]
                                  - q[(a + 2) % 3] * v[(a + 1) % 3])
                        for m, q, v in zip(masses, points, velocities, strict=True)) for a in range(3)]
    pairs = []
    for i, j, distance, delta in distances:
        radial = math.fsum((velocities[j][a] - velocities[i][a]) * delta[a]
                           for a in range(3)) / distance
        pairs.append({"ids": [ids[i], ids[j]], "distance": distance,
                      "radial_velocity": radial, "relative_angle": math.atan2(delta[1], delta[0])})
    return {"kinetic_energy": kinetic, "potential_energy": potential,
            "total_energy": kinetic + potential, "momentum": momentum,
            "angular_momentum": angular, "center_of_mass": center,
            "min_separation": min(row[2] for row in distances), "pairs": pairs}, acceleration


def kepler_reference(times, initial_positions, speed):
    """Closed-form Kepler reduction and scalar root solve, no time integration."""
    points, velocities, residuals = [], [], []
    if speed == 1.0:
        for t in times:
            c, s = math.cos(float(t)), math.sin(float(t))
            points.append([[c*x-s*y, s*x+c*y, z] for x, y, z in initial_positions])
            velocities.append([[-s*x-c*y, c*x-s*y, 0.] for x, y, _ in initial_positions])
        return np.asarray(points), np.asarray(velocities), 0.
    require(speed == .8, "exact preregistered speed required")
    semimajor, eccentricity = 25 / 34, .36
    frequency = semimajor ** -1.5
    for t in times:
        mean_anomaly = math.pi + float(t) * frequency
        eccentric_anomaly = mean_anomaly
        for _ in range(24):
            residual = eccentric_anomaly - eccentricity * math.sin(eccentric_anomaly) - mean_anomaly
            if abs(residual) < 5e-15:
                break
            eccentric_anomaly -= residual / (1 - eccentricity * math.cos(eccentric_anomaly))
        residual = abs(eccentric_anomaly - eccentricity * math.sin(eccentric_anomaly) - mean_anomaly)
        require(residual < 1e-13, "Kepler equation reference residual")
        residuals.append(residual)
        sine, cosine = math.sin(eccentric_anomaly), math.cos(eccentric_anomaly)
        rate = frequency / (1 - eccentricity * cosine)
        x = semimajor * (eccentricity - cosine)
        y = -semimajor * math.sqrt(1 - eccentricity ** 2) * sine
        dx = semimajor * sine * rate
        dy = -semimajor * math.sqrt(1 - eccentricity ** 2) * cosine * rate
        points.append([[x*qx-y*qy, y*qx+x*qy, qz] for qx, qy, qz in initial_positions])
        velocities.append([[dx*qx-dy*qy, dy*qx+dx*qy, 0.] for qx, qy, _ in initial_positions])
    return np.asarray(points), np.asarray(velocities), max(residuals, default=0.)


def reference_checks():
    masses, positions = [1., 2., 3.], [[0., 0., 0.], [1., 0., 0.], [0., 2., 0.]]
    potential, acceleration, _ = force_reference(masses, positions)
    s = math.sqrt(5)
    expected = np.asarray([[2., .75, 0.], [-1-3/(5*s), 6/(5*s), 0.],
                           [2/(5*s), -.25-4/(5*s), 0.]])
    require(abs(potential - (-3.5 - 6 / s)) < 1e-12, "frozen reference potential")
    require(np.max(np.abs(acceleration - expected)) < 1e-12, "frozen reference accelerations")
    fd = np.zeros((3, 3))
    for i in range(3):
        for a in range(3):
            plus, minus = copy.deepcopy(positions), copy.deepcopy(positions)
            plus[i][a] += 1e-5
            minus[i][a] -= 1e-5
            fd[i, a] = -(force_reference(masses, plus)[0] - force_reference(masses, minus)[0]) / 2e-5
    force_error = float(np.max(np.abs(fd - expected * np.asarray(masses)[:, None])))
    require(force_error < 1e-8, "reference force finite differences")
    results = []
    for kind in ("binary", "triangle"):
        m, q, _ = initial_state(kind)
        for speed in (1., .8):
            period = TAU if speed == 1 else TAU * (25 / 34) ** 1.5
            times = [0., period / 2, period, TAU]
            points, velocities, residual = kepler_reference(times, q, speed)
            expected_v = np.asarray(initial_state(kind, speed)[2])
            require(np.max(np.abs(points[0] - q)) < 1e-14, "reference initial positions")
            require(np.max(np.abs(velocities[0] - expected_v)) < 1e-14, "reference initial velocities")
            initial, _ = measurements_reference(m, points[0], velocities[0], list(range(len(m))))
            invariants = []
            for pp, vv in zip(points, velocities, strict=True):
                row, _ = measurements_reference(m, pp, vv, list(range(len(m))))
                require(abs(row["total_energy"] - initial["total_energy"]) < 1e-13, "analytic energy")
                require(np.max(np.abs(np.asarray(row["angular_momentum"]) - initial["angular_momentum"])) < 1e-13,
                        "analytic angular momentum")
                invariants.append(row["min_separation"])
            if speed == .8:
                require(abs(invariants[1] - 8 / 17) < 1e-14, "analytic pericenter")
            results.append({"kind": kind, "speed": speed, "period": period,
                            "reference_residual": residual, "minimum_distances": invariants})
    return {"passed": True, "scope": "pure reference mathematics; no worker or integrator executed",
            "potential": potential, "accelerations": acceleration.tolist(),
            "finite_difference_max_force_error": force_error,
            "eccentric_period_ratio": (25 / 34) ** 1.5, "cases": results}


def build_cases():
    cases = []
    for kind in ("binary", "triangle"):
        for speed in (1., .8):
            name = f"{kind}-{'circular' if speed == 1 else 'eccentric'}"
            for denominator in (1000, 2000, 4000):
                cases.append({"name": f"{name}-{denominator}", "group": "refinement",
                              "kind": kind, "speed": speed, "denominator": denominator,
                              "parameters": parameters(kind, speed, denominator)})
            cases.append({"name": f"{name}-period", "group": "period", "kind": kind,
                          "speed": speed, "parameters": parameters(kind, speed, 4000, True)})
    p = parameters()
    p.update(body_ids=["fixture-0", "fixture-1", "fixture-2"], masses=[1., 2., 3.],
             positions=[[0., 0., 0.], [1., 0., 0.], [0., 2., 0.]],
             velocities=[[0., 0., 0.]] * 3, timestep=1e-6, steps=1)
    cases.insert(0, {"name": "frozen-force", "group": "force", "parameters": p})
    p = parameters("binary", .8, 4000)
    # Tilt and translate a reference-compatible binary; this is a coordinate
    # fidelity fixture, not another integration-based source of expected answers.
    axis = np.asarray([1., 2., 3.]) / math.sqrt(14)
    angle = .71
    cross = np.asarray([[0., -axis[2], axis[1]], [axis[2], 0., -axis[0]], [-axis[1], axis[0], 0.]])
    rotation = np.eye(3) * math.cos(angle) + (1-math.cos(angle))*np.outer(axis, axis) + math.sin(angle)*cross
    translation, drift = np.asarray([.31, -.22, .17]), np.asarray([.03, -.02, .01])
    p["positions"] = (np.asarray(p["positions"]) @ rotation.T + translation).tolist()
    p["velocities"] = (np.asarray(p["velocities"]) @ rotation.T + drift).tolist()
    cases.append({"name": "tilted-translated", "group": "coordinates", "kind": "binary", "speed": .8,
                  "rotation": rotation.tolist(), "translation": translation.tolist(), "drift": drift.tolist(),
                  "parameters": p})
    return cases


class Runner:
    def __init__(self, root, worker, python, timeout):
        self.root, self.worker, self.python, self.timeout = root, worker, python, timeout

    def prepare(self, name, p, raw=None):
        folder = self.root / name
        folder.mkdir()
        inp = folder / "input.json"
        inp.write_bytes(raw if raw is not None else canonical({"engine": ENGINE, "parameters": p}))
        return folder, inp, folder / "output"

    def command(self, inp, out):
        return [self.python, "-I", str(self.worker), "--input", str(inp), "--output", str(out)]

    def execute(self, folder, inp, out, label="process", cancel=False):
        command = self.command(inp, out)
        receipt = {"command": command, "input_sha256": sha(inp), "worker_sha256": sha(self.worker),
                   "started_unix_s": time.time(), "timeout_s": self.timeout,
                   "cancel_requested_at_step": None}
        started = time.monotonic()
        process = None
        with (folder / f"{label}.stdout.txt").open("wb") as stdout, (folder / f"{label}.stderr.txt").open("wb") as stderr:
            try:
                process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
                while process.poll() is None:
                    if cancel and receipt["cancel_requested_at_step"] is None and (out / "progress.json").exists():
                        try:
                            progress = read(out / "progress.json")
                            if 0 < progress.get("step", 0) < read(inp)["parameters"]["steps"]:
                                (out / "cancel.request").write_text("Independent acceptance checkpoint request\n")
                                receipt["cancel_requested_at_step"] = progress["step"]
                        except (OSError, ValueError):
                            pass  # atomic publication may be briefly unavailable
                    if time.monotonic() - started > self.timeout:
                        process.kill()
                        process.wait()
                        receipt["timed_out"] = True
                        break
                    time.sleep(.002)
                receipt["exit_code"] = process.returncode
            except BaseException as error:
                receipt["launch_error"] = str(error)
                raise
            finally:
                if process is not None and process.poll() is None:
                    process.kill()
                    process.wait()
                receipt["elapsed_s"] = time.monotonic() - started
                write(folder / f"{label}.receipt.json", receipt)
        return receipt

    def launch(self, case):
        folder, inp, out = self.prepare(case["name"], case["parameters"])
        receipt = self.execute(folder, inp, out)
        require(receipt.get("exit_code") == 0 and not receipt.get("timed_out"), f"worker failed: {folder}")
        return out, receipt


def expected_steps(p, final=None):
    final = p["steps"] if final is None else final
    steps = list(range(0, final + 1, p["sample_interval"]))
    if steps[-1] != final:
        steps.append(final)
    return steps


def inspect_artifacts(out, p, worker, final=None, manifest_parameters=None, extra_steps=()):
    manifest, topology, index, measured = [read(out / name) for name in
        ("manifest.json", "topology.json", "trajectory/index.json", "measurements.json")]
    require(manifest["engine"] == ENGINE and manifest["platform"] == "CPU", "engine/platform provenance")
    require(manifest["worker_sha256"] == sha(worker), "worker source hash")
    require(isinstance(manifest["engine_version"], str) and manifest["engine_version"], "missing implementation version")
    require(manifest["numpy_version"] == np.__version__ == "2.4.6", "pinned NumPy runtime mismatch")
    require(manifest["runtime"]["numpy_version"] == manifest["numpy_version"]
            and isinstance(manifest["runtime"]["python_version"], str)
            and manifest["runtime"]["python_version"], "runtime descriptor/version")
    require(manifest["runtime_sha256"] == digest(manifest["runtime"]), "runtime descriptor hash")
    require(manifest["runtime"]["requirements_sha256"] == sha(PROPOSAL.parents[2] / "tools/requirements-science.txt"),
            "pinned requirements source hash")
    require(manifest["initial_state_sha256"] == digest({key: p[key] for key in
            ("body_ids", "masses", "positions", "velocities")}), "initial numerical state hash")
    require(manifest["input_sha256"] == digest(manifest["input"]), "normalized input hash")
    for key, value in (p if manifest_parameters is None else manifest_parameters).items():
        require(manifest["input"]["parameters"].get(key) == value, f"input parameter changed: {key}")
    for document in (manifest, topology, index, measured):
        require(document["units"] == UNITS, "scientific units")
    require(manifest["boundary"] == topology["boundary"] == index["boundary"] == "isolated", "boundary changed")
    require("box_nm" not in topology and "box_nm" not in index, "isolated mechanics assigned a molecular box")
    require(index["representation"] == "particle_trajectory" and index["time_unit"] == "T0"
            and index["position_unit"] == "L0", "representation/time/position units")
    ids = p["body_ids"]
    require([entity["id"] for entity in topology["entities"]] == ids, "body identity/order")
    require([entity["mass"] for entity in topology["entities"]] == p["masses"], "topology masses")
    require(topology["bonds"] == [], "unrequested rigid bonds")
    require(len({entity["color"] for entity in topology["entities"]}) == len(ids), "body marker colors must distinguish IDs")
    require(all(math.isfinite(entity["display_radius"]) and entity["display_radius"] > 0
                for entity in topology["entities"]), "invalid display radii")
    frames, positions, velocities, accelerations, steps, times, sources = [], [], [], [], [], [], []
    for chunk in index["chunks"]:
        path, arrays_path = artifact(out, chunk["path"]), artifact(out, chunk["arrays_path"])
        require(sha(path) == chunk["sha256"] and sha(arrays_path) == chunk["arrays_sha256"], "chunk hash")
        require(path.stat().st_size <= 16 * 1024 * 1024, "oversized JSON chunk")
        content, arrays = read(path)["frames"], numeric_archive(arrays_path)
        count = len(content)
        require(count > 0 and count <= p["chunk_frames"], "chunk count")
        require(chunk["start_frame"] == len(frames) and chunk["frame_count"] == count
                and chunk["end_frame"] == len(frames) + count - 1, "chunk frame range")
        require(arrays["steps"].dtype.kind in "iu" and arrays["steps"].dtype.itemsize == 8,
                "step array must be int64")
        for key in ("times", "positions", "velocities", "accelerations"):
            require(arrays[key].dtype.kind == "f" and arrays[key].dtype.itemsize == 8, f"{key} precision")
        for key in ("positions", "velocities", "accelerations"):
            require(arrays[key].shape == (count, len(ids), 3), f"{key} axis order/shape")
        require(np.array_equal(chunk["bounds"]["min"], arrays["positions"].min(axis=(0, 1)))
                and np.array_equal(chunk["bounds"]["max"], arrays["positions"].max(axis=(0, 1))),
                "immutable chunk position bounds")
        require(arrays["times"].shape == arrays["steps"].shape == (count,), "time/step shape")
        require(chunk["start_step"] == int(arrays["steps"][0]) and chunk["end_step"] == int(arrays["steps"][-1]),
                "chunk step bounds")
        require(chunk["start_time"] == float(arrays["times"][0]) and chunk["end_time"] == float(arrays["times"][-1]),
                "chunk time bounds")
        for frame, step, t, pp in zip(content, arrays["steps"], arrays["times"], arrays["positions"], strict=True):
            require(frame["step"] == int(step) and frame["time"] == float(t), "JSON/array timing mismatch")
            require([entity["id"] for entity in frame["entities"]] == ids, "JSON body order")
            require(np.array_equal([entity["position"] for entity in frame["entities"]], pp),
                    "display JSON must contain exact unwrapped coordinates")
        frames.extend(content)
        sources.extend({"source_arrays_path": chunk["arrays_path"],
                        "source_arrays_sha256": chunk["arrays_sha256"], "source_frame_index": i}
                       for i in range(count))
        steps.extend(arrays["steps"].tolist())
        times.extend(arrays["times"].tolist())
        positions.extend(arrays["positions"])
        velocities.extend(arrays["velocities"])
        accelerations.extend(arrays["accelerations"])
    require(steps == sorted(set(expected_steps(p, final) + list(extra_steps))), "initial/sampled/final step sequence")
    require(index["frame_count"] == len(frames) and index["start_time"] == times[0]
            and index["end_time"] == times[-1], "index total/time range")
    require(np.max(np.abs(np.asarray(times) - np.asarray(steps)*p["timestep"])) < 1e-12, "physical time != step*h")
    require(np.array_equal(positions[0], p["positions"]) and np.array_equal(velocities[0], p["velocities"]),
            "initial state changed")
    rows = measured["series"]
    require([row["step"] for row in rows] == steps and len(rows) == len(frames), "measurement step mapping")
    reference_rows, instrument_error, acceleration_error = [], 0., 0.
    for pp, vv, aa, t, row, source in zip(positions, velocities, accelerations, times, rows, sources, strict=True):
        expected, expected_acceleration = measurements_reference(p["masses"], pp, vv, ids)
        require(row["time"] == t, "measurement time mapping")
        require(all(row.get(key) == value for key, value in source.items()), "measurement source-array binding")
        acceleration_error = max(acceleration_error, float(np.max(np.abs(aa-expected_acceleration))))
        for key in ("kinetic_energy", "potential_energy", "total_energy", "momentum",
                    "angular_momentum", "center_of_mass", "min_separation"):
            actual_values = np.asarray(row[key])
            require(actual_values.dtype.kind in "fiu" and np.isfinite(actual_values).all(), "nonfinite/nonnumeric instrument")
            require(actual_values.shape == np.asarray(expected[key]).shape, "instrument scalar/vector shape")
            instrument_error = max(instrument_error, float(np.max(np.abs(actual_values-expected[key]))))
        require(len(row["pairs"]) == len(expected["pairs"]), "pair instrument count")
        for actual_pair, expected_pair in zip(row["pairs"], expected["pairs"], strict=True):
            require(actual_pair["ids"] == expected_pair["ids"], "pair instrument body mapping")
            for key in ("distance", "radial_velocity", "relative_angle"):
                require(type(actual_pair[key]) in (int, float) and math.isfinite(actual_pair[key]),
                        "nonfinite/nonnumeric pair instrument")
                difference = float(actual_pair[key]) - expected_pair[key]
                if key == "relative_angle":
                    difference = math.atan2(math.sin(difference), math.cos(difference))
                instrument_error = max(instrument_error, abs(difference))
        reference_rows.append(expected)
    require(instrument_error < 1e-10, f"instrument error {instrument_error}")
    require(acceleration_error < 1e-10, f"saved acceleration error {acceleration_error}")
    checkpoint = read(out / "checkpoint.json")
    require(checkpoint["step"] == steps[-1] and checkpoint["input_sha256"] == manifest["input_sha256"],
            "checkpoint endpoint/input")
    for key in ("initial_state_sha256", "worker_sha256", "runtime_sha256", "engine_version"):
        require(checkpoint[key] == manifest[key], f"checkpoint identity {key}")
    for key, name in (("trajectory_index_sha256", "trajectory/index.json"),
                      ("measurements_sha256", "measurements.json"),
                      ("observations_index_sha256", "observations/index.json")):
        require(checkpoint[key] == sha(artifact(out, name)), f"checkpoint committed receipt {key}")
    checkpoint_path = artifact(out, checkpoint["checkpoint_path"])
    require(sha(checkpoint_path) == checkpoint["checkpoint_sha256"], "checkpoint hash")
    saved = numeric_archive(checkpoint_path)
    require(saved["step"].shape == () and saved["step"].dtype.kind in "iu" and saved["step"].dtype.itemsize == 8
            and int(saved["step"]) == steps[-1], "checkpoint integer step")
    require(saved["time"].shape == () and saved["time"].dtype.kind == "f" and saved["time"].dtype.itemsize == 8
            and float(saved["time"]) == times[-1], "checkpoint physical time")
    for key, arrays in (("positions", positions), ("velocities", velocities), ("accelerations", accelerations)):
        state = saved[key]
        last = np.asarray(arrays[-1])
        require(state.dtype == last.dtype and state.shape == last.shape and state.tobytes() == last.tobytes(),
                f"checkpoint {key} is not the committed full-step state")
    result = read(out / "result.json")
    require(result["engine"] == ENGINE and result["status"] == ("completed" if final is None else "cancelled"),
            "scientific completion/cancellation result status")
    return {"positions": np.asarray(positions), "velocities": np.asarray(velocities), "times": np.asarray(times),
            "steps": steps, "frames": frames, "rows": reference_rows, "index": index, "topology": topology,
            "instrument_error": instrument_error, "acceleration_error": acceleration_error}


def invariant_errors(data, p):
    rows, times = data["rows"], data["times"]
    first = rows[0]
    total_mass = math.fsum(p["masses"])
    energy = max(abs(row["total_energy"]-first["total_energy"]) for row in rows) / abs(first["total_energy"])
    momentum = max(float(np.linalg.norm(np.asarray(row["momentum"])-first["momentum"])) for row in rows)
    angular = max(float(np.linalg.norm(np.asarray(row["angular_momentum"])-first["angular_momentum"])) for row in rows)
    center = max(float(np.linalg.norm(np.asarray(row["center_of_mass"])-first["center_of_mass"]
                -np.asarray(first["momentum"])*t/total_mass)) for row, t in zip(rows, times, strict=True))
    result = {"relative_energy_drift": energy, "momentum_drift": momentum,
              "angular_momentum_drift": angular, "center_of_mass_drift": center}
    require(energy < 2e-4 and momentum < 1e-11 and angular < 1e-11 and center < 1e-11,
            f"invariant tolerance: {result}")
    return result


def trajectory_errors(data, case):
    initial = initial_state(case["kind"], case["speed"])[1]
    expected_q, expected_v, residual = kepler_reference(data["times"], initial, case["speed"])
    if case["group"] == "coordinates":
        rotation = np.asarray(case["rotation"])
        expected_q = expected_q @ rotation.T + case["translation"] + data["times"][:, None, None]*np.asarray(case["drift"])
        expected_v = expected_v @ rotation.T + case["drift"]
    q_error = float(np.sqrt(np.mean(np.sum((data["positions"]-expected_q)**2, axis=2))))
    v_error = float(np.sqrt(np.mean(np.sum((data["velocities"]-expected_v)**2, axis=2))))
    return {"position_rms_L0": q_error, "velocity_rms_L0_T0": v_error, "kepler_residual": residual}


def full_orbit_period(data):
    delta = data["positions"][:, 1] - data["positions"][:, 0]
    angle = np.unwrap(np.arctan2(delta[:, 1], delta[:, 0]))
    phase = angle - angle[0]
    crossings = np.flatnonzero(phase >= TAU)
    require(len(crossings) > 0 and crossings[0] > 0, "no bracketed first full orbit")
    i = int(crossings[0])
    t0, t1, a0, a1 = data["times"][i-1], data["times"][i], phase[i-1], phase[i]
    require(a1 > a0, "orbit event not a forward crossing")
    period = t0 + (TAU-a0)/(a1-a0)*(t1-t0)
    return {"period_T0": float(period), "bracket_T0": [float(t0), float(t1)],
            "method": "linear interpolation of first forward 2pi pair angle advance"}


def inspect_images(out, data, require_milestones=True):
    from PIL import Image
    observations = read(out / "observations/index.json")
    images = observations["images"]
    if require_milestones:
        count = len(data["frames"])
        expected = {data["steps"][round((count-1)*fraction)] for fraction in (0., .25, .5, .75, 1.)}
        require(expected.issubset({entry["step"] for entry in images}), "missing declared image milestones")
    require(len(images) > 0, "no observed scientific image")
    cameras, comparisons, max_center_error = [], 0, 0.
    rows = {row["step"]: row for row in read(out / "measurements.json")["series"]}
    for entry in images:
        path, source_path = artifact(out, entry["path"]), artifact(out, entry["source_chunk"])
        require(sha(path) == entry["sha256"] and sha(source_path) == entry["source_chunk_sha256"], "image/source hash")
        source = read(source_path)["frames"][entry["source_frame_index"]]
        registered = next((chunk for chunk in data["index"]["chunks"]
                           if chunk["path"] == entry["source_chunk"]), None)
        require(registered is not None and registered["sha256"] == entry["source_chunk_sha256"],
                "observation source is not a registered trajectory chunk")
        local_index = entry["source_frame_index"]
        require(type(local_index) is int and 0 <= local_index < registered["frame_count"], "observation source frame index")
        require(source == data["frames"][registered["start_frame"] + local_index],
                "observation source differs from the validated numerical frame")
        require(digest(source) == entry["source_frame_sha256"], "observed frame hash")
        require(entry["step"] == source["step"] and entry["time"] == source["time"]
                and entry["time_unit"] == "T0", "observed scientific time")
        require(entry["measurements"] == rows[entry["step"]], "observation instrument snapshot")
        camera = entry["camera"]
        require(camera["projection"] == "orthographic", "unexpected observation projection")
        origin, right, up = [np.asarray(camera[key], dtype=float) for key in ("origin", "right", "up")]
        require(all(x.shape == (3,) and np.isfinite(x).all() for x in (origin, right, up)), "camera vectors")
        require(abs(np.linalg.norm(right)-1) < 1e-12 and abs(np.linalg.norm(up)-1) < 1e-12
                and abs(np.dot(right, up)) < 1e-12, "camera basis is not orthonormal")
        left, top, edge_right, bottom = camera["plot_bbox"]
        xmin, xmax = camera["x_limits"]
        ymin, ymax = camera["y_limits"]
        radius = camera["marker_radius_px"]
        require(0 <= left < edge_right <= 1024 and 0 <= top < bottom <= 1024
                and xmax > xmin and ymax > ymin and 1 <= radius <= 12, "camera bounds")
        cameras.append(camera)
        with Image.open(path) as image:
            require(image.size == (1024, 1024) and image.format == "PNG", "observation PNG shape")
            pixels = np.asarray(image.convert("RGB"))
        for entity, topology in zip(source["entities"], data["topology"]["entities"], strict=True):
            point = np.asarray(entity["position"]) - origin
            x, y = np.dot(point, right), np.dot(point, up)
            px = left + (x-xmin)/(xmax-xmin)*(edge_right-left-1)
            py = top + (ymax-y)/(ymax-ymin)*(bottom-top-1)
            require(left+radius <= px < edge_right-radius and top+radius <= py < bottom-radius,
                    "declared fixed camera clips acceptance body")
            color = topology["color"]
            require(isinstance(color, str) and len(color) == 7 and color[0] == "#", "body palette color")
            rgb = np.asarray([int(color[a:a+2], 16) for a in (1, 3, 5)])
            x0, x1 = max(left, int(math.floor(px))-radius-2), min(edge_right, int(math.ceil(px))+radius+3)
            y0, y1 = max(top, int(math.floor(py))-radius-2), min(bottom, int(math.ceil(py))+radius+3)
            mask = np.all(pixels[y0:y1, x0:x1] == rgb, axis=2)
            yy, xx = np.nonzero(mask)
            require(len(xx) >= 3, "body marker is not identifiable at projected position")
            error = float(math.hypot(float(xx.mean()+x0)-px, float(yy.mean()+y0)-py))
            require(error <= 1., f"rendered center differs from stored state by {error} pixels")
            max_center_error = max(max_center_error, error)
            comparisons += 1
    require(all(camera == cameras[0] for camera in cameras), "camera changes within a scientific comparison")
    return {"images": len(images), "projected_markers": comparisons,
            "max_marker_center_error_px": max_center_error, "camera": cameras[0]}


def recovery_check(runner):
    p = parameters("triangle", .8, 4000)
    folder, inp, out = runner.prepare("recovery", p)
    paused = runner.execute(folder, inp, out, "pause", cancel=True)
    require(paused.get("exit_code") == 3 and paused["cancel_requested_at_step"] is not None,
            "cooperative cancellation did not interrupt actual numerical progress")
    checkpoint = read(out / "checkpoint.json")
    require(0 < checkpoint["step"] < p["steps"], "checkpoint is not intermediate")
    inspect_artifacts(out, p, runner.worker, final=checkpoint["step"])
    # This checker owns the cooperative request. Clear it deliberately before
    # testing resume so cancellation cannot mask compatibility/hash validation.
    (out / "cancel.request").unlink(missing_ok=True)
    index = read(out / "trajectory/index.json")
    old_bytes = {entry[key]: sha(artifact(out, entry[key])) for entry in index["chunks"]
                 for key in ("path", "arrays_path")}
    # Invalid resumes each use a copy, preserving the original recoverable evidence.
    copied = folder / "changed-output"
    shutil.copytree(out, copied)
    changed = copy.deepcopy(p)
    changed["timestep"] /= 2
    changed_inp = folder / "changed-input.json"
    write(changed_inp, {"engine": ENGINE, "parameters": changed})
    refused = runner.execute(folder, changed_inp, copied, "changed-input")
    require(refused.get("exit_code") == 2, "incompatible checkpoint accepted")
    corrupt = folder / "corrupt-output"
    shutil.copytree(out, corrupt)
    path = artifact(corrupt, checkpoint["checkpoint_path"])
    content = bytearray(path.read_bytes())
    content[len(content)//2] ^= 1
    path.write_bytes(content)
    refused_corrupt = runner.execute(folder, inp, corrupt, "corrupt-checkpoint")
    require(refused_corrupt.get("exit_code") == 2, "corrupt checkpoint accepted")
    resumed = runner.execute(folder, inp, out, "resume")
    require(resumed.get("exit_code") == 0, "same-input resume failed")
    require(all(sha(artifact(out, name)) == old_hash for name, old_hash in old_bytes.items()),
            "resume rewrote published trajectory samples")
    continuous_case = {"name": "recovery-uninterrupted", "parameters": p}
    continuous, _ = runner.launch(continuous_case)
    resumed_data = inspect_artifacts(out, p, runner.worker, extra_steps=[checkpoint["step"]])
    continuous_data = inspect_artifacts(continuous, p, runner.worker)
    final = [resumed_data[key][-1] for key in ("positions", "velocities")]
    comparison = [continuous_data[key][-1] for key in ("positions", "velocities")]
    require(all(a.dtype == b.dtype and a.shape == b.shape and a.tobytes() == b.tobytes()
                for a, b in zip(final, comparison, strict=True)),
            "resumed final state is not bit-identical")
    final_index = read(out / "trajectory/index.json")
    all_steps = [frame["step"] for chunk in final_index["chunks"]
                 for frame in read(artifact(out, chunk["path"]))["frames"]]
    require(all_steps == sorted(set(expected_steps(p) + [checkpoint["step"]])),
            "resume duplicated/missed regular or pause sample")
    return {"paused_step": checkpoint["step"], "original_samples_preserved": True,
            "changed_input_exit": refused["exit_code"], "corrupt_checkpoint_exit": refused_corrupt["exit_code"],
            "resume_exit": resumed["exit_code"], "final_bit_identical": True}


def invalid_checks(runner):
    base = parameters()
    cases = []
    def invalid(name, **changes):
        p = copy.deepcopy(base)
        p.update(changes)
        cases.append((name, p, None))
    invalid("zero-mass", masses=[0., .5])
    invalid("negative-mass", masses=[-.5, .5])
    invalid("wrong-shape", positions=[[0., 0.], [1., 0.]])
    invalid("duplicate-id", body_ids=["same", "same"])
    invalid("coincident", positions=[[0., 0., 0.], [0., 0., 0.]])
    invalid("subguard", positions=[[0., 0., 0.], [.04, 0., 0.]])
    invalid("unresolved-step", timestep=.1)
    invalid("unsupported-softening", softening=.001)
    invalid("unsupported-boundary", boundary="periodic")
    invalid("too-many-steps", steps=200001)
    invalid("too-many-frames", steps=20000, sample_interval=1)
    invalid("pair-work-budget", body_ids=[f"p-{i}" for i in range(128)], masses=[1/128]*128,
            positions=[[float(i), 0., 0.] for i in range(128)], velocities=[[0., 0., 0.]]*128,
            steps=30000, sample_interval=100)
    raw = canonical({"engine": ENGINE, "parameters": base}).replace(b'"timestep":'+canonical(base["timestep"]),
                                                                     b'"timestep":NaN')
    require(b":NaN" in raw, "nonfinite invalid fixture construction")
    cases.append(("nonfinite-json", base, raw))
    results = []
    for name, p, raw in cases:
        folder, inp, out = runner.prepare("invalid-"+name, p, raw)
        receipt = runner.execute(folder, inp, out)
        require(receipt.get("exit_code") == 2, f"invalid input {name} did not fail closed")
        if (out / "result.json").exists():
            require(read(out / "result.json").get("status") != "completed", f"invalid {name} reported completed")
        if (out / "progress.json").exists():
            require(read(out / "progress.json").get("step", 0) == 0, f"invalid {name} advanced")
        results.append({"name": name, "exit_code": receipt["exit_code"]})
    # A numeric archive is an alternate initial-state contract, not arbitrary code.
    p = copy.deepcopy(base)
    for key in ("masses", "positions", "velocities"):
        del p[key]
    folder, inp, out = runner.prepare("invalid-object-array", p)
    (out / "inputs").mkdir(parents=True)
    np.savez(out / "inputs/state.npz", masses=np.asarray(["not", "numeric"], dtype=object),
             positions=np.asarray(base["positions"]), velocities=np.asarray(base["velocities"]))
    p["initial_state"] = {"path": "inputs/state.npz", "sha256": sha(out / "inputs/state.npz")}
    write(inp, {"engine": ENGINE, "parameters": p})
    receipt = runner.execute(folder, inp, out)
    require(receipt.get("exit_code") == 2, "object archive accepted")
    results.append({"name": "object-array", "exit_code": receipt["exit_code"]})
    return results


def imported_state_check(runner):
    """A positive import control prevents unsupported-parameter rejection passing as dtype validation."""
    inline = parameters("triangle", .8, 4000)
    inline.update(steps=40, sample_interval=4)
    p = copy.deepcopy(inline)
    for key in ("masses", "positions", "velocities"):
        del p[key]
    folder, inp, out = runner.prepare("numeric-initial-state", p)
    (out / "inputs").mkdir(parents=True)
    path = out / "inputs/state.npz"
    np.savez(path, **{key: np.asarray(inline[key], dtype=np.float64)
                    for key in ("masses", "positions", "velocities")})
    source_hash = sha(path)
    p["initial_state"] = {"path": "inputs/state.npz", "sha256": source_hash}
    write(inp, {"engine": ENGINE, "parameters": p})
    receipt = runner.execute(folder, inp, out)
    require(receipt.get("exit_code") == 0, "valid numeric initial archive rejected")
    require(sha(path) == source_hash, "worker modified the imported initial archive")
    data = inspect_artifacts(out, inline, runner.worker, manifest_parameters=p)
    # Compare to a separately supplied, identical inline case: same equations,
    # no reference generated by running the worker itself.
    reference_q, reference_v, _ = kepler_reference(data["times"], inline["positions"], .8)
    q_error = float(np.max(np.abs(data["positions"]-reference_q)))
    v_error = float(np.max(np.abs(data["velocities"]-reference_v)))
    require(q_error < 2e-4 and v_error < 2e-4, "imported-state reference error")
    changed_output = folder / "changed-source-output"
    shutil.copytree(out, changed_output)
    changed_source = changed_output / "inputs/state.npz"
    changed_values = {key: np.asarray(inline[key], dtype=np.float64)
                      for key in ("masses", "positions", "velocities")}
    changed_values["positions"][0, 0] += .001
    np.savez(changed_source, **changed_values)
    refusal = runner.execute(folder, inp, changed_output, "changed-source")
    require(refusal.get("exit_code") == 2, "changed imported source accepted under old identity")
    return {"source_sha256": source_hash, "exit_code": receipt["exit_code"],
            "changed_source_exit": refusal["exit_code"],
            "max_position_error": q_error, "max_velocity_error": v_error}


def execute_suite(runner, report):
    cases = build_cases()
    outputs, inspected = {}, {}
    def group(name, function):
        started = time.monotonic()
        try:
            value = function()
            report["tests"][name] = {"passed": True, "result": value, "elapsed_s": time.monotonic()-started}
        except Exception as error:
            report["tests"][name] = {"passed": False, "error": str(error), "traceback": traceback.format_exc(),
                                      "elapsed_s": time.monotonic()-started}
        write(runner.root / "report.json", report)

    for case in cases:
        def run_case(case=case):
            out, receipt = runner.launch(case)
            data = inspect_artifacts(out, case["parameters"], runner.worker)
            outputs[case["name"]], inspected[case["name"]] = out, data
            result = {"output": str(out), "process": receipt, "frames": len(data["frames"]),
                      "instrument_error": data["instrument_error"], "acceleration_error": data["acceleration_error"]}
            if case["group"] == "force":
                expected = reference_checks()
                first = numeric_archive(artifact(out, data["index"]["chunks"][0]["arrays_path"]))
                require(np.max(np.abs(first["accelerations"][0]-expected["accelerations"])) < 1e-12,
                        "worker frozen acceleration error")
                actual_potential = read(out / "measurements.json")["series"][0]["potential_energy"]
                require(abs(actual_potential-expected["potential"]) < 1e-12,
                        "worker frozen potential error")
            else:
                result.update(invariant_errors(data, case["parameters"]))
                result.update(trajectory_errors(data, case))
                if case["group"] == "coordinates":
                    require(result["position_rms_L0"] < 2e-4 and result["velocity_rms_L0_T0"] < 2e-4,
                            "tilted/translated fixture exceeds the same finest eccentric reference limit")
                if case["group"] == "refinement":
                    denominator = case["denominator"]
                    limit = (2e-4 if case["speed"] == 1. else 3.2e-3)*(1000/denominator)**2
                    if denominator in (1000, 4000):
                        require(result["position_rms_L0"] < limit and result["velocity_rms_L0_T0"] < limit,
                                f"trajectory tolerance {limit}: {result}")
                if case["group"] == "period":
                    result.update(full_orbit_period(data))
            if case["group"] in ("coordinates", "period"):
                result["images"] = inspect_images(out, data)
            return result
        group(case["name"], run_case)

    def comparisons():
        results = []
        for kind in ("binary", "triangle"):
            for speed in ("circular", "eccentric"):
                prefix = f"{kind}-{speed}"
                errors = [report["tests"][f"{prefix}-{d}"] for d in (1000, 2000, 4000)]
                require(all(entry["passed"] for entry in errors), "refinement prerequisites failed")
                rates = {}
                for key in ("position_rms_L0", "velocity_rms_L0_T0"):
                    ratios = [errors[i]["result"][key]/errors[i+1]["result"][key] for i in (0, 1)]
                    require(all(3.5 <= ratio <= 4.5 for ratio in ratios), f"{prefix} {key} convergence {ratios}")
                    rates[key] = ratios
                results.append({"case": prefix, "refinement_factors": rates})
            circle, eccentric = [inspected[f"{kind}-{shape}-4000"] for shape in ("circular", "eccentric")]
            minimum = min(row["min_separation"] for row in eccentric["rows"])
            require(abs(minimum-8/17) <= 5e-4 and minimum < .5, "controlled pericenter")
            require(max(abs(row["min_separation"]-1) for row in circle["rows"]) <= 2e-4, "circular separation")
            periods = [report["tests"][f"{kind}-{shape}-period"] for shape in ("circular", "eccentric")]
            require(all(entry["passed"] for entry in periods), "period prerequisites failed")
            ratio = periods[1]["result"]["period_T0"] / periods[0]["result"]["period_T0"]
            require(abs(ratio/(25/34)**1.5-1) <= .001, "controlled period ratio")
            cameras = [entry["result"]["images"]["camera"] for entry in periods]
            require(cameras[0] == cameras[1], "control/intervention cameras changed")
            results.append({"kind": kind, "minimum_separation": minimum, "period_ratio": ratio,
                            "false_constant_separation_hypothesis_rejected": True})
        return results
    group("refinement-and-intervention", comparisons)
    group("checkpoint-recovery", lambda: recovery_check(runner))
    group("numeric-initial-state", lambda: imported_state_check(runner))
    group("invalid-inputs", lambda: invalid_checks(runner))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("references", "prepare", "execute"), default="references")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--worker", type=Path)
    parser.add_argument("--python", default=sys.executable)
    parser.add_argument("--timeout", type=float, default=120.)
    parser.add_argument("--describe-contract", action="store_true")
    args = parser.parse_args()
    if args.describe_contract:
        print(json.dumps(CONTRACT, indent=2))
        return 0
    if args.output is None:
        parser.error("--output is required; use a new evidence directory")
    root = args.output.resolve()
    if root.exists():
        parser.error("--output must be a new directory; prior evidence is never overwritten")
    if not math.isfinite(args.timeout) or args.timeout <= 0:
        parser.error("--timeout must be a positive finite checker process budget")
    if args.mode == "execute" and (args.worker is None or not args.worker.is_file()):
        parser.error("--mode execute requires --worker; reference/prepare modes never launch a worker")
    root.mkdir(parents=True)
    started = time.monotonic()
    shutil.copy2(__file__, root / "checker-source.py")
    shutil.copy2(PROPOSAL, root / "criteria.md")
    write(root / "worker-contract.json", CONTRACT)
    report = {"schema_version": 1, "mode": args.mode, "criteria_sha256": sha(PROPOSAL),
              "checker_sha256": sha(Path(__file__)), "started_unix_s": time.time(),
              "numpy_version": np.__version__, "worker_executed": False, "tests": {},
              "numerical_acceptance_passed": False, "installed_acceptance": "not evaluated"}
    try:
        report["tests"]["pure-references"] = reference_checks()
        cases = build_cases()
        write(root / "cases.json", cases)
        if args.mode == "prepare":
            for case in cases:
                folder = root / "inputs" / case["name"]
                folder.mkdir(parents=True)
                write(folder / "input.json", {"engine": ENGINE, "parameters": case["parameters"]})
        if args.mode == "execute":
            worker = args.worker.resolve()
            shutil.copy2(worker, root / "worker-source.py")
            report["worker_sha256"] = sha(worker)
            report["worker_executed"] = True
            execute_suite(Runner(root, worker, args.python, args.timeout), report)
            report["numerical_acceptance_passed"] = all(entry["passed"] for entry in report["tests"].values())
        report["passed"] = all(entry["passed"] for entry in report["tests"].values())
    except Exception as error:
        report.update(passed=False, error=str(error), traceback=traceback.format_exc())
    finally:
        report["elapsed_s"] = time.monotonic()-started
        write(root / "report.json", report)
    print(json.dumps({"report": str(root / "report.json"), "mode": args.mode,
                      "passed": report["passed"], "worker_executed": report["worker_executed"],
                      "numerical_acceptance_passed": report["numerical_acceptance_passed"]}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
