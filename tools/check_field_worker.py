#!/usr/bin/env python3
"""Independent acceptance checker for the periodic diffusion laboratory.

The criteria in docs/validation/diffusion-lab-proposal.md predate execution.
This file never imports worker code. Expected fields use scalar math.cos,
math.sin and math.exp evaluated on input-defined cell-center coordinates.
NumPy and Pillow read retained arrays and images; they are not reference solvers.
Every subprocess receipt, including failed attempts, is retained under --output.
"""
from __future__ import annotations

import argparse
import copy
import datetime as datetime_module
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import time
import traceback

import numpy as np
from PIL import Image


FIELD_TOL = 1e-10
INTEGRAL_TOL = 1e-11
EXTREMA_TOL = 1e-12
INSTRUMENT_TOL = 1e-10


def encode(value):
    return json.dumps(value, sort_keys=True, indent=2, allow_nan=False).encode("utf-8")


def write(path, value):
    Path(path).write_bytes(encode(value))


def read(path):
    def reject(value):
        raise ValueError(f"nonfinite JSON value: {value}")
    return json.loads(Path(path).read_text(encoding="utf-8"), parse_constant=reject)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, detail):
    if not condition:
        raise AssertionError(detail)


def contained(root, name):
    path = (root / name).resolve()
    require(path.is_relative_to(root.resolve()), f"artifact escapes output: {name}")
    return path


def fourier_parameters(n=64, diffusivity=.2):
    return {
        "nx": n, "ny": n, "length_x_um": 10., "length_y_um": 10.,
        "diffusivity_um2_s": diffusivity, "dt_s": .005 * (64 / n) ** 2,
        "steps": int(512 * (n / 64) ** 2), "record_interval": int(8 * (n / 64) ** 2),
        "initial": {"kind": "fourier", "baseline": 1., "amplitude": .2,
                    "mode_x": 1, "mode_y": 2}, "boundary": "periodic",
        "probes": [
            {"name": "off_center_point", "kind": "point", "x_um": .2, "y_um": 2.4},
            {"name": "interior_region", "kind": "region", "x_min_um": 1.3,
             "x_max_um": 3.8, "y_min_um": 4.2, "y_max_um": 7.1},
        ],
    }


def basis(parameters):
    """Cell centers and Fourier values reconstructed from saved input only."""
    p = parameters
    nx, ny = p["nx"], p["ny"]
    dx, dy = p["length_x_um"] / nx, p["length_y_um"] / ny
    x = [(ix + .5) * dx for ix in range(nx)]
    y = [(iy + .5) * dy for iy in range(ny)]
    mx, my = p["initial"]["mode_x"], p["initial"]["mode_y"]
    return [math.cos(2 * math.pi * mx * xx / p["length_x_um"])
            * math.cos(2 * math.pi * my * yy / p["length_y_um"])
            for yy in y for xx in x]


def amplification(parameters):
    p = parameters
    dx, dy = p["length_x_um"] / p["nx"], p["length_y_um"] / p["ny"]
    modes = p["initial"]
    return 1 - 4 * p["diffusivity_um2_s"] * p["dt_s"] * (
        math.sin(math.pi * modes["mode_x"] / p["nx"]) ** 2 / dx ** 2
        + math.sin(math.pi * modes["mode_y"] / p["ny"]) ** 2 / dy ** 2)


def continuum_amplitude(parameters, step):
    p, initial = parameters, parameters["initial"]
    wavenumber_squared = ((2 * math.pi * initial["mode_x"] / p["length_x_um"]) ** 2
                          + (2 * math.pi * initial["mode_y"] / p["length_y_um"]) ** 2)
    return initial["amplitude"] * math.exp(
        -p["diffusivity_um2_s"] * wavenumber_squared * step * p["dt_s"])


