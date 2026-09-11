#!/usr/bin/env python3
"""Retry only documented operational timeouts with unchanged frozen science.

The original checker source is loaded by its recorded SHA256. Its numerical,
invariant, image and comparison functions are reused without changed tolerances.
Passing solver executions are never repeated. Their arrays are independently
inspected again; passing recovery/invalid-input groups retain their prior receipt.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import time
import traceback


def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--previous", type=Path, required=True)
    parser.add_argument("--plan", type=Path, required=True)
    parser.add_argument("--worker", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    previous, output, worker, plan_path = [p.resolve() for p in
        (args.previous, args.output, args.worker, args.plan)]
    prior, plan = read(previous / "report.json"), read(plan_path)
    planned_previous = Path(__file__).resolve().parents[1] / plan["original_evidence"]
    require(previous == planned_previous.resolve(), "Previous attempt is not the one named in the frozen operational plan")
    require("passed" in prior, "Wait until the original acceptance attempt completes")
    require(not output.exists(), "Use a fresh retry output directory; prior evidence is immutable")
    checker_source = previous / "checker-source.py"
    proposal = Path(__file__).resolve().parents[1] / "docs/validation/mechanics-lab-proposal.md"
    require(sha(worker) == prior["worker_sha256"] == plan["worker_sha256"], "Worker source changed")
    require(sha(checker_source) == prior["checker_sha256"] == plan["checker_sha256"], "Frozen checker source changed")
    require(sha(proposal) == prior["criteria_sha256"] == plan["scientific_criteria_sha256"], "Scientific criteria changed")
    require(plan["retry_process_timeout_seconds"] == 300, "Expected the explicitly preregistered operational budget")
    spec = importlib.util.spec_from_file_location("frozen_mechanics_acceptance", checker_source)
    checker = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(checker)
    checker.PROPOSAL = proposal
    output.mkdir(parents=True)
    for source, destination in [(Path(__file__), "retry-source.py"), (checker_source, "checker-source.py"),
            (previous / "report.json", "prior-report.json"), (plan_path, "operational-plan.json"),
            (proposal, "criteria.md"), (worker, "worker-source.py")]:
        shutil.copy2(source, output / destination)
    reused, retried = [], []
    fixed_cases = {case["name"]: case for case in checker.build_cases()}

    class RetryRunner(checker.Runner):
        def launch(self, case):
            name = case["name"]
            require(name in fixed_cases, "Only fixed timed-out cases are eligible for this retry helper")
            old_group = prior["tests"][name]
            old_folder = previous / name
            inp = old_folder / "input.json"
            original = inp.read_bytes()
            require(original == checker.canonical({"engine": checker.ENGINE, "parameters": case["parameters"]}),
                    f"Frozen input bytes changed: {name}")
            receipt = read(old_folder / "process.receipt.json")
            require(receipt["input_sha256"] == sha(inp) and receipt["worker_sha256"] == sha(worker),
                    f"Original process identity changed: {name}")
            if old_group["passed"]:
                require(receipt.get("exit_code") == 0 and not receipt.get("timed_out"),
                        f"Passing receipt is inconsistent: {name}")
                reused.append(name)
                return old_folder / "output", {**receipt, "reused_original_execution": True}
            require(receipt.get("timed_out") is True,
                    f"Non-timeout failure is ineligible for operational retry: {name}")
            folder, new_input, out = self.prepare(name, case["parameters"], raw=original)
            retried.append(name)
            report["worker_executed"] = True
            result = self.execute(folder, new_input, out)
            require(result.get("exit_code") == 0 and not result.get("timed_out"), f"Retried worker failed: {folder}")
            return out, result

    # Passing auxiliary groups are evidence already established by the frozen
    # attempt. An incomplete multi-process recovery procedure requires a separate
    # continuation plan; this helper must not repeat its successful subprocesses.
    for function_name, group_name in [
            ("recovery_check", "checkpoint-recovery"),
            ("imported_state_check", "numeric-initial-state"),
            ("invalid_checks", "invalid-inputs")]:
        def auxiliary(runner, group_name=group_name):
            entry = prior["tests"][group_name]
            if entry["passed"]:
                reused.append(group_name)
                return copy.deepcopy(entry["result"])
            raise RuntimeError(f"Auxiliary failure needs its own preserved continuation evidence; no supporting subprocesses were rerun: {group_name}")
        setattr(checker, function_name, auxiliary)

    report = {"schema_version": 1, "mode": "operational-timeout-retry",
        "started_unix_s": time.time(), "worker_sha256": sha(worker),
        "checker_sha256": sha(checker_source), "retry_checker_sha256": sha(Path(__file__)),
        "criteria_sha256": sha(proposal), "prior_report_sha256": sha(previous / "report.json"),
        "prior_directory": str(previous), "operational_plan_sha256": sha(plan_path),
        "process_timeout_seconds": 300, "reused_executions_or_auxiliary_groups": reused,
        "retry_cases_or_auxiliary_groups": retried, "worker_executed": False,
        "numerical_acceptance_passed": False, "installed_acceptance": "not evaluated", "tests": {}}
    started = time.monotonic()
    try:
        report["tests"]["pure-references"] = checker.reference_checks()
        checker.execute_suite(RetryRunner(output, worker, sys.executable, 300), report)
        report["worker_executed"] = bool(retried)
        report["passed"] = report["numerical_acceptance_passed"] = all(test["passed"] for test in report["tests"].values())
    except Exception as error:
        report.update(passed=False, error=str(error), traceback=traceback.format_exc())
    finally:
        report["elapsed_s"] = time.monotonic() - started
        checker.write(output / "report.json", report)
    print(json.dumps({"report": str(output / "report.json"), "passed": report["passed"],
                      "retried": retried, "reused": reused}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
