"""Deterministic receipt and data-product fixtures; no scientific execution."""
import hashlib
from contextlib import closing
import json
import math
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
import uuid

import numpy as np
sys.path.insert(0,str(Path(__file__).resolve().parent))
import export_ml_check_manifest as assembler
import check_ml_study as independent


def raw(value):
    return json.dumps(value,sort_keys=True,separators=(",",":"),ensure_ascii=False,allow_nan=False).encode()


def sha(value):
    return hashlib.sha256(value).hexdigest()


class ReceiptFixture:
    def __init__(self,path):
        self.path=path;self.project=str(uuid.uuid4());self.study_id=str(uuid.uuid4());self.jobs={};self.stages={}
        self.worker=b"synthetic frozen source only";self.solver=b"synthetic solver source only"
        self.study=self.new("ml_study",None,{"worker_sha256":sha(self.worker),"solver_worker_sha256":sha(self.solver)},self.study_id)

    def new(self,kind,parent,input_value,id_value=None):
        identity=id_value or str(uuid.uuid4())
        job={"id":identity,"kind":kind,"parent_id":parent,"project_id":self.project,"input":input_value,
             "state":"completed","created_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:00:01Z","result":{}}
        self.jobs[identity]=job
        self.directory(identity).mkdir(parents=True,exist_ok=True)
        return job

    def directory(self,identity):
        return self.path/"artifacts"/"laboratory"/identity

    def write(self,job,path,data):
        full=self.directory(job["id"])/path;full.parent.mkdir(parents=True,exist_ok=True)
        full.write_bytes(data if isinstance(data,bytes) else raw(data))

    def completion(self,job):
        self.write(job,"result.json",job["result"])
        rows=[]
        for path in sorted(self.directory(job["id"]).rglob("*")):
            if path.is_file():
                rows.append({"path":path.relative_to(self.directory(job["id"])).as_posix(),"bytes":path.stat().st_size,"sha256":assembler.hash_file(path)})
        return {"job_id":job["id"],"input_sha256":sha(raw(job["input"])),"artifacts":rows,"solver_wall_seconds":1.0,"wall_seconds":1.0}

    def stage(self,name,files,kind="generated",input_value=None):
        job=self.new(kind,self.study_id,input_value or {"code":self.worker.decode(),"inputs":{"phase":name}})
        for path,data in files.items():
            self.write(job,path,data)
        self.register(name,job)
        return job

    def register(self,name,job):
        completion=self.completion(job)
        self.stages[name]={"kind":job["kind"],"input_sha256":completion["input_sha256"],"attempts":[job["id"]],"completion":completion}

    def finish(self):
        runs=[{"case_id":f"c{index}-s{replica}","condition_index":index,"replicate_index":replica,"role":"synthetic",
               "seed":110000+1000*index+replica,"engine":"openmm_argon","parameters":{"value":index+.25,"seed":110000+1000*index+replica}}
              for index in range(33) for replica in range(3)]
        split=raw({"scope":"synthetic identity fixture only"})
        freeze=self.stage("freeze",{"work/split.json":split,"work/runs.json":{"runs":runs}})
        model=b"synthetic model bytes; no model fitted";card=raw({"model_sha256":sha(model)})
        fit=self.stage("fit",{"work/model.npz":model,"work/model-card.json":card,"work/experiment.py":self.worker})
        calibration=self.stage("calibrate",{"work/calibration.json":{}})
        evaluation=self.stage("finalize-costs",{"work/evaluation.json":{}})
        export=self.stage("export-dataset",{"work/dataset.npz":b"synthetic dataset bytes","work/dataset.json":{}})
        self.stage("measured-costs",{"bundle.json":{}},kind="study_data")
        self.stage("train-reduce-0",{"work/shard.json":{}},input_value={"code":self.worker.decode(),"inputs":{"phase":"reduce"}})
        sweep=self.new("sweep",self.study_id,{"cases":[{"case_id":run["case_id"],"engine":run["engine"],"parameters":run["parameters"]} for run in runs]})
        cases=[]
        for run in runs:
            solver=self.new("solver",sweep["id"],{"engine":run["engine"],"parameters":run["parameters"]})
            self.write(solver,"scientific_worker.py",self.solver)
            self.write(solver,"measurements.json",{"scope":"synthetic receipt only"})
            completion=self.completion(solver)
            cases.append({"case_id":run["case_id"],"input_sha256":completion["input_sha256"],"attempts":[solver["id"]],"completion":completion})
        self.write(sweep,"sweep-ledger.json",{"input_sha256":sha(raw(sweep["input"])),"cases":cases})
        self.register("all-fixture-runs",sweep)
        model_freeze={"model_sha256":sha(model),"model_card_sha256":sha(card),"split_sha256":sha(split)}
        result={"freeze_job_id":freeze["id"],"fit_job_id":fit["id"],"calibration_job_id":calibration["id"],
                "evaluation_job_id":evaluation["id"],"dataset_export_job_id":export["id"],"model_freeze":model_freeze}
        self.study["result"]=result
        self.write(self.study,"model-freeze.json",model_freeze)
        self.write(self.study,"result.json",result)
        self.write(self.study,"study-ledger.json",{"input_sha256":sha(raw(self.study["input"])),"stages":self.stages,"model_freeze":model_freeze})
        with closing(sqlite3.connect(self.path/"phaseforge.sqlite3")) as database:
            database.execute("CREATE TABLE objects(kind TEXT,id TEXT,json TEXT)")
            database.executemany("INSERT INTO objects VALUES('laboratory_job',?,?)",[(identity,raw(job).decode()) for identity,job in self.jobs.items()])
            database.commit()
        return cases


