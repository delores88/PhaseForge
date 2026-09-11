"""Trusted isolated Newtonian N-body worker, in scaled units with G*=1.

The scientific state is float64 position, full-step velocity and acceleration.
Every step is an actual pair-force velocity-Verlet half kick / drift / half kick.
Numeric chunks are authoritative; JSON coordinates and PNGs are derivatives.
"""
from __future__ import annotations

import argparse
import colorsys
import ctypes
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import sys
import time
import zipfile

import numpy as np
from PIL import Image, ImageDraw, ImageFont, __version__ as PILLOW_VERSION

ENGINE = "newtonian_nbody"
ENGINE_VERSION = "1.0.0"
UNITS = {"time": "T0", "position": "L0", "velocity": "L0/T0",
         "acceleration": "L0/T0^2", "mass": "M0", "energy": "M0*L0^2/T0^2",
         "momentum": "M0*L0/T0", "angular_momentum": "M0*L0^2/T0"}
MAX_OUTPUT = 512 * 1024**2
MAX_WORKING = 128 * 1024**2
MAX_PROCESS = 512 * 1024**2  # Includes interpreter and native dependency overhead.
MAX_ARCHIVE = 16 * 1024**2
PUBLICATION_RETRY_SECONDS = 2.0
IDENTITIES = ("input_sha256", "initial_state_sha256", "worker_sha256",
              "runtime_sha256", "engine_version")
