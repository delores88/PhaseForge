"""Pinned-protocol numerical surrogate instrument; stdlib/NumPy, no network.

Production: copy this source to the existing LPAC experiment.py. Read input.json
and explicit imports/ only; emit finite JSON and pickle-free numerical arrays.
Synthetic matrix fixtures exercise the same numerical operations independently
of scientific label generation. Execution does not certify scientific validity.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import math
from pathlib import Path
import platform
import sys
import time

import numpy as np


PROPOSAL = "b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737"
STUDY_PREFIX = "phaseforge-ml-argon-v1"
TEMPERATURES = [220, 250, 280, 310, 325, 370, 400, 430]
DENSITIES = [0.25, 0.45, 0.65, 0.85]
ROLES = ("train", "validation", "calibration", "test", "regime_test", "ood")
RIDGES = [1e-6, 1e-4, 1e-2, 1.0]
LENGTHS = [0.15, 0.3, 0.6, 1.2]
KERNEL_RIDGES = [1e-6, 1e-4, 1e-2, 0.1]
FIXED_PARAMETERS = {"atom_count": 108, "thermostat": "langevin", "friction_per_ps": 5.0,
                    "timestep_fs": 1.0, "steps": 20000, "sample_interval": 100,
                    "chunk_frames": 50, "platform": "CPU", "cpu_threads": 1}
R = 0.00831446261815324
NA = 6.02214076e23
MASS = 39.948


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=True, allow_nan=False).encode("ascii")


def digest(data):
    return hashlib.sha256(data).hexdigest()


def sha_string(value):
    require(isinstance(value, str) and len(value) == 64 and
            all(c in "0123456789abcdef" for c in value), "Invalid SHA-256")
    return value


def json_read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"),
                      parse_constant=lambda text: (_ for _ in ()).throw(ValueError(text)))


def json_write(path, value):
    raw = canonical(value)
    Path(path).write_bytes(raw)
    return digest(raw)


def finite(values, shape=None):
    values = np.asarray(values, dtype=np.float64)
    require(np.isfinite(values).all(), "Nonfinite numeric value")
    if shape is not None:
        require(values.shape == shape, f"Unexpected numeric shape {values.shape}, expected {shape}")
    return values


def imported(spec):
    require(isinstance(spec, dict), "An explicit pinned import is required")
    raw = spec.get("path")
    require(isinstance(raw, str) and raw and "\\" not in raw and ":" not in raw,
            "Invalid import path")
    pieces = raw.split("/")
    require(all(part not in ("", ".", "..") for part in pieces), "Unsafe import path")
    path = Path("imports").joinpath(*pieces)
    require(path.resolve().is_relative_to(Path("imports").resolve()) and path.is_file(),
            "Import must be a retained plain file")
    data = path.read_bytes()
    require(len(data) <= 64 * 1024 * 1024 and digest(data) == sha_string(spec.get("sha256")),
            "Imported artifact hash mismatch")
    return path


def fixed_protocol(worker_sha256, engine_version):
    sha_string(worker_sha256)
    require(engine_version == "8.5.2.dev-36a30cb", "Changed OpenMM engine identity")
    return {"engine": "openmm_argon", "openmm_wheel": "8.5.2", "engine_version": engine_version,
            "worker_sha256": worker_sha256, "platform": "CPU",
            "platform_properties": {"DeterministicForces": "true", "Threads": "1"},
            "fixed_parameters": FIXED_PARAMETERS,
            "potential": {"mass_dalton": MASS, "sigma_nm": 0.3405, "epsilon_kj_mol": 0.997,
                          "cutoff": "min(2.5*sigma,0.49*box_length)", "switch": "0.8*cutoff",
                          "switch_function": "quintic", "dispersion_correction": False,
                          "charges": 0, "boundary": "cubic periodic",
                          "initialization": "FCC; seeded zero-COM Maxwell velocities"},
            "target": {"name": "mean_pressure_bar", "step_exclusive": 10000, "step_inclusive": 20000,
                       "replicates": 3, "observations_per_seed": 100, "normalization": "P/P0",
                       "p0_definition": "(N-1)*R*T/V*(1e25/NA)", "R": R, "NA": NA},
            "feature_schema": [{"name": "temperature_kelvin", "unit": "K", "bounds": [220, 430]},
                               {"name": "density_g_cm3", "unit": "g/cm^3", "bounds": [0.25, 0.85]}]}


def membership():
    conditions = []
    for index, (temperature, density) in enumerate(itertools.product(TEMPERATURES, DENSITIES)):
        key = f"{STUDY_PREFIX}|T={temperature}|rho={density:.2f}"
        conditions.append({"condition_index": index, "temperature_kelvin": temperature,
                           "density_g_cm3": density, "split_key": key,
                           "split_sha256": digest(key.encode("ascii")),
                           "role": "regime_test" if temperature == 325 else None})
    order = sorted((row for row in conditions if row["role"] is None),
                   key=lambda row: (row["split_sha256"], row["temperature_kelvin"], row["density_g_cm3"]))
    for role, start, stop in [("train", 0, 12), ("validation", 12, 15), ("calibration", 15, 24), ("test", 24, 28)]:
        for row in order[start:stop]:
            row["role"] = role
    return conditions


def freeze(request):
    require(request.get("proposal_sha256") == PROPOSAL, "Preregistration hash mismatch")
    require(isinstance(request.get("study_id"), str) and request["study_id"], "A durable study ID is required")
    protocol = fixed_protocol(request["solver_worker_sha256"], request["solver_engine_version"])
    common = {"schema_version": 1, "study_id": request["study_id"], "proposal_sha256": PROPOSAL,
              "protocol": protocol, "protocol_sha256": digest(canonical(protocol))}
    conditions = membership()
    split = {**common, "conditions": conditions,
             "ood": {"condition_index": 32, "temperature_kelvin": 180, "density_g_cm3": 0.55, "role": "ood"}}
    split_sha = json_write("split.json", split)
    runs = []
    # The first scheduled training run is also the pilot and is reused by identity.
    order = sorted(conditions + [split["ood"]], key=lambda row: (ROLES.index(row["role"]), row["condition_index"]))
    for row in order:
        for replicate in range(3):
            seed = 110000 + 1000 * row["condition_index"] + replicate
            runs.append({"condition_index": row["condition_index"], "replicate_index": replicate,
                         "role": row["role"], "seed": seed,
                         "case_id": f"condition-{row['condition_index']:02d}-replicate-{replicate}",
                         "engine": "openmm_argon",
                         "parameters": {**FIXED_PARAMETERS, "temperature_kelvin": row["temperature_kelvin"],
                                        "density_g_cm3": row["density_g_cm3"], "seed": seed}})
    json_write("runs.json", {**common, "split_sha256": split_sha, "runs": runs})
    json_write("study.json", {**common, "split_sha256": split_sha, "solver_run_count": 99,
                              "pilot_case_id": runs[0]["case_id"], "status": "frozen_before_labels",
                              "predictive_success": "not_evaluated"})
    return {"status": "frozen_before_labels", "split_sha256": split_sha,
            "protocol_sha256": common["protocol_sha256"], "solver_run_count": len(runs),
            "artifacts": ["study.json", "split.json", "runs.json"]}


def normalized(points):
    points = finite(points)
    require(points.ndim == 2 and points.shape[1] == 2, "Exactly temperature and density are features")
    return (points - np.array([220.0, 0.25])) / np.array([210.0, 0.60])


def polynomial(points, family):
    x, y = finite(points).T
    require(family in ("linear", "quadratic"), "Unknown polynomial family")
    columns = [np.ones(len(x)), x, y]
    if family == "quadratic":
        columns += [x*x, x*y, y*y]
    return np.column_stack(columns)


def kernel(left, right, lengths):
    lengths = finite(lengths, (2,))
    require((lengths > 0).all(), "Kernel lengths must be positive")
    displacement = (finite(left)[:, None, :] - finite(right)[None, :, :]) / lengths
    return np.exp(-0.5 * np.sum(displacement * displacement, axis=2))


def fit_numeric(points, z, configuration):
    points = finite(points)
    z = finite(z, (len(points),))
    require(points.ndim == 2 and points.shape[1] == 2 and len(points) > 0, "Invalid fitting coordinates")
    residual = z - 1.0
    center = float(residual.mean())
    scale = max(float(residual.std()), 1e-6)
    target = (residual - center) / scale
    regularizer = float(configuration["regularizer"])
    require(math.isfinite(regularizer) and regularizer >= 0, "Invalid regularizer")
    family = configuration["family"]
    if family == "kernel":
        matrix = kernel(points, points, configuration["lengths"]) + regularizer * np.eye(len(points))
        lower = np.linalg.cholesky(matrix)
        weights = np.linalg.solve(lower.T, np.linalg.solve(lower, target))
    else:
        design = polynomial(points, family)
        penalty = np.eye(design.shape[1]); penalty[0, 0] = 0.0
        matrix = design.T @ design / len(points) + regularizer * penalty
        weights = np.linalg.solve(matrix, design.T @ target / len(points))
    finite(weights)
    return {"configuration": configuration, "center": center, "scale": scale,
            "points": points, "weights": weights}


def predict_numeric(model, points):
    configuration = model["configuration"]
    design = (kernel(points, model["points"], configuration["lengths"])
              if configuration["family"] == "kernel" else polynomial(points, configuration["family"]))
    return finite(1.0 + model["center"] + model["scale"] * (design @ model["weights"]))


def configurations():
    choices = [{"family": family, "regularizer": regularizer}
               for family in ("linear", "quadratic") for regularizer in RIDGES]
    choices += [{"family": "kernel", "lengths": [x, y], "regularizer": regularizer}
                for x, y, regularizer in itertools.product(LENGTHS, LENGTHS, KERNEL_RIDGES)]
    return choices


def calibration_quantile(residuals):
    values = finite(residuals, (9,))
    require((values >= 0).all(), "Calibration residuals must be absolute")
    return float(np.sort(values)[math.ceil((len(values) + 1) * 0.9) - 1])


def load_split(request):
    split = json_read(imported(request["split"]))
    require(split["proposal_sha256"] == PROPOSAL and request.get("proposal_sha256") == PROPOSAL,
            "Preregistration identity changed")
    require(split["study_id"] == request["study_id"], "Wrong study")
    require(split["conditions"] == membership(), "Frozen condition membership changed")
    protocol = split["protocol"]
    require(protocol == fixed_protocol(protocol["worker_sha256"], protocol["engine_version"]),
            "Scientific protocol changed")
    require(digest(canonical(protocol)) == split["protocol_sha256"], "Protocol hash changed")
    return split


def common_identity(request, split):
    return {"schema_version": 1, "study_id": split["study_id"], "proposal_sha256": PROPOSAL,
            "split_sha256": request["split"]["sha256"], "protocol_sha256": split["protocol_sha256"]}


def same_identity(value, request, split):
    expected = common_identity(request, split)
    require(all(value.get(key) == item for key, item in expected.items()), "Changed study/split/protocol identity")


def condition_for(split, index):
    if index == 32:
        return split["ood"]
    require(type(index) is int and 0 <= index < 32, "Invalid condition index")
    return split["conditions"][index]


def p0(temperature, volume):
    require(math.isfinite(volume) and volume > 0, "Invalid box volume")
    return (108 - 1) * R * temperature / volume * (1e25 / NA)


def reduce_labels(request):
    split = load_split(request)
    bundle = json_read(imported(request["manifest"]))
    same_identity(bundle, request, split)
    role = request["role"]
    require(role in ROLES and bundle["role"] == role, "One trusted role per reduction")
    runs = bundle["runs"]
    require(isinstance(runs, list) and 0 < len(runs) <= 24, "Reduction needs 1–24 pinned scalar files")
    keys = set()
    summaries = []
    for run in runs:
        condition = condition_for(split, run["condition_index"])
        replicate = run["replicate_index"]
        key = (run["condition_index"], replicate)
        require(type(replicate) is int and replicate in (0,1,2) and key not in keys, "Invalid or duplicate replica")
        keys.add(key)
        require(run["role"] == condition["role"] == role, "Role leakage in scalar reduction")
        seed = 110000 + 1000*run["condition_index"] + replicate
        parameters = {**FIXED_PARAMETERS, "temperature_kelvin": condition["temperature_kelvin"],
                      "density_g_cm3": condition["density_g_cm3"], "seed": seed}
        manifest_text = run["manifest_text"]
        require(isinstance(manifest_text,str) and digest(manifest_text.encode("utf-8")) == sha_string(run["manifest_sha256"]),
                "Original solver manifest byte hash mismatch")
        manifest = json.loads(manifest_text,parse_constant=lambda text: (_ for _ in ()).throw(ValueError(text)))
        if "manifest" in run:
            require(manifest == run["manifest"],"Parsed manifest differs from retained original bytes")
        require(run["seed"] == seed and run["parameters"] == manifest["parameters"] == parameters, "Solver request identity changed")
        protocol = split["protocol"]
        require(manifest["engine"] == "openmm_argon" and manifest["engine_version"] == protocol["engine_version"]
                and manifest["worker_sha256"] == protocol["worker_sha256"] and manifest["platform"] == "CPU"
                and manifest["platform_properties"] == protocol["platform_properties"], "Solver environment changed")
        model = manifest["model"]
        volume = 108 * MASS / NA / condition["density_g_cm3"] * 1e21
        box = finite(model["box_nm"], (3,))
        require((box > 0).all() and abs(float(np.prod(box)) - volume) <= volume*1e-12, "Density/volume mismatch")
        cutoff = min(2.5*.3405, .49*volume**(1/3))
        require(model["mass_dalton"] == MASS and model["sigma_nm"] == .3405 and model["epsilon_kj_mol"] == .997
                and model["dispersion_correction"] is False and model["charges"] == 0
                and model["boundary"] == "cubic periodic" and abs(model["cutoff_nm"]-cutoff) <= 1e-12
                and abs(model["switch_nm"]-.8*cutoff) <= 1e-12, "Potential/boundary mismatch")
        scalar_spec = {"path": run["measurements_path"], "sha256": run["measurements_sha256"]}
        scalar = json_read(imported(scalar_spec))
        for units in (scalar["units"], manifest["units"]):
            require(all(units.get(key) == value for key,value in
                        {"pressure":"bar", "temperature":"K", "time":"ps", "position":"nm", "density":"g/cm^3"}.items()), "Wrong scalar units")
        rows = scalar["series"]
        require(len(rows) == 201 and all(type(row["step"]) is int and row["step"] == index*100
                                       for index,row in enumerate(rows)), "Missing or reordered scalar samples")
        for row in rows:
            require(all(type(value) in (int,float) and math.isfinite(value) for value in row.values()), "Nonfinite scalar sample")
            require(abs(row["time_ps"]-row["step"]*.001) <= 1e-8, "Incorrect scalar sample time")
        pressure = finite([row["pressure_bar"] for row in rows if 10000 < row["step"] <= 20000], (100,))
        blocks = pressure.reshape(5,20).mean(axis=1)
        correlation = (float(np.corrcoef(blocks[:-1],blocks[1:])[0,1])
                       if np.std(blocks[:-1]) > 0 and np.std(blocks[1:]) > 0 else None)
        summaries.append({"condition_index": key[0], "replicate_index": replicate, "seed": seed, "role": role,
                          "temperature_kelvin": condition["temperature_kelvin"], "density_g_cm3": condition["density_g_cm3"],
                          "volume_nm3": float(np.prod(box)), "p0_bar": p0(condition["temperature_kelvin"],float(np.prod(box))),
                          "pressure_bar": float(pressure.mean()), "block_means_bar": blocks.tolist(),
                          "block_lag1_correlation": correlation, "late_minus_early_bar": float(pressure[50:].mean()-pressure[:50].mean()),
                          "solver_job_id": run["solver_job_id"], "attempt_lineage": run.get("attempt_lineage", []),
                          "measurements_sha256": run["measurements_sha256"], "manifest_sha256": run["manifest_sha256"],
                          "worker_sha256": manifest["worker_sha256"], "sample_steps": [10100,20000], "sample_count": 100})
    summaries.sort(key=lambda row:(row["condition_index"],row["replicate_index"]))
    json_write("shard.json", {**common_identity(request,split), "role": role, "seeds": summaries,
                               "bundle_sha256": request["manifest"]["sha256"],
                               "scope": "Correlated finite-window solver endpoints; no equilibrium or empirical claim"})
    np.savez("shard.npz", condition_index=np.array([row["condition_index"] for row in summaries],dtype=np.int64),
             replicate_index=np.array([row["replicate_index"] for row in summaries],dtype=np.int64),
             pressure_bar=np.array([row["pressure_bar"] for row in summaries]),
             block_means_bar=np.array([row["block_means_bar"] for row in summaries]))
    return {"status":"reduced", "role":role, "seed_count":len(summaries), "artifacts":["shard.json","shard.npz"]}


def condition_labels(request, split, allowed):
    groups = {}
    for spec in request.get("sources", []):
        require(spec.get("role") in allowed, "Withheld labels are forbidden in this phase")
        shard = json_read(imported(spec))
        same_identity(shard,request,split)
        require(shard["role"] == spec["role"], "Source role was relabeled")
        for seed in shard["seeds"]:
            condition = condition_for(split,seed["condition_index"])
            require(seed["role"] == condition["role"] == shard["role"], "Condition crossed its frozen role")
            require(seed["temperature_kelvin"] == condition["temperature_kelvin"] and seed["density_g_cm3"] == condition["density_g_cm3"], "Condition features changed")
            replica = seed["replicate_index"]
            require(type(replica) is int and replica in (0,1,2) and seed["seed"] == 110000+1000*seed["condition_index"]+replica, "Replica changed")
            group = groups.setdefault(seed["condition_index"], {})
            require(replica not in group, "A replicate was used twice")
            require(math.isfinite(seed["pressure_bar"]) and math.isfinite(seed["p0_bar"]) and seed["p0_bar"] > 0, "Nonfinite label")
            group[replica] = seed
    expected = [row for row in split["conditions"]+[split["ood"]] if row["role"] in allowed]
    require(set(groups) == {row["condition_index"] for row in expected}, "Incomplete or extra role conditions")
    labels = []
    for condition in sorted(expected,key=lambda row:row["condition_index"]):
        seeds = groups[condition["condition_index"]]
        require(set(seeds) == {0,1,2}, "A condition requires all three independent seeds")
        means = finite([seeds[index]["pressure_bar"] for index in range(3)],(3,))
        ideal = p0(condition["temperature_kelvin"],108*MASS/NA/condition["density_g_cm3"]*1e21)
        require(all(abs(seeds[index]["p0_bar"]-ideal) <= ideal*1e-12 for index in range(3)), "P0 convention changed")
        mean, sd = float(means.mean()), float(means.std(ddof=1))
        se = sd/math.sqrt(3)
        labels.append({**condition, "pressure_bar":mean,"Z":mean/ideal,"p0_bar":ideal,
                       "seed_sd_bar":sd,"seed_se_bar":se,"seed_t95_bar":[mean-4.302653*se,mean+4.302653*se],
                       "seeds":[seeds[index] for index in range(3)]})
    return labels


def save_model(model, labels, request, split):
    np.savez("model.npz", fitting_x=model["points"],weights=model["weights"],
             target_center=np.array([model["center"]]),target_scale=np.array([model["scale"]]))
    model_hash = digest(Path("model.npz").read_bytes())
    card = {**common_identity(request,split),"protocol":split["protocol"],"configuration":model["configuration"],
            "fitting_condition_indices":[row["condition_index"] for row in labels],
            "model_sha256":model_hash,"source_sha256":digest(Path(__file__).read_bytes()),
            "fit_data_sha256":digest(Path("fit-data.npz").read_bytes()),
            "arrays":{"fitting_x":list(model["points"].shape),"weights":list(model["weights"].shape),
                      "target_center":[1],"target_scale":[1]},"state":"frozen_before_calibration",
            "scope":"Three-seed 10–20 ps computational pressure endpoint; no empirical or equilibrium validation"}
    json_write("model-card.json",card)
    return card


def fit_phase(request):
    split = load_split(request)
    labels = condition_labels(request,split,{"train","validation"})
    train = [row for row in labels if row["role"] == "train"]
    validation = [row for row in labels if row["role"] == "validation"]
    points = lambda rows: normalized([[row["temperature_kelvin"],row["density_g_cm3"]] for row in rows])
    z = lambda rows: finite([row["Z"] for row in rows])
    scores = []
    winner = None
    for configuration in configurations():
        entry = {"configuration":configuration,"lexical_order":canonical(configuration).decode("ascii"),
                 "coefficient_count":12 if configuration["family"] == "kernel" else (3 if configuration["family"] == "linear" else 6)}
        try:
            model = fit_numeric(points(train),z(train),configuration)
            prediction = predict_numeric(model,points(validation))
            entry.update({"status":"fitted","validation_rmse_Z":float(np.sqrt(np.mean((prediction-z(validation))**2))),
                          "validation_predictions_Z":prediction.tolist()})
            if (winner is None or entry["validation_rmse_Z"] < winner["validation_rmse_Z"]-1e-12 or
                (abs(entry["validation_rmse_Z"]-winner["validation_rmse_Z"]) <= 1e-12 and
                 (entry["coefficient_count"],entry["lexical_order"]) < (winner["coefficient_count"],winner["lexical_order"]))):
                winner = entry
        except (ValueError,np.linalg.LinAlgError,FloatingPointError) as error:
            entry.update({"status":"failed","error":str(error)})
        scores.append(entry)
    json_write("selection.json",{**common_identity(request,split),"candidates":scores,"selected":winner})
    require(winner is not None,"Every preregistered candidate failed; no replacement model was invented")
    model = fit_numeric(points(labels),z(labels),winner["configuration"])
    np.savez("fit-data.npz",condition_index=np.array([row["condition_index"] for row in labels],dtype=np.int64),
             fitting_x=points(labels),Z=z(labels),p0_bar=np.array([row["p0_bar"] for row in labels]))
    card = save_model(model,labels,request,split)
    json_write("fit-labels.json",{**common_identity(request,split),"conditions":labels})
    return {"status":"model_frozen","model_sha256":card["model_sha256"],"selected_configuration":winner["configuration"],
            "candidate_count":len(scores),"artifacts":["selection.json","model.npz","model-card.json","fit-data.npz","fit-labels.json"]}


def load_model(request,split):
    card = json_read(imported(request["model_card"]))
    same_identity(card,request,split)
    require(card["protocol"] == split["protocol"],"Model protocol mismatch")
    require(card["source_sha256"] == digest(Path(__file__).read_bytes()),"Frozen ML source identity changed")
    require(request["model"]["sha256"] == card["model_sha256"],"Model file hash mismatch")
    expected = sorted(row["condition_index"] for row in split["conditions"] if row["role"] in ("train","validation"))
    require(card["fitting_condition_indices"] == expected,"Model used incorrect fitting conditions")
    require(card["configuration"] in configurations(),"Model configuration was not preregistered")
    with np.load(imported(request["model"]),allow_pickle=False) as arrays:
        require(set(arrays.files) == {"fitting_x","weights","target_center","target_scale"},"Unexpected model arrays")
        points = finite(arrays["fitting_x"],(15,2))
        expected_points = normalized([[split["conditions"][index]["temperature_kelvin"],split["conditions"][index]["density_g_cm3"]] for index in expected])
        require(np.array_equal(points,expected_points),"Model fitting coordinates changed")
        size = 15 if card["configuration"]["family"] == "kernel" else (3 if card["configuration"]["family"] == "linear" else 6)
        weights = finite(arrays["weights"],(size,))
        center = float(finite(arrays["target_center"],(1,))[0]); scale = float(finite(arrays["target_scale"],(1,))[0])
        require(scale >= 1e-6,"Invalid frozen target scale")
    return {"configuration":card["configuration"],"points":points,"weights":weights,"center":center,"scale":scale},card


def calibrate_phase(request):
    split = load_split(request)
    model,card = load_model(request,split)
    labels = condition_labels(request,split,{"calibration"})
    prediction = predict_numeric(model,normalized([[row["temperature_kelvin"],row["density_g_cm3"]] for row in labels]))
    residuals = np.abs(prediction-np.array([row["Z"] for row in labels]))
    q = calibration_quantile(residuals)
    json_write("calibration.json",{**common_identity(request,split),"model_sha256":card["model_sha256"],
                                   "model_card_sha256":request["model_card"]["sha256"],"q_Z":q,"rank":9,"sample_count":9,
                                   "condition_indices":[row["condition_index"] for row in labels],
                                   "predictions_Z":prediction.tolist(),"absolute_residuals_Z":residuals.tolist(),
                                   "limitations":"Designed grid, nine calibration conditions; no population or equilibrium coverage guarantee"})
    json_write("calibration-labels.json",{**common_identity(request,split),"conditions":labels})
    return {"status":"calibrated","q_Z":q,"model_sha256":card["model_sha256"],
            "artifacts":["calibration.json","calibration-labels.json"]}


def load_calibration(request,split,card):
    calibration = json_read(imported(request["calibration"]))
    same_identity(calibration,request,split)
    require(calibration["model_sha256"] == card["model_sha256"] and
            calibration["model_card_sha256"] == request["model_card"]["sha256"],"Calibration model identity changed")
    expected = sorted(row["condition_index"] for row in split["conditions"] if row["role"] == "calibration")
    require(calibration["condition_indices"] == expected and calibration["rank"] == 9 and calibration["sample_count"] == 9,
            "Calibration membership or rank changed")
    require(calibration["q_Z"] == calibration_quantile(calibration["absolute_residuals_Z"]),"Calibration width changed")
    return calibration


def validate_query(query):
    require(isinstance(query,dict),"A numerical query is required")
    features = query.get("features")
    require(isinstance(features,dict) and set(features) == {"temperature_kelvin","density_g_cm3"},"Exactly temperature and density are required")
    require(query.get("units") == {"temperature_kelvin":"K","density_g_cm3":"g/cm^3"},"Wrong or missing feature units")
    for key,low,high in [("temperature_kelvin",20.,500.),("density_g_cm3",.02,2.)]:
        require(type(features[key]) in (int,float) and math.isfinite(features[key]) and low <= features[key] <= high,
                f"{key} is outside supported solver limits")
    protocol = query.get("protocol")
    if isinstance(protocol,dict) and isinstance(protocol.get("fixed_parameters"),dict):
        parameters = protocol["fixed_parameters"]
        integer_bounds = {"atom_count":(32,512),"steps":(1,5000000),"sample_interval":(1,1000),"chunk_frames":(1,100),"cpu_threads":(1,8)}
        real_bounds = {"timestep_fs":(.1,5.),"friction_per_ps":(.01,100.)}
        for key,(low,high) in integer_bounds.items():
            require(type(parameters.get(key)) is int and low <= parameters[key] <= high,f"Invalid solver {key}")
        for key,(low,high) in real_bounds.items():
            require(type(parameters.get(key)) in (int,float) and math.isfinite(parameters[key]) and low <= parameters[key] <= high,f"Invalid solver {key}")
        require(parameters.get("thermostat") in ("langevin","nve") and parameters.get("platform") in ("CPU","Reference","CUDA","OpenCL"),"Unsupported solver configuration")
        require(math.ceil(parameters["steps"]/parameters["sample_interval"])+1 <= 50001
                and parameters["steps"]*parameters["atom_count"] <= 50000000
                and parameters["sample_interval"]*parameters["timestep_fs"] <= 1000,"Solver resource/sampling limit exceeded")
    return [features["temperature_kelvin"],features["density_g_cm3"]]


def inference_decision(query,model,card,calibration,available):
    try:
        point = validate_query(query)
    except (ValueError,TypeError,KeyError) as error:
        return {"status":"invalid_input","reason":str(error),"schedule_solver":False}
    def fallback(reason):
        return {"status":"requires_solver","reason":reason,"model_sha256":card["model_sha256"],
                "schedule_solver":query.get("intent") == "scientific",
                "solver_launched":False,"solver_lineage":"must be supplied by the trusted coordinator after real execution"}
    if query.get("protocol") != card["protocol"]:
        return fallback("protocol_mismatch")
    if not 220 <= point[0] <= 430 or not .25 <= point[1] <= .85:
        return fallback("outside_surrogate_rectangle")
    coordinates = normalized([point])
    distance = float(np.sqrt(np.min(np.sum((model["points"]-coordinates)**2,axis=1))))
    if distance > .40:
        return fallback("insufficient_fitted_support")
    if calibration["q_Z"] > .15:
        return fallback("calibrated_interval_too_wide")
    if not available:
        return fallback("candidate_rejected_or_evaluation_incomplete")
    prediction = float(predict_numeric(model,coordinates)[0])
    ideal = p0(point[0],108*MASS/NA/point[1]*1e21)
    q = calibration["q_Z"]
    return {"status":"prediction","prediction_Z":prediction,"prediction_bar":prediction*ideal,
            "interval_Z":[prediction-q,prediction+q],"interval_bar":[(prediction-q)*ideal,(prediction+q)*ideal],
            "q_Z":q,"support_distance":distance,"model_sha256":card["model_sha256"],"schedule_solver":False,
            "scope":"Three-seed 10–20 ps computational endpoint; interval has no established population or equilibrium coverage guarantee"}


def infer_phase(request):
    query = request.get("query")
    try:
        validate_query(query)
    except (ValueError,TypeError,KeyError) as error:
        decision = {"status":"invalid_input","reason":str(error),"schedule_solver":False}
    else:
        try:
            split = load_split(request); model,card = load_model(request,split)
            calibration = load_calibration(request,split,card)
            evaluation = json_read(imported(request["evaluation"]))
            same_identity(evaluation,request,split)
            require(evaluation["model_sha256"] == card["model_sha256"] and
                    evaluation["calibration_sha256"] == request["calibration"]["sha256"],"Evaluation model identity changed")
            decision = inference_decision(query,model,card,calibration,evaluation.get("useful_acceleration") is True)
        except (ValueError,KeyError,TypeError,OSError,np.linalg.LinAlgError) as error:
            decision = {"status":"requires_solver","reason":"unknown_or_corrupt_model", "detail":str(error),
                        "schedule_solver":query.get("intent") == "scientific","solver_launched":False}
    json_write("inference.json",decision)
    return {**decision,"artifacts":["inference.json"]}


def error_metrics(prediction,labels):
    error = finite(prediction)-np.array([row["Z"] for row in labels])
    pressure_error = error*np.array([row["p0_bar"] for row in labels])
    return {"rmse_Z":float(np.sqrt(np.mean(error**2))),"maximum_absolute_error_Z":float(np.max(np.abs(error))),
            "rmse_bar":float(np.sqrt(np.mean(pressure_error**2))),"maximum_absolute_error_bar":float(np.max(np.abs(pressure_error)))}


def measured_costs(request,benchmark):
    if "costs" not in request:
        return {"status":"incomplete","reason":"All 99 solver and setup costs have not been supplied","speed_pass":False,"workload_pass":False}
    costs = json_read(imported(request["costs"]))
    runs = costs.get("solver_runs",[])
    require(isinstance(runs,list),"Invalid solver timing receipt")
    expected = {f"condition-{index:02d}-replicate-{replica}" for index in range(33) for replica in range(3)}
    ids = [row["case_id"] for row in runs]
    if set(ids) != expected or len(ids) != 99 or any(row.get("status") != "completed" for row in runs):
        return {"status":"incomplete","reason":"Missing or failed study/fallback solver costs","speed_pass":False,"workload_pass":False}
    walls = finite([row["wall_seconds"] for row in runs],(99,))
    require((walls > 0).all() and all(isinstance(row.get("solver_job_id"),str) and row["solver_job_id"] for row in runs),"Solver costs require real positive timing and job lineage")
    overhead = {}
    for key in ["environment_startup_seconds","model_selection_seconds","calibration_evaluation_seconds","cold_model_load_seconds"]:
        value = costs.get(key)
        require(type(value) in (int,float) and math.isfinite(value) and value >= 0,f"Missing measured cost {key}")
        overhead[key] = value
    attempt_extra = costs.get("failed_attempt_seconds")
    require(type(attempt_extra) in (int,float) and math.isfinite(attempt_extra) and attempt_extra >= 0,"Invalid failed-attempt cost")
    single = benchmark.get("warm_single_query_seconds")
    mean_run = float(np.mean(walls)); endpoint = mean_run*3
    setup = float(walls.sum())+sum(overhead.values())+attempt_extra
    if single is None:
        return {"status":"complete","reason":"No admitted supported query; the frozen speed and reuse-workload hypotheses are rejected",
                "study_solver_jobs":99,"solver_wall_seconds":walls.tolist(),"solver_median_seconds":float(np.median(walls)),
                "solver_mean_seconds":mean_run,"three_seed_endpoint_mean_seconds":endpoint,"setup_seconds":setup,
                "setup_components":overhead,"failed_attempt_seconds":attempt_extra,
                "warm_single_query_seconds":None,"warm_vs_single_solver_speedup":None,"speed_pass":False,"workload_pass":False,
                "declared_query_count":1000,"fallback_fraction":.20,"direct_workload_seconds":1000*endpoint,
                "accelerated_workload_seconds":None,"break_even_queries":None,
                "assumption":"The declared 20% fallback scenario is unavailable because no model query is admissible; no speedup is claimed"}
    per_query = single+.20*endpoint
    saved = endpoint-per_query
    accelerated = setup+1000*per_query
    baseline = 1000*endpoint
    return {"status":"complete","study_solver_jobs":99,"solver_wall_seconds":walls.tolist(),"solver_median_seconds":float(np.median(walls)),
            "solver_mean_seconds":mean_run,"three_seed_endpoint_mean_seconds":endpoint,"setup_seconds":setup,
            "setup_components":overhead,"failed_attempt_seconds":attempt_extra,"warm_single_query_seconds":single,
            "warm_vs_single_solver_speedup":mean_run/single,"speed_pass":single*10 <= mean_run,
            "declared_query_count":1000,"fallback_fraction":.20,"direct_workload_seconds":baseline,
            "accelerated_workload_seconds":accelerated,"workload_pass":accelerated < baseline,
            "break_even_queries":math.ceil(setup/saved) if saved > 0 else None,
            "assumption":"1,000-query reuse scenario with 20% three-seed direct-solver fallback; not a user-demand forecast"}


def evaluate_phase(request):
    cold_started = time.perf_counter()
    split = load_split(request); model,card = load_model(request,split)
    calibration = load_calibration(request,split,card)
    cold_load = time.perf_counter()-cold_started
    labels = condition_labels(request,split,{"test","regime_test"})
    points = normalized([[row["temperature_kelvin"],row["density_g_cm3"]] for row in labels])
    prediction = predict_numeric(model,points)
    require(request["fit_data"]["sha256"] == card["fit_data_sha256"],"Frozen baseline data hash changed")
    with np.load(imported(request["fit_data"]),allow_pickle=False) as fit:
        require(set(fit.files) == {"condition_index","fitting_x","Z","p0_bar"},"Unexpected baseline arrays")
        indices = np.asarray(fit["condition_index"])
        require(indices.dtype.kind in "iu" and indices.tolist() == card["fitting_condition_indices"],"Baselines use incorrect conditions")
        fitting_x = finite(fit["fitting_x"],(15,2)); fitting_z = finite(fit["Z"],(15,))
        require(np.array_equal(fitting_x,model["points"]),"Baseline fitting coordinates changed")
    distances = np.sqrt(np.sum((points[:,None,:]-fitting_x[None,:,:])**2,axis=2))
    nearest = np.argmin(distances,axis=1)  # Sorted condition indices break exact ties.
    baselines = {"p0":np.ones(8),"training_mean":np.full(8,float(fitting_z.mean())),"nearest_condition":fitting_z[nearest]}
    baseline_metrics = {name:error_metrics(value,labels) for name,value in baselines.items()}
    combined = error_metrics(prediction,labels)
    groups = {}
    coverage = np.abs(prediction-np.array([row["Z"] for row in labels])) <= calibration["q_Z"]
    for role in ("test","regime_test"):
        selected = np.array([row["role"] == role for row in labels])
        groups[role] = {**error_metrics(prediction[selected],[row for row in labels if row["role"] == role]),
                        "covered":int(np.sum(coverage[selected])),"count":int(np.sum(selected))}
    best_baseline = min(baseline_metrics,key=lambda name:(baseline_metrics[name]["rmse_Z"],name))
    baseline_rmse = baseline_metrics[best_baseline]["rmse_Z"]
    noise = float(np.sqrt(np.mean([(row["seed_se_bar"]/row["p0_bar"])**2 for row in labels])))
    support = distances.min(axis=1) <= .40
    gates = {
        "accuracy":combined["rmse_Z"] <= .05 and all(group["rmse_Z"] <= .07 for group in groups.values()) and combined["maximum_absolute_error_Z"] <= .15,
        "baseline_improvement":combined["rmse_Z"] <= .8*baseline_rmse and baseline_rmse-combined["rmse_Z"] >= 3*noise,
        "coverage_and_support":int(coverage.sum()) >= 7 and all(group["covered"] >= 3 for group in groups.values()) and calibration["q_Z"] <= .15 and int(support.sum()) >= 6}
    cases = []
    for index,row in enumerate(labels):
        cases.append({**row,"prediction_Z":float(prediction[index]),"prediction_bar":float(prediction[index]*row["p0_bar"]),
                      "interval_Z":[float(prediction[index]-calibration["q_Z"]),float(prediction[index]+calibration["q_Z"])],
                      "interval_bar":[float((prediction[index]-calibration["q_Z"])*row["p0_bar"]),float((prediction[index]+calibration["q_Z"])*row["p0_bar"])],
                      "covered":bool(coverage[index]),"support_distance":float(distances[index].min()),"support_admitted":bool(support[index]),
                      "baseline_predictions_Z":{name:float(value[index]) for name,value in baselines.items()}})
    benchmark = {"cold_model_load_seconds":cold_load,"warm_single_query_seconds":None,
                 "scope":"Provisional admitted-query timing only; does not enable scientific substitution"}
    candidates = [row for row in cases if row["support_admitted"] and calibration["q_Z"] <= .15]
    if candidates:
        row = candidates[0]
        query = {"features":{"temperature_kelvin":row["temperature_kelvin"],"density_g_cm3":row["density_g_cm3"]},
                 "units":{"temperature_kelvin":"K","density_g_cm3":"g/cm^3"},"protocol":card["protocol"],"intent":"explanation"}
        require(inference_decision(query,model,card,calibration,True)["status"] == "prediction","Benchmark query is not admitted")
        samples = []
        for _ in range(64):
            started = time.perf_counter(); inference_decision(query,model,card,calibration,True); samples.append(time.perf_counter()-started)
        started = time.perf_counter()
        for _ in range(256):
            inference_decision(query,model,card,calibration,True)
        benchmark.update({"warm_single_query_seconds":float(np.median(samples)),"single_query_samples_seconds":samples,
                          "batch_query_count":256,"batch_wall_seconds":time.perf_counter()-started,"condition_index":row["condition_index"]})
    costs = measured_costs(request,benchmark)
    gates["warm_speed"] = costs["speed_pass"]; gates["total_workload_cost"] = costs["workload_pass"]
    report = {**common_identity(request,split),"model_sha256":card["model_sha256"],"calibration_sha256":request["calibration"]["sha256"],
              "held_out_evaluation_complete":True,"cost_evaluation_complete":costs["status"] == "complete",
              "useful_acceleration":all(gates.values()),"gates":gates,"rejections":[key for key,value in gates.items() if not value],
              "combined":combined,"groups":groups,"baselines":baseline_metrics,"best_baseline":best_baseline,
              "rms_seed_standard_error_Z":noise,"required_improvement_Z":3*noise,"q_Z":calibration["q_Z"],
              "coverage_count":int(coverage.sum()),"held_out_count":8,"support_count":int(support.sum()),
              "cases":cases,"benchmark":benchmark,"costs":costs,
              "integration_correctness":"Requires coordinator role-access, actual OOD lineage, recovery and independent-check receipts",
              "scope":"Finite computational protocol; these eight conditions do not prove population coverage or empirical validity"}
    json_write("evaluation.json",report)
    return {"status":"evaluated","useful_acceleration":report["useful_acceleration"],"rejections":report["rejections"],
            "cost_evaluation_complete":report["cost_evaluation_complete"],"artifacts":["evaluation.json"]}


def finalize_costs(request):
    require(not request.get("sources"),"Cost finalization cannot import scientific label roles")
    split = load_split(request); _,card = load_model(request,split)
    calibration = load_calibration(request,split,card)
    previous = json_read(imported(request["evaluation"]))
    same_identity(previous,request,split)
    require(previous["model_sha256"] == card["model_sha256"] and
            previous["calibration_sha256"] == request["calibration"]["sha256"] and
            previous["q_Z"] == calibration["q_Z"] and previous["held_out_evaluation_complete"] is True,
            "Cost finalization requires the completed frozen held-out evaluation")
    costs = measured_costs(request,previous["benchmark"])
    report = dict(previous)
    gates = dict(previous["gates"])
    gates["warm_speed"] = costs["speed_pass"]; gates["total_workload_cost"] = costs["workload_pass"]
    report.update({"costs":costs,"cost_evaluation_complete":costs["status"] == "complete", "gates":gates,
                   "useful_acceleration":all(gates.values()),"rejections":[key for key,value in gates.items() if not value],
                   "cost_finalization":{"prior_evaluation_sha256":request["evaluation"]["sha256"],
                                        "costs_sha256":request.get("costs",{}).get("sha256"),
                                        "model_sha256":card["model_sha256"],"refit":False,"reselected":False,
                                        "labels_reopened":False,"predictions_recomputed":False}})
    json_write("evaluation.json",report)
    return {"status":"costs_finalized","useful_acceleration":report["useful_acceleration"],
            "rejections":report["rejections"],"cost_evaluation_complete":report["cost_evaluation_complete"],
            "artifacts":["evaluation.json"]}


def matrix_fixture(request):
    """Explicit synthetic algebra fixture; cannot write a scientific model card."""
    model = fit_numeric(request["points"], request["labels"], request["configuration"])
    prediction = predict_numeric(model, request["queries"])
    json_write("fixture.json", {"scope": "synthetic_matrix_fixture", "predictions": prediction.tolist(),
                                "weights": model["weights"].tolist(), "center": model["center"],
                                "scale": model["scale"]})
    return {"status": "fixture_only", "scientific_model": False, "artifacts": ["fixture.json"]}


def calibration_fixture(request):
    json_write("fixture.json", {"scope": "synthetic_calibration_fixture",
                                "q": calibration_quantile(request["absolute_residuals"])})
    return {"status": "fixture_only", "scientific_model": False, "artifacts": ["fixture.json"]}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--workdir", type=Path)
    args = parser.parse_args()
    if args.workdir is not None:
        import os
        os.chdir(args.workdir)
    request = json_read("input.json")
    started = time.perf_counter()
    phases = {"freeze": freeze, "reduce": reduce_labels,"fit": fit_phase,"calibrate": calibrate_phase,
              "evaluate": evaluate_phase,"infer": infer_phase,"finalize_costs":finalize_costs,
              "matrix_fixture": matrix_fixture, "calibration_fixture": calibration_fixture}
    require(request.get("phase") in phases, "Unknown or not yet implemented ML phase")
    result = phases[request["phase"]](request)
    result.update({"phase": request["phase"], "wall_seconds": time.perf_counter() - started,
                   "source_sha256": digest(Path(__file__).read_bytes()),
                   "environment": {"python": platform.python_version(), "numpy": np.__version__},
                   "scientific_validity": "requires independent evaluation"})
    json_write("result.json", result)


if __name__ == "__main__":
    main()
