#!/usr/bin/env python3
"""Read-only verification of an existing installed extended argon trajectory.

This entry point never launches a solver or calls a model. It preserves checker
sources and the actual retained worker bytes, then runs the previously frozen
extended-state verifier directly against the installed job directory.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import time
import traceback


def module(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--job-directory", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    job, root = args.job_directory.resolve(), args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    source = root / "source"
    source.mkdir()
    tools = Path(__file__).resolve().parent
    for name in ("check_installed_extended.py", "check_extended_trajectory.py", "check_scientific_worker.py", "requirements-science.txt"):
        shutil.copy2(tools / name, source / name)
    shutil.copy2(job / "scientific_worker.py", source / "scientific_worker.py")
    extended = module(source / "check_extended_trajectory.py", "frozen_extended_check")
    reference = module(source / "check_scientific_worker.py", "independent_pair_reference")
    report = {"scope": "Independent numerical and artifact audit of an existing installed job; no solver/model execution or native UI operation",
        "solver_executed": False, "job_directory": str(job), "job_id": job.name,
        "started_unix_s": time.time(), "parameters": extended.PARAMETERS, "criteria": extended.CRITERIA,
        "source_sha256": {path.name: digest(path) for path in source.iterdir() if path.is_file()}}
    extended.write(root / "audit-plan.json", report)
    started = time.monotonic()
    try:
        report["verification"] = extended.verify(root, reference, existing_output=job)
        report["passed"] = report["verification"]["passed"]
    except Exception as error:
        report.update(passed=False, failure=str(error), traceback=traceback.format_exc())
    report["elapsed_s"] = time.monotonic() - started
    extended.write(root / "report.json", report)
    print(json.dumps({"passed": report["passed"], "report": str(root / "report.json"),
                      "solver_executed": False, "failure": report.get("failure")}, allow_nan=False))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
