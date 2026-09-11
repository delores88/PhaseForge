"""Independent checks for the LPAC NumPy surrogate source; never import it.

This checker uses scalar Gaussian elimination for numerical expectations. Fixed
matrix fixtures and membership freeze are source checks, not scientific labels.
"""
from __future__ import annotations

import argparse
import hashlib
import itertools
import json
import math
from pathlib import Path
import subprocess
import sys
import time
import shutil

import numpy as np


PROPOSAL = "b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737"
TOLERANCE = 1e-10


def require(ok, message):
    if not ok:
        raise AssertionError(message)


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def read(path):
    return json.loads(Path(path).read_text(), parse_constant=lambda v: (_ for _ in ()).throw(ValueError(v)))


def write(path, value):
    Path(path).write_bytes(encode(value))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def solve(matrix, rhs):
    rows = [list(map(float, row)) + [float(value)] for row, value in zip(matrix, rhs)]
    size = len(rows)
    for column in range(size):
        pivot = max(range(column, size), key=lambda row: abs(rows[row][column]))
        require(abs(rows[pivot][column]) > 1e-15, "Independent system is singular")
        rows[column], rows[pivot] = rows[pivot], rows[column]
        factor = rows[column][column]
        rows[column] = [v / factor for v in rows[column]]
        for row in range(size):
            if row != column:
                factor = rows[row][column]
                rows[row] = [left - factor * right for left, right in zip(rows[row], rows[column])]
    return [row[-1] for row in rows]


def polynomial(point, family):
    x, y = point
    return [1., x, y] + ([x*x, x*y, y*y] if family == "quadratic" else [])


def reference(points, labels, queries, configuration):
    center = sum(z - 1 for z in labels) / len(labels)
    scale = max(math.sqrt(sum((z - 1 - center)**2 for z in labels) / len(labels)), 1e-6)
    rhs = [(z - 1 - center) / scale for z in labels]
    reg = configuration["regularizer"]
    if configuration["family"] == "kernel":
        def row(point):
            return [math.exp(-sum(((left-right)/ell)**2 for left, right, ell in
                                 zip(point, train, configuration["lengths"])) / 2) for train in points]
        matrix = [row(point) for point in points]
        for index in range(len(matrix)):
            matrix[index][index] += reg
        weights = solve(matrix, rhs)
    else:
        def row(point):
            return polynomial(point, configuration["family"])
        design = [row(point) for point in points]
        count = len(design[0])
        matrix = [[sum(point[i] * point[j] for point in design) / len(points)
                   + (reg if i == j and i > 0 else 0) for j in range(count)] for i in range(count)]
        weights = solve(matrix, [sum(point[i]*target for point, target in zip(design, rhs)) / len(points)
                                for i in range(count)])
    return [1 + center + scale * sum(a*b for a, b in zip(row(query), weights)) for query in queries]


class Runner:
    def __init__(self, root, python, worker):
        self.root, self.python, self.worker = root, python, worker
        self.receipts = []

    def call(self, name, request, succeeds=True, imports=None):
        folder = self.root / name
        folder.mkdir(parents=True, exist_ok=False)
        if imports:
            (folder/"imports").mkdir()
            for destination, source in imports.items():
                require("/" not in destination and "\\" not in destination,"Flat fixture imports only")
                shutil.copyfile(source,folder/"imports"/destination)
        write(folder / "input.json", request)
        started = time.perf_counter()
        completed = subprocess.run([str(self.python), str(self.worker), "--workdir", str(folder)],
                                   capture_output=True, timeout=30)
        (folder / "stdout.log").write_bytes(completed.stdout)
        (folder / "stderr.log").write_bytes(completed.stderr)
        receipt = {"name": name, "return_code": completed.returncode, "expected_success": succeeds,
                   "wall_seconds": time.perf_counter()-started, "input_sha256": sha(folder / "input.json")}
        self.receipts.append(receipt)
        require((completed.returncode == 0) == succeeds,
                f"Unexpected fixture outcome {name}: {completed.stderr.decode(errors='replace')[-1800:]}")
        if succeeds:
            result = read(folder / "result.json")
            receipt["result_sha256"] = sha(folder / "result.json")
            return folder, result
        require(not (folder / "result.json").exists(), f"Failed fixture {name} reported success")
        return folder, None


