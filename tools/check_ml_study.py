"""Independent audit of exported real ML study evidence; never import workers.

No execution or fitting. Every measurement is read from a pinned solver job.
--self-test checks scalar pair algebra only, without generating study labels.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
from datetime import datetime

import numpy as np

PROPOSAL = "b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737"
NA, MASS, R = 6.02214076e23, 39.948, .00831446261815324


def require(condition,message):
    if not condition:
        raise AssertionError(message)


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"),parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def hash_file(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b""):
            value.update(chunk)
    return value.hexdigest()


def pinned(root,spec):
    path = (root/spec["path"]).resolve()
    require(path.is_relative_to(root.resolve()) and path.is_file(),"Artifact must remain within the evidence root")
    require(hash_file(path) == spec["sha256"],f"Changed evidence: {spec['path']}")
    if "bytes" in spec:
        require(path.stat().st_size == spec["bytes"],"Changed artifact length")
    return path


def expected_membership():
    rows = []
    for temperature in [220,250,280,310,325,370,400,430]:
        for density in [.25,.45,.65,.85]:
            key = f"phaseforge-ml-argon-v1|T={temperature}|rho={density:.2f}"
            rows.append({"condition_index":len(rows),"temperature_kelvin":temperature,"density_g_cm3":density,
                         "split_key":key,"split_sha256":hashlib.sha256(key.encode("ascii")).hexdigest(),
                         "role":"regime_test" if temperature == 325 else None})
    ordered = sorted([row for row in rows if row["role"] is None],key=lambda row:(row["split_sha256"],row["temperature_kelvin"],row["density_g_cm3"]))
    roles = ["train"]*12+["validation"]*3+["calibration"]*9+["test"]*4
    for row,role in zip(ordered,roles):
        row["role"] = role
    return rows


def pair_virial(points,box,cutoff):
    points = np.asarray(points,dtype=float)
    require(points.ndim == 2 and points.shape[1] == 3 and np.isfinite(points).all(),"Invalid pair coordinates")
    virial = 0.
    start = cutoff*.8
    for i in range(len(points)):
        for j in range(i):
            displacement = points[i]-points[j]
            displacement -= box*np.rint(displacement/box)
            radius = math.sqrt(float(displacement @ displacement))
            require(radius > 0,"Coincident atoms")
            if radius >= cutoff:
                continue
            q6 = (.3405/radius)**6
            energy = 4*.997*(q6*q6-q6)
            bare_virial = 24*.997*(2*q6*q6-q6)
            if radius > start:
                t = (radius-start)/(cutoff-start)
                switch = 1-10*t**3+15*t**4-6*t**5
                derivative = (-30*t*t+60*t**3-30*t**4)/(cutoff-start)
                virial += switch*bare_virial-radius*energy*derivative
            else:
                virial += bare_virial
    return virial


def check_seed(root,entry,condition,protocol,check_pressure):
    directory = (root/entry["directory"]).resolve()
    require(directory.is_relative_to(root.resolve()),"Solver folder escaped evidence root")
    artifacts = {row["path"]:row for row in entry["artifacts"]}
    require(len(artifacts) == len(entry["artifacts"]),"Duplicate artifact path")
    for row in artifacts.values():
        pinned(directory,row)
    require(all(name in artifacts for name in ["manifest.json","measurements.json","result.json","trajectory/index.json","scientific_worker.py"]),"Missing registered solver instruments")
    manifest = read(directory/"manifest.json")
    require(read(directory/"result.json")["status"] == "completed","A missing/failed seed cannot be scored")
    index,replica = entry["condition_index"],entry["replicate_index"]
    seed = 110000+1000*index+replica
    require(type(replica) is int and replica in (0,1,2) and entry["seed"] == seed,"Replica identity")
    require(entry["role"] == condition["role"],"Condition split leakage")
    expected = {"atom_count":108,"thermostat":"langevin","friction_per_ps":5.,"timestep_fs":1.,"steps":20000,
                "sample_interval":100,"chunk_frames":50,"platform":"CPU","cpu_threads":1,
                "temperature_kelvin":condition["temperature_kelvin"],"density_g_cm3":condition["density_g_cm3"],"seed":seed}
    require(manifest["parameters"] == expected == entry["parameters"],"Solver protocol changed")
    require(manifest["engine"] == "openmm_argon" and manifest["engine_version"] == protocol["engine_version"]
            and manifest["worker_sha256"] == protocol["worker_sha256"] and manifest["platform"] == "CPU"
            and manifest["platform_properties"] == {"Threads":"1","DeterministicForces":"true"},"Solver source/platform changed")
    require(artifacts["scientific_worker.py"]["sha256"] == protocol["worker_sha256"],"Executed solver source differs from pinned protocol")
    normalized_parameters = dict(expected)
    for key in ["temperature_kelvin","density_g_cm3","timestep_fs","friction_per_ps"]:
        normalized_parameters[key] = float(normalized_parameters[key])
    input_hash = hashlib.sha256(json.dumps({"engine":"openmm_argon","parameters":normalized_parameters},sort_keys=True,separators=(",",":"),allow_nan=False).encode()).hexdigest()
    require(manifest["input_sha256"] == input_hash,"Normalized solver input hash mismatch")
    measurements = read(directory/"measurements.json")
    for units in [manifest["units"],measurements["units"]]:
        require(all(units.get(key) == value for key,value in {"temperature":"K","time":"ps","pressure":"bar","density":"g/cm^3","position":"nm"}.items()),"Wrong instrument units")
    series = measurements["series"]
    require(len(series) == 201,"Exactly 201 retained scalar states required")
    for number,row in enumerate(series):
        require(type(row["step"]) is int and row["step"] == 100*number,"Integer sample sequence")
        require(all(type(value) in (int,float) and math.isfinite(value) for value in row.values()),"Nonfinite scalar")
        require(abs(row["time_ps"]-.1*number) <= 1e-8,"Scalar time mismatch")
    selected = [row["pressure_bar"] for row in series if 10000 < row["step"] <= 20000]
    require(len(selected) == 100,"Fixed label window")
    mean = math.fsum(selected)/100
    model = manifest["model"]
    volume = math.prod(model["box_nm"])
    expected_volume = 108*MASS/NA/condition["density_g_cm3"]*1e21
    require(abs(volume-expected_volume) <= expected_volume*1e-12,"Fixed density/volume")
    expected_cutoff = min(2.5*.3405,.49*expected_volume**(1/3))
    require(model["mass_dalton"] == MASS and model["sigma_nm"] == .3405 and model["epsilon_kj_mol"] == .997
            and model["dispersion_correction"] is False and model["charges"] == 0 and model["boundary"] == "cubic periodic"
            and abs(model["cutoff_nm"]-expected_cutoff) <= 1e-12 and abs(model["switch_nm"]-.8*expected_cutoff) <= 1e-12,"Potential/cutoff identity changed")
    trajectory = read(directory/"trajectory/index.json")
    require(trajectory["frame_count"] == 201,"Retained frame count changed")
    count = 0
    for chunk in trajectory["chunks"]:
        for key,hash_key in [("arrays_path","arrays_sha256"),("path","sha256")]:
            require(chunk[key] in artifacts and artifacts[chunk[key]]["sha256"] == chunk[hash_key],"Registered chunk/array identity mismatch")
        frames = read(directory/chunk["path"])["frames"]
        size = len(frames)
        require(0 < size <= 50 and chunk["start_frame"] == count and chunk["end_frame"] == count+size-1,"Chunk ordering/fidelity changed")
        with np.load(directory/chunk["arrays_path"],allow_pickle=False) as arrays:
            for name in ["positions_unwrapped_nm","velocities_nm_ps","forces_kj_mol_nm"]:
                require(arrays[name].shape == (size,108,3) and np.isfinite(arrays[name]).all(),"Nonfinite or malformed retained state")
            require(np.array_equal(arrays["steps"],np.arange(count,count+size)*100),"Numerical state order changed")
            require(np.isfinite(arrays["time_ps"]).all() and np.max(np.abs(arrays["time_ps"]-np.arange(count,count+size)*.1)) <= 1e-8,"Numerical sample times changed")
            for offset,frame in enumerate(frames):
                require(frame["step"] == (count+offset)*100 and abs(frame["time"]-float(arrays["time_ps"][offset])) <= 1e-8,"Rendered state does not match numerical sample")
        count += size
    require(count == 201,"Missing retained numerical states")
    checks = []
    if check_pressure:
        for step in [10100,20000]:
            chunk = next(row for row in trajectory["chunks"] if row["start_step"] <= step <= row["end_step"])
            for key,hash_key in [("arrays_path","arrays_sha256"),("path","sha256")]:
                require(chunk[key] in artifacts and artifacts[chunk[key]]["sha256"] == chunk[hash_key],"Trajectory registration/hash mismatch")
            with np.load(directory/chunk["arrays_path"],allow_pickle=False) as arrays:
                offset = step//100-chunk["start_frame"]
                points = np.asarray(arrays["positions_unwrapped_nm"][offset],dtype=float)
                require(points.shape == (108,3),"Changed retained atom count")
                virial = pair_virial(points,np.asarray(model["box_nm"]),model["cutoff_nm"])
            scalar = series[step//100]
            pressure = (2*scalar["kinetic_energy_kj_mol"]+virial)/(3*volume)*(1e25/NA)
            error = abs(pressure-scalar["pressure_bar"])
            require(error <= 1e-8,f"Independent pair-virial pressure {entry['solver_job_id']} step{step}: {error}")
            checks.append({"step":step,"pressure_error_bar":error})
    return {"pressure_bar":mean,"p0_bar":107*R*condition["temperature_kelvin"]/volume*(1e25/NA),
            "source_job_id":entry["solver_job_id"],"measurements_sha256":artifacts["measurements.json"]["sha256"],
            "blocks_bar":[math.fsum(selected[start:start+20])/20 for start in range(0,100,20)],
            "pair_virial_checks":checks}


def prediction(card,arrays,points):
    x = np.asarray(points,dtype=float)
    configuration = card["configuration"]
    if configuration["family"] == "kernel":
        distances = (x[:,None,:]-arrays["fitting_x"][None,:,:])/np.asarray(configuration["lengths"])
        design = np.exp(-np.sum(distances*distances,axis=2)/2)
    else:
        a,b = x.T
        columns = [np.ones(len(x)),a,b]
        if configuration["family"] == "quadratic":
            columns += [a*a,a*b,b*b]
        design = np.column_stack(columns)
    return 1+float(arrays["target_center"][0])+float(arrays["target_scale"][0])*(design@arrays["weights"])


def check_evaluation(root,receipt,split,labels):
    card = read(pinned(root,receipt["model_card"]))
    calibration = read(pinned(root,receipt["calibration"]))
    evaluation = read(pinned(root,receipt["evaluation"]))
    require(card["model_sha256"] == receipt["model"]["sha256"] == evaluation["model_sha256"] == calibration["model_sha256"],"Frozen model chain")
    require(calibration["model_card_sha256"] == receipt["model_card"]["sha256"] and evaluation["calibration_sha256"] == receipt["calibration"]["sha256"],"Frozen calibration chain")
    require(hash_file(pinned(root,receipt["worker"])) == card["source_sha256"],"Frozen fitting source changed")
    with np.load(pinned(root,receipt["model"]),allow_pickle=False) as loaded:
        arrays = {name:loaded[name].copy() for name in loaded.files}
    require(all(value.dtype.kind in "fiu" and np.isfinite(value).all() for value in arrays.values()),"Finite pickle-free model")
    fit_ids = [row["condition_index"] for row in split["conditions"] if row["role"] in ("train","validation")]
    require(card["fitting_condition_indices"] == fit_ids,"Fitting membership")
    fitting_residual = np.array([labels[index]["Z"]-1 for index in fit_ids])
    require(abs(float(arrays["target_center"][0])-float(fitting_residual.mean())) <= 1e-10 and
            abs(float(arrays["target_scale"][0])-max(float(fitting_residual.std()),1e-6)) <= 1e-10,"Target normalization used incorrect rows")
    xy = lambda ids: [[(split["conditions"][i]["temperature_kelvin"]-220)/210,(split["conditions"][i]["density_g_cm3"]-.25)/.6] for i in ids]
    require(np.array_equal(arrays["fitting_x"],np.asarray(xy(fit_ids))),"Frozen coordinate normalization")
    cal_ids = [row["condition_index"] for row in split["conditions"] if row["role"] == "calibration"]
    cal_predictions = prediction(card,arrays,xy(cal_ids))
    q = max(abs(value-labels[index]["Z"]) for index,value in zip(cal_ids,cal_predictions))
    require(abs(q-calibration["q_Z"]) <= 1e-10,"Independent calibration statistic")
    test_ids = [row["condition_index"] for row in split["conditions"] if row["role"] in ("test","regime_test")]
    predictions = prediction(card,arrays,xy(test_ids))
    truth = np.array([labels[index]["Z"] for index in test_ids])
    errors = predictions-truth
    rmse = float(np.sqrt(np.mean(errors**2)))
    require(abs(rmse-evaluation["combined"]["rmse_Z"]) <= 1e-10,"Independent RMSE")
    fit_z = np.array([labels[index]["Z"] for index in fit_ids])
    distance = np.sqrt(np.sum((np.asarray(xy(test_ids))[:,None,:]-np.asarray(xy(fit_ids))[None,:,:])**2,axis=2))
    baselines = {"p0":np.ones(8),"training_mean":np.full(8,float(fit_z.mean())),"nearest_condition":fit_z[np.argmin(distance,axis=1)]}
    baseline_errors = {name:float(np.sqrt(np.mean((values-truth)**2))) for name,values in baselines.items()}
    for name,value in baseline_errors.items():
        require(abs(value-evaluation["baselines"][name]["rmse_Z"]) <= 1e-10,"Independent baseline RMSE")
    covered = np.abs(errors) <= calibration["q_Z"]
    supported = distance.min(axis=1) <= .40
    group_accuracy,group_coverage = True,True
    for role in ["test","regime_test"]:
        mask = np.array([split["conditions"][index]["role"] == role for index in test_ids])
        group_rmse = float(np.sqrt(np.mean(errors[mask]**2)))
        require(abs(group_rmse-evaluation["groups"][role]["rmse_Z"]) <= 1e-10,"Independent regime RMSE")
        require(int(covered[mask].sum()) == evaluation["groups"][role]["covered"],"Independent regime coverage")
        group_accuracy &= group_rmse <= .07; group_coverage &= int(covered[mask].sum()) >= 3
    noise = math.sqrt(math.fsum(labels[index]["se_Z"]**2 for index in test_ids)/8)
    best = min(baseline_errors.values())
    gates = {"accuracy":rmse <= .05 and group_accuracy and float(np.max(np.abs(errors))) <= .15,
             "baseline_improvement":rmse <= .8*best and best-rmse >= 3*noise,
             "coverage_and_support":int(covered.sum()) >= 7 and group_coverage and calibration["q_Z"] <= .15 and int(supported.sum()) >= 6}
    for name,value in gates.items():
        require(bool(value) == evaluation["gates"][name],f"Independent rejection rule {name}")
    require(int(covered.sum()) == evaluation["coverage_count"] and int(supported.sum()) == evaluation["support_count"],"Independent counts")
    by_id = {row["condition_index"]:row for row in evaluation["cases"]}
    require(set(by_id) == set(test_ids),"Missing held-out cases")
    for index,pred in zip(test_ids,predictions):
        case = by_id[index]
        require(abs(case["prediction_Z"]-pred) <= 1e-10 and abs(case["pressure_bar"]-labels[index]["pressure_bar"]) <= 1e-10,"Recomputed prediction/condition label")
        require(abs(case["seed_se_bar"]-labels[index]["se_bar"]) <= 1e-10,"Replicate uncertainty")
    costs = read(pinned(root,receipt["costs"]))
    registered = {row["case_id"]:row for row in receipt["runs"]}
    require(len(costs["solver_runs"]) == 99 and {row["case_id"] for row in costs["solver_runs"]} == set(registered),"All study/fallback costs required")
    total_wall = 0.
    for row in costs["solver_runs"]:
        source = registered[row["case_id"]]
        require(row["solver_job_id"] == source["solver_job_id"] and row["status"] == "completed","Cost/solver lineage mismatch")
        record = read(pinned(root,source["job_record"]))
        require(record["id"] == row["solver_job_id"] and record["state"] == "completed","Cost requires completed source record")
        start = datetime.fromisoformat(record["created_at"].replace("Z","+00:00"))
        end = datetime.fromisoformat(record["completed_at"].replace("Z","+00:00"))
        elapsed = math.floor((end-start).total_seconds()*1000)/1000
        require(row["wall_seconds"] > 0 and abs(row["wall_seconds"]-elapsed) <= .001,"Solver cost disagrees with retained job timing")
        total_wall += row["wall_seconds"]
    components = ["environment_startup_seconds","model_selection_seconds","calibration_evaluation_seconds","cold_model_load_seconds","failed_attempt_seconds"]
    require(all(type(costs.get(key)) in (int,float) and math.isfinite(costs[key]) and costs[key] >= 0 for key in components),"Missing setup/failed-attempt costs")
    setup = total_wall+math.fsum(costs[key] for key in components)
    endpoint = total_wall/99*3
    single = evaluation["benchmark"]["warm_single_query_seconds"]
    if single is None:
        speed,workload = False,False
    else:
        require(single > 0 and math.isfinite(single),"Invalid inference timing")
        amortized = single+.2*endpoint
        expected_total = setup+1000*amortized
        reported = evaluation["costs"]
        require(abs(reported["setup_seconds"]-setup) <= 1e-8 and abs(reported["accelerated_workload_seconds"]-expected_total) <= 1e-7,
                "Independent total-cost arithmetic")
        expected_break_even = math.ceil(setup/(endpoint-amortized)) if endpoint > amortized else None
        require(reported["break_even_queries"] == expected_break_even,"Independent break-even arithmetic")
        speed,workload = single*10 <= total_wall/99,expected_total < 1000*endpoint
    gates.update({"warm_speed":speed,"total_workload_cost":workload})
    require(all(bool(value) == evaluation["gates"][key] for key,value in gates.items()) and bool(all(gates.values())) == evaluation["useful_acceleration"],"Independent final availability/rejection")
    return {"rmse_Z":rmse,"coverage_count":int(covered.sum()),"support_count":int(supported.sum()),"gates":gates,
            "setup_seconds":setup,"three_seed_endpoint_seconds":endpoint}


def audit(receipt_path):
    receipt = read(receipt_path)
    root = (receipt_path.parent/receipt.get("root",".")).resolve()
    split = read(pinned(root,receipt["split"]))
    require(split["proposal_sha256"] == PROPOSAL and split["conditions"] == expected_membership(),"Frozen protocol/split")
    runs = receipt["runs"]
    require(len(runs) == 99 and len({(row["condition_index"],row["replicate_index"]) for row in runs}) == 99,"All 99 unique seeds must be registered")
    groups = {}
    for entry in runs:
        index = entry["condition_index"]
        require(type(index) is int and 0 <= index <= 32,"Invalid condition")
        condition = split["conditions"][index] if index < 32 else {"temperature_kelvin":180,"density_g_cm3":.55,"role":"ood"}
        result = check_seed(root,entry,condition,split["protocol"],entry["replicate_index"] == 0 and condition["role"] in ("test","regime_test"))
        groups.setdefault(index,[]).append(result)
    require(set(groups) == set(range(33)) and all(len(rows) == 3 for rows in groups.values()),"Every condition needs three measured replicas")
    labels = {}
    for index,rows in groups.items():
        mean = math.fsum(row["pressure_bar"] for row in rows)/3
        sd = math.sqrt(math.fsum((row["pressure_bar"]-mean)**2 for row in rows)/2)
        ideal = rows[0]["p0_bar"]
        labels[index] = {"pressure_bar":mean,"Z":mean/ideal,"se_bar":sd/math.sqrt(3),"se_Z":sd/math.sqrt(3)/ideal,"seeds":rows}
    # Check every reduction seed, including fitting/calibration/OOD, against its
    # original registered pressure samples rather than accepting a table label.
    observed = set()
    source_lookup = {row["source_job_id"]:row for values in groups.values() for row in values}
    for spec in receipt["role_shards"]:
        shard = read(pinned(root,spec))
        for seed in shard["seeds"]:
            job = seed["solver_job_id"]
            require(job in source_lookup and job not in observed,"Unknown or duplicate reduced solver source")
            observed.add(job); actual = source_lookup[job]
            require(seed["measurements_sha256"] == actual["measurements_sha256"] and abs(seed["pressure_bar"]-actual["pressure_bar"]) <= 1e-10,"Independent seed reduction")
            require(max(abs(a-b) for a,b in zip(seed["block_means_bar"],actual["blocks_bar"])) <= 1e-10,"Independent block reduction")
    require(observed == set(source_lookup),"Every source seed must have a checked reduction")
    dataset = check_dataset(root,receipt,split,labels,runs)
    evaluation = check_evaluation(root,receipt,split,labels)
    return {"passed":True,"scope":"Independent stored-data/ML numerical consistency; not empirical validation or proof of coordinator access isolation",
            "solver_runs":99,"conditions":33,"seed_mean_tolerance_bar":1e-10,"pair_pressure_tolerance_bar":1e-8,
            "sampled_pair_pressure_checks":sum(len(seed["pair_virial_checks"]) for seeds in groups.values() for seed in seeds),
            "evaluation":evaluation,"dataset":dataset,"ood_measured_endpoint_bar":labels[32]["pressure_bar"],
            "coordinator_checks":"Role access, source seals, timers, cancellation and installed ordinary-chat behavior require their separate receipts"}


def check_dataset(root,receipt,split,labels,runs):
    """Check the archival 33x3 data product against independent raw-run means."""
    path = pinned(root,receipt["dataset"])
    metadata = read(pinned(root,receipt["dataset_metadata"]))
    require(metadata["dataset_sha256"] == receipt["dataset"]["sha256"],"Dataset metadata/hash mismatch")
    require(metadata["study_id"] == split["study_id"] and metadata["proposal_sha256"] == PROPOSAL
            and metadata["protocol_sha256"] == split["protocol_sha256"] and metadata["split_sha256"] == receipt["split"]["sha256"],"Dataset study/split identity")
    conditions = split["conditions"]+[split["ood"]]
    require(metadata["conditions"] == conditions and metadata["units"] == {"temperature":"K","density":"g/cm^3","pressure":"bar","Z":"1"},"Dataset condition schema or units changed")
    role_names = ["train","validation","calibration","test","regime_test","ood"]
    require(metadata["role_codes"] == {str(index):name for index,name in enumerate(role_names)},"Dataset role codes changed")
    indexed = {(row["condition_index"],row["replicate_index"]):row for row in runs}
    measured = {seed["source_job_id"]:seed for label in labels.values() for seed in label["seeds"]}
    pressure = np.array([[measured[indexed[index,replica]["solver_job_id"]]["pressure_bar"] for replica in range(3)] for index in range(33)])
    means = np.array([labels[index]["pressure_bar"] for index in range(33)])
    se = np.array([labels[index]["se_bar"] for index in range(33)])
    p0 = np.array([labels[index]["seeds"][0]["p0_bar"] for index in range(33)])
    expected = {"condition_index":np.arange(33),"temperature_kelvin":np.array([row["temperature_kelvin"] for row in conditions]),
                "density_g_cm3":np.array([row["density_g_cm3"] for row in conditions]),
                "role_code":np.array([role_names.index(row["role"]) for row in conditions]),
                "replicate_seeds":np.array([[110000+1000*index+replica for replica in range(3)] for index in range(33)]),
                "seed_pressure_bar":pressure,"mean_pressure_bar":means,"seed_sd_bar":se*math.sqrt(3),
                "seed_se_bar":se,"p0_bar":p0,"Z":means/p0}
    with np.load(path,allow_pickle=False) as arrays:
        require(set(arrays.files) == set(expected),"Unexpected or missing dataset arrays")
        for key,want in expected.items():
            actual = arrays[key]
            require(actual.dtype.kind in "fiu" and actual.shape == want.shape and np.isfinite(actual).all(),f"Invalid dataset array {key}")
            tolerance = 0 if key in {"condition_index","role_code","replicate_seeds"} else 1e-10
            require(float(np.max(np.abs(actual-want))) <= tolerance,f"Dataset differs from independently reduced solver evidence: {key}")
    seeds = metadata["seeds"]
    require(len(seeds) == 99,"Dataset metadata must retain 99 measured seed sources")
    for offset,seed in enumerate(seeds):
        index,replica = divmod(offset,3)
        source = indexed[index,replica]
        actual = measured[source["solver_job_id"]]
        require((seed["condition_index"],seed["replicate_index"]) == (index,replica) and seed["solver_job_id"] == source["solver_job_id"]
                and seed["seed"] == source["seed"] and seed["role"] == source["role"],"Dataset source ordering or lineage changed")
        require(seed["measurements_sha256"] == actual["measurements_sha256"] and abs(seed["pressure_bar"]-actual["pressure_bar"]) <= 1e-10,"Dataset metadata changed measured labels")
    shard_pins = {spec["sha256"] for spec in receipt["role_shards"]}
    require({spec["sha256"] for spec in metadata["source_shards"]} == shard_pins,"Dataset is not derived from all retained role shards")
    return {"conditions":33,"seed_rows":99,"mean_sd_se_and_roles_verified":True,"dataset_sha256":receipt["dataset"]["sha256"]}


def self_test():
    box = np.array([4.,4.,4.])
    value = pair_virial([[0,0,0],[.3405,0,0]],box,.85)
    require(abs(value-24*.997) <= 1e-10,"Bare pair reference")
    radius,cutoff = .765,.85
    q6 = (.3405/radius)**6
    expected = .5*24*.997*(2*q6*q6-q6)-radius*4*.997*(q6*q6-q6)*(-1.875/(cutoff*.2))
    require(abs(pair_virial([[0,0,0],[radius,0,0]],box,cutoff)-expected) <= 1e-10,"Switched pair reference")
    require(abs(pair_virial([[0,0,0],[4-.3405,0,0]],box,.85)-24*.997) <= 1e-10,"Periodic pair reference")
    return {"passed":True,"scope":"Fixed independent pair algebra only","scientific_solver_runs":0}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest",type=Path)
    parser.add_argument("--output",required=True,type=Path)
    parser.add_argument("--self-test",action="store_true")
    args = parser.parse_args()
    report = {"passed":False,"checker_sha256":hash_file(__file__)}
    try:
        require(args.self_test or args.manifest is not None,"Choose --manifest or --self-test")
        report.update(self_test() if args.self_test else audit(args.manifest.resolve()))
    except Exception as error:
        report["error"] = str(error)
        raise
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,sort_keys=True,indent=2,allow_nan=False),encoding="utf-8")
    print(json.dumps(report,indent=2))


if __name__ == "__main__":
    main()