class ReceiptTests(unittest.TestCase):
    def test_original_input_bytes_preserve_float_format_unicode_and_nested_input_keys(self):
        document='{"id":"a","other":{"input":42},"input":{"é":1e-7,"tiny":-0.0,"nested":{"input":"x"}}}'
        expected='{"é":1e-7,"tiny":-0.0,"nested":{"input":"x"}}'.encode()
        self.assertEqual(assembler.raw_field(document,"input"),expected)
        self.assertNotEqual(raw(json.loads(expected)),expected,"Reserialization would lose the original representation")

    def test_complete_synthetic_receipt_assembly_pins_99_cases_without_mutating_source(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);fixture=ReceiptFixture(root);cases=fixture.finish()
            before=assembler.hash_file(root/"phaseforge.sqlite3")
            evidence=assembler.Evidence(root,fixture.study_id,root/"audit")
            try:
                manifest=json.loads(evidence.build().read_text())
            finally:
                evidence.close()
            self.assertEqual(len(manifest["runs"]),99);self.assertEqual(len(manifest["role_shards"]),1)
            self.assertEqual(assembler.hash_file(root/"phaseforge.sqlite3"),before)
            self.assertEqual({run["solver_job_id"] for run in manifest["runs"]},{case["completion"]["job_id"] for case in cases})
            for row in manifest["runs"]:
                self.assertTrue(independent.pinned(Path(manifest["root"]),row["job_record"]).is_file())

    def test_mutated_completed_bytes_or_missing_seed_cannot_be_assembled(self):
        for mutation in ("bytes","missing"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as folder:
                root=Path(folder);fixture=ReceiptFixture(root);cases=fixture.finish()
                solver=cases[0]["completion"]["job_id"]
                if mutation=="bytes":
                    (fixture.directory(solver)/"measurements.json").write_bytes(b"changed")
                else:
                    with closing(sqlite3.connect(root/"phaseforge.sqlite3")) as database:
                        database.execute("DELETE FROM objects WHERE id=?",(solver,))
                        database.commit()
                evidence=assembler.Evidence(root,fixture.study_id,root/"audit")
                try:
                    with self.assertRaises(AssertionError):
                        evidence.build()
                finally:
                    evidence.close()
                self.assertFalse((root/"audit/audit.json").exists())

    def test_incomplete_study_is_refused_without_exporting_labels(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);fixture=ReceiptFixture(root);fixture.finish()
            with closing(sqlite3.connect(root/"phaseforge.sqlite3")) as database:
                job=fixture.study.copy();job["state"]="paused"
                database.execute("UPDATE objects SET json=? WHERE id=?",(raw(job).decode(),job["id"]))
                database.commit()
            with self.assertRaises(AssertionError):
                assembler.Evidence(root,fixture.study_id,root/"audit")
            self.assertFalse((root/"audit").exists())


class DatasetTests(unittest.TestCase):
    def test_independent_dataset_arrays_mean_sd_se_roles_and_lineage(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);conditions=independent.expected_membership()+[{"condition_index":32,"temperature_kelvin":180,"density_g_cm3":.55,"role":"ood"}]
            roles=["train","validation","calibration","test","regime_test","ood"]
            split={"study_id":"synthetic","proposal_sha256":independent.PROPOSAL,"protocol_sha256":"fixture","conditions":conditions[:32],"ood":conditions[32]}
            runs=[];labels={};seeds=[]
            for index in range(33):
                measured=[]
                for replica in range(3):
                    identity=f"fixture-{index}-{replica}";pressure=index*10.+replica;role=conditions[index]["role"]
                    run={"condition_index":index,"replicate_index":replica,"solver_job_id":identity,"seed":110000+1000*index+replica,"role":role}
                    runs.append(run)
                    measured.append({"source_job_id":identity,"pressure_bar":pressure,"p0_bar":100.,"measurements_sha256":f"fixture-hash-{index}-{replica}"})
                    seeds.append({**run,"pressure_bar":pressure,"measurements_sha256":measured[-1]["measurements_sha256"]})
                labels[index]={"pressure_bar":index*10.+1,"se_bar":1/math.sqrt(3),"seeds":measured}
            pressure=np.arange(33)[:,None]*10.+np.arange(3)[None,:]
            arrays={"condition_index":np.arange(33),"temperature_kelvin":np.array([row["temperature_kelvin"] for row in conditions]),
                    "density_g_cm3":np.array([row["density_g_cm3"] for row in conditions]),"role_code":np.array([roles.index(row["role"]) for row in conditions]),
                    "replicate_seeds":np.array([[110000+1000*index+replica for replica in range(3)] for index in range(33)]),
                    "seed_pressure_bar":pressure,"mean_pressure_bar":np.arange(33)*10.+1,"seed_sd_bar":np.ones(33),"seed_se_bar":np.full(33,1/math.sqrt(3)),
                    "p0_bar":np.full(33,100.),"Z":(np.arange(33)*10.+1)/100}
            metadata={"study_id":"synthetic","proposal_sha256":independent.PROPOSAL,"protocol_sha256":"fixture","split_sha256":"fixture-split",
                      "conditions":conditions,"units":{"temperature":"K","density":"g/cm^3","pressure":"bar","Z":"1"},
                      "role_codes":{str(index):name for index,name in enumerate(roles)},"seeds":seeds,"source_shards":[{"sha256":"fixture-shard"}]}
            def receipt():
                np.savez(root/"dataset.npz",**arrays)
                metadata["dataset_sha256"]=assembler.hash_file(root/"dataset.npz")
                (root/"dataset.json").write_bytes(raw(metadata))
                return {"dataset":{"path":"dataset.npz","sha256":metadata["dataset_sha256"]},
                        "dataset_metadata":{"path":"dataset.json","sha256":assembler.hash_file(root/"dataset.json")},
                        "split":{"sha256":"fixture-split"},"role_shards":[{"sha256":"fixture-shard"}]}
            self.assertEqual(independent.check_dataset(root,receipt(),split,labels,runs)["seed_rows"],99)
            arrays["mean_pressure_bar"][5]+=1
            with self.assertRaisesRegex(AssertionError,"independently reduced"):
                independent.check_dataset(root,receipt(),split,labels,runs)
            arrays["mean_pressure_bar"][5]-=1;metadata["seeds"][0]["solver_job_id"]="wrong-source"
            with self.assertRaisesRegex(AssertionError,"lineage"):
                independent.check_dataset(root,receipt(),split,labels,runs)


if __name__=="__main__":
    unittest.main(verbosity=2)