def scalar_measurements(values, parameters, mode):
    mean = math.fsum(values) / len(values)
    centered = [value - mean for value in values]
    return {
        "integral": mean * parameters["length_x_um"] * parameters["length_y_um"],
        "mean": mean, "min": min(values), "max": max(values),
        "variance": math.fsum(value * value for value in centered) / len(values),
        "mode_amplitude": (math.fsum(value * component for value, component
                                      in zip(centered, mode, strict=True))
                           / math.fsum(component * component for component in mode)),
    }


class Runner:
    def __init__(self, worker, root):
        self.worker, self.root = worker.resolve(), root.resolve()

    def prepare(self, label, parameters):
        folder = self.root / label
        folder.mkdir()
        input_path = folder / "input.json"
        write(input_path, {"engine": "diffusion_2d", "parameters": parameters})
        return folder, input_path, folder / "output"

    def command(self, input_path, output):
        # Interpreter flags are not inherited from this checker's own process.
        # The bundled source runtime is immutable: imported libraries must never
        # create __pycache__ entries in its checked inventory.
        return [sys.executable, "-I", "-B", str(self.worker), "--input", str(input_path),
                "--output", str(output)]

    def execute(self, folder, input_path, output, label="process", timeout=300):
        command, started = self.command(input_path, output), time.time()
        receipt = {"command": command, "started_unix_s": started,
                   "input_sha256": sha(input_path), "worker_sha256": sha(self.worker)}
        with (folder / f"{label}.stdout.txt").open("wb") as stdout, \
                (folder / f"{label}.stderr.txt").open("wb") as stderr:
            try:
                result = subprocess.run(command, stdout=stdout, stderr=stderr, timeout=timeout)
                receipt["exit_code"] = result.returncode
            except subprocess.TimeoutExpired:
                receipt.update(exit_code=None, timed_out=True)
            finally:
                receipt["elapsed_s"] = time.time() - started
                write(folder / f"{label}.receipt.json", receipt)
        return receipt

    def launch(self, label, parameters):
        folder, input_path, output = self.prepare(label, parameters)
        receipt = self.execute(folder, input_path, output)
        require(receipt["exit_code"] == 0,
                f"{label}: worker exit {receipt['exit_code']}; retained {folder}")
        return output, receipt