def check_membership(runner, worker_hash):
    folder, result = runner.call("membership-freeze", {
        "phase": "freeze", "study_id": "source-component-membership-freeze", "proposal_sha256": PROPOSAL,
        "solver_worker_sha256": worker_hash, "solver_engine_version": "8.5.2.dev-36a30cb"})
    rows = read(folder / "split.json")["conditions"]
    independent = []
    for index, (temperature, density) in enumerate(itertools.product([220,250,280,310,325,370,400,430], [.25,.45,.65,.85])):
        key = f"phaseforge-ml-argon-v1|T={temperature}|rho={density:.2f}"
        independent.append((index, temperature, density, hashlib.sha256(key.encode("ascii")).hexdigest()))
    sorted_rows = sorted((row for row in independent if row[1] != 325), key=lambda row: (row[3],row[1],row[2]))
    expected = {row[0]: "regime_test" for row in independent if row[1] == 325}
    for role, members in [("train", sorted_rows[:12]), ("validation", sorted_rows[12:15]),
                          ("calibration", sorted_rows[15:24]), ("test", sorted_rows[24:])]:
        expected.update({row[0]: role for row in members})
    require(len(rows) == 32 and len({row["condition_index"] for row in rows}) == 32, "Condition uniqueness")
    for row, fixed in zip(rows, independent):
        require(row["condition_index"] == fixed[0] and row["temperature_kelvin"] == fixed[1]
                and row["density_g_cm3"] == fixed[2] and row["split_sha256"] == fixed[3]
                and row["role"] == expected[fixed[0]], "Frozen split mismatch")
    runs = read(folder / "runs.json")["runs"]
    require(len(runs) == 99 and len({row["seed"] for row in runs}) == 99, "99 unique requests")
    for row in runs:
        index, replica = row["condition_index"], row["replicate_index"]
        require(row["seed"] == 110000 + 1000*index + replica and replica in (0,1,2), "Fixed seed recipe")
        require(row["role"] == ("ood" if index == 32 else expected[index]), "Replica role leakage")
        require(row["parameters"]["steps"] == 20000 and row["parameters"]["sample_interval"] == 100
                and row["parameters"]["atom_count"] == 108 and row["parameters"]["friction_per_ps"] == 5,
                "Changed scientific fidelity")
    require(runs[0]["role"] == "train", "Pilot must be the first scheduled training replicate")
    require(result["status"] == "frozen_before_labels", "No fitting claim at freeze")
    return {role: sorted(index for index, assigned in expected.items() if assigned == role) for role in set(expected.values())}


