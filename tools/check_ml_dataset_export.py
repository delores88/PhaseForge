"""Independent scalar checks for the archival NPZ export; synthetic inputs only."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import numpy as np

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument("--fixtures",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
    imports=root/"imports";imports.mkdir()
    split_path=args.fixtures/"membership-freeze/split.json"
    split=json.loads(split_path.read_text())
    shutil.copyfile(split_path,imports/"split.json")
    specifications=[];original=[]
    for role in ("train","validation","calibration","test","regime_test"):
        source=args.fixtures/"synthetic-stage-inputs"/(role+".json")
        shutil.copyfile(source,imports/source.name)
        shard=json.loads(source.read_text());original.extend(shard["seeds"])
        specifications.append({"path":source.name,"sha256":sha(source),"role":role})
    ood={key:split[key] for key in ("study_id","proposal_sha256","protocol_sha256")}
    ood.update({"role":"ood","split_sha256":sha(split_path),"seeds":[]})
    for replica,pressure in enumerate([101.0,103.0,107.0]):
        row={"condition_index":32,"replicate_index":replica,"seed":142000+replica,"role":"ood",
             "temperature_kelvin":180,"density_g_cm3":.55,"pressure_bar":pressure,"p0_bar":200.0,
             "fixture_only":True,"solver_job_id":f"synthetic-export-{replica}"}
        ood["seeds"].append(row);original.append(row)
    ood_path=imports/"ood.json";ood_path.write_text(json.dumps(ood))
    specifications.append({"path":"ood.json","sha256":sha(ood_path),"role":"ood"})
    request={"split":{"path":"split.json","sha256":sha(split_path)},"sources":specifications}
    (root/"input.json").write_text(json.dumps(request))
    worker=Path(__file__).with_name("ml_dataset_export.py").resolve()
    shutil.copyfile(worker,root/"experiment.py")
    execution=subprocess.run([sys.executable,"-I",str(root/"experiment.py")],cwd=root,capture_output=True,text=True,timeout=30)
    (root/"stderr.txt").write_text(execution.stderr)
    assert execution.returncode==0,execution.stderr
    with np.load(root/"dataset.npz",allow_pickle=False) as saved:
        assert saved["seed_pressure_bar"].shape==(33,3)
        for index in range(33):
            rows=sorted((row for row in original if row["condition_index"]==index),key=lambda row:row["replicate_index"])
            values=[row["pressure_bar"] for row in rows]
            mean=math.fsum(values)/3
            deviation=math.sqrt(math.fsum((value-mean)**2 for value in values)/2)
            assert np.array_equal(saved["seed_pressure_bar"][index],values)
            assert np.array_equal(saved["replicate_seeds"][index],[row["seed"] for row in rows])
            for key,expected in [("mean_pressure_bar",mean),("seed_sd_bar",deviation),("seed_se_bar",deviation/math.sqrt(3)),("Z",mean/rows[0]["p0_bar"])]:
                assert abs(float(saved[key][index])-expected)<=1e-10,(index,key)
        assert saved["role_code"][-1]==5 and all(value.dtype.kind in "ifu" and np.isfinite(value).all() for value in saved.values())
    missing=root/"missing-role";missing.mkdir();shutil.copytree(imports,missing/"imports");shutil.copyfile(worker,missing/"experiment.py")
    request["sources"]=request["sources"][:-1];(missing/"input.json").write_text(json.dumps(request))
    failure=subprocess.run([sys.executable,"-I",str(missing/"experiment.py")],cwd=missing,capture_output=True,text=True,timeout=30)
    (missing/"stderr.txt").write_text(failure.stderr)
    assert failure.returncode!=0 and not (missing/"dataset.npz").exists()
    report={"passed":True,"scope":"Synthetic archival dataset export only; no scientific labels or model fitting", "conditions_checked":33,"seed_rows_checked":99,"missing_role_rejected":True,"worker_sha256":sha(worker),"dataset_sha256":sha(root/"dataset.npz")}
    (root/"report.json").write_text(json.dumps(report,indent=2))
    print(json.dumps(report))

if __name__=="__main__":
    main()