def inspect_fourier(output, parameters):
    index = read(output / "fields/index.json")
    measurements = read(output / "measurements.json")
    inspect_metadata(output, parameters, index)
    series = measurements["series"]
    expected_steps = list(range(0, parameters["steps"] + 1, parameters["record_interval"]))
    if expected_steps[-1] != parameters["steps"]:
        expected_steps.append(parameters["steps"])
    require([frame["step"] for frame in index["frames"]] == expected_steps,
            "retained frame steps must include initial, interval, and final")
    require([row["step"] for row in series] == expected_steps, "instrument step indexing")
    mode, factor = basis(parameters), amplification(parameters)
    initial = parameters["initial"]
    initial_values = [initial["baseline"] + initial["amplitude"] * value for value in mode]
    initial_integral = math.fsum(initial_values) / len(initial_values) \
        * parameters["length_x_um"] * parameters["length_y_um"]
    low, high = min(initial_values), max(initial_values)
    results = []
    for frame, measured in zip(index["frames"], series, strict=True):
        path = contained(output, frame["path"])
        require(sha(path) == frame["sha256"], f"field hash at step {frame['step']}")
        require(measured["field_path"] == frame["path"]
                and measured["field_sha256"] == frame["sha256"], "instrument source field receipt")
        require(path.stat().st_size <= 16 * 1024 * 1024, "field chunk exceeds 16 MiB")
        array = np.load(path, allow_pickle=False)
        require(array.dtype == np.dtype("float64"), "authoritative dtype must be float64")
        require(array.shape == (parameters["ny"], parameters["nx"]), "saved array shape")
        values = array.ravel(order="C").tolist()
        require(all(math.isfinite(value) for value in values), "nonfinite saved field")
        step, physical_time = frame["step"], frame["step"] * parameters["dt_s"]
        require(abs(frame["time"] - physical_time) < 1e-12, "field physical time")
        require(abs(measured["time"] - physical_time) < 1e-12, "instrument physical time")
        amplitude = initial["amplitude"] * factor ** step
        expected = [initial["baseline"] + amplitude * component for component in mode]
        field_error = max(abs(a - b) for a, b in zip(values, expected, strict=True))
        stats = scalar_measurements(values, parameters, mode)
        integral_drift = abs(stats["integral"] - initial_integral) / abs(initial_integral)
        instrument_errors = {key: abs(value - measured[key]) for key, value in stats.items()}
        probes = inspect_probes(values, parameters, measured)
        analytic_amplitude = continuum_amplitude(parameters, step)
        fluctuation = [value - stats["mean"] for value in values]
        exact_fluctuation = [analytic_amplitude * component for component in mode]
        continuum_error = math.sqrt(math.fsum((a - b) ** 2 for a, b in zip(
            fluctuation, exact_fluctuation, strict=True)) / math.fsum(
                value ** 2 for value in exact_fluctuation))
        row = {"step": step, "time_s": physical_time, "max_absolute_field_error": field_error,
               "relative_integral_drift": integral_drift, "independent": stats,
               "instrument_absolute_errors": instrument_errors, "probes": probes,
               "continuum_relative_l2_fluctuation_error": continuum_error,
               "passed": field_error < FIELD_TOL and integral_drift < INTEGRAL_TOL
               and stats["min"] >= low - EXTREMA_TOL and stats["max"] <= high + EXTREMA_TOL
               and max(instrument_errors.values()) < INSTRUMENT_TOL
               and all(probe["passed"] for probe in probes)}
        results.append(row)
    return {"frames": results, "discrete_amplification_factor": factor,
            "max_absolute_field_error": max(row["max_absolute_field_error"] for row in results),
            "max_relative_integral_drift": max(row["relative_integral_drift"] for row in results),
            "final_continuum_relative_l2_error": results[-1]["continuum_relative_l2_fluctuation_error"],
            "passed": all(row["passed"] for row in results)}


def inspect_metadata(output, parameters, index):
    require(index["shape"] == [parameters["ny"], parameters["nx"]], "index shape")
    require(index["time_unit"] == "s" and index["field_unit"] == "1"
            and index["length_unit"] == "um", "index physical units")
    require(index["axis_order"] == ["y", "x"] and index["grid_location"] == "cell_center",
            "index axis order and sampling location")
    require(index["boundary"] == parameters["boundary"] == "periodic", "boundary model")
    for axis in ("x", "y"):
        n, length = parameters[f"n{axis}"], parameters[f"length_{axis}_um"]
        coordinates = index[f"{axis}_um"]
        require(len(coordinates) == n, f"{axis} coordinate count")
        require(max(abs(coordinate - (cell + .5) * length / n)
                    for cell, coordinate in enumerate(coordinates)) < 1e-12,
                f"{axis} coordinates reconstructed from input")
    manifest = read(output / "manifest.json")
    require(manifest["engine"] == "diffusion_2d", "manifest engine")

    def same_requested_value(actual, requested, location):
        # Normalization may add defaults or derived provenance; every requested
        # scientific value must remain exact. Added source hashes are checked below.
        if isinstance(requested, dict):
            require(isinstance(actual, dict), f"worker substituted parameter {location}")
            for key, value in requested.items():
                require(key in actual, f"worker dropped parameter {location}.{key}")
                same_requested_value(actual[key], value, f"{location}.{key}")
        else:
            require(actual == requested, f"worker substituted parameter {location}")

    for key, value in parameters.items():
        same_requested_value(manifest["input"]["parameters"][key], value, key)
    if parameters["initial"]["kind"] == "array":
        initial = manifest["input"]["parameters"]["initial"]
        require(initial["source_sha256"] == sha(contained(output, parameters["initial"]["path"])),
                "imported array source hash")
    require(read(output / "result.json")["status"] == "completed", "completed worker receipt")
    return True


