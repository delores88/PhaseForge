"""Bounded SI heat conduction and periodic incompressible 2D flow.

Numerical arrays are authoritative. The legacy scalar display coordinates are
explicitly converted from metres to micrometres; they do not change solver units.
Only shared safe file publication helpers are imported from the diffusion worker.
"""
from __future__ import annotations

import argparse
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import re
import sys
import time

import numpy as np
from PIL import Image, ImageDraw, __version__ as PILLOW_VERSION

_helper = Path(__file__).with_name("field_worker.py")
_spec = importlib.util.spec_from_file_location("retained_field_io", _helper)
io = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(io)
VERSION = "1.0.0"
HEAT, FLOW = "heat_conduction_2d", "navier_stokes_2d"


def parameters(engine, raw, root):
    common = {"nx", "ny", "length_x_m", "length_y_m", "dt_s", "steps", "record_interval", "boundary", "max_output_mb", "initial"}
    extra = {"conductivity_W_mK", "density_kg_m3", "heat_capacity_J_kgK"} if engine == HEAT else {"kinematic_viscosity_m2_s", "density_kg_m3"}
    io.only_keys(raw, common | extra, "parameters")
    p = {"nx": io.integer("nx", raw.get("nx", 64), 16, 256), "ny": io.integer("ny", raw.get("ny", 64), 16, 256)}
    for key in ("length_x_m", "length_y_m"):
        p[key] = io.number(key, raw.get(key, .01 if engine == HEAT else 1.), 1e-9, 1e6, positive=True)
    p["dt_s"] = io.number("dt_s", raw.get("dt_s", .01 if engine == HEAT else .005), 1e-12, 1e6, positive=True)
    p["steps"] = io.integer("steps", raw.get("steps", 256), 1, 100000)
    p["record_interval"] = io.integer("record_interval", raw.get("record_interval", 8), 1, p["steps"])
    p["boundary"] = raw.get("boundary", "periodic")
    if p["boundary"] != "periodic":
        raise ValueError("Only periodic rectangle boundaries are implemented; walls and inlets require another adapter")
    p["max_output_mb"] = io.integer("max_output_mb", raw.get("max_output_mb", 256), 64, 2048)
    p["density_kg_m3"] = io.number("density_kg_m3", raw.get("density_kg_m3", 1000. if engine == HEAT else 1.), 1e-12, 1e12, positive=True)
    frames = math.ceil(p["steps"] / p["record_interval"]) + 1
    estimated = frames * (p["nx"] * p["ny"] * 104 + 4096) + 32 * 1024**2
    if frames > 1001 or estimated > p["max_output_mb"] * 1024**2:
        raise ValueError("Requested retained fields exceed 1001 frames or the conservative output budget; increase record_interval or budget explicitly")
    dx, dy = p["length_x_m"] / p["nx"], p["length_y_m"] / p["ny"]
    initial = dict(raw.get("initial", {"kind": "fourier"} if engine == HEAT else {"kind": "taylor_green"}))
    if engine == HEAT:
        p["conductivity_W_mK"] = io.number("conductivity_W_mK", raw.get("conductivity_W_mK", .6), 0, 1e12)
        p["heat_capacity_J_kgK"] = io.number("heat_capacity_J_kgK", raw.get("heat_capacity_J_kgK", 4184.), 1e-12, 1e12, positive=True)
        alpha = p["conductivity_W_mK"] / (p["density_kg_m3"] * p["heat_capacity_J_kgK"])
        stability = alpha * p["dt_s"] * (dx**-2 + dy**-2)
        if not math.isfinite(stability) or stability > .5:
            raise ValueError("Heat FTCS requires alpha*dt*(1/dx^2+1/dy^2) <= 0.5; no timestep adjustment was applied")
        kind = initial.get("kind", "fourier")
        if kind == "array":
            io.only_keys(initial, ("kind", "path"), "initial")
            if not isinstance(initial.get("path"), str) or not initial["path"].startswith("imports/"):
                raise ValueError("Initial array must be explicitly retained under imports/")
            io.local_file(root, initial.get("path"))
        elif kind in ("fourier", "gaussian"):
            allowed = ("mode_x", "mode_y") if kind == "fourier" else ("center_x_m", "center_y_m", "sigma_m")
            io.only_keys(initial, ("kind", "baseline_K", "amplitude_K", *allowed), "initial")
            initial = {"kind": kind, "baseline_K": io.number("baseline_K", initial.get("baseline_K", 300.), 0, 1e9),
                       "amplitude_K": io.number("amplitude_K", initial.get("amplitude_K", 10.), 0, 1e9), **{key: initial[key] for key in allowed if key in initial}}
            if kind == "fourier":
                for axis in ("x", "y"):
                    key = "mode_" + axis
                    initial[key] = io.integer(key, initial.get(key, 1), 0, p["n" + axis] // 4)
                if not initial["mode_x"] and not initial["mode_y"] or initial["baseline_K"] < initial["amplitude_K"]:
                    raise ValueError("Fourier temperature requires a nonzero mode and baseline_K >= amplitude_K")
            else:
                for axis in ("x", "y"):
                    key = "center_" + axis + "_m"
                    initial[key] = io.number(key, initial.get(key, p["length_" + axis + "_m"] / 2), 0, p["length_" + axis + "_m"])
                initial["sigma_m"] = io.number("sigma_m", initial.get("sigma_m", min(p["length_x_m"], p["length_y_m"]) / 10), min(dx, dy), max(p["length_x_m"], p["length_y_m"]))
        else:
            raise ValueError("Heat initial.kind must be fourier, gaussian or retained array")
        estimates = {"thermal_diffusivity_m2_s": alpha, "ftcs_number": stability, "ftcs_limit": .5}
    else:
        p["kinematic_viscosity_m2_s"] = io.number("kinematic_viscosity_m2_s", raw.get("kinematic_viscosity_m2_s", .001), 0, 1e6)
        # Strict cut-off excludes n/3 itself so quadratic convolution cannot alias.
        mx, my = (p["nx"] - 1) // 3, (p["ny"] - 1) // 3
        stability = p["kinematic_viscosity_m2_s"] * p["dt_s"] * ((2 * math.pi * mx / p["length_x_m"])**2 + (2 * math.pi * my / p["length_y_m"])**2)
        if not math.isfinite(stability) or stability > .5:
            raise ValueError("Flow requires nu*dt*(kx_max^2+ky_max^2) <= 0.5; no timestep adjustment was applied")
        kind = initial.get("kind", "taylor_green")
        if kind == "taylor_green":
            io.only_keys(initial, ("kind", "velocity_m_s", "mode"), "initial")
            if p["length_x_m"] != p["length_y_m"]:
                raise ValueError("Taylor-Green initial condition requires a square; use streamfunction_modes on a rectangle")
            initial = {"kind": kind, "velocity_m_s": io.number("velocity_m_s", initial.get("velocity_m_s", .1), 0, 1e6),
                       "mode": io.integer("mode", initial.get("mode", 1), 1, min(mx, my))}
        elif kind == "streamfunction_modes":
            io.only_keys(initial, ("kind", "modes"), "initial")
            modes = initial.get("modes")
            if not isinstance(modes, list) or not 1 <= len(modes) <= 16:
                raise ValueError("Provide 1..16 resolved sine streamfunction modes")
            normalized = []
            for item in modes:
                io.only_keys(item, ("mode_x", "mode_y", "amplitude_m2_s"), "mode")
                normalized.append({"mode_x": io.integer("mode_x", item.get("mode_x"), 1, mx),
                                   "mode_y": io.integer("mode_y", item.get("mode_y"), 1, my),
                                   "amplitude_m2_s": io.number("amplitude_m2_s", item.get("amplitude_m2_s"), -1e12, 1e12)})
            initial = {"kind": kind, "modes": normalized}
        else:
            raise ValueError("Flow initial.kind must be taylor_green or streamfunction_modes")
        estimates = {"diffusive_number": stability, "diffusive_limit": .5, "advective_cfl_limit": .5,
                     "dealiasing": "strict abs(integer mode)<N/3, state and nonlinear term at every RK stage", "retained_mode_max": [mx, my]}
    p["initial"] = initial
    return p, {**estimates, "expected_frames": frames, "estimated_output_bytes": estimated}


class Physics:
    def __init__(self, engine, p, root):
        self.engine, self.p = engine, p
        self.dx, self.dy = p["length_x_m"] / p["nx"], p["length_y_m"] / p["ny"]
        self.x = (np.arange(p["nx"]) + .5) * self.dx
        self.y = (np.arange(p["ny"]) + .5) * self.dy
        self.X, self.Y = np.meshgrid(self.x, self.y)
        self.kx = 2 * math.pi * np.fft.fftfreq(p["nx"], d=self.dx)[None, :]
        self.ky = 2 * math.pi * np.fft.fftfreq(p["ny"], d=self.dy)[:, None]
        self.k2 = self.kx**2 + self.ky**2
        self.inverse_k2 = np.divide(1., self.k2, out=np.zeros_like(self.k2), where=self.k2 != 0)
        mx = np.rint(np.fft.fftfreq(p["nx"]) * p["nx"])
        my = np.rint(np.fft.fftfreq(p["ny"]) * p["ny"])
        self.mask = (np.abs(mx)[None, :] < p["nx"] / 3) & (np.abs(my)[:, None] < p["ny"] / 3)
        initial = p["initial"]
        if engine == HEAT:
            if initial["kind"] == "array":
                state = np.load(io.local_file(root, initial["path"]), allow_pickle=False)
                if state.dtype.kind not in "fiu" or state.shape != (p["ny"], p["nx"]):
                    raise ValueError("Temperature array must be real numeric with exact [ny,nx] shape")
                self.initial = np.array(state, dtype=np.float64, order="C", copy=True)
            elif initial["kind"] == "fourier":
                self.basis = np.cos(2 * math.pi * initial["mode_x"] * self.X / p["length_x_m"]) * np.cos(2 * math.pi * initial["mode_y"] * self.Y / p["length_y_m"])
                self.initial = initial["baseline_K"] + initial["amplitude_K"] * self.basis
            else:
                ax = (self.X - initial["center_x_m"] + p["length_x_m"] / 2) % p["length_x_m"] - p["length_x_m"] / 2
                ay = (self.Y - initial["center_y_m"] + p["length_y_m"] / 2) % p["length_y_m"] - p["length_y_m"] / 2
                self.initial = initial["baseline_K"] + initial["amplitude_K"] * np.exp(-(ax**2 + ay**2) / (2 * initial["sigma_m"]**2))
            if not np.isfinite(self.initial).all() or np.any(self.initial < 0):
                raise ValueError("Temperature must be finite and nonnegative kelvin")
            self.units = {"temperature_K": "K", "heat_flux_x_W_m2": "W/m^2", "heat_flux_y_W_m2": "W/m^2"}
            self.primary, self.unit = "temperature_K", "K"
        else:
            modes = initial.get("modes")
            if initial["kind"] == "taylor_green":
                k = 2 * math.pi * initial["mode"] / p["length_x_m"]
                modes = [{"mode_x": initial["mode"], "mode_y": initial["mode"], "amplitude_m2_s": initial["velocity_m_s"] / k}]
            omega = np.zeros((p["ny"], p["nx"]))
            for mode in modes:
                kx, ky = 2 * math.pi * mode["mode_x"] / p["length_x_m"], 2 * math.pi * mode["mode_y"] / p["length_y_m"]
                omega += mode["amplitude_m2_s"] * (kx**2 + ky**2) * np.sin(kx * self.X) * np.sin(ky * self.Y)
            self.initial = omega
            self.units = {"vorticity_s_inv": "1/s", "velocity_x_m_s": "m/s", "velocity_y_m_s": "m/s", "pressure_Pa": "Pa", "divergence_s_inv": "1/s"}
            self.primary, self.unit = "vorticity_s_inv", "1/s"
            self.rhs(self.initial)  # Admission includes actual velocity-based CFL.

    def velocity(self, omega):
        wh = np.fft.fft2(omega) * self.mask
        psi = wh * self.inverse_k2
        return wh, (1j * self.ky * psi), (-1j * self.kx * psi)

    def rhs(self, omega):
        wh, uh, vh = self.velocity(omega)
        u, v = np.fft.ifft2(uh).real, np.fft.ifft2(vh).real
        cfl = self.p["dt_s"] * (float(np.max(np.abs(u))) / self.dx + float(np.max(np.abs(v))) / self.dy)
        if not math.isfinite(cfl) or cfl > .5:
            raise ValueError(f"Actual advective CFL {cfl:.8g} exceeds 0.5; integration stopped without changing physics/timestep")
        nonlinear = u * np.fft.ifft2(1j * self.kx * wh).real + v * np.fft.ifft2(1j * self.ky * wh).real
        rh = -np.fft.fft2(nonlinear) * self.mask - self.p["kinematic_viscosity_m2_s"] * self.k2 * wh
        rh[0, 0] = 0
        return np.fft.ifft2(rh).real

    def advance(self, state):
        dt = self.p["dt_s"]
        if self.engine == HEAT:
            alpha = self.p["conductivity_W_mK"] / (self.p["density_kg_m3"] * self.p["heat_capacity_J_kgK"])
            result = state + alpha * dt * ((np.roll(state, 1, 1) - 2 * state + np.roll(state, -1, 1)) / self.dx**2 + (np.roll(state, 1, 0) - 2 * state + np.roll(state, -1, 0)) / self.dy**2)
        else:
            a = self.rhs(state)
            b = self.rhs(state + dt * a / 2)
            c = self.rhs(state + dt * b / 2)
            d = self.rhs(state + dt * c)
            result = np.fft.ifft2(np.fft.fft2(state + dt * (a + 2*b + 2*c + d) / 6) * self.mask).real
        if not np.isfinite(result).all():
            raise FloatingPointError("Non-finite numerical state; no replacement state published")
        return result

    def channels(self, state):
        if self.engine == HEAT:
            k = self.p["conductivity_W_mK"]
            return {"temperature_K": state, "heat_flux_x_W_m2": -k * (np.roll(state, -1, 1) - np.roll(state, 1, 1)) / (2*self.dx),
                    "heat_flux_y_W_m2": -k * (np.roll(state, -1, 0) - np.roll(state, 1, 0)) / (2*self.dy)}
        wh, uh, vh = self.velocity(state)
        ux, uy = np.fft.ifft2(1j*self.kx*uh).real, np.fft.ifft2(1j*self.ky*uh).real
        vx, vy = np.fft.ifft2(1j*self.kx*vh).real, np.fft.ifft2(1j*self.ky*vh).real
        source = -self.p["density_kg_m3"] * (ux*ux + 2*uy*vx + vy*vy)
        pressure = np.fft.ifft2(-np.fft.fft2(source) * self.mask * self.inverse_k2).real
        return {"vorticity_s_inv": state, "velocity_x_m_s": np.fft.ifft2(uh).real, "velocity_y_m_s": np.fft.ifft2(vh).real,
                "pressure_Pa": pressure, "divergence_s_inv": ux + vy}

    def measure(self, channels, step):
        state = channels[self.primary]
        result = {"step": step, "time_s": step*self.p["dt_s"], "mean": float(np.mean(state)), "min": float(np.min(state)), "max": float(np.max(state)), "variance": float(np.var(state))}
        if self.engine == HEAT:
            result["thermal_energy_per_depth_J_m"] = float(np.sum(state) * self.dx * self.dy * self.p["density_kg_m3"] * self.p["heat_capacity_J_kgK"])
            if hasattr(self, "basis"):
                result["mode_amplitude_K"] = float(np.mean((state-np.mean(state))*self.basis) / np.mean(self.basis**2))
        else:
            u, v = channels["velocity_x_m_s"], channels["velocity_y_m_s"]
            result.update(kinetic_energy_per_mass_m2_s2=float(.5*np.mean(u*u+v*v)), enstrophy_s_inv2=float(.5*np.mean(state*state)),
                          divergence_max_s_inv=float(np.max(np.abs(channels["divergence_s_inv"]))), mean_velocity_x_m_s=float(np.mean(u)), mean_velocity_y_m_s=float(np.mean(v)),
                          circulation_m2_s=float(np.sum(state)*self.dx*self.dy), advective_cfl=float(self.p["dt_s"]*(np.max(np.abs(u))/self.dx+np.max(np.abs(v))/self.dy)),
                          mean_pressure_Pa=float(np.mean(channels["pressure_Pa"])))
        return result


def publish_array(path, state, *, archive=False):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("wb") as stream:
        if archive:
            np.savez(stream, **{k: np.asarray(v, dtype="<f8") for k, v in state.items()})
        else:
            np.save(stream, np.asarray(state, dtype="<f8"), allow_pickle=False)
        stream.flush()
        os.fsync(stream.fileno())
    io.replace_published(temporary, path)


def retained_artifact(root, relative):
    if not isinstance(relative, str) or not re.fullmatch(r"(?:fields/(?:field-[0-9]{8}\.npy|view-[0-9]{8}\.json|state-[0-9]{8}\.npz)|observations/frame-[0-9]{8}\.png)", relative):
        raise ValueError("Checkpoint contains an invalid retained artifact path")
    path = root
    for part in relative.split("/"):
        path = path / part
        if path.is_symlink() or path.is_junction():
            raise ValueError("Retained artifact links are forbidden")
    if not path.is_file() or path.stat().st_nlink != 1 or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("Retained artifact is not a plain job-local file")
    return path


def image_observation(root, state, frame, phys, scale):
    fraction = np.clip((state-scale["min"])/(scale["max"]-scale["min"]), 0, 1)
    rgb = np.rint(np.array(io.LOW_RGB) + fraction[:, :, None]*(np.array(io.HIGH_RGB)-np.array(io.LOW_RGB))).astype(np.uint8)
    image = Image.new("RGB", (1024, 1024), (13, 19, 31))
    # Retain geometric aspect ratio; pixel magnification is explicitly nearest-neighbour.
    aspect = phys.p["length_x_m"] / phys.p["length_y_m"]
    width, height = (768, max(1, round(768/aspect))) if aspect >= 1 else (max(1, round(768*aspect)), 768)
    left, top = (1024-width)//2, 130+(768-height)//2
    image.paste(Image.fromarray(np.flipud(rgb)).resize((width, height), Image.Resampling.NEAREST), (left, top))
    draw = ImageDraw.Draw(image)
    draw.text((36, 25), f"{phys.engine}: {phys.primary} [{phys.unit}]", fill="white", font=io.font(23))
    draw.text((36, 67), f"t = {frame['time']:.8g} s; actual {phys.p['nx']} x {phys.p['ny']} cells", fill="white", font=io.font(20))
    draw.text((36, 930), f"Periodic domain {phys.p['length_x_m']:.6g} x {phys.p['length_y_m']:.6g} m; y points up", fill="white", font=io.font(19))
    draw.text((36, 965), f"Fixed colour range [{scale['min']:.6g}, {scale['max']:.6g}] {phys.unit}; clipping affects pixels only", fill="white", font=io.font(18))
    path = root / "observations" / f"frame-{frame['step']:08d}.png"
    path.parent.mkdir(exist_ok=True)
    temporary = path.with_suffix(".tmp")
    image.save(temporary, format="PNG")
    io.replace_published(temporary, path)
    return {"step": frame["step"], "time": frame["time"], "path": path.relative_to(root).as_posix(), "sha256": io.file_hash(path),
            "field_source": {"path": frame["path"], "sha256": frame["sha256"]}, "shape": [phys.p["ny"], phys.p["nx"]],
            "color_scale": scale, "camera": {"projection": "orthographic", "plot_bbox_pixels": [left, top, width, height], "resampling": "nearest", "y_up": True},
            "scope": "Rendered direct numerical field; illustrative pixels do not establish real-material validity"}


def run(input_path, root):
    if not input_path.is_absolute() or not root.is_absolute():
        raise ValueError("Input and output paths must be absolute")
    root.mkdir(parents=True, exist_ok=True)
    request = io.load_json(input_path)
    io.only_keys(request, ("engine", "parameters"), "input")
    engine = request.get("engine")
    if engine not in (HEAT, FLOW):
        raise ValueError("Unsupported continuum engine")
    p, estimates = parameters(engine, request.get("parameters", {}), root)
    phys = Physics(engine, p, root)
    normalized = {"engine": engine, "parameters": p}
    input_sha = io.digest(json.dumps(normalized, sort_keys=True, separators=(",", ":"), allow_nan=False).encode())
    pins = {"input_sha256": input_sha, "worker_sha256": io.file_hash(Path(__file__)), "shared_io_sha256": io.file_hash(_helper),
            "numpy_version": np.__version__, "initial_state_sha256": io.digest(phys.initial.astype("<f8").tobytes())}
    low, high = float(phys.initial.min()), float(phys.initial.max())
    if low == high:
        low, high = low-.5, high+.5
    scale = {"min": low, "max": high, "map": "linear_blue_orange", "stops": [[0, list(io.LOW_RGB)], [1, list(io.HIGH_RGB)]]}
    scope = ("Constant-property 2D periodic conduction per unit depth; no material calibration, sources, radiation, advection, phase change or walls." if engine == HEAT else
             "2D periodic incompressible Newtonian flow with constant density/viscosity, zero mean velocity and unforced resolved modes. No walls, inlets, free surfaces, compressibility, 3D turbulence or experimental calibration.")
    manifest = {"schema_version": 1, "engine": engine, "adapter_version": VERSION, "engine_version": f"NumPy {np.__version__}", **pins,
                "input": normalized, "platform": {"os": platform.platform(), "processor": "CPU"}, "environment": {"python": platform.python_version(), "numpy": np.__version__, "pillow": PILLOW_VERSION},
                "equations": "rho*cp*dT/dt=k*laplacian(T); q=-k*grad(T)" if engine == HEAT else "domega/dt+u*domega/dx+v*domega/dy=nu*laplacian(omega); -laplacian(psi)=omega; u=psi_y,v=-psi_x; div(u)=0; pressure Poisson with mean-zero gauge",
                "solver": {"method": "conservative five-point FTCS" if engine == HEAT else "Fourier vorticity-streamfunction pseudospectral RK4, strict 2/3 dealiasing", "dtype": "float64", "boundary": "periodic", **estimates},
                "units": {"length": "m", "time": "s", "channels": phys.units}, "scientific_scope": scope,
                "display_coordinates": {"length_unit": "um", "metres_to_display": 1e6, "note": "Exact dimensional conversion for existing scalar viewer; scientific channels remain SI"},
                "pressure": "Mean-zero diagnostic pressure of resolved velocity, dealiased to the same retained modes" if engine == FLOW else None}
    index = {"schema_version": 1, "representation": "scalar_field", "field_name": phys.primary, "field_unit": phys.unit,
             "time_unit": "s", "length_unit": "um", "shape": [p["ny"], p["nx"]], "axis_order": ["y", "x"], "grid_location": "cell_center",
             "x_um": (phys.x*1e6).tolist(), "y_um": (phys.y*1e6).tolist(), "lengths_um": [p["length_x_m"]*1e6, p["length_y_m"]*1e6],
             "solver_coordinates": {"length_unit": "m", "x_m": phys.x.tolist(), "y_m": phys.y.tolist()}, "channels": phys.units,
             "boundary": "periodic", "color_scale": scale, "frames": []}
    measurements = {"schema_version": 1, "time_unit": "s", "field_unit": phys.unit, "observation_model": "direct numerical cell-centre channels, SI units in instrument names", "series": []}
    observations = {"schema_version": 1, "images": []}
    checkpoint_path = root / "checkpoint.json"
    state, step = phys.initial.copy(), 0
    if checkpoint_path.is_file():
        cp = io.load_json(checkpoint_path)
        if any(cp.get(key) != value for key, value in pins.items()):
            raise ValueError("Checkpoint immutable input/source/initial/runtime pins changed")
        step = io.integer("checkpoint step", cp["step"], 0, p["steps"])
        index, measurements, observations = cp["index"], cp["measurements"], cp["observations"]
        if not index["frames"] or index["frames"][-1]["step"] != step:
            raise ValueError("Checkpoint frame boundary is inconsistent")
        for frame in index["frames"]:
            for pathkey, hashkey in (("path", "sha256"), ("view_path", "view_sha256"), ("state_path", "state_sha256")):
                if io.file_hash(retained_artifact(root, frame[pathkey])) != frame[hashkey]:
                    raise ValueError("Retained numerical frame hash changed")
        for row in observations["images"]:
            if io.file_hash(retained_artifact(root, row["path"])) != row["sha256"]:
                raise ValueError("Retained observation hash changed")
        with np.load(root/index["frames"][-1]["state_path"], allow_pickle=False) as saved:
            state = saved[phys.primary].copy()
        if state.shape != phys.initial.shape or not np.isfinite(state).all():
            raise ValueError("Invalid retained checkpoint state")
        # Public aliases may lag the last committed checkpoint after a crash.
        io.write_json(root/"fields/index.json", index)
        io.write_json(root/"measurements.json", measurements)
        io.write_json(root/"observations/index.json", observations)
        manifest["resumed_from_step"] = step
    io.write_json(root/"manifest.json", manifest)
    start = time.perf_counter()

    def save(current, current_step, status):
        if sum(row["step"] < current_step for row in index["frames"]) + 1 > 1001:
            raise ValueError("Retained state limit reached, including extra interruption frames; previous checkpoint remains available")
        channels = phys.channels(current)
        if any(not np.isfinite(value).all() for value in channels.values()):
            raise FloatingPointError("Non-finite derived channel")
        path = root/"fields"/f"field-{current_step:08d}.npy"
        archive = root/"fields"/f"state-{current_step:08d}.npz"
        view = root/"fields"/f"view-{current_step:08d}.json"
        publish_array(path, current)
        publish_array(archive, channels, archive=True)
        io.write_json(view, {"step": current_step, "time": current_step*p["dt_s"], "shape": [p["ny"], p["nx"]], "field_unit": phys.unit, "values": current.tolist()})
        frame = {"step": current_step, "time": current_step*p["dt_s"], "path": path.relative_to(root).as_posix(), "sha256": io.file_hash(path),
                 "view_path": view.relative_to(root).as_posix(), "view_sha256": io.file_hash(view), "state_path": archive.relative_to(root).as_posix(), "state_sha256": io.file_hash(archive), "channels": phys.units}
        index["frames"] = [row for row in index["frames"] if row["step"] < current_step] + [frame]
        index.update(frame_count=len(index["frames"]), start_time=0, end_time=frame["time"])
        measure = phys.measure(channels, current_step)
        measure.update(field_path=frame["path"], field_sha256=frame["sha256"], state_path=frame["state_path"], state_sha256=frame["state_sha256"])
        measurements["series"] = [row for row in measurements["series"] if row["step"] < current_step] + [measure]
        last = max((row["step"] for row in observations["images"]), default=-1)
        if current_step in (0, p["steps"]) or status == "paused" or any(last < math.ceil(p["steps"]*fraction) <= current_step for fraction in (.25, .5, .75)):
            observations["images"] = [row for row in observations["images"] if row["step"] != current_step] + [image_observation(root, current, frame, phys, scale)]
        # One atomic commit point contains the numerical boundary and all indices.
        # It references immutable per-step states; aliases below can be reconstructed.
        io.write_json(checkpoint_path, {"schema_version": 1, **pins, "step": current_step, "time_s": frame["time"], "index": index, "measurements": measurements, "observations": observations})
        io.write_json(root/"fields/index.json", index)
        io.write_json(root/"measurements.json", measurements)
        io.write_json(root/"observations/index.json", observations)
        io.write_json(root/"progress.json", {"status": status, "step": current_step, "steps": p["steps"], "fraction": current_step/p["steps"], "simulated_time_s": frame["time"], "field_frames": len(index["frames"]), "latest_instruments": measure})
        if sum(item.stat().st_size for item in root.rglob("*") if item.is_file()) > p["max_output_mb"]*1024**2:
            raise ValueError("Retained output storage exceeded its explicit budget")

    if not index["frames"]:
        save(state, 0, "running")
    while step < p["steps"]:
        if (root/"cancel.request").is_file():
            if index["frames"][-1]["step"] != step:
                save(state, step, "paused")
            io.write_json(root/"result.json", {"status": "paused", "engine": engine, "step": step, "checkpoint": "checkpoint.json"})
            return 3
        state = phys.advance(state)
        step += 1
        if step % p["record_interval"] == 0 or step == p["steps"]:
            save(state, step, "completed" if step == p["steps"] else "running")
    first, final = measurements["series"][0], measurements["series"][-1]
    result = {"status": "completed", "engine": engine, "adapter_version": VERSION, "steps": step, "simulated_time_s": step*p["dt_s"], "compute_wall_seconds": time.perf_counter()-start,
              "initial": first, "final": final, "field_index": "fields/index.json", "measurements": "measurements.json", "observations": "observations/index.json", "checkpoint": "checkpoint.json", "manifest": "manifest.json",
              "scientific_scope": scope, "scientific_validation": "Execution is not self-certified predictive validity; use independent analytic/convergence and study-specific checks"}
    if engine == HEAT:
        result["relative_energy_drift"] = (final["thermal_energy_per_depth_J_m"]-first["thermal_energy_per_depth_J_m"])/max(abs(first["thermal_energy_per_depth_J_m"]), 1e-300)
    else:
        result["relative_kinetic_energy_change"] = (final["kinetic_energy_per_mass_m2_s2"]-first["kinetic_energy_per_mass_m2_s2"])/max(abs(first["kinetic_energy_per_mass_m2_s2"]), 1e-300)
    io.write_json(root/"result.json", result)
    return 0


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        return run(args.input, args.output)
    except Exception as error:
        detail = {"type": type(error).__name__, "message": str(error)}
        print(json.dumps(detail), file=sys.stderr)
        if args.output.is_absolute() and args.output.is_dir():
            io.write_json(args.output/"error.json", detail)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
