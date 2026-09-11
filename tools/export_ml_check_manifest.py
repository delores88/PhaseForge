"""Read-only receipt assembler for tools/check_ml_study.py.

Reads a completed study from a SQLite read transaction and streams the retained
stage/sweep artifact hashes. Writes only explicit audit output (job snapshots and
an audit manifest). It never runs solvers, fits models, repairs evidence or calls
the app. The independent checker performs the numerical audit afterward.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import sqlite3
import uuid


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def parse(raw):
    return json.loads(raw, parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def hash_file(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024*1024), b""):
            digest.update(block)
    return digest.hexdigest()


def raw_field(document, field):
    """Preserve Rust's original input bytes: do not reserialize floats/Unicode."""
    decoder = json.JSONDecoder()
    cursor = 0
    while cursor < len(document) and document[cursor].isspace():
        cursor += 1
    require(document[cursor:cursor+1] == "{", "Stored job must be an object")
    cursor += 1
    while True:
        while cursor < len(document) and document[cursor].isspace():
            cursor += 1
        require(document[cursor:cursor+1] not in ("", "}"), f"Stored job lacks {field}")
        key, cursor = decoder.raw_decode(document, cursor)
        while document[cursor].isspace():
            cursor += 1
        require(document[cursor] == ":", "Invalid stored object")
        cursor += 1
        while document[cursor].isspace():
            cursor += 1
        start = cursor
        _, cursor = decoder.raw_decode(document, cursor)
        if key == field:
            return document[start:cursor].encode("utf-8")
        while cursor < len(document) and document[cursor].isspace():
            cursor += 1
        require(document[cursor:cursor+1] == ",", f"Stored job lacks {field}")
        cursor += 1


def safe(root, relative):
    require(isinstance(relative, str) and relative and ":" not in relative, "Invalid relative artifact path")
    parts = relative.replace("\\", "/").split("/")
    require(all(part not in ("", ".", "..") for part in parts), "Unsafe artifact path")
    path = root.joinpath(*parts)
    require(path.resolve().is_relative_to(root.resolve()) and path.is_file(), "Artifact escaped its recorded job")
    require(not path.is_symlink() and path.stat().st_nlink == 1, "Linked artifacts are not admissible evidence")
    return path