def inspect_probes(values, parameters, measured):
    """Containing-cell points and unweighted half-open cell-center regions."""
    nx, ny = parameters["nx"], parameters["ny"]
    dx, dy = parameters["length_x_um"] / nx, parameters["length_y_um"] / ny
    configured, observed = parameters.get("probes", []), measured.get("probes", [])
    require(len(configured) == len(observed), "probe count")
    results = []
    for probe, result in zip(configured, observed, strict=True):
        require(probe["name"] == result["name"], "probe name/order")
        if probe["kind"] == "point":
            ix, iy = math.floor(probe["x_um"] / dx), math.floor(probe["y_um"] / dy)
            selected = [values[iy * nx + ix]]
        elif probe["kind"] == "region":
            selected = [values[iy * nx + ix] for iy in range(ny) for ix in range(nx)
                        if probe["x_min_um"] <= (ix + .5) * dx < probe["x_max_um"]
                        and probe["y_min_um"] <= (iy + .5) * dy < probe["y_max_um"]]
        else:
            raise AssertionError(f"unknown probe kind {probe['kind']}")
        require(bool(selected), "probe must select at least one cell")
        expected = math.fsum(selected) / len(selected)
        error = abs(expected - result["value"])
        results.append({"name": probe["name"], "expected": expected,
                        "observed": result["value"], "cell_count": len(selected),
                        "absolute_error": error, "passed": error < INSTRUMENT_TOL})
    return results


def expected_rgb(value, scale):
    """Declared two-stop linear blue/orange map, independently evaluated."""
    require(scale["map"] == "linear_blue_orange", "undeclared color map")
    require(scale["stops"] == [[0, [20, 38, 80]], [1, [255, 190, 60]]], "palette stops")
    low, high = scale["min"], scale["max"]
    require(math.isfinite(low) and math.isfinite(high) and high > low, "color scale range")
    fraction = min(1., max(0., (value - low) / (high - low)))
    return tuple(round(start + fraction * (finish - start))
                 for start, finish in zip((20, 38, 80), (255, 190, 60), strict=True))