RECEIPTS = {"trajectory/index.json": "trajectory_index_sha256",
            "measurements.json": "measurements_sha256",
            "observations/index.json": "observations_index_sha256"}


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_hash(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def duplicate_safe(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def parse_json(data):
    def invalid(value):
        raise ValueError(f"Nonfinite JSON number: {value}")
    result = json.loads(data, parse_constant=invalid, object_pairs_hook=duplicate_safe)
    canonical(result)  # Also rejects overflow such as 1e999, recursively.
    return result


def replace_published(temporary, path):
    """Bounded retry for native Windows readers denying atomic replacement."""
    deadline = time.monotonic() + PUBLICATION_RETRY_SECONDS
    delay = .005
    while True:
        try:
            os.replace(temporary, path)
            return
        except OSError as error:
            if os.name != "nt" or getattr(error, "winerror", None) not in (5, 32, 33):
                raise
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise
            time.sleep(min(delay, remaining))
            delay = min(delay * 2, .05)


def require(value, message):
    if not value:
        raise ValueError(message)


def only_keys(value, allowed, name):
    require(isinstance(value, dict) and not set(value) - set(allowed),
            f"{name} must be an object with only supported fields")


def integer(name, value, low, high):
    require(type(value) is int and low <= value <= high,
            f"{name} must be an integer in [{low}, {high}]")
    return value


def number(name, value, low, high):
    require(type(value) in (int, float) and math.isfinite(value) and low <= value <= high,
            f"{name} must be a finite number in [{low}, {high}]")
    return float(value)


def hash_string(value):
    require(isinstance(value, str) and len(value) == 64
            and all(c in "0123456789abcdef" for c in value), "Expected lowercase SHA-256")
    return value


def safe_path(root, relative, *, existing=False):
    require(isinstance(relative, str) and 0 < len(relative) <= 240
            and "\\" not in relative and ":" not in relative, "Invalid run-relative path")
    parts = relative.split("/")
    require(all(p not in ("", ".", "..") and not p.endswith((".", " ")) for p in parts),
            "Invalid run-relative path components")
    path = root
    for part in parts:
        path = path / part
        require(not path.is_symlink() and not (hasattr(path, "is_junction") and path.is_junction()),
                "Artifact links are forbidden")
    require(path.resolve().is_relative_to(root.resolve()), "Artifact escapes run directory")
    if path.exists():
        require(path.is_file() and path.stat().st_nlink == 1, "Artifact must be a plain unlinked file")
    elif existing:
        raise ValueError(f"Missing retained artifact: {relative}")
    return path


def numeric_archive(data):
    require(len(data) <= MAX_ARCHIVE, "Numeric archive exceeds 16 MiB")
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        entries = archive.infolist()
        require(0 < len(entries) <= 8 and sum(e.file_size for e in entries) <= MAX_ARCHIVE,
                "Numeric archive expands beyond its bound")
        require(len({e.filename for e in entries}) == len(entries)
                and all("/" not in e.filename and "\\" not in e.filename
                        and e.filename.endswith(".npy") for e in entries), "Invalid numeric archive members")
        for entry in entries:
            with archive.open(entry) as stream:
                version = np.lib.format.read_magic(stream)
                require(version in ((1, 0), (2, 0)), "Unsupported numeric array header version")
                reader = (np.lib.format.read_array_header_1_0 if version == (1, 0)
                          else np.lib.format.read_array_header_2_0)
                shape, _, dtype = reader(stream, max_header_size=4096)
                require(dtype.kind in "fiu" and not dtype.hasobject and len(shape) <= 3,
                        "Numeric archive header must describe bounded real arrays")
                size = math.prod(shape) * dtype.itemsize
                require(0 <= size <= MAX_ARCHIVE and stream.tell()+size == entry.file_size,
                        "Numeric array shape exceeds its stored bytes or memory bound")
    with np.load(io.BytesIO(data), allow_pickle=False) as archive:
        result = {key: archive[key] for key in archive.files}
    require(all(a.dtype.kind in "fiu" and np.isfinite(a).all() for a in result.values()),
            "Archive arrays must be finite real numbers without objects or pickle")
    return result


def array_value(name, value, shape):
    if isinstance(value, list):
        def numeric(item):
            return all(numeric(x) for x in item) if isinstance(item, list) else type(item) in (int, float)
        require(numeric(value), f"{name} contains a nonnumeric or boolean value")
    elif not isinstance(value, np.ndarray):
        raise ValueError(f"{name} must be an array")
    original = np.asarray(value)
    require(original.dtype.kind in "fiu" and original.shape == shape,
            f"{name} must be a real numeric array with shape {shape}")
    result = np.asarray(original, dtype=np.float64, order="C")
    require(np.isfinite(result).all(), f"{name} must be finite")
    return result


def parameters(raw, root):
    only_keys(raw, ("body_ids", "masses", "positions", "velocities", "initial_state",
                   "timestep", "steps", "sample_interval", "chunk_frames", "min_separation",
                   "boundary", "unit_system"), "parameters")
    ids = raw.get("body_ids")
    require(isinstance(ids, list) and 2 <= len(ids) <= 128
            and all(isinstance(x, str) and bool(x.strip()) and 1 <= len(x) <= 80
                    and not any(ord(c) < 32 or 127 <= ord(c) <= 159 for c in x) for x in ids),
            "body_ids requires 2..128 bounded strings without control characters")
    require(len(set(ids)) == len(ids), "body_ids must be unique")
    n = len(ids)
    p = {"body_ids": ids,
         "timestep": number("timestep", raw.get("timestep", .001), 1e-12, 1e6),
         "steps": integer("steps", raw.get("steps", 1000), 1, 200_000),
         "sample_interval": integer("sample_interval", raw.get("sample_interval", 10), 1, 200_000),
         "chunk_frames": integer("chunk_frames", raw.get("chunk_frames", 50), 1, 100),
         "min_separation": number("min_separation", raw.get("min_separation", .05), 1e-12, 1e6),
         "boundary": raw.get("boundary", "isolated"), "unit_system": raw.get("unit_system", "scaled_G1")}
    require(p["boundary"] == "isolated" and p["unit_system"] == "scaled_G1",
            "Only isolated boundaries and explicit scaled_G1 units are supported")
    pairs = n * (n - 1) // 2
    frames = math.ceil(p["steps"] / p["sample_interval"]) + 1
    require(frames <= 10_001, "More than 10001 retained frames requested")
    require((p["steps"] + 1) * pairs <= 200_000_000, "Pair evaluation budget exceeds 200000000")
    estimate = 32 * 1024**2 + frames * (512*n + 384*(pairs if n <= 16 else 0) + 4096)
    working = 16 * 1024**2 + n*n*128 + p["chunk_frames"]*n*9*8
    require(estimate <= MAX_OUTPUT, "Estimated retained output exceeds 512 MiB")
    require(working <= MAX_WORKING, "Estimated working memory exceeds 128 MiB")
    names = ("masses", "positions", "velocities")
    if "initial_state" in raw:
        require(not any(key in raw for key in names), "Use inline arrays or initial_state, never both")
        source = raw["initial_state"]
        only_keys(source, ("path", "sha256"), "initial_state")
        path = safe_path(root, source.get("path"), existing=True)
        require(path.suffix == ".npz" and path.stat().st_size <= MAX_ARCHIVE,
                "initial_state must be a numeric NPZ of at most 16 MiB")
        data = path.read_bytes()
        require(digest(data) == hash_string(source.get("sha256")), "Initial source SHA-256 mismatch")
        values = numeric_archive(data)
        require(set(values) == set(names), "Initial NPZ must contain masses, positions and velocities only")
        p["initial_state"] = dict(source)
    else:
        require(all(key in raw for key in names), "Inline masses, positions and velocities are required")
        values = raw
    m = array_value("masses", values["masses"], (n,))
    q = array_value("positions", values["positions"], (n, 3))
    v = array_value("velocities", values["velocities"], (n, 3))
    require(np.all((m >= 1e-9) & (m <= 1e9)), "Masses must be positive in [1e-9, 1e9] M0")
    require(np.max(np.abs(q)) <= 1e6 and np.max(np.abs(v)) <= 1e6,
            "Initial coordinate/velocity components exceed 1e6 scaled units")
    initial = {"body_ids": ids, "masses": m.tolist(), "positions": q.tolist(), "velocities": v.tolist()}
    if "initial_state" not in raw:
        p.update({name: initial[name] for name in names})
    return p, m, q, v, initial, {"estimated_output_bytes": estimate,
                                "estimated_working_bytes": working, "pair_evaluations": (p["steps"]+1)*pairs}


class GuardViolation(ValueError):
    def __init__(self, message, ids, step, h, distance):
        super().__init__(message)
        self.details = {"pair": ids, "step": step, "time": step*h, "time_unit": "T0", "distance": distance}


class Dynamics:
    def __init__(self, masses, p):
        self.m, self.p = masses, p
        self.i, self.j = np.triu_indices(len(masses), 1)

    def reject(self, mask, message, step, distances):
        if np.any(mask):
            k = int(np.flatnonzero(mask)[0])
            raise GuardViolation(message, [self.p["body_ids"][self.i[k]], self.p["body_ids"][self.j[k]]],
                                 step, self.p["timestep"], float(distances[k]))

    def geometry(self, q, step):
        require(np.isfinite(q).all(), "Nonfinite position; no modified equations were substituted")
        delta = q[self.j] - q[self.i]
        r = np.sqrt(np.einsum("ij,ij->i", delta, delta))
        require(np.isfinite(r).all(), "Nonfinite pair distance")
        self.reject(r <= self.p["min_separation"], "Minimum-separation endpoint guard", step, r)
        return delta, r

    def acceleration(self, q, step):
        delta, r = self.geometry(q, step)
        factor = delta / (r*r*r)[:, None]
        a = np.zeros_like(q)
        np.add.at(a, self.i, self.m[self.j, None] * factor)
        np.add.at(a, self.j, -self.m[self.i, None] * factor)
        require(np.isfinite(a).all(), "Nonfinite acceleration")
        return a

    def endpoint(self, q, v, step):
        require(np.isfinite(v).all(), "Nonfinite full-step velocity")
        _, r = self.geometry(q, step)
        gravity = self.p["timestep"] * np.sqrt((self.m[self.i] + self.m[self.j]) / r**3)
        speed = np.linalg.norm(v[self.j] - v[self.i], axis=1)
        self.reject(gravity > .03, "Gravity timescale resolution guard exceeds 0.03", step, r)
        self.reject(self.p["timestep"] * speed / r > .03,
                    "Relative-velocity resolution guard exceeds 0.03", step, r)

    def advance(self, q, v, a, step):
        h = self.p["timestep"]
        self.endpoint(q, v, step)
        half_velocity = v + (h * .5) * a
        next_q = q + h * half_velocity
        start = q[self.j] - q[self.i]
        displacement = (next_q[self.j] - next_q[self.i]) - start
        length2 = np.einsum("ij,ij->i", displacement, displacement)
        fraction = np.zeros_like(length2)
        np.divide(-np.einsum("ij,ij->i", start, displacement), length2, out=fraction, where=length2 > 0)
        closest = start + np.clip(fraction, 0., 1.)[:, None] * displacement
        distances = np.linalg.norm(closest, axis=1)
        if np.any(distances <= self.p["min_separation"]):
            k = int(np.flatnonzero(distances <= self.p["min_separation"])[0])
            error = GuardViolation("Straight-drift minimum-separation guard",
                [self.p["body_ids"][self.i[k]], self.p["body_ids"][self.j[k]]], step, h, float(distances[k]))
            error.details.update(time=(step+float(np.clip(fraction[k], 0., 1.)))*h,
                                 drift_interval=[step*h, (step+1)*h])
            raise error
        next_a = self.acceleration(next_q, step+1)
        next_v = half_velocity + (h * .5) * next_a
        self.endpoint(next_q, next_v, step+1)
        return next_q, next_v, next_a

    def measurements(self, q, v, step):
        delta, r = self.geometry(q, step)
        kinetic = float(.5 * np.sum(self.m[:, None] * v*v))
        potential = float(-np.sum(self.m[self.i] * self.m[self.j] / r))
        momentum = np.sum(self.m[:, None] * v, axis=0)
        angular = np.sum(np.cross(q, self.m[:, None] * v), axis=0)
        com = np.sum(self.m[:, None] * q, axis=0) / np.sum(self.m)
        pairs = []
        if len(self.m) <= 16:
            for k, (i, j) in enumerate(zip(self.i, self.j)):
                pairs.append({"ids": [self.p["body_ids"][i], self.p["body_ids"][j]],
                              "distance": float(r[k]),
                              "radial_velocity": float(np.dot(delta[k], v[j]-v[i]) / r[k]),
                              "relative_angle": math.atan2(float(delta[k, 1]), float(delta[k, 0]))})
        row = {"step": step, "time": step*self.p["timestep"], "kinetic_energy": kinetic,
               "potential_energy": potential, "total_energy": kinetic+potential,
               "momentum": momentum.tolist(), "angular_momentum": angular.tolist(),
               "center_of_mass": com.tolist(), "min_separation": float(np.min(r)), "pairs": pairs}
        canonical(row)
        return row


def process_memory_bytes():
    if os.name == "nt":
        from ctypes import wintypes
        class MemoryCounters(ctypes.Structure):
            _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
                (key, ctypes.c_size_t) for key in ("PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage",
                 "QuotaPagedPoolUsage", "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        psapi = ctypes.WinDLL("psapi", use_last_error=True)
        kernel.GetCurrentProcess.restype = wintypes.HANDLE
        psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(MemoryCounters), wintypes.DWORD]
        psapi.GetProcessMemoryInfo.restype = wintypes.BOOL
        counters = MemoryCounters()
        counters.cb = ctypes.sizeof(counters)
        require(bool(psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(), ctypes.byref(counters), counters.cb)),
                "Cannot monitor worker memory")
        return int(counters.PeakWorkingSetSize)
    import resource
    value = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return int(value if sys.platform == "darwin" else value*1024)


class Publisher:
    def __init__(self, root):
        self.root = root
        self.sizes = {}
        for path in root.rglob("*"):
            if path.is_file():
                relative = path.relative_to(root).as_posix()
                safe_path(root, relative, existing=True)
                self.sizes[relative] = path.stat().st_size
        self.total = sum(self.sizes.values())
        require(self.total <= MAX_OUTPUT, "Existing output exceeds 512 MiB")

    def bytes(self, relative, data, *, immutable=False):
        path = safe_path(self.root, relative)
        if immutable and path.exists():
            require(path.read_bytes() == data, f"Existing immutable artifact differs: {relative}")
            return digest(data)
        temporary_relative = relative + ".tmp"
        temporary = safe_path(self.root, temporary_relative)
        # Account for the old destination, new temporary, and any old temporary.
        peak = self.total - self.sizes.get(temporary_relative, 0) + len(data)
        require(peak <= MAX_OUTPUT, "Actual retained output exceeds 512 MiB; prior artifacts preserved")
        path.parent.mkdir(parents=True, exist_ok=True)
        with temporary.open("wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        replace_published(temporary, path)
        self.total += len(data) - self.sizes.get(relative, 0) - self.sizes.pop(temporary_relative, 0)
        self.sizes[relative] = len(data)
        return digest(data)

    def json(self, relative, value, **kwargs):
        return self.bytes(relative, canonical(value), **kwargs)

    def arrays(self, relative, **values):
        stream = io.BytesIO()
        np.savez(stream, **values)
        data = stream.getvalue()
        require(len(data) <= MAX_ARCHIVE, "Numeric publication exceeds 16 MiB")
        return self.bytes(relative, data, immutable=relative.startswith("trajectory/"))

    def remove(self, relative):
        path = safe_path(self.root, relative)
        path.unlink(missing_ok=True)
        self.total -= self.sizes.pop(relative, 0)


def palette(index):
    rgb = colorsys.hsv_to_rgb((.02 + index * .6180339887498949) % 1, .72, .97)
    return "#" + "".join(f"{round(component*255):02x}" for component in rgb)


def fixed_camera(m, q):
    com = np.sum(m[:, None]*q, axis=0) / np.sum(m)
    halfspan = max(1.25, 2 * float(np.max(np.linalg.norm(q-com, axis=1))))
    return {"projection": "orthographic", "origin": com.tolist(), "right": [1., 0., 0.], "up": [0., 1., 0.],
            "x_limits": [-halfspan, halfspan], "y_limits": [-halfspan, halfspan],
            "plot_bbox": [100, 100, 900, 900], "marker_radius_px": 6}


def render_frame(publisher, frame, row, chunk, topology, camera):
    image = Image.new("RGB", (1024, 1024), (13, 19, 29))
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default(size=19)
    small = ImageFont.load_default(size=15)
    left, top, right, bottom = camera["plot_bbox"]
    xmin, xmax = camera["x_limits"]
    ymin, ymax = camera["y_limits"]
    origin = np.asarray(camera["origin"])
    basis_r, basis_u = np.asarray(camera["right"]), np.asarray(camera["up"])
    draw.rectangle((left-1, top-1, right, bottom), outline=(98, 111, 128), width=1)
    draw.text((80, 22), "Isolated Newtonian bodies | G*=1 | orthographic xy", font=font, fill=(230, 235, 243))
    draw.text((80, 55), f"Step {frame['step']}  |  t/T0 = {frame['time']:.12g}", font=font, fill=(194, 205, 220))
    draw.text((420, 913), "x / L0", font=font, fill=(200, 210, 224))
    draw.text((8, 475), "y / L0", font=small, fill=(200, 210, 224))
    # Axes use neutral colors, never any body-ID color used by pixel validation.
    if xmin <= 0 <= xmax:
        px = left + (0-xmin)/(xmax-xmin)*(right-left-1)
        draw.line((round(px), top, round(px), bottom-1), fill=(45, 57, 72))
    if ymin <= 0 <= ymax:
        py = top + (ymax-0)/(ymax-ymin)*(bottom-top-1)
        draw.line((left, round(py), right-1, round(py)), fill=(45, 57, 72))
    clipped = []
    radius = camera["marker_radius_px"]
    for entity, body in zip(frame["entities"], topology["entities"]):
        point = np.asarray(entity["position"]) - origin
        x, y = float(np.dot(point, basis_r)), float(np.dot(point, basis_u))
        px = left + (x-xmin)/(xmax-xmin)*(right-left-1)
        py = top + (ymax-y)/(ymax-ymin)*(bottom-top-1)
        if not (left+radius <= px < right-radius and top+radius <= py < bottom-radius):
            clipped.append(entity["id"])
            continue
        cx, cy = round(px), round(py)
        draw.ellipse((cx-radius, cy-radius, cx+radius, cy+radius), fill=body["color"])
        draw.text((cx+radius+3, cy-radius-2), entity["id"][:28], font=small, fill=(208, 215, 226))
    draw.text((80, 946), f"H = {row['total_energy']:.12g}  |  min r/L0 = {row['min_separation']:.12g}",
              font=small, fill=(220, 227, 237))
    draw.text((80, 970), f"Full-step velocities; marker size is display only; clipped bodies: {len(clipped)}",
              font=small, fill=(175, 189, 207))
    stream = io.BytesIO()
    image.save(stream, format="PNG")
    path = f"observations/frame-{frame['step']:08d}.png"
    image_hash = publisher.bytes(path, stream.getvalue(), immutable=True)
    return {"step": frame["step"], "time": frame["time"], "time_unit": "T0", "path": path,
            "sha256": image_hash, "source_chunk": chunk["path"], "source_chunk_sha256": chunk["sha256"],
            "source_frame_index": 0, "source_frame_sha256": digest(canonical(frame)),
            "measurements": row, "camera": camera, "clipped_body_ids": clipped,
            "display_derivative": True, "marker_size_is_physical": False}


def verified_json(root, relative, expected):
    data = safe_path(root, relative, existing=True).read_bytes()
    require(digest(data) == hash_string(expected), f"Retained JSON hash mismatch: {relative}")
    return parse_json(data)


def restore_receipts(publisher, checkpoint):
    """Recover only a journaled, interrupted publication; otherwise fail on changes."""
    root = publisher.root
    pending_path = root / "publication.pending.json"
    pending = parse_json(pending_path.read_bytes()) if pending_path.exists() else None
    checkpoint_hash = digest(canonical(checkpoint))
    if pending:
        require(checkpoint_hash in (pending["previous_checkpoint_sha256"], pending["checkpoint_sha256"]),
                "Publication journal does not match the committed checkpoint")
    documents = {}
    for relative, key in RECEIPTS.items():
        snapshot = checkpoint["receipt_snapshots"][relative]
        data = safe_path(root, snapshot, existing=True).read_bytes()
        require(digest(data) == checkpoint[key], f"Checkpoint receipt snapshot changed: {relative}")
        live = safe_path(root, relative, existing=True).read_bytes()
        live_hash = digest(live)
        if live_hash != checkpoint[key]:
            require(pending and checkpoint_hash == pending["previous_checkpoint_sha256"]
                    and live_hash == pending["receipt_hashes"][relative],
                    f"Committed receipt changed outside an interrupted publication: {relative}")
            publisher.bytes(relative, data)
        documents[relative] = parse_json(data)
    if pending:
        publisher.remove("publication.pending.json")
    return documents


class Run:
    def __init__(self, root, publisher, p, masses, q, v, manifest, topology, camera):
        self.root, self.publisher, self.p, self.m = root, publisher, p, masses
        self.manifest, self.topology, self.camera = manifest, topology, camera
        self.dynamics = Dynamics(masses, p)
        self.q, self.v, self.step = q, v, 0
        self.a = self.dynamics.acceleration(q, 0)
        self.dynamics.endpoint(q, v, 0)
        self.index = {"schema_version": 1, "representation": "particle_trajectory", "boundary": "isolated",
                      "time_unit": "T0", "position_unit": "L0", "units": UNITS, "frame_count": 0,
                      "start_time": 0., "end_time": 0., "chunks": [], "topology_path": "topology.json",
                      "topology_sha256": digest(canonical(topology)), "axis_order": ["frame", "body", "xyz"]}
        self.measured = {"units": UNITS, "series": [], "configuration": {
            "angular_momentum_origin": [0., 0., 0.], "relative_angle_plane": "xy",
            "pair_selection": "all" if len(masses) <= 16 else "minimum_only",
            "velocity_time": "full_integer_step", "G_scaled": 1.}}
        self.observations = {"schema_version": 1, "representation": "particle_trajectory", "images": []}
        self.checkpoint = None
        sampled = list(range(0, p["steps"]+1, p["sample_interval"]))
        if sampled[-1] != p["steps"]:
            sampled.append(p["steps"])
        self.image_steps = {sampled[round((len(sampled)-1)*fraction)] for fraction in (0., .25, .5, .75, 1.)}
        self.timing = {"integration_seconds": 0., "serialization_seconds": 0., "rendering_seconds": 0.}
        self.max_memory = 0

    def resume(self):
        root = self.root
        old = parse_json(safe_path(root, "manifest.json", existing=True).read_bytes())
        require(all(old.get(key) == self.manifest[key] for key in IDENTITIES),
                "Input, initial source, worker, implementation or runtime changed; use a fresh attempt")
        require(old == self.manifest, "Manifest differs from the normalized execution contract")
        topology_data = safe_path(root, "topology.json", existing=True).read_bytes()
        require(topology_data == canonical(self.topology), "Retained topology changed")
        checkpoint = parse_json(safe_path(root, "checkpoint.json", existing=True).read_bytes())
        require(all(checkpoint.get(key) == self.manifest[key] for key in IDENTITIES), "Incompatible checkpoint identity")
        documents = restore_receipts(self.publisher, checkpoint)
        index = documents["trajectory/index.json"]
        measured = documents["measurements.json"]
        observations = documents["observations/index.json"]
        require({k: value for k, value in index.items() if k not in ("chunks", "frame_count", "end_time")}
                == {k: value for k, value in self.index.items() if k not in ("chunks", "frame_count", "end_time")},
                "Retained trajectory metadata changed")
        require(measured["units"] == UNITS and measured["configuration"] == self.measured["configuration"],
                "Measurement configuration changed")
        last = None
        frames, seen_steps = [], []
        for chunk in index["chunks"]:
            content = verified_json(root, chunk["path"], chunk["sha256"])
            data = safe_path(root, chunk["arrays_path"], existing=True).read_bytes()
            require(digest(data) == hash_string(chunk["arrays_sha256"]), "Retained numerical chunk changed")
            arrays = numeric_archive(data)
            require(set(arrays) == {"steps", "times", "positions", "velocities", "accelerations"}, "Unexpected chunk keys")
            require(len(content["frames"]) == chunk["frame_count"] == 1
                    and chunk["start_frame"] == chunk["end_frame"] == len(frames), "Invalid chunk frame range")
            for key in ("positions", "velocities", "accelerations"):
                require(arrays[key].shape == (1, len(self.m), 3) and arrays[key].dtype == np.dtype("float64"),
                        f"Invalid retained {key} precision or shape")
            require(arrays["steps"].shape == arrays["times"].shape == (1,)
                    and arrays["steps"].dtype == np.dtype("int64") and arrays["times"].dtype == np.dtype("float64"),
                    "Invalid retained step/time dtype")
            frame = content["frames"][0]
            s, t = int(arrays["steps"][0]), float(arrays["times"][0])
            require(s == frame["step"] == chunk["start_step"] == chunk["end_step"]
                    and t == s*self.p["timestep"] == frame["time"] == chunk["start_time"] == chunk["end_time"],
                    "Retained timing mismatch")
            require([x["id"] for x in frame["entities"]] == self.p["body_ids"]
                    and np.array_equal([x["position"] for x in frame["entities"]], arrays["positions"][0]),
                    "Retained JSON body identity or position differs from numeric data")
            require(chunk["bounds"] == {"min": arrays["positions"].min(axis=(0, 1)).tolist(),
                                        "max": arrays["positions"].max(axis=(0, 1)).tolist()}, "Retained bounds changed")
            row = measured["series"][len(frames)]
            require(row["step"] == s and row["time"] == t and row["source_frame_index"] == 0
                    and row["source_arrays_path"] == chunk["arrays_path"]
                    and row["source_arrays_sha256"] == chunk["arrays_sha256"], "Retained instrument source mismatch")
            frames.append(frame)
            seen_steps.append(s)
            last = arrays
        require(last is not None and seen_steps[0] == 0 and seen_steps == sorted(set(seen_steps))
                and len(frames) == index["frame_count"] == len(measured["series"])
                and index["end_time"] == frames[-1]["time"]
                and checkpoint["step"] == seen_steps[-1], "Invalid committed checkpoint endpoint")
        expected = set(range(0, checkpoint["step"]+1, self.p["sample_interval"])) | {checkpoint["step"]}
        require(expected.issubset(seen_steps) and all(0 <= s <= self.p["steps"] for s in seen_steps),
                "Committed sequence misses requested samples")
        for entry in observations["images"]:
            require(file_hash(safe_path(root, entry["path"], existing=True)) == hash_string(entry["sha256"]),
                    "Retained observation image changed")
            chunk = next((item for item in index["chunks"] if item["path"] == entry["source_chunk"]), None)
            require(chunk and entry["source_chunk_sha256"] == chunk["sha256"] and entry["source_frame_index"] == 0,
                    "Retained observation source changed")
            frame = frames[chunk["start_frame"]]
            require(entry["source_frame_sha256"] == digest(canonical(frame)) and entry["step"] == frame["step"]
                    and entry["time"] == frame["time"] and entry["camera"] == self.camera
                    and entry["measurements"] == measured["series"][chunk["start_frame"]], "Observation provenance mismatch")
        data = safe_path(root, checkpoint["checkpoint_path"], existing=True).read_bytes()
        require(digest(data) == hash_string(checkpoint["checkpoint_sha256"]), "Checkpoint numerical hash mismatch")
        state = numeric_archive(data)
        require(set(state) == {"step", "time", "positions", "velocities", "accelerations"}, "Checkpoint array keys")
        require(state["step"].shape == () and state["step"].dtype == np.dtype("int64")
                and int(state["step"]) == checkpoint["step"]
                and state["time"].shape == () and state["time"].dtype == np.dtype("float64")
                and float(state["time"]) == checkpoint["step"]*self.p["timestep"], "Checkpoint physical time mismatch")
        for key in ("positions", "velocities", "accelerations"):
            require(state[key].dtype == last[key].dtype and state[key].shape == last[key][0].shape
                    and state[key].tobytes() == last[key][0].tobytes(), f"Checkpoint {key} differs from committed sample")
        self.step = checkpoint["step"]
        self.q, self.v, self.a = (state[key] for key in ("positions", "velocities", "accelerations"))
        self.dynamics.endpoint(self.q, self.v, self.step)
        require(self.dynamics.acceleration(self.q, self.step).tobytes() == self.a.tobytes(),
                "Checkpoint acceleration differs from declared pair forces")
        self.index, self.measured, self.observations, self.checkpoint = index, measured, observations, checkpoint

    def commit(self, *, force_image=False):
        started = time.perf_counter()
        previous_rendering = self.timing["rendering_seconds"]
        p, step, publisher = self.p, self.step, self.publisher
        retained = self.index["chunks"] and self.index["chunks"][-1]["end_step"] == step
        if not retained:
            require(self.index["frame_count"] < 10_001, "Actual retained frames exceed 10001")
            relative = f"trajectory/chunk-{step:08d}.json"
            arrays_path = f"trajectory/arrays-{step:08d}.npz"
            arrays_hash = publisher.arrays(arrays_path, steps=np.asarray([step], dtype=np.int64),
                times=np.asarray([step*p["timestep"]], dtype=np.float64), positions=self.q[None],
                velocities=self.v[None], accelerations=self.a[None])
            frame = {"step": step, "time": step*p["timestep"],
                     "entities": [{"id": identifier, "position": position.tolist()}
                                  for identifier, position in zip(p["body_ids"], self.q)]}
            chunk_hash = publisher.json(relative, {"frames": [frame]}, immutable=True)
            count = self.index["frame_count"]
            chunk = {"path": relative, "sha256": chunk_hash, "arrays_path": arrays_path,
                     "arrays_sha256": arrays_hash, "start_frame": count, "end_frame": count, "frame_count": 1,
                     "start_step": step, "end_step": step, "start_time": frame["time"], "end_time": frame["time"],
                     "bounds": {"min": self.q.min(axis=0).tolist(), "max": self.q.max(axis=0).tolist()}}
            row = self.dynamics.measurements(self.q, self.v, step)
            row.update(source_arrays_path=arrays_path, source_arrays_sha256=arrays_hash, source_frame_index=0)
            self.index["chunks"].append(chunk)
            self.index.update(frame_count=count+1, end_time=frame["time"])
            self.measured["series"].append(row)
        else:
            chunk = self.index["chunks"][-1]
            frame = verified_json(self.root, chunk["path"], chunk["sha256"])["frames"][0]
            row = self.measured["series"][-1]
        if (force_image or step in self.image_steps) and not any(e["step"] == step for e in self.observations["images"]):
            rendering = time.perf_counter()
            self.observations["images"].append(render_frame(publisher, frame, row, chunk, self.topology, self.camera))
            self.timing["rendering_seconds"] += time.perf_counter()-rendering
        # Two slots keep the preceding committed generation intact while its
        # replacement is assembled. Public indexes are replicas of those bytes.
        slot = 0 if self.checkpoint is None else 1-self.checkpoint["slot"]
        prefix = f"checkpoint/slot-{slot}"
        checkpoint_path = prefix + "/state.npz"
        state_hash = publisher.arrays(checkpoint_path, step=np.asarray(step, dtype=np.int64),
            time=np.asarray(step*p["timestep"], dtype=np.float64), positions=self.q,
            velocities=self.v, accelerations=self.a)
        documents = {"trajectory/index.json": self.index, "measurements.json": self.measured,
                     "observations/index.json": self.observations}
        checkpoint = {"schema_version": 1, "slot": slot, "step": step, "time": step*p["timestep"],
                      **{key: self.manifest[key] for key in IDENTITIES}, "checkpoint_path": checkpoint_path,
                      "checkpoint_sha256": state_hash, "receipt_snapshots": {}}
        serialized = {}
        for relative, value in documents.items():
            data = canonical(value)
            snapshot = prefix + "/" + relative.replace("/", "-")
            checkpoint["receipt_snapshots"][relative] = snapshot
            checkpoint[RECEIPTS[relative]] = publisher.bytes(snapshot, data)
            serialized[relative] = data
        publisher.json("publication.pending.json", {
            "previous_checkpoint_sha256": digest(canonical(self.checkpoint)) if self.checkpoint else None,
            "checkpoint_sha256": digest(canonical(checkpoint)),
            "receipt_hashes": {relative: digest(data) for relative, data in serialized.items()}})
        for relative, data in serialized.items():
            publisher.bytes(relative, data)
        publisher.json("checkpoint.json", checkpoint)
        self.checkpoint = checkpoint
        publisher.remove("publication.pending.json")
        self.timing["serialization_seconds"] += time.perf_counter()-started-(
            self.timing["rendering_seconds"]-previous_rendering)
        self.max_memory = max(self.max_memory, process_memory_bytes())
        require(self.max_memory <= MAX_PROCESS, "Measured process memory exceeds 512 MiB; checkpoint preserved")
        publisher.json("progress.json", {"step": step, "fraction": step/p["steps"], "time": step*p["timestep"],
            "time_unit": "T0", "state": "running", "storage_bytes": publisher.total,
            "worker_peak_memory_bytes": self.max_memory,
            "message": f"Mechanics integration {step}/{p['steps']}; full-step numerical state and checkpoint retained"})

    def result(self, status, elapsed, error=None):
        result = {"status": status, "engine": ENGINE, "engine_version": ENGINE_VERSION, "step": self.step,
                  "simulated_time": self.step*self.p["timestep"], "time_unit": "T0", "compute_wall_seconds": elapsed,
                  "timing": self.timing, "worker_peak_memory_bytes": self.max_memory,
                  "trajectory_index": "trajectory/index.json", "topology": "topology.json",
                  "measurements": "measurements.json", "observations": "observations/index.json",
                  "manifest": "manifest.json", "checkpoint": "checkpoint.json",
                  "scientific_validation": "Execution requires an independent reference and study-specific acceptance evidence"}
        if error:
            result["failure"] = {"message": str(error), **getattr(error, "details", {})}
        self.publisher.json("result.json", result)
        self.publisher.json("progress.json", {"step": self.step, "fraction": self.step/self.p["steps"],
            "time": self.step*self.p["timestep"], "time_unit": "T0",
            "state": "paused" if status == "cancelled" else status,
            "message": f"Mechanics {status}; last committed state at step {self.step}"})


def run(input_path, root):
    started = time.perf_counter()
    require(input_path.is_absolute() and root.is_absolute(), "Input and output paths must be absolute")
    require(input_path.is_file() and input_path.stat().st_size <= 2*1024**2, "Input JSON must be at most 2 MiB")
    require(not root.is_symlink() and not (hasattr(root, "is_junction") and root.is_junction()), "Output links forbidden")
    root.mkdir(parents=True, exist_ok=True)
    raw = parse_json(input_path.read_bytes())
    only_keys(raw, ("engine", "parameters"), "input")
    require(raw.get("engine") == ENGINE, "Unsupported mechanics engine")
    p, m, q, v, initial, estimates = parameters(raw.get("parameters"), root)
    require(np.__version__ == "2.4.6" and PILLOW_VERSION == "12.3.0", "Pinned NumPy/Pillow runtime required")
    requirements = Path(__file__).resolve().with_name("requirements-science.txt")
    require(requirements.is_file(), "Missing shipped requirements-science.txt")
    runtime = {"python_version": platform.python_version(), "numpy_version": np.__version__,
               "requirements_sha256": file_hash(requirements)}
    normalized = {"engine": ENGINE, "parameters": p}
    manifest = {"schema_version": 1, "engine": ENGINE, "engine_version": ENGINE_VERSION,
                "input": normalized, "input_sha256": digest(canonical(normalized)),
                "worker_sha256": file_hash(Path(__file__).resolve()), "numpy_version": np.__version__,
                "initial_state_sha256": digest(canonical(initial)), "runtime": runtime,
                "runtime_sha256": digest(canonical(runtime)), "platform": "CPU", "units": UNITS,
                "boundary": "isolated", "unit_system": "scaled_G1", "G_scaled": 1.,
                "equations": "a_i=sum(j!=i) m_j*(q_j-q_i)/|q_j-q_i|^3; unsoftened velocity-Verlet",
                "velocity_time": "full_integer_step", "source_array_axis_order": ["frame", "body", "xyz"],
                "estimates": estimates, "checkpoint_policy": "every retained state; one-frame immutable chunks",
                "limitations": "Isolated point masses; no collision, softening, external field or adaptive timestep"}
    camera = fixed_camera(m, q)
    radius = (camera["x_limits"][1]-camera["x_limits"][0])*.0075
    topology = {"schema_version": 1, "entities": [{"id": identifier, "mass": float(mass),
                "display_radius": radius, "color": palette(i)} for i, (identifier, mass) in enumerate(zip(p["body_ids"], m))],
                "bonds": [], "boundary": "isolated", "units": UNITS, "display_radius_is_physical": False}
    publisher = Publisher(root)
    worker = Run(root, publisher, p, m, q, v, manifest, topology, camera)  # Preflight guards before any advancement.
    if (root / "checkpoint.json").exists():
        worker.resume()
    else:
        require(not (root / "manifest.json").exists() and not (root / "trajectory/index.json").exists(),
                "Incomplete initial publication without a checkpoint; preserve evidence and use a fresh attempt")
        publisher.json("manifest.json", manifest, immutable=True)
        publisher.json("topology.json", topology, immutable=True)
        worker.commit()
    while worker.step < p["steps"]:
        if (root / "cancel.request").is_file():
            worker.commit(force_image=True)
            worker.result("cancelled", time.perf_counter()-started)
            return 3
        numerical = time.perf_counter()
        try:
            nq, nv, na = worker.dynamics.advance(worker.q, worker.v, worker.a, worker.step)
        except (ValueError, FloatingPointError) as error:
            worker.commit(force_image=True)
            worker.result("failed", time.perf_counter()-started, error)
            raise
        worker.q, worker.v, worker.a = nq, nv, na
        worker.step += 1
        worker.timing["integration_seconds"] += time.perf_counter()-numerical
        if worker.step % p["sample_interval"] == 0 or worker.step == p["steps"]:
            worker.commit()
    worker.result("completed", time.perf_counter()-started)
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        with np.errstate(over="raise", invalid="raise", divide="raise"):
            code = run(args.input, args.output)
        print(json.dumps({"engine": ENGINE, "exit_code": code, "output": str(args.output)}))
        return code
    except Exception as error:
        receipt = {"engine": ENGINE, "type": type(error).__name__, "message": str(error),
                   **getattr(error, "details", {})}
        print(json.dumps(receipt, allow_nan=False), file=sys.stderr)
        try:
            if args.output.is_absolute() and args.output.is_dir():
                Publisher(args.output).json("error.json", receipt)
        except Exception as publication_error:
            print(json.dumps({"error_receipt_publication": str(publication_error)}), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