class Evidence:
    def __init__(self, data_directory, study_id, output):
        self.data = data_directory.resolve()
        self.output = output.resolve()
        self.root = Path(os.path.commonpath([self.data, self.output]))
        self.study_id = str(uuid.UUID(study_id))
        database = self.data / "phaseforge.sqlite3"
        require(database.is_file(), "The selected data directory has no PhaseForge database")
        self.connection = sqlite3.connect(database.as_uri()+"?mode=ro", uri=True)
        self.connection.execute("PRAGMA query_only = ON")
        self.connection.execute("BEGIN")
        self.records = {}
        self.verified_files = {}
        self.stage_jobs = {}
        try:
            self.study = self.job(self.study_id)
            require(self.study["kind"] == "ml_study" and self.study["state"] == "completed", "Only a completed ML study can receive a full audit manifest")
        except Exception:
            self.connection.close()
            raise
        self.project_id = self.study["project_id"]

    def close(self):
        self.connection.close()

    def directory(self, job_id):
        return self.data / "artifacts" / "laboratory" / str(uuid.UUID(job_id))

    def job(self, job_id):
        job_id = str(uuid.UUID(job_id))
        if job_id not in self.records:
            row = self.connection.execute("SELECT json FROM objects WHERE kind='laboratory_job' AND id=?", (job_id,)).fetchone()
            require(row is not None, f"Missing retained job {job_id}")
            job = parse(row[0])
            require(job["id"] == job_id, "Stored job identity mismatch")
            self.records[job_id] = (job, row[0])
        return self.records[job_id][0]

    def input_hash(self, job_id):
        self.job(job_id)
        return hashlib.sha256(raw_field(self.records[job_id][1], "input")).hexdigest()

    def json_file(self, job_id, relative):
        return parse(safe(self.directory(job_id), relative).read_bytes())

    def verify_files(self, job_id, rows):
        require(isinstance(rows, list) and rows, "Missing retained artifact inventory")
        paths = set()
        for row in rows:
            relative = row["path"]
            require(relative not in paths, "Duplicate retained artifact path")
            paths.add(relative)
            path = safe(self.directory(job_id), relative)
            require(path.stat().st_size == row["bytes"] and hash_file(path) == row["sha256"], f"Retained artifact changed: {job_id}/{relative}")
            self.verified_files[job_id, relative] = row
        require("result.json" in paths, "Completed job has no result receipt")

    def verify_stage(self, name, stage):
        completion = stage.get("completion")
        require(isinstance(completion, dict), f"Study stage {name} is incomplete")
        job_id = completion["job_id"]
        job = self.job(job_id)
        require(job_id in stage["attempts"] and job["state"] == "completed" and job["kind"] == stage["kind"]
                and job["project_id"] == self.project_id and job["parent_id"] == self.study_id, "Completed study stage ownership changed")
        require(stage["input_sha256"] == completion["input_sha256"] == self.input_hash(job_id), "Frozen stage input changed")
        self.verify_files(job_id, completion["artifacts"])
        self.stage_jobs[name] = job_id
        return job

    def descriptor(self, job_id, relative):
        row = self.verified_files.get((job_id, relative))
        require(row is not None, f"Artifact is not in a verified completion receipt: {job_id}/{relative}")
        path = safe(self.directory(job_id), relative)
        return {"path":path.relative_to(self.root).as_posix(), "sha256":row["sha256"], "bytes":row["bytes"]}

    def build(self):
        ledger = self.json_file(self.study_id, "study-ledger.json")
        require(ledger["input_sha256"] == self.input_hash(self.study_id), "Frozen study input differs from original retained bytes")
        source_hash = self.study["input"]["worker_sha256"]
        solver_hash = self.study["input"]["solver_worker_sha256"]
        require(len(source_hash) == 64 and len(solver_hash) == 64, "Missing executed source hashes")
        for name, stage in ledger["stages"].items():
            self.verify_stage(name, stage)
        result = self.study["result"]
        require(self.json_file(self.study_id, "result.json") == result, "Final study result disagrees with its database receipt")
        for key in ["freeze_job_id", "fit_job_id", "calibration_job_id", "evaluation_job_id", "dataset_export_job_id"]:
            require(result[key] in self.stage_jobs.values(), f"Final {key} is not a completed registered stage")
        freeze, fit = result["freeze_job_id"], result["fit_job_id"]
        runs = self.json_file(freeze, "work/runs.json")["runs"]
        require(len(runs) == 99 and len({row["case_id"] for row in runs}) == 99, "Frozen study must retain all 99 unique cases")
        all_cases = {}
        for name, stage in ledger["stages"].items():
            if stage["kind"] != "sweep":
                continue
            sweep_id = stage["completion"]["job_id"]
            sweep_ledger = self.json_file(sweep_id, "sweep-ledger.json")
            require(sweep_ledger["input_sha256"] == self.input_hash(sweep_id), "Frozen sweep input changed")
            requested = {case["case_id"]:case for case in self.job(sweep_id)["input"]["cases"]}
            require(set(requested) == {case["case_id"] for case in sweep_ledger["cases"]}, "Retained sweep membership changed")
            for case in sweep_ledger["cases"]:
                completion = case["completion"]
                require(isinstance(completion, dict), "A missing or failed seed cannot enter an audit")
                solver_id = completion["job_id"]
                solver = self.job(solver_id)
                require(solver_id in case["attempts"] and solver["kind"] == "solver" and solver["state"] == "completed"
                        and solver["project_id"] == self.project_id and solver["parent_id"] == sweep_id, "Solver completion ownership changed")
                require(case["input_sha256"] == completion["input_sha256"] == self.input_hash(solver_id), "Frozen solver request changed")
                original = requested[case["case_id"]]
                require(solver["input"]["engine"] == original["engine"] and solver["input"]["parameters"] == original["parameters"], "Solver no longer matches its reserved sweep case")
                self.verify_files(solver_id, completion["artifacts"])
                require(case["case_id"] not in all_cases, "Duplicate completed solver case across sweeps")
                all_cases[case["case_id"]] = (solver_id, completion, case["attempts"])
        require(set(all_cases) == {row["case_id"] for row in runs}, "All and only the frozen dataset and OOD cases are required")
        manifest = {"schema_version":1, "root":str(self.root), "study_id":self.study_id,
                    "scope":"Verified receipt assembly only; run check_ml_study.py for independent numerical acceptance", "runs":[], "role_shards":[]}
        for key, source, path in [("split",freeze,"work/split.json"), ("model",fit,"work/model.npz"), ("model_card",fit,"work/model-card.json"),
                ("worker",fit,"work/experiment.py"), ("calibration",result["calibration_job_id"],"work/calibration.json"),
                ("evaluation",result["evaluation_job_id"],"work/evaluation.json"), ("costs",self.stage_jobs["measured-costs"],"bundle.json"),
                ("dataset",result["dataset_export_job_id"],"work/dataset.npz"), ("dataset_metadata",result["dataset_export_job_id"],"work/dataset.json")]:
            manifest[key] = self.descriptor(source, path)
        require(manifest["worker"]["sha256"] == source_hash, "The fitted worker differs from the frozen source")
        model_freeze = self.json_file(self.study_id,"model-freeze.json")
        require(model_freeze == result["model_freeze"] == ledger["model_freeze"], "The model freeze ledger and result disagree")
        require(model_freeze["model_sha256"] == manifest["model"]["sha256"] and model_freeze["model_card_sha256"] == manifest["model_card"]["sha256"]
                and model_freeze["split_sha256"] == manifest["split"]["sha256"], "Frozen model or split bytes changed")
        for name, stage in ledger["stages"].items():
            source = stage["completion"]["job_id"]
            job = self.job(source)
            if job["kind"] == "generated" and job["input"].get("inputs", {}).get("phase") == "reduce":
                require(job["input"]["code"].encode("utf-8") == safe(self.directory(fit),"work/experiment.py").read_bytes(), "A reducer used a different frozen worker")
                manifest["role_shards"].append(self.descriptor(source,"work/shard.json"))
        require(manifest["role_shards"], "Missing role-restricted measured reductions")
        records = self.output / "records"
        records.mkdir(parents=True, exist_ok=True)
        for row in runs:
            solver_id, completion, attempts = all_cases[row["case_id"]]
            solver = self.job(solver_id)
            require(solver["input"]["parameters"] == row["parameters"] and solver["input"]["engine"] == row["engine"], "Frozen run request differs from actual solver")
            require(self.verified_files[solver_id,"scientific_worker.py"]["sha256"] == solver_hash, "A label used changed scientific source")
            entry = dict(row)
            entry.update({"solver_job_id":solver_id, "attempt_lineage":attempts,
                          "directory":self.directory(solver_id).relative_to(self.root).as_posix(), "artifacts":completion["artifacts"]})
            entry["job_record"] = self.snapshot(solver_id, records)
            manifest["runs"].append(entry)
        manifest["study_record"] = self.snapshot(self.study_id, records)
        manifest["stage_records"] = [self.snapshot(job_id, records) for job_id in self.stage_jobs.values()]
        manifest["assembler_sha256"] = hash_file(__file__)
        manifest["source_hashes"] = {"worker":source_hash,"solver":solver_hash}
        manifest["verified_artifact_files"] = len(self.verified_files)
        path = self.output / "audit.json"
        path.write_text(json.dumps(manifest, sort_keys=True, indent=2, allow_nan=False), encoding="utf-8")
        return path

    def snapshot(self, job_id, records):
        path = records / f"{job_id}.json"
        raw = self.records[job_id][1].encode("utf-8")
        if path.exists():
            require(path.read_bytes() == raw, "Audit snapshot already exists with different job data; choose a fresh output directory")
        else:
            path.write_bytes(raw)
        return {"path":path.relative_to(self.root).as_posix(), "sha256":hashlib.sha256(raw).hexdigest(), "bytes":len(raw)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-directory", type=Path, required=True)
    parser.add_argument("--study-id", required=True)
    parser.add_argument("--output", type=Path, required=True, help="Explicit audit output directory; source records stay untouched")
    args = parser.parse_args()
    evidence = Evidence(args.data_directory,args.study_id,args.output)
    try:
        path = evidence.build()
        print(json.dumps({"assembled":True,"manifest":str(path),"verified_artifact_files":len(evidence.verified_files),
                          "scientific_acceptance":False,"next":"Run check_ml_study.py --manifest <audit.json> --output <report.json>"}))
    finally:
        evidence.close()


if __name__ == "__main__":
    main()