def inspect_images(output):
    fields = read(output / "fields/index.json")
    observations = read(output / "observations/index.json")
    lookup = {frame["step"]: frame for frame in fields["frames"]}
    instruments = {row["step"]: row for row in read(output / "measurements.json")["series"]}
    require(bool(observations["images"]), "no evidence images")
    image_results, scales = [], []
    for record in observations["images"]:
        frame = lookup[record["step"]]
        require(record["field_path"] == frame["path"], "image source path")
        require(record["field_sha256"] == frame["sha256"], "image source receipt hash")
        require(record["measurements"] == instruments[frame["step"]], "image measurement provenance")
        field_path, image_path = contained(output, frame["path"]), contained(output, record["path"])
        require(sha(field_path) == record["field_sha256"], "actual source hash")
        require(sha(image_path) == record["sha256"], "PNG hash")
        require(record["time_unit"] == "s" and abs(record["time"] - frame["time"]) < 1e-12,
                "image physical time")
        require(record["camera"]["orientation"] == "x-right_y-up", "declared orientation")
        left, top, right, bottom = record["camera"]["plot_bbox"]
        array = np.load(field_path, allow_pickle=False)
        ny, nx = array.shape
        with Image.open(image_path) as source:
            require(source.format == "PNG" and source.size == (1024, 1024), "1024-square PNG")
            require(0 <= left < right <= 1024 and 0 <= top < bottom <= 1024, "plot bounds")
            image = source.convert("RGB")
            samples = []
            for iy in sorted({1, ny // 5, ny // 2, 4 * ny // 5, ny - 2}):
                for ix in sorted({1, nx // 5, nx // 2, 4 * nx // 5, nx - 2}):
                    px = left + math.floor((ix + .5) * (right - left) / nx)
                    py = top + math.floor((ny - iy - .5) * (bottom - top) / ny)
                    expected = expected_rgb(float(array[iy, ix]), record["color_scale"])
                    observed = image.getpixel((px, py))
                    error = max(abs(a - b) for a, b in zip(expected, observed, strict=True))
                    samples.append({"cell_yx": [iy, ix], "pixel_xy": [px, py],
                                    "expected_rgb": list(expected), "observed_rgb": list(observed),
                                    "max_channel_error": error})
        scales.append(record["color_scale"])
        image_results.append({"step": frame["step"], "path": record["path"], "samples": samples,
                              "max_channel_error": max(s["max_channel_error"] for s in samples),
                              "passed": all(s["max_channel_error"] <= 1 for s in samples)})
    require(all(scale == scales[0] for scale in scales), "color scale changed across run images")
    return {"images": image_results, "color_scale": scales[0],
            "passed": all(result["passed"] for result in image_results)}


def inspect_array_instruments(output, parameters, initial_values):
    index, measures = read(output / "fields/index.json"), read(output / "measurements.json")
    inspect_metadata(output, parameters, index)
    results = []
    for frame, measured in zip(index["frames"], measures["series"], strict=True):
        path = contained(output, frame["path"])
        require(sha(path) == frame["sha256"], "array fixture hash")
        require(measured["field_path"] == frame["path"]
                and measured["field_sha256"] == frame["sha256"], "array instrument source receipt")
        array = np.load(path, allow_pickle=False)
        require(array.dtype == np.dtype("float64"), "array fixture dtype")
        values = array.ravel(order="C").tolist()
        if frame["step"] == 0:
            require(values == initial_values, "imported initial values changed")
        mean = math.fsum(values) / len(values)
        stats = {"mean": mean, "integral": mean * parameters["length_x_um"] * parameters["length_y_um"],
                 "min": min(values), "max": max(values),
                 "variance": math.fsum((value - mean) ** 2 for value in values) / len(values)}
        errors = {key: abs(value - measured[key]) for key, value in stats.items()}
        probes = inspect_probes(values, parameters, measured)
        results.append({"step": frame["step"], "instrument_absolute_errors": errors,
                        "probes": probes, "passed": max(errors.values()) < INSTRUMENT_TOL
                        and all(probe["passed"] for probe in probes)})
    return {"frames": results, "passed": bool(results) and all(row["passed"] for row in results)}


def array_fixture(runner):
    parameters = fourier_parameters(64)
    parameters.update(nx=48, ny=32, length_x_um=12., length_y_um=8., steps=16, record_interval=8,
                      initial={"kind": "array", "path": "imports/initial.npy"})
    folder, input_path, output = runner.prepare("asymmetric-array-orientation", parameters)
    (output / "imports").mkdir(parents=True)
    values = [0.2 + .02 * iy + .006 * ix + .07 * math.sin(2 * math.pi * (ix + .5) / 48)
              for iy in range(32) for ix in range(48)]
    np.save(output / "imports/initial.npy", np.array(values, dtype="float64").reshape((32, 48)),
            allow_pickle=False)
    receipt = runner.execute(folder, input_path, output)
    require(receipt["exit_code"] == 0, "asymmetric array fixture failed")
    instruments, visual = inspect_array_instruments(output, parameters, values), inspect_images(output)
    require(any(image["step"] == 0 for image in visual["images"]), "initial orientation evidence absent")
    return {"process": receipt, "output": str(output), "instruments": instruments, "visual": visual,
            "fixture": "48x32 array, independently specified asymmetric x/y gradient with sinusoid",
            "passed": instruments["passed"] and visual["passed"]}


def invalid_inputs(runner):
    base = fourier_parameters(32)
    base.update(steps=8, record_interval=4)
    changes = [
        ("unstable_dt", {"dt_s": 2.}),
        ("unsupported_boundary", {"boundary": "reflecting"}),
        ("wrong_shape_array", {"initial": {"kind": "array", "path": "imports/initial.npy"}}),
        ("object_array", {"initial": {"kind": "array", "path": "imports/initial.npy"}}),
        ("nonfinite_array", {"initial": {"kind": "array", "path": "imports/initial.npy"}}),
        ("unknown_parameter", {"made_up": 1}),
        ("too_many_steps", {"steps": 100001, "record_interval": 200}),
        ("too_many_frames", {"steps": 1001, "record_interval": 1}),
        ("grid_below_bound", {"nx": 15}),
        ("grid_above_bound", {"nx": 513}),
    ]
    results = []
    for name, update in changes:
        parameters = copy.deepcopy(base)
        parameters.update(update)
        folder, input_path, output = runner.prepare(f"invalid-{name}", parameters)
        if name.endswith("array"):
            (output / "imports").mkdir(parents=True)
            if name == "wrong_shape_array":
                array = np.zeros((31, 32), dtype="float64")
            elif name == "object_array":
                array = np.full((32, 32), "not a number", dtype=object)
            else:
                array = np.ones((32, 32), dtype="float64")
                array[3, 7] = float("nan")
            np.save(output / "imports/initial.npy", array, allow_pickle=name == "object_array")
        receipt = runner.execute(folder, input_path, output)
        success_receipt = False
        if (output / "result.json").exists():
            result = read(output / "result.json")
            success_receipt = result.get("status") in {"completed", "complete", "success", "succeeded"}
        advanced = False
        if (output / "fields/index.json").exists():
            advanced = any(frame["step"] > 0 for frame in read(output / "fields/index.json")["frames"])
        results.append({"case": name, "process": receipt, "success_result_receipt": success_receipt,
                        "advanced_numeric_frame": advanced,
                        "passed": receipt["exit_code"] not in (None, 0, 3)
                        and not success_receipt and not advanced})
    return {"cases": results, "passed": all(result["passed"] for result in results)}


def recovery(runner):
    parameters = fourier_parameters(128)
    parameters.update(steps=32768, record_interval=1024)
    folder, input_path, output = runner.prepare("recovery-interrupted", parameters)
    command, started = runner.command(input_path, output), time.time()
    receipt = {"command": command, "started_unix_s": started, "input_sha256": sha(input_path),
               "worker_sha256": sha(runner.worker), "cancel_mechanism": "touch output/cancel.request"}
    with (folder / "interrupted.stdout.txt").open("wb") as stdout, \
            (folder / "interrupted.stderr.txt").open("wb") as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
        try:
            while time.time() - started < 180:
                if process.poll() is not None:
                    raise AssertionError("worker terminated before observed mid-integration cancellation")
                try:
                    progress = read(output / "progress.json")
                except (FileNotFoundError, json.JSONDecodeError, PermissionError):
                    time.sleep(.01)
                    continue
                if 1024 <= progress.get("step", -1) < parameters["steps"]:
                    (output / "cancel.request").touch()
                    receipt.update(cancel_requested_unix_s=time.time(), observed_progress=progress)
                    break
                time.sleep(.01)
            require("cancel_requested_unix_s" in receipt, "no running checkpoint before cancellation deadline")
            receipt["exit_code"] = process.wait(timeout=120)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
                receipt["forced_cleanup_after_check_failure"] = True
            receipt.setdefault("exit_code", process.returncode)
            receipt["elapsed_s"] = time.time() - started
            write(folder / "interrupted.receipt.json", receipt)
    require(receipt["exit_code"] == 3, f"cancellation exit {receipt['exit_code']} instead of 3")
    checkpoint = read(output / "checkpoint.json")
    checkpoint_step = checkpoint["step"]
    require(0 < checkpoint_step < parameters["steps"], "checkpoint is not mid-integration")
    paused = folder / "cancelled-checkpoint"
    paused.mkdir()
    for name in ("checkpoint.json", "checkpoint.npz", "progress.json", "fields/index.json", "measurements.json"):
        source, destination = output / name, paused / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
    previous_frames = read(output / "fields/index.json")["frames"]
    require(checkpoint["checkpoint_sha256"] == sha(output / "checkpoint.npz"), "checkpoint file receipt")
    require(checkpoint["field_sha256"] == previous_frames[-1]["sha256"], "checkpoint field receipt")
    require(checkpoint["initial_field_sha256"] == read(output / "manifest.json")["initial_field_sha256"],
            "checkpoint initial identity")
    require(checkpoint["field_index_sha256"] == sha(output / "fields/index.json"), "checkpoint output index receipt")
    with np.load(output / "checkpoint.npz", allow_pickle=False) as saved:
        require(bool(saved.files), "checkpoint has no arrays")
        for key in saved.files:
            require(not saved[key].dtype.hasobject, "checkpoint contains object array")
    incompatible_folder, incompatible_input, incompatible_output = runner.prepare(
        "recovery-incompatible-input", dict(parameters, diffusivity_um2_s=.21))
    shutil.copytree(output, incompatible_output)
    (incompatible_output / "cancel.request").unlink()
    incompatible = runner.execute(incompatible_folder, incompatible_input, incompatible_output)
    incompatible_refused = incompatible["exit_code"] not in (None, 0, 3) \
        and sha(incompatible_output / "checkpoint.npz") == checkpoint["checkpoint_sha256"]
    require(incompatible_refused, "resume accepted altered immutable input or changed checkpoint")
    (output / "cancel.request").unlink()
    resumed = runner.execute(folder, input_path, output, label="resumed")
    require(resumed["exit_code"] == 0, "resume did not complete")
    resumed_frames = read(output / "fields/index.json")["frames"]
    require(resumed_frames[:len(previous_frames)] == previous_frames, "resume changed prior field receipts")
    for frame in previous_frames:
        require(sha(contained(output, frame["path"])) == frame["sha256"], "resume changed prior field bytes")
    continuous_output, continuous = runner.launch("recovery-uninterrupted", parameters)
    continuous_frames = read(continuous_output / "fields/index.json")["frames"]
    final_a = contained(output, resumed_frames[-1]["path"])
    final_b = contained(continuous_output, continuous_frames[-1]["path"])
    a, b = np.load(final_a, allow_pickle=False), np.load(final_b, allow_pickle=False)
    same = a.dtype == b.dtype == np.dtype("float64") and a.shape == b.shape \
        and a.tobytes(order="C") == b.tobytes(order="C")
    return {"interrupted_process": receipt, "resumed_process": resumed,
            "uninterrupted_process": continuous, "checkpoint_step": checkpoint_step,
            "incompatible_input_process": incompatible, "incompatible_input_refused": incompatible_refused,
            "checkpoint_json_sha256": sha(paused / "checkpoint.json"),
            "checkpoint_npz_sha256": sha(paused / "checkpoint.npz"),
            "prior_retained_frames": len(previous_frames), "prior_receipts_unchanged": True,
            "resumed_final_sha256": sha(final_a), "uninterrupted_final_sha256": sha(final_b),
            "final_float64_bits_identical": same, "passed": same}


def report_exception(name, function, report, root):
    try:
        result = function()
    except Exception as error:
        result = {"passed": False, "error_type": type(error).__name__, "error": str(error),
                  "traceback": traceback.format_exc()}
    report["checks"][name] = result
    write(root / "report.json", report)
    print(json.dumps({"check": name, "passed": result.get("passed", False)}), flush=True)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", type=Path, default=Path(__file__).with_name("field_worker.py"))
    parser.add_argument("--output", type=Path, required=True,
                        help="New, nonexistent evidence directory; existing attempts are preserved")
    args = parser.parse_args()
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=False)
    proposal = Path(__file__).resolve().parent.parent / "docs/validation/diffusion-lab-proposal.md"
    report = {
        "schema": "phaseforge.diffusion-independent-check.v1",
        "started_utc": datetime_module.datetime.now(datetime_module.timezone.utc).isoformat(),
        "python": sys.version, "numpy": np.__version__, "pillow": Image.__version__,
        "worker": str(args.worker.resolve()), "worker_sha256": sha(args.worker),
        "checker_sha256": sha(Path(__file__)), "proposal_sha256": sha(proposal),
        "criteria": {"field_max_absolute_error_below": FIELD_TOL,
                     "relative_integral_drift_below": INTEGRAL_TOL,
                     "initial_extrema_slack": EXTREMA_TOL,
                     "instrument_absolute_error_below": INSTRUMENT_TOL,
                     "coarse_continuum_relative_l2_error_below": .008,
                     "fine_continuum_relative_l2_error_below": .0005,
                     "refinement_reduction_range": [3.5, 4.5],
                     "intervention_ratio_relative_error_at_most": .01,
                     "png_channel_error_at_most": 1,
                     "recovery_final_field_bit_identical": True}, "checks": {},
    }
    runner = Runner(args.worker, args.output)
    benchmark = {}

    def benchmark_case(n):
        parameters = fourier_parameters(n)
        output, receipt = runner.launch(f"fourier-{n}", parameters)
        result = inspect_fourier(output, parameters)
        result.update(process=receipt, output=str(output))
        benchmark[n] = result
        return result

    for n in (32, 64, 128):
        report_exception(f"fourier_{n}", lambda n=n: benchmark_case(n), report, args.output)

    def refinement():
        errors = [benchmark[n]["final_continuum_relative_l2_error"] for n in (32, 64, 128)]
        reductions = [errors[0] / errors[1], errors[1] / errors[2]]
        return {"errors_32_64_128": errors, "successive_reduction_factors": reductions,
                "passed": errors[0] < .008 and errors[2] < .0005
                and all(3.5 <= value <= 4.5 for value in reductions)}

    report_exception("continuum_refinement", refinement, report, args.output)

    def intervention():
        parameters = fourier_parameters(64, .4)
        output, receipt = runner.launch("fourier-64-double-d", parameters)
        doubled = inspect_fourier(output, parameters)
        base_final = benchmark[64]["frames"][-1]["independent"]["mode_amplitude"]
        final = doubled["frames"][-1]["independent"]["mode_amplitude"]
        expected_ratio = continuum_amplitude(parameters, parameters["steps"]) / continuum_amplitude(
            fourier_parameters(64), parameters["steps"])
        ratio = final / base_final
        relative_error = abs(ratio / expected_ratio - 1)
        return {"double_d": doubled, "process": receipt, "output": str(output),
                "measured_amplitude_ratio": ratio, "continuum_amplitude_ratio": expected_ratio,
                "relative_ratio_error": relative_error,
                "passed": doubled["passed"] and benchmark[64]["passed"]
                and final < base_final and relative_error <= .01}

    report_exception("controlled_diffusivity", intervention, report, args.output)
    report_exception("invalid_inputs", lambda: invalid_inputs(runner), report, args.output)
    report_exception("asymmetric_orientation_and_instruments", lambda: array_fixture(runner), report, args.output)

    def benchmark_images():
        results = [inspect_images(Path(benchmark[n]["output"])) for n in (32, 64, 128)]
        results.append(inspect_images(Path(report["checks"]["controlled_diffusivity"]["output"])))
        same_scale = all(result["color_scale"] == results[0]["color_scale"] for result in results)
        return {"runs": results, "cross_run_color_scale_identical": same_scale,
                "passed": same_scale and all(result["passed"] for result in results)}

    report_exception("benchmark_image_provenance", benchmark_images, report, args.output)
    report_exception("interruption_checkpoint_resume", lambda: recovery(runner), report, args.output)
    report["passed"] = all(result.get("passed", False) for result in report["checks"].values())
    report["finished_utc"] = datetime_module.datetime.now(datetime_module.timezone.utc).isoformat()
    write(args.output / "report.json", report)
    print(str(args.output / "report.json"), flush=True)
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