def matrix_checks(runner):
    points = [[0,0], [.3,.1], [.1,.8], [.9,.2], [.6,.7], [1,1]]
    labels = [.75, 1.03, 1.11, .9, 1.5, 1.78]
    queries = [[.15,.23], [.41,.63], [.91,.95]]
    configs = [{"family": family, "regularizer": reg} for family in ("linear", "quadratic")
               for reg in [1e-6, 1e-4, .01, 1.]]
    configs += [{"family": "kernel", "lengths": [x,y], "regularizer": reg}
                for x,y,reg in itertools.product([.15,.3,.6,1.2], [.15,.3,.6,1.2], [1e-6,1e-4,.01,.1])]
    maximum = 0.
    for index, config in enumerate(configs):
        folder, result = runner.call(f"matrix-{index:02d}", {"phase": "matrix_fixture", "points": points,
                                    "labels": labels, "queries": queries, "configuration": config})
        predicted = read(folder / "fixture.json")["predictions"]
        expected = reference(points, labels, queries, config)
        error = max(abs(a-b) for a,b in zip(predicted, expected))
        maximum = max(maximum, error)
        require(error <= TOLERANCE, f"Independent solve disagreement: {config}, {error}")
        require(result["scientific_model"] is False, "Fixture mislabeled as scientific model")
    for family in ("linear", "quadratic", "kernel"):
        config = {"family": family, "regularizer": .01, "lengths": [.3,.6]}
        folder, _ = runner.call("constant-"+family, {"phase": "matrix_fixture", "points": points,
                               "labels": [1.7]*6, "queries": queries, "configuration": config})
        require(all(abs(value-1.7) <= TOLERANCE for value in read(folder / "fixture.json")["predictions"]), "Constant labels")
    request = {"phase": "matrix_fixture", "points": [[0,0],[0,0]], "labels": [1,1.2],
               "queries": queries, "configuration": {"family": "kernel", "regularizer": .01, "lengths": [.3,.6]}}
    folder, _ = runner.call("duplicated-regularized", request)
    expected = reference(request["points"],request["labels"],queries,request["configuration"])
    require(max(abs(a-b) for a,b in zip(expected,read(folder/"fixture.json")["predictions"])) <= TOLERANCE, "Duplicate coordinates")
    request["configuration"]["regularizer"] = 0
    runner.call("singular-no-jitter", request, False)
    request["configuration"]["regularizer"] = .01
    request["labels"][0] = "NaN"
    runner.call("nonfinite-label", request, False)
    residuals = [.03, .08, .02, .11, .07, .04, .10, .06, .01]
    folder, _ = runner.call("calibration-order", {"phase": "calibration_fixture", "absolute_residuals": residuals})
    require(read(folder / "fixture.json")["q"] == sorted(residuals)[math.ceil(10*.9)-1], "Calibration rank")
    runner.call("calibration-missing", {"phase": "calibration_fixture", "absolute_residuals": residuals[:-1]}, False)
    return {"configurations": len(configs), "maximum_prediction_disagreement_Z": maximum, "tolerance_Z": TOLERANCE}


