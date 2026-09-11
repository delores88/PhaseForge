"""Trusted shipped two-dimensional passive-scalar diffusion adapter.

Float64 five-point FTCS integration; field arrays are the scientific state.
Images and bounded JSON views are derived observations, never solver inputs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import sys
import time

import numpy as np
from PIL import Image, ImageDraw, ImageFont, __version__ as PILLOW_VERSION

ENGINE = "diffusion_2d"
ADAPTER_VERSION = "1.0.0"
MAX_STEPS = 100_000
MAX_FRAMES = 1001
LOW_RGB, HIGH_RGB = (20, 38, 80), (255, 190, 60)
PUBLICATION_RETRY_SECONDS = 2.0


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def file_hash(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def replace_published(temporary: Path, path: Path):
    """Publish a complete file without truncating a file held by a Windows reader.

    Readers/scanners can briefly deny replacement even with read-only handles.
    Access denied can also mean a persistent permission problem: only the known
    Windows sharing/access errors receive a bounded retry, then the original
    error is raised. Disk-full, invalid paths and other failures remain immediate.
    The prior published bytes and complete temporary file survive any failure.
    """
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


def atomic_bytes(path: Path, data: bytes):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with temporary.open("wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())
    replace_published(temporary, path)


def write_json(path: Path, value):
    atomic_bytes(path, json.dumps(value, allow_nan=False, sort_keys=True, separators=(",", ":")).encode("utf-8"))


def integer(name, value, low, high):
    if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
        raise ValueError(f"{name} must be an integer between {low} and {high}")
    return value


def number(name, value, low, high, *, positive=False):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"{name} must be a finite number")
    value = float(value)
    if not low <= value <= high or (positive and value == 0):
        raise ValueError(f"{name} is outside its supported range")
    return value


def only_keys(value, allowed, name):
    if not isinstance(value, dict) or set(value) - set(allowed):
        raise ValueError(f"{name} contains unsupported fields or is not an object")


def local_file(root: Path, relative: str) -> Path:
    if not isinstance(relative, str) or not relative or len(relative) > 240 or "\\" in relative or ":" in relative:
        raise ValueError("Array input must use a bounded run-relative file path")
    parts = relative.split("/")
    if any(part in ("", ".", "..") or part.endswith((".", " ")) for part in parts):
        raise ValueError("Invalid array input path")
    path = root
    for part in parts:
        path = path / part
        if path.is_symlink() or (hasattr(path, "is_junction") and path.is_junction()):
            raise ValueError("Array input links are forbidden")
    if not path.is_file() or not path.resolve().is_relative_to(root.resolve()) or path.stat().st_nlink != 1:
        raise ValueError("Array input must be a plain file inside the run")
    if path.suffix != ".npy" or path.stat().st_size > 64 * 1024 * 1024:
        raise ValueError("Array input must be a numeric .npy file of at most 64 MiB")
    return path


def parameters(raw, root: Path):
    only_keys(raw, ("nx", "ny", "length_x_um", "length_y_um", "diffusivity_um2_s",
                   "dt_s", "steps", "record_interval", "boundary", "initial",
                   "probes", "max_output_mb"), "parameters")
    p = {
        "nx": integer("nx", raw.get("nx", 64), 16, 512),
        "ny": integer("ny", raw.get("ny", 64), 16, 512),
        "length_x_um": number("length_x_um", raw.get("length_x_um", 10), 1e-3, 1e6),
        "length_y_um": number("length_y_um", raw.get("length_y_um", 10), 1e-3, 1e6),
        "diffusivity_um2_s": number("diffusivity_um2_s", raw.get("diffusivity_um2_s", .2), 0, 1e6),
        "dt_s": number("dt_s", raw.get("dt_s", .005), 1e-12, 1e6),
        "steps": integer("steps", raw.get("steps", 512), 1, MAX_STEPS),
        "boundary": raw.get("boundary", "periodic"),
        "max_output_mb": integer("max_output_mb", raw.get("max_output_mb", 256), 64, 2048),
    }
    p["record_interval"] = integer("record_interval", raw.get("record_interval", min(8, p["steps"])), 1, p["steps"])
    if p["boundary"] != "periodic":
        raise ValueError("Only periodic boundaries are supported by this adapter")
    dx, dy = p["length_x_um"] / p["nx"], p["length_y_um"] / p["ny"]
    cfl = p["diffusivity_um2_s"] * p["dt_s"] * (1 / dx**2 + 1 / dy**2)
    if not math.isfinite(cfl) or cfl > .5:
        raise ValueError(f"Unstable FTCS timestep: D*dt*(1/dx^2+1/dy^2)={cfl:.9g} exceeds 0.5; choose a smaller explicit dt_s")
    count = math.ceil(p["steps"] / p["record_interval"]) + 1
    if count > MAX_FRAMES:
        raise ValueError("Requested output exceeds 1001 retained field frames")
    estimate = count * (p["nx"] * p["ny"] * 40 + 2048) + 32 * 1024**2 + p["nx"] * p["ny"] * 16
    if estimate > p["max_output_mb"] * 1024**2:
        raise ValueError(f"Estimated retained output {estimate} bytes exceeds max_output_mb; explicitly change output cadence or budget")
    initial = raw.get("initial", {"kind": "fourier"})
    if not isinstance(initial, dict):
        raise ValueError("initial must be an object")
    kind = initial.get("kind", "fourier")
    if kind == "fourier":
        only_keys(initial, ("kind", "baseline", "amplitude", "mode_x", "mode_y"), "initial")
        baseline = number("baseline", initial.get("baseline", 1), 0, 1e9)
        amplitude = number("amplitude", initial.get("amplitude", .2), -1e9, 1e9)
        mx = integer("mode_x", initial.get("mode_x", 1), 0, p["nx"] // 4)
        my = integer("mode_y", initial.get("mode_y", 2), 0, p["ny"] // 4)
        if mx == my == 0 or baseline < abs(amplitude):
            raise ValueError("Fourier initial state requires a nonconstant resolved mode and nonnegative concentration")
        p["initial"] = {"kind": kind, "baseline": baseline, "amplitude": amplitude, "mode_x": mx, "mode_y": my}
    elif kind == "gaussian":
        only_keys(initial, ("kind", "baseline", "amplitude", "center_x_um", "center_y_um", "sigma_um"), "initial")
        p["initial"] = {
            "kind": kind,
            "baseline": number("baseline", initial.get("baseline", 0), 0, 1e9),
            "amplitude": number("amplitude", initial.get("amplitude", 1), 0, 1e9),
            "center_x_um": number("center_x_um", initial.get("center_x_um", p["length_x_um"] / 2), 0, p["length_x_um"]),
            "center_y_um": number("center_y_um", initial.get("center_y_um", p["length_y_um"] / 2), 0, p["length_y_um"]),
            "sigma_um": number("sigma_um", initial.get("sigma_um", .7), min(dx, dy), min(p["length_x_um"], p["length_y_um"]) / 2),
        }
    elif kind == "array":
        only_keys(initial, ("kind", "path", "source_sha256"), "initial")
        path = local_file(root, initial.get("path"))
        if initial.get("source_sha256", file_hash(path)) != file_hash(path):
            raise ValueError("Imported initial-field SHA256 differs from the pinned source")
        p["initial"] = {"kind": kind, "path": initial["path"], "source_sha256": file_hash(path)}
    else:
        raise ValueError("Supported initial conditions are fourier, gaussian and array")
    probes = raw.get("probes", [])
    if not isinstance(probes, list) or len(probes) > 32:
        raise ValueError("probes must contain at most 32 declared instruments")
    names = set()
    p["probes"] = []
    for probe in probes:
        if not isinstance(probe, dict) or not isinstance(probe.get("name"), str) or not 1 <= len(probe["name"]) <= 80 or probe["name"] in names:
            raise ValueError("Each probe needs a unique name of 1–80 characters")
        names.add(probe["name"])
        if probe.get("kind") == "point":
            only_keys(probe, ("name", "kind", "x_um", "y_um"), "point probe")
            parsed = {"kind": "point", "name": probe["name"],
                      "x_um": number("x_um", probe.get("x_um"), 0, p["length_x_um"]),
                      "y_um": number("y_um", probe.get("y_um"), 0, p["length_y_um"])}
            if parsed["x_um"] == p["length_x_um"] or parsed["y_um"] == p["length_y_um"]:
                raise ValueError("Point locations use the half-open periodic domain")
        elif probe.get("kind") == "region":
            only_keys(probe, ("name", "kind", "x_min_um", "x_max_um", "y_min_um", "y_max_um"), "region probe")
            parsed = {"kind": "region", "name": probe["name"]}
            for axis in ("x", "y"):
                for endpoint in ("min", "max"):
                    key = f"{axis}_{endpoint}_um"
                    parsed[key] = number(key, probe.get(key), 0, p[f"length_{axis}_um"])
                if parsed[f"{axis}_min_um"] >= parsed[f"{axis}_max_um"]:
                    raise ValueError("Region bounds must be strictly increasing")
        else:
            raise ValueError("Probe kind must be point or region")
        p["probes"].append(parsed)
    return p, {"cfl": cfl, "estimated_output_bytes": estimate, "estimated_state_bytes": p["nx"] * p["ny"] * 8 * 10}


def make_initial(p, root, x, y):
    init = p["initial"]
    if init["kind"] == "fourier":
        field = init["baseline"] + init["amplitude"] * np.cos(2 * np.pi * init["mode_y"] * y[:, None] / p["length_y_um"]) * np.cos(2 * np.pi * init["mode_x"] * x[None, :] / p["length_x_um"])
    elif init["kind"] == "gaussian":
        # Periodic minimum-image patch: a declared initial condition, not an
        # infinite-domain Gaussian reference at later times.
        dx = (x - init["center_x_um"] + p["length_x_um"] / 2) % p["length_x_um"] - p["length_x_um"] / 2
        dy = (y - init["center_y_um"] + p["length_y_um"] / 2) % p["length_y_um"] - p["length_y_um"] / 2
        field = init["baseline"] + init["amplitude"] * np.exp(-(dy[:, None]**2 + dx[None, :]**2) / (2 * init["sigma_um"]**2))
    else:
        source = local_file(root, init["path"])
        field = np.load(source, allow_pickle=False, mmap_mode="r", max_header_size=4096)
        if field.dtype.kind not in "fiu" or field.shape != (p["ny"], p["nx"]):
            raise ValueError("Initial array must have numeric real dtype and exact shape [ny,nx]")
        field = np.array(field, dtype=np.float64, order="C", copy=True)
    if field.shape != (p["ny"], p["nx"]) or not np.isfinite(field).all() or field.min() < 0 or field.max() > 2e9:
        raise ValueError("Initial field must be finite, nonnegative and within the supported scalar range")
    return np.asarray(field, dtype=np.float64, order="C")


def instruments(field, step, p, x, y):
    mx, my = (p["initial"].get("mode_x", 1), p["initial"].get("mode_y", 1))
    mode = np.cos(2 * np.pi * my * y[:, None] / p["length_y_um"]) * np.cos(2 * np.pi * mx * x[None, :] / p["length_x_um"])
    mean = float(field.mean())
    row = {"step": step, "time": step * p["dt_s"], "time_unit": "s",
           "integral": float(field.sum() * p["length_x_um"] / p["nx"] * p["length_y_um"] / p["ny"]),
           "mean": mean, "min": float(field.min()), "max": float(field.max()), "variance": float(np.mean((field - mean)**2)),
           "mode_amplitude": float(np.mean((field - mean) * mode) / np.mean(mode**2)),
           "mode_x": mx, "mode_y": my, "probes": []}
    for probe in p["probes"]:
        item = dict(probe)
        if probe["kind"] == "point":
            ix = min(p["nx"] - 1, int(probe["x_um"] / p["length_x_um"] * p["nx"]))
            iy = min(p["ny"] - 1, int(probe["y_um"] / p["length_y_um"] * p["ny"]))
            item.update(value=float(field[iy, ix]), cell=[iy, ix], sample_location_um=[float(x[ix]), float(y[iy])], observation_model="containing cell-center sample")
        else:
            mask_x = (x >= probe["x_min_um"]) & (x < probe["x_max_um"])
            mask_y = (y >= probe["y_min_um"]) & (y < probe["y_max_um"])
            region = field[np.ix_(mask_y, mask_x)]
            if region.size == 0:
                raise ValueError("A requested region contains no cell centers")
            item.update(value=float(region.mean()), cell_count=int(region.size), observation_model="arithmetic mean of cell centers in half-open rectangle")
        item["unit"] = "1"
        row["probes"].append(item)
    return row


def font(size):
    try:
        return ImageFont.truetype("arial.ttf", size)
    except OSError:
        return ImageFont.load_default(size=size)


def render_field(root, field, record, measurement, p, color_scale):
    span = color_scale["max"] - color_scale["min"]
    normalized = np.clip((field - color_scale["min"]) / span, 0, 1)
    rgb = np.rint(np.asarray(LOW_RGB) + normalized[:, :, None] * (np.asarray(HIGH_RGB) - np.asarray(LOW_RGB))).astype(np.uint8)
    width, height = 768, 768
    aspect = p["length_x_um"] / p["length_y_um"]
    if aspect > 1:
        height = max(1, int(round(width / aspect)))
    else:
        width = max(1, int(round(height * aspect)))
    left, top = 128 + (768 - width) // 2, 112 + (768 - height) // 2
    right, bottom = left + width, top + height
    image = Image.new("RGB", (1024, 1024), (13, 18, 29))
    image.paste(Image.fromarray(np.flipud(rgb)).resize((width, height), Image.Resampling.NEAREST), (left, top))
    draw = ImageDraw.Draw(image)
    draw.text((64, 24), "PhaseForge | numerical diffusion field", fill=(240, 242, 247), font=font(28))
    draw.text((64, 64), f"t = {record['time']:.6g} s    step {record['step']}    D = {p['diffusivity_um2_s']:.6g} um^2/s", fill=(182, 194, 215), font=font(20))
    # Tick labels remain outside the calibrated plot rectangle.
    draw.text((left, bottom + 8), "0", fill=(220, 225, 236), font=font(17))
    draw.text((max(left, right - 76), bottom + 8), f"{p['length_x_um']:.5g}", fill=(220, 225, 236), font=font(17))
    draw.text((left + width // 2 - 25, bottom + 28), "x (um)", fill=(220, 225, 236), font=font(18))
    draw.text((max(6, left - 74), top - 2), f"{p['length_y_um']:.5g}", fill=(220, 225, 236), font=font(17))
    draw.text((max(6, left - 30), bottom - 20), "0", fill=(220, 225, 236), font=font(17))
    draw.text((14, top + height // 2), "y (um)", fill=(220, 225, 236), font=font(18))
    for index in range(320):
        ratio = index / 319
        color = tuple(round(a + ratio * (b - a)) for a, b in zip(LOW_RGB, HIGH_RGB))
        draw.line([(352 + index, 948), (352 + index, 967)], fill=color)
    draw.text((64, 943), "Normalized concentration (1)", fill=(220, 225, 236), font=font(18))
    draw.text((352, 970), f"{color_scale['min']:.5g}", fill=(220, 225, 236), font=font(17))
    draw.text((620, 970), f"{color_scale['max']:.5g}", fill=(220, 225, 236), font=font(17))
    path = root / "observations" / f"field-{record['step']:08d}.png"
    path.parent.mkdir(exist_ok=True)
    temp = path.with_suffix(".tmp")
    with temp.open("wb") as stream:
        image.save(stream, format="PNG")
        stream.flush()
        os.fsync(stream.fileno())
    replace_published(temp, path)
    return {"step": record["step"], "time": record["time"], "time_unit": "s",
            "path": path.relative_to(root).as_posix(), "sha256": file_hash(path),
            "field_path": record["path"], "field_sha256": record["sha256"],
            "source": {"path": record["path"], "sha256": record["sha256"], "shape": [p["ny"], p["nx"]]},
            "measurements": measurement, "color_scale": color_scale,
            "camera": {"projection": "orthographic", "plot_bbox": [left, top, right, bottom],
                       "orientation": "x-right_y-up", "grid_resampling": "nearest", "grid_location": "cell_center"},
            "scope": "Synthetic normalized passive scalar; visualization of recorded field, not a calibrated microscopy image"}


def load_json(path):
    if path.stat().st_size > 32 * 1024**2:
        raise ValueError(f"Retained metadata is too large: {path.name}")
    return json.loads(path.read_text(encoding="utf-8"), parse_constant=lambda value: (_ for _ in ()).throw(ValueError(f"Nonfinite JSON {value}")))


def run(input_path: Path, root: Path):
    if not input_path.is_absolute() or not root.is_absolute():
        raise ValueError("Input and output paths must be absolute")
    root.mkdir(parents=True, exist_ok=True)
    request = load_json(input_path)
    only_keys(request, ("engine", "parameters"), "worker input")
    if request.get("engine") != ENGINE:
        raise ValueError("Unsupported field engine")
    p, estimates = parameters(request.get("parameters", {}), root)
    x = (np.arange(p["nx"], dtype=np.float64) + .5) * p["length_x_um"] / p["nx"]
    y = (np.arange(p["ny"], dtype=np.float64) + .5) * p["length_y_um"] / p["ny"]
    initial = make_initial(p, root, x, y)
    instruments(initial, 0, p, x, y)  # Validate instrument geometry before integration.
    normalized_input = {"engine": ENGINE, "parameters": p}
    input_hash = digest(json.dumps(normalized_input, sort_keys=True, separators=(",", ":"), allow_nan=False).encode())
    worker_hash = file_hash(Path(__file__))
    minimum, maximum = float(initial.min()), float(initial.max())
    if p["initial"]["kind"] == "fourier":
        minimum = p["initial"]["baseline"] - abs(p["initial"]["amplitude"])
        maximum = p["initial"]["baseline"] + abs(p["initial"]["amplitude"])
    elif p["initial"]["kind"] == "gaussian":
        minimum = p["initial"]["baseline"]
        maximum = minimum + p["initial"]["amplitude"]
    color_scale = {"min": minimum if minimum != maximum else minimum - .5,
                   "max": maximum if minimum != maximum else maximum + .5,
                   "map": "linear_blue_orange", "stops": [[0, list(LOW_RGB)], [1, list(HIGH_RGB)]]}
    manifest = {
        "schema_version": 1, "engine": ENGINE, "adapter_version": ADAPTER_VERSION,
        "engine_version": f"NumPy {np.__version__}", "worker_sha256": worker_hash,
        "input": normalized_input, "input_sha256": input_hash, "platform": {"os": platform.platform(), "processor": "CPU"},
        "environment": {"python": platform.python_version(), "numpy": np.__version__, "pillow": PILLOW_VERSION},
        "equations": "dc/dt = D*(d2c/dx2 + d2c/dy2)",
        "solver": {"method": "five-point FTCS", "dtype": "float64", "boundary": "periodic", **estimates},
        "units": {"length": "um", "time": "s", "diffusivity": "um^2/s", "field": "1", "integral": "um^2"},
        "coordinates": {"grid_location": "cell_center", "axis_order": ["y", "x"], "shape": [p["ny"], p["nx"]]},
        "initial_field_sha256": digest(initial.astype("<f8").tobytes()), "color_scale": color_scale,
        "references": ["https://math.mit.edu/classes/18.086/2006/am54.pdf", "https://numpy.org/doc/2.4/"],
        "scientific_scope": "Synthetic passive scalar with constant isotropic diffusivity on a periodic rectangle. Numerical validation is separate from physical predictive validity.",
    }
    index = {"schema_version": 1, "representation": "scalar_field", "field_name": "normalized_concentration",
             "field_unit": "1", "time_unit": "s", "length_unit": "um", "shape": [p["ny"], p["nx"]],
             "axis_order": ["y", "x"], "grid_location": "cell_center", "x_um": x.tolist(), "y_um": y.tolist(),
             "lengths_um": [p["length_x_um"], p["length_y_um"]], "boundary": "periodic",
             "color_scale": color_scale, "frames": []}
    measurements = {"schema_version": 1, "time_unit": "s", "field_unit": "1", "integral_unit": "um^2",
                    "observation_model": "direct float64 cell-center field measurements", "series": []}
    observations = {"schema_version": 1, "images": []}
    field, step = initial.copy(), 0
    checkpoint_path = root / "checkpoint.json"
    resumed = checkpoint_path.is_file()
    if resumed:
        checkpoint = load_json(checkpoint_path)
        if checkpoint["input_sha256"] != input_hash or checkpoint["worker_sha256"] != worker_hash or checkpoint["numpy_version"] != np.__version__:
            raise ValueError("Checkpoint does not match the immutable input, worker or NumPy version")
        if file_hash(root / "checkpoint.npz") != checkpoint["checkpoint_sha256"]:
            raise ValueError("Checkpoint file hash mismatch")
        if checkpoint["initial_field_sha256"] != manifest["initial_field_sha256"]:
            raise ValueError("Checkpoint initial field hash mismatch")
        for name, path in [("field_index_sha256", "fields/index.json"), ("measurements_sha256", "measurements.json"), ("observations_sha256", "observations/index.json")]:
            if file_hash(root / path) != checkpoint[name]:
                raise ValueError("Checkpoint output index receipt mismatch; incomplete output requires a fresh attempt")
        with np.load(root / "checkpoint.npz", allow_pickle=False) as saved:
            field = np.array(saved["field"], dtype=np.float64, order="C", copy=True)
        step = integer("checkpoint step", checkpoint["step"], 0, p["steps"])
        if field.shape != initial.shape or not np.isfinite(field).all():
            raise ValueError("Invalid checkpoint field")
        index = load_json(root / "fields/index.json")
        measurements = load_json(root / "measurements.json")
        observations = load_json(root / "observations/index.json")
        index["frames"] = [row for row in index["frames"] if row["step"] <= step]
        measurements["series"] = [row for row in measurements["series"] if row["step"] <= step]
        observations["images"] = [row for row in observations["images"] if row["step"] <= step]
        if not index["frames"] or index["frames"][-1]["step"] != step:
            raise ValueError("Checkpoint has no matching registered field frame")
        for row in index["frames"]:
            if file_hash(root / row["path"]) != row["sha256"] or file_hash(root / row["view_path"]) != row["view_sha256"]:
                raise ValueError("A retained field artifact changed after registration")
        manifest["resumed_from_step"] = step
    write_json(root / "manifest.json", manifest)
    start = time.perf_counter()
    last_image_step = max((image["step"] for image in observations["images"]), default=-1)
    image_thresholds = sorted(set([0, math.ceil(p["steps"] / 4), math.ceil(p["steps"] / 2), math.ceil(3 * p["steps"] / 4), p["steps"]]))

    def save(state, current_step, *, force_image=False, status="running"):
        nonlocal last_image_step
        field_path = root / "fields" / f"field-{current_step:08d}.npy"
        field_path.parent.mkdir(exist_ok=True)
        temp = field_path.with_suffix(".tmp")
        with temp.open("wb") as stream:
            np.save(stream, state.astype("<f8", copy=False), allow_pickle=False)
            stream.flush()
            os.fsync(stream.fileno())
        replace_published(temp, field_path)
        view_path = root / "fields" / f"view-{current_step:08d}.json"
        write_json(view_path, {"step": current_step, "time": current_step * p["dt_s"], "shape": [p["ny"], p["nx"]],
                               "field_unit": "1", "values": state.tolist()})
        row = {"step": current_step, "time": current_step * p["dt_s"],
               "path": field_path.relative_to(root).as_posix(), "sha256": file_hash(field_path),
               "view_path": view_path.relative_to(root).as_posix(), "view_sha256": file_hash(view_path)}
        measure = instruments(state, current_step, p, x, y)
        measure.update(field_path=row["path"], field_sha256=row["sha256"])
        index["frames"] = [item for item in index["frames"] if item["step"] < current_step] + [row]
        measurements["series"] = [item for item in measurements["series"] if item["step"] < current_step] + [measure]
        if force_image or any(last_image_step < threshold <= current_step for threshold in image_thresholds):
            image = render_field(root, state, row, measure, p, color_scale)
            observations["images"] = [item for item in observations["images"] if item["step"] != current_step] + [image]
            observations["images"].sort(key=lambda item: item["step"])
            last_image_step = current_step
        index.update(frame_count=len(index["frames"]), start_time=0, end_time=row["time"])
        write_json(root / "fields/index.json", index)
        write_json(root / "measurements.json", measurements)
        write_json(root / "observations/index.json", observations)
        temporary = root / "checkpoint.npz.tmp"
        with temporary.open("wb") as stream:
            np.savez(stream, field=state.astype("<f8", copy=False))
            stream.flush()
            os.fsync(stream.fileno())
        replace_published(temporary, root / "checkpoint.npz")
        write_json(checkpoint_path, {"schema_version": 1, "step": current_step, "time": row["time"],
                                    "input_sha256": input_hash, "worker_sha256": worker_hash, "numpy_version": np.__version__,
                                    "checkpoint_sha256": file_hash(root / "checkpoint.npz"), "field_sha256": row["sha256"],
                                    "initial_field_sha256": manifest["initial_field_sha256"],
                                    "field_index_sha256": file_hash(root / "fields/index.json"),
                                    "measurements_sha256": file_hash(root / "measurements.json"),
                                    "observations_sha256": file_hash(root / "observations/index.json")})
        total_bytes = sum(path.stat().st_size for path in root.rglob("*") if path.is_file())
        write_json(root / "progress.json", {"fraction": current_step / p["steps"], "step": current_step,
                   "time": row["time"], "time_unit": "s", "state": status, "storage_bytes": total_bytes,
                   "message": f"Diffusion integration {current_step}/{p['steps']} steps; numeric field and instruments retained"})
        if len(index["frames"]) > MAX_FRAMES or total_bytes > p["max_output_mb"] * 1024**2:
            raise ValueError("Retained output exceeded its explicit monitored storage budget; artifacts preserved")

    if not resumed:
        save(field, 0)
    rx = p["diffusivity_um2_s"] * p["dt_s"] / (p["length_x_um"] / p["nx"])**2
    ry = p["diffusivity_um2_s"] * p["dt_s"] / (p["length_y_um"] / p["ny"])**2
    while step < p["steps"]:
        if (root / "cancel.request").is_file():
            save(field, step, force_image=True, status="paused")
            write_json(root / "result.json", {"status": "paused", "engine": ENGINE, "step": step, "checkpoint": "checkpoint.json",
                       "field_index": "fields/index.json", "observations": "observations/index.json"})
            return 3
        # Independent checker derives the discrete spectrum; it never imports this stencil.
        field = field + rx * (np.roll(field, 1, axis=1) - 2 * field + np.roll(field, -1, axis=1)) + ry * (np.roll(field, 1, axis=0) - 2 * field + np.roll(field, -1, axis=0))
        step += 1
        if step % p["record_interval"] == 0 or step == p["steps"]:
            if not np.isfinite(field).all():
                raise FloatingPointError("Non-finite field encountered; no stabilized substitute was applied")
            save(field, step, status="completed" if step == p["steps"] else "running")
    # A resumed completed checkpoint still yields a valid, explicitly retained summary.
    final = measurements["series"][-1]
    first = measurements["series"][0]
    result = {
        "status": "completed", "engine": ENGINE, "adapter_version": ADAPTER_VERSION,
        "steps": step, "simulated_time_s": step * p["dt_s"], "compute_wall_seconds": time.perf_counter() - start,
        "initial": first, "final": final, "relative_integral_drift": (final["integral"] - first["integral"]) / max(abs(first["integral"]), 1e-300),
        "field_index": "fields/index.json", "measurements": "measurements.json", "observations": "observations/index.json",
        "checkpoint": "checkpoint.json", "manifest": "manifest.json",
        "scientific_validation": "Requires the independent field checker and study-specific reference; execution alone does not establish predictive validity",
    }
    write_json(root / "result.json", result)
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        code = run(args.input, args.output)
        print(json.dumps({"engine": ENGINE, "exit_code": code, "output": str(args.output)}))
        return code
    except Exception as error:
        print(json.dumps({"engine": ENGINE, "error": str(error), "type": type(error).__name__}), file=sys.stderr)
        if args.output.is_absolute() and args.output.is_dir():
            write_json(args.output / "error.json", {"engine": ENGINE, "message": str(error), "type": type(error).__name__})
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
