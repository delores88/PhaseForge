#!/usr/bin/env python3
"""Independent scalar reference checks for the frozen harmonic gauge-wave fixture.

This is a flat spacetime in time-dependent coordinates, not radiation or a
black-hole collision. Analytic errors use math.fsum, not engine diagnostics.
Native constraints are reported separately as engine-computed diagnostics.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

from athenak_decode import read_binary, sha256

AMPLITUDE = 0.01
END_TIME = 0.1
MAX_ERRORS = {"alpha": 5e-5, "gxx": 1e-4, "gyy": 1e-4, "gzz": 1e-4,
              "Kxx": 5e-4, "Kyy": 5e-4, "Kzz": 5e-4,
              "betax": 1e-12, "betay": 1e-12, "betaz": 1e-12}
MAX_NATIVE_HAMILTONIAN = 1e-3
MIN_CONVERGENCE_RATIO = 2.0


def exact_at(x: float, time: float) -> dict:
    phase = 2 * math.pi * (x - time)
    h = AMPLITUDE * math.sin(phase)
    alpha = math.sqrt(1 - h)
    return {"alpha": alpha, "gxx": 1 - h, "gyy": 1., "gzz": 1.,
            "Kxx": -math.pi * AMPLITUDE * math.cos(phase) / alpha,
            "Kyy": 0., "Kzz": 0., "betax": 0., "betay": 0., "betaz": 0.}


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def validate_parameters(frame: dict):
    p = frame["parameters"]
    expected = {
        "problem": {"amp": .01},
        "time": {"tlim": .1, "cfl_number": .25},
        "z4c": {"lapse_harmonic": 1, "lapse_harmonicf": 1, "lapse_oplog": 0,
                "lapse_advect": 0, "shift_Gamma": 0, "shift_alpha2Gamma": 0,
                "shift_H": 0, "shift_advect": 0, "shift_eta": 0,
                "chi_psi_power": -4, "diss": 0, "damp_kappa1": 0, "damp_kappa2": 0},
    }
    try:
        for block, values in expected.items():
            for key, value in values.items():
                _require(float(p[block][key]) == value, f"unexpected {block}/{key}")
        _require(p["problem"]["pgen_name"] == "z4c_gauge_wave", "wrong problem generator")
        _require(p["time"]["integrator"] == "rk4", "wrong integration method")
        _require(p["z4c"]["slow_start_lapse"].lower() == "false", "wrong lapse gauge")
        for a in (1, 2, 3):
            _require(float(p["mesh"][f"x{a}min"]) == 0 and
                     float(p["mesh"][f"x{a}max"]) == 1, "wrong coordinate domain")
            for side in ("i", "o"):
                _require(p["mesh"][f"{side}x{a}_bc"] == "periodic", "wrong boundary condition")
    except KeyError as exc:
        raise ValueError(f"missing frozen fixture parameter {exc}") from exc
    _require(frame["root_shape"][1:] == (4, 4), "unexpected transverse resolution")
    _require(frame["root_shape"] == frame["block_shape"], "fixture must be one unrefined block")
    _require(frame["nghost"] == 2 and frame["location_bytes"] == 8,
             "fixture requires double evolution coordinates and two ghost cells")
    _require(len(frame["blocks"]) == 1, "fixture must retain the full single block")
    block = frame["blocks"][0]
    _require(block["logical"] == (0, 0, 0, 0), "fixture cannot contain refined blocks")
    dims = frame["block_shape"]
    expected_index = tuple(v for n in dims for v in (2, n + 1))
    _require(block["index"] == expected_index, "sliced or ghost-only fixture data")
    _require(block["geometry"] == (0., 1., 0., 1., 0., 1.), "wrong retained geometry")


def measure(frame: dict) -> dict:
    validate_parameters(frame)
    block = frame["blocks"][0]
    fields = block["fields"]
    required = {"z4c_alpha", "z4c_chi", "z4c_Khat", "z4c_Theta"}
    required |= {f"z4c_{prefix}{axis}{axis}" for prefix in ("g", "A") for axis in "xyz"}
    required |= {f"z4c_beta{axis}" for axis in "xyz"}
    _require(required <= fields.keys(), "missing retained metric, curvature or lapse channel")
    _require((fields["z4c_chi"] > 0).all(), "nonpositive conformal factor")
    errors = {key: [] for key in MAX_ERRORS}
    actual_alpha = []
    nx, ny, nz = frame["block_shape"]
    for k in range(nz):
        for j in range(ny):
            for i, x in enumerate(block["coordinates"][0]):
                exact = exact_at(float(x), frame["time"])
                at = lambda name: float(fields["z4c_" + name][k, j, i])
                chi, trace = at("chi"), at("Khat") + 2 * at("Theta")
                actual = {"alpha": at("alpha")}
                for axis in "xyz":
                    g = at(f"g{axis}{axis}") / chi
                    actual[f"g{axis}{axis}"] = g
                    actual[f"K{axis}{axis}"] = at(f"A{axis}{axis}") / chi + trace * g / 3
                    actual[f"beta{axis}"] = at(f"beta{axis}")
                actual_alpha.append(actual["alpha"])
                for key in errors:
                    errors[key].append(abs(actual[key] - exact[key]))
    summaries = {key: {"l1": math.fsum(values) / len(values), "linf": max(values),
                       "l2": math.sqrt(math.fsum(v*v for v in values) / len(values)),
                       "limit_linf": MAX_ERRORS[key]} for key, values in errors.items()}
    return {"time": frame["time"], "cycle": frame["cycle"],
            "source": {"path": frame["path"], "sha256": frame["sha256"], "bytes": frame["bytes"]},
            "cells": nx * ny * nz, "native_variable_bytes": frame["variable_bytes"],
            "errors": summaries, "alpha": actual_alpha,
            "passed": all(row["linf"] <= row["limit_linf"] for row in summaries.values())}


def check_run(directory: Path) -> dict:
    paths = sorted(directory.rglob("*.bin"))
    _require(1 <= len(paths) <= 64, "expected bounded retained native output set")
    frames = [read_binary(path) for path in paths]
    z4c = sorted((f for f in frames if "z4c_alpha" in f["variables"]), key=lambda f: f["time"])
    con = sorted((f for f in frames if "con_H" in f["variables"]), key=lambda f: f["time"])
    _require(len(z4c) >= 3 and len(z4c) == len(con), "missing temporal metric/constraint data")
    _require(z4c[0]["time"] == 0 and abs(z4c[-1]["time"] - END_TIME) <= 1e-12,
             "missing initial/final physical endpoint")
    _require(all(b["time"] > a["time"] and b["cycle"] > a["cycle"]
                 for a, b in zip(z4c, z4c[1:])), "nonincreasing retained time or cycle")
    _require(all(f["root_shape"] == z4c[0]["root_shape"] for f in z4c + con),
             "resolution changes inside frozen unrefined fixture")
    metrics, constraints = [], []
    for f, c in zip(z4c, con):
        _require(f["time"] == c["time"] and f["cycle"] == c["cycle"],
                 "metric/constraint temporal mismatch")
        metrics.append(measure(f))
        validate_parameters(c)
        values = [float(x) for x in c["blocks"][0]["fields"]["con_H"].flat]
        constraints.append({"time": c["time"], "sha256": c["sha256"],
            "path": c["path"], "hamiltonian_l1": math.fsum(abs(v) for v in values) / len(values),
            "hamiltonian_linf": max(abs(v) for v in values),
            "provenance": "engine-computed diagnostic; separate from independent analytic errors"})
    dynamic_change = max(abs(a-b) for a, b in zip(metrics[0]["alpha"], metrics[-1]["alpha"]))
    for row in metrics:
        del row["alpha"]
    _require(dynamic_change > 1e-3, "retained gauge wave did not evolve")
    passed = all(m["passed"] for m in metrics) and all(
        row["hamiltonian_linf"] <= MAX_NATIVE_HAMILTONIAN for row in constraints)
    return {"run": str(directory), "passed": passed, "nx": z4c[0]["root_shape"][0],
            "frames": metrics, "constraints": constraints, "alpha_change_linf": dynamic_change}


def compare_runs(runs: list[dict]) -> list[dict]:
    comparisons = []
    ordered = sorted(runs, key=lambda r: r["nx"])
    for coarse, fine in zip(ordered, ordered[1:]):
        _require(fine["nx"] == 2 * coarse["nx"], "convergence pairs must double x resolution")
        ratios = {}
        for key in ("alpha", "gxx", "Kxx"):
            a = coarse["frames"][-1]["errors"][key]["l1"]
            b = fine["frames"][-1]["errors"][key]["l1"]
            ratios[key] = a / b if b > 0 else None
        comparisons.append({"coarse_nx": coarse["nx"], "fine_nx": fine["nx"],
            "final_l1_ratios": ratios, "minimum_ratio": MIN_CONVERGENCE_RATIO,
            "passed": all(r is not None and r >= MIN_CONVERGENCE_RATIO for r in ratios.values())})
    return comparisons


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", action="append", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = {"schema": "phaseforge.athenak-gauge-check.v1", "scope": "analytic gauge-wave engine benchmark",
              "black_hole_collision_validated": False, "physical_radiation": False,
              "coordinate_units": "dimensionless geometrized length L=1, c=1; time L/c",
              "checker_sha256": sha256(Path(__file__)),
              "limits": {"linf": MAX_ERRORS, "native_hamiltonian_linf": MAX_NATIVE_HAMILTONIAN}}
    try:
        report["runs"] = [check_run(path) for path in args.run]
        report["convergence"] = compare_runs(report["runs"])
        report["passed"] = all(r["passed"] for r in report["runs"] + report["convergence"])
    except (ValueError, OSError, KeyError) as exc:
        report.update(passed=False, error=str(exc))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n", encoding="utf-8")
    print(json.dumps({"passed": report["passed"], "report": str(args.output)}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