def stage_checks(runner):
    """Mathematical labels only; never a claimed OpenMM measurement or study."""
    freeze = runner.root/"membership-freeze"
    split = read(freeze/"split.json")
    base = {"study_id":split["study_id"],"proposal_sha256":PROPOSAL,
            "split":{"path":"split.json","sha256":sha(freeze/"split.json")},"fixture_only":True}
    identity = {"schema_version":1,"study_id":split["study_id"],"proposal_sha256":PROPOSAL,
                "split_sha256":base["split"]["sha256"],"protocol_sha256":split["protocol_sha256"]}
    inputs = runner.root/"synthetic-stage-inputs"; inputs.mkdir()
    role_paths = {}
    labels = {}
    for role in ["train","validation","calibration","test","regime_test"]:
        seeds = []
        for condition in split["conditions"]:
            if condition["role"] != role:
                continue
            index = condition["condition_index"]
            temperature,density = condition["temperature_kelvin"],condition["density_g_cm3"]
            x,y = (temperature-220)/210,(density-.25)/.6
            z = 1.2+.4*x-.3*y+.05*x*y
            volume = 108*39.948/6.02214076e23/density*1e21
            ideal = 107*.00831446261815324*temperature/volume*(1e25/6.02214076e23)
            labels[index] = {"xy":[x,y],"Z":z,"p0_bar":ideal,"role":role}
            for replica in range(3):
                seeds.append({"condition_index":index,"replicate_index":replica,"role":role,
                              "seed":110000+1000*index+replica,"temperature_kelvin":temperature,
                              "density_g_cm3":density,"pressure_bar":ideal*(z+(replica-1)*.002),
                              "p0_bar":ideal,"fixture_only":True,"solver_job_id":f"synthetic-matrix-{index}-{replica}"})
        path = inputs/(role+".json")
        write(path,{**identity,"role":role,"seeds":seeds,"scope":"synthetic polynomial matrix fixture, no solver data"})
        role_paths[role] = path
    def sources(roles):
        return [{"path":role+".json","sha256":sha(role_paths[role]),"role":role} for role in roles]
    def imports(roles):
        return {"split.json":freeze/"split.json",**{role+".json":role_paths[role] for role in roles}}
    fitted,result = runner.call("stage-fit",{**base,"phase":"fit","sources":sources(["train","validation"])},
                                imports=imports(["train","validation"]))
    require(result["candidate_count"] == 72,"Candidate ledger was truncated")
    selection = read(fitted/"selection.json")
    train_ids = sorted(index for index,row in labels.items() if row["role"] == "train")
    val_ids = sorted(index for index,row in labels.items() if row["role"] == "validation")
    independently_selected = None
    for candidate in selection["candidates"]:
        predicted = reference([labels[index]["xy"] for index in train_ids], [labels[index]["Z"] for index in train_ids],
                              [labels[index]["xy"] for index in val_ids],candidate["configuration"])
        rmse = math.sqrt(sum((pred-labels[index]["Z"])**2 for pred,index in zip(predicted,val_ids))/3)
        require(abs(rmse-candidate["validation_rmse_Z"]) <= TOLERANCE,"Independent validation score")
        row = (rmse,candidate["coefficient_count"],candidate["lexical_order"],candidate["configuration"])
        if independently_selected is None or rmse < independently_selected[0]-1e-12 or (abs(rmse-independently_selected[0]) <= 1e-12 and row[1:3] < independently_selected[1:3]):
            independently_selected = row
    require(selection["selected"]["configuration"] == independently_selected[3],"Selection tie rule changed")
    model_imports = {"model.npz":fitted/"model.npz","model-card.json":fitted/"model-card.json"}
    model_request = {"model":{"path":"model.npz","sha256":sha(fitted/"model.npz")},
                     "model_card":{"path":"model-card.json","sha256":sha(fitted/"model-card.json")}}
    fit_hash = sha(fitted/"model.npz")
    calibrated,_ = runner.call("stage-calibrate",{**base,**model_request,"phase":"calibrate","sources":sources(["calibration"])},
                               imports={**imports(["calibration"]),**model_imports})
    fitted_ids = sorted(train_ids+val_ids)
    cal_ids = sorted(index for index,row in labels.items() if row["role"] == "calibration")
    predict = lambda ids: reference([labels[index]["xy"] for index in fitted_ids], [labels[index]["Z"] for index in fitted_ids],
                                    [labels[index]["xy"] for index in ids],independently_selected[3])
    expected_q = max(abs(pred-labels[index]["Z"]) for pred,index in zip(predict(cal_ids),cal_ids))
    require(abs(read(calibrated/"calibration.json")["q_Z"]-expected_q) <= TOLERANCE,"Independent calibrated interval")
    cal_request = {"calibration":{"path":"calibration.json","sha256":sha(calibrated/"calibration.json")}}
    eval_imports = {**imports(["test","regime_test"]),**model_imports,"calibration.json":calibrated/"calibration.json", "fit-data.npz":fitted/"fit-data.npz"}
    evaluated,summary = runner.call("stage-evaluate",{**base,**model_request,**cal_request,"phase":"evaluate",
                                    "fit_data":{"path":"fit-data.npz","sha256":sha(fitted/"fit-data.npz")},"sources":sources(["test","regime_test"])},imports=eval_imports)
    evaluation = read(evaluated/"evaluation.json")
    require(summary["useful_acceleration"] is False and summary["cost_evaluation_complete"] is False,"Missing scientific timing cannot pass")
    test_ids = sorted(index for index,row in labels.items() if row["role"] in ("test","regime_test"))
    expected_predictions = predict(test_ids)
    for case,prediction in zip(evaluation["cases"],expected_predictions):
        require(abs(case["prediction_Z"]-prediction) <= TOLERANCE,"Retained model prediction")
    rmse = math.sqrt(sum((p-labels[i]["Z"])**2 for i,p in zip(test_ids,expected_predictions))/8)
    require(abs(evaluation["combined"]["rmse_Z"]-rmse) <= TOLERANCE,"Independent held-out RMSE")
    card = read(fitted/"model-card.json")
    query = {"features":{"temperature_kelvin":280,"density_g_cm3":.65},
             "units":{"temperature_kelvin":"K","density_g_cm3":"g/cm^3"},"protocol":card["protocol"],"intent":"scientific"}
    infer_imports = {"split.json":freeze/"split.json",**model_imports,"calibration.json":calibrated/"calibration.json","evaluation.json":evaluated/"evaluation.json"}
    infer_request = {**base,**model_request,**cal_request,"phase":"infer","evaluation":{"path":"evaluation.json","sha256":sha(evaluated/"evaluation.json")}}
    _,decision = runner.call("stage-infer-rejected",{**infer_request,"query":query},imports=infer_imports)
    require(decision["status"] == "requires_solver" and decision["solver_launched"] is False,"Rejected candidate must request actual solver")
    ood = json.loads(json.dumps(query)); ood["features"] = {"temperature_kelvin":180,"density_g_cm3":.55}
    _,decision = runner.call("stage-infer-ood",{**infer_request,"query":ood},imports=infer_imports)
    require(decision["status"] == "requires_solver" and decision["reason"] == "outside_surrogate_rectangle","OOD must not extrapolate")
    ood["intent"] = "explanation"
    _,decision = runner.call("stage-infer-explanation",{**infer_request,"query":ood},imports=infer_imports)
    require(decision["schedule_solver"] is False,"Explanation cannot schedule work")
    bad = json.loads(json.dumps(query)); bad["units"]["temperature_kelvin"] = "C"
    _,decision = runner.call("stage-infer-wrong-units",{**infer_request,"query":bad},imports=infer_imports)
    require(decision["status"] == "invalid_input" and not decision["schedule_solver"],"Wrong units must be rejected")
    bad = json.loads(json.dumps(query)); bad["protocol"]["fixed_parameters"]["atom_count"] = 32
    _,decision = runner.call("stage-infer-changed-protocol",{**infer_request,"query":bad},imports=infer_imports)
    require(decision["reason"] == "protocol_mismatch","Changed N must not silently reuse surrogate")
    bad = json.loads(json.dumps(query)); bad["features"]["temperature_kelvin"] = 501
    _,decision = runner.call("stage-infer-invalid-solver-input",{**infer_request,"query":bad},imports=infer_imports)
    require(decision["status"] == "invalid_input","Out-of-solver bounds must not launch fallback")
    broken = {**infer_request,"model":{"path":"model.npz","sha256":"0"*64},"query":query}
    _,decision = runner.call("stage-infer-corrupt-hash",broken,imports=infer_imports)
    require(decision["status"] == "requires_solver" and decision["reason"] == "unknown_or_corrupt_model","Corrupt model must not produce a prediction")
    runner.call("stage-fit-leakage",{**base,"phase":"fit","sources":sources(["train","validation","calibration"])},False,
                imports=imports(["train","validation","calibration"]))
    missing = read(role_paths["train"]); missing["seeds"].pop(); write(inputs/"missing.json",missing)
    missing_sources = sources(["train","validation"]); missing_sources[0] = {"path":"missing.json","sha256":sha(inputs/"missing.json"),"role":"train"}
    runner.call("stage-fit-missing-replica",{**base,"phase":"fit","sources":missing_sources},False,
                imports={**imports(["validation"]),"missing.json":inputs/"missing.json"})
    require(sha(fitted/"model.npz") == fit_hash,"Calibration/evaluation changed frozen model")
    # Fixed cost arithmetic inputs are explicitly synthetic, not timings of 99
    # real solver jobs. They exercise the positive inference path and formulas.
    costs = {"scope":"synthetic_cost_arithmetic_fixture","solver_runs":[
        {"case_id":f"condition-{index:02d}-replicate-{replica}","solver_job_id":f"synthetic-{index}-{replica}",
         "status":"completed","wall_seconds":10.+.01*index} for index in range(33) for replica in range(3)],
        "environment_startup_seconds":2.,"model_selection_seconds":3.,"calibration_evaluation_seconds":4.,
        "cold_model_load_seconds":.5,"failed_attempt_seconds":1.5}
    write(inputs/"synthetic-costs.json",costs)
    cost_eval,_ = runner.call("stage-finalize-synthetic-costs",{**base,**model_request,**cal_request,"phase":"finalize_costs",
        "evaluation":{"path":"evaluation.json","sha256":sha(evaluated/"evaluation.json")},
        "costs":{"path":"synthetic-costs.json","sha256":sha(inputs/"synthetic-costs.json")}},
        imports={**infer_imports,"synthetic-costs.json":inputs/"synthetic-costs.json"})
    final_eval = read(cost_eval/"evaluation.json")
    for key in ["cases","combined","groups","baselines","benchmark","q_Z","coverage_count","support_count"]:
        require(encode(final_eval[key]) == encode(evaluation[key]),f"Cost finalization changed frozen {key}")
    require(final_eval["cost_finalization"]["prior_evaluation_sha256"] == sha(evaluated/"evaluation.json") and
            not final_eval["cost_finalization"]["refit"] and not (cost_eval/"model.npz").exists(),"Cost-only lineage and no refit")
    runner.call("stage-finalize-label-leakage",{**base,**model_request,**cal_request,"phase":"finalize_costs",
                "sources":sources(["calibration"])},False,imports=imports(["calibration"]))
    timing = final_eval["costs"]
    total = sum(row["wall_seconds"] for row in costs["solver_runs"])
    direct = total/99*3
    setup = total+11.
    expected_cost = setup+1000*(timing["warm_single_query_seconds"]+.2*direct)
    require(abs(timing["setup_seconds"]-setup) <= 1e-10 and abs(timing["accelerated_workload_seconds"]-expected_cost) <= 1e-9,"Independent workload arithmetic")
    require(timing["break_even_queries"] == math.ceil(setup/(direct-timing["warm_single_query_seconds"]-.2*direct)),"Independent break-even count")
    positive_request = {**infer_request,"evaluation":{"path":"evaluation.json","sha256":sha(cost_eval/"evaluation.json")},"query":query}
    positive_imports = {**infer_imports,"evaluation.json":cost_eval/"evaluation.json"}
    _,decision = runner.call("stage-infer-accepted-synthetic",positive_request,imports=positive_imports)
    require(final_eval["useful_acceleration"] is True and decision["status"] == "prediction","Positive synthetic decision fixture")
    query_point = [(280-220)/210,(.65-.25)/.6]
    expected_prediction = reference([labels[index]["xy"] for index in fitted_ids],[labels[index]["Z"] for index in fitted_ids],[query_point],independently_selected[3])[0]
    require(abs(decision["prediction_Z"]-expected_prediction) <= TOLERANCE,"Actual retained-array inference")
    wide = read(role_paths["calibration"])
    for seed in wide["seeds"]:
        seed["pressure_bar"] += .5*seed["p0_bar"]
    write(inputs/"wide-calibration.json",wide)
    wide_calibrated,_ = runner.call("stage-wide-calibration",{**base,**model_request,"phase":"calibrate",
        "sources":[{"path":"wide-calibration.json","sha256":sha(inputs/"wide-calibration.json"),"role":"calibration"}]},
        imports={"split.json":freeze/"split.json",**model_imports,"wide-calibration.json":inputs/"wide-calibration.json"})
    wide_request = {**cal_request,"calibration":{"path":"calibration.json","sha256":sha(wide_calibrated/"calibration.json")}}
    wide_eval,_ = runner.call("stage-wide-evaluation",{**base,**model_request,**wide_request,"phase":"evaluate",
        "fit_data":{"path":"fit-data.npz","sha256":sha(fitted/"fit-data.npz")},"sources":sources(["test","regime_test"])},
        imports={**eval_imports,"calibration.json":wide_calibrated/"calibration.json"})
    wide_final,result = runner.call("stage-wide-completed-rejection",{**base,**model_request,**wide_request,"phase":"finalize_costs",
        "evaluation":{"path":"evaluation.json","sha256":sha(wide_eval/"evaluation.json")},
        "costs":{"path":"synthetic-costs.json","sha256":sha(inputs/"synthetic-costs.json")}},
        imports={**infer_imports,"evaluation.json":wide_eval/"evaluation.json","calibration.json":wide_calibrated/"calibration.json",
                 "synthetic-costs.json":inputs/"synthetic-costs.json"})
    require(result["cost_evaluation_complete"] is True and result["useful_acceleration"] is False,"Rejected candidate is a completed evaluation")
    require(read(wide_final/"evaluation.json")["costs"]["warm_vs_single_solver_speedup"] is None,"No invented speed when all queries fall back")
    with np.load(fitted/"model.npz",allow_pickle=False) as arrays:
        require(all(arrays[name].dtype.kind in "fiu" for name in arrays.files),"Pickle/object model arrays")
    return {"scope":"synthetic polynomial matrix stages only; no scientific labels",
            "selected_configuration":independently_selected[3],"model_unchanged":True,"held_out_rmse_Z":rmse,
            "actual_scientific_solver_runs":0,"useful_acceleration_claim":False}


def reduction_checks(runner):
    split_path = runner.root/"membership-freeze/split.json"
    split = read(split_path); condition = next(row for row in split["conditions"] if row["role"] == "train")
    root = runner.root/"synthetic-reduction-inputs"; root.mkdir()
    identity = {"schema_version":1,"study_id":split["study_id"],"proposal_sha256":PROPOSAL,
                "protocol_sha256":split["protocol_sha256"],"split_sha256":sha(split_path)}
    volume = 108*39.948/6.02214076e23/condition["density_g_cm3"]*1e21
    length = volume**(1/3); cutoff = min(2.5*.3405,.49*length)
    seed = 110000+1000*condition["condition_index"]
    parameters = {**split["protocol"]["fixed_parameters"],"temperature_kelvin":condition["temperature_kelvin"],
                  "density_g_cm3":condition["density_g_cm3"],"seed":seed}
    units = {"pressure":"bar","temperature":"K","time":"ps","position":"nm","density":"g/cm^3"}
    manifest = {"scope":"synthetic scalar reduction fixture λ; not solver output","engine":"openmm_argon",
                "engine_version":split["protocol"]["engine_version"],"worker_sha256":split["protocol"]["worker_sha256"],
                "platform":"CPU","platform_properties":split["protocol"]["platform_properties"],"units":units,
                "parameters":parameters,"model":{"box_nm":[length]*3,"mass_dalton":39.948,"sigma_nm":.3405,
                "epsilon_kj_mol":.997,"dispersion_correction":False,"charges":0,"boundary":"cubic periodic",
                "cutoff_nm":cutoff,"switch_nm":.8*cutoff}}
    raw_manifest = json.dumps(manifest,indent=3,ensure_ascii=False)
    pressures = [1+.01*index+(index%7)*.001 for index in range(201)]
    scalar = {"units":units,"series":[{"step":index*100,"time_ps":index*.1,"pressure_bar":pressure}
                                       for index,pressure in enumerate(pressures)]}
    write(root/"measurements.json",scalar)
    run = {"condition_index":condition["condition_index"],"replicate_index":0,"seed":seed,"role":"train",
           "parameters":parameters,"manifest_text":raw_manifest,"manifest":manifest,
           "manifest_sha256":hashlib.sha256(raw_manifest.encode("utf-8")).hexdigest(),
           "measurements_path":"measurements.json","measurements_sha256":sha(root/"measurements.json"),
           "solver_job_id":"synthetic-scalar-fixture","attempt_lineage":[]}
    bundle = {**identity,"role":"train","runs":[run]}; write(root/"bundle.json",bundle)
    base = {"phase":"reduce","study_id":split["study_id"],"proposal_sha256":PROPOSAL,"role":"train",
            "split":{"path":"split.json","sha256":sha(split_path)},"fixture_only":True}
    def call(name,bundle,scalar,succeeds):
        write(root/"measurements.json",scalar)
        bundle["runs"][0]["measurements_sha256"] = sha(root/"measurements.json")
        write(root/"bundle.json",bundle)
        return runner.call(name,{**base,"manifest":{"path":"bundle.json","sha256":sha(root/"bundle.json")}},succeeds,
                           imports={"split.json":split_path,"bundle.json":root/"bundle.json","measurements.json":root/"measurements.json"})
    folder,_ = call("reduce-exact-utf8-manifest",bundle,scalar,True)
    summary = read(folder/"shard.json")["seeds"][0]
    expected = sum(pressures[101:201])/100
    require(abs(summary["pressure_bar"]-expected) <= 1e-10,"Independent 100-sample mean")
    blocks = [sum(pressures[101+20*block:121+20*block])/20 for block in range(5)]
    require(max(abs(a-b) for a,b in zip(blocks,summary["block_means_bar"])) <= 1e-10,"Independent five block means")
    require(abs(summary["late_minus_early_bar"]-(sum(pressures[151:201])-sum(pressures[101:151]))/50) <= 1e-10,"Independent late/early drift")
    broken = json.loads(json.dumps(bundle)); broken["runs"][0]["manifest_text"] += " "
    call("reduce-mutated-raw-manifest",broken,scalar,False)
    broken = json.loads(json.dumps(bundle)); broken["runs"][0]["manifest"]["scope"] = "parsed metadata changed"
    call("reduce-parsed-manifest-mismatch",broken,scalar,False)
    broken = json.loads(json.dumps(scalar)); broken["series"][101]["step"] = 10000
    call("reduce-reordered-label-window",bundle,broken,False)
    broken = json.loads(json.dumps(scalar)); broken["series"][101]["time_ps"] += .01
    call("reduce-wrong-time",bundle,broken,False)
    broken = json.loads(json.dumps(scalar)); broken["units"]["pressure"] = "Pa"
    call("reduce-wrong-units",bundle,broken,False)
    return {"scope":"synthetic scalar arithmetic only; no solver data","mean_disagreement_bar":abs(summary["pressure_bar"]-expected),
            "tolerance_bar":1e-10,"exact_original_utf8_manifest_verified":True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--python", type=Path, default=Path(sys.executable))
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    args.output = args.output.resolve(); args.output.mkdir(parents=True, exist_ok=False)
    worker = repo / "tools/ml_surrogate_worker.py"
    runner = Runner(args.output, args.python.resolve(), worker)
    report = {"scope": "source matrix fixtures and pre-label membership only", "passed": False,
              "scientific_label_runs": 0, "scientific_model_fitted": False,
              "worker_sha256": sha(worker), "checker_sha256": sha(__file__), "receipts": runner.receipts}
    try:
        require(sha(repo / "docs/validation/ml-lab-proposal.md") == PROPOSAL, "Preregistration changed")
        report["membership"] = check_membership(runner,sha(repo / "tools/scientific_worker.py"))
        report["matrices"] = matrix_checks(runner)
        report["stages"] = stage_checks(runner)
        report["reductions"] = reduction_checks(runner)
        report["passed"] = True
    finally:
        write(args.output / "report.json", report)
    print(json.dumps({k:v for k,v in report.items() if k != "receipts"}, indent=2))


if __name__ == "__main__":
    main()
