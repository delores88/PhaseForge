#!/usr/bin/env python3
"""Source-only real OpenMM retention check; no application, model or installation calls."""
from __future__ import annotations
import argparse
import ctypes as c
from ctypes import wintypes as w
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

import numpy as np
import openmm

PARAMETERS = dict(atom_count=32, temperature_kelvin=120.0, density_g_cm3=0.8,
                  seed=314159, timestep_fs=1.0, steps=500000, sample_interval=20,
                  chunk_frames=100, cpu_threads=1, platform="CPU", thermostat="langevin",
                  friction_per_ps=1.0)
CRITERIA = dict(expected_frames=25001, expected_steps=500000, expected_time_ps=500.0,
                time_error_ps=1e-6, energy_error_kj_mol=2e-4,
                force_error_kj_mol_nm=3e-3, pressure_error_bar=1e-8,
                msd_error_nm2=1e-12, maximum_worker_wall_seconds=480)

def encode(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True, allow_nan=False).encode()

def read(path):
    return json.loads(Path(path).read_text(encoding="utf-8"), parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))

def write(path, value):
    Path(path).write_bytes(encode(value))

def sha(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()

class BasicLimit(c.Structure):
    _fields_ = [("ProcessTime", c.c_int64), ("JobTime", c.c_int64), ("Flags", w.DWORD),
                ("MinWS", c.c_size_t), ("MaxWS", c.c_size_t), ("Active", w.DWORD),
                ("Affinity", c.c_size_t), ("Priority", w.DWORD), ("Scheduling", w.DWORD)]

class IO(c.Structure):
    _fields_ = [(name, c.c_uint64) for name in ("read_ops", "write_ops", "other_ops", "read_bytes", "write_bytes", "other_bytes")]

class ExtendedLimit(c.Structure):
    _fields_ = [("Basic", BasicLimit), ("IO", IO), ("ProcessMemory", c.c_size_t),
                ("JobMemory", c.c_size_t), ("PeakProcessMemory", c.c_size_t), ("PeakJobMemory", c.c_size_t)]

class Accounting(c.Structure):
    _fields_ = [("user", c.c_int64), ("kernel", c.c_int64), ("period_user", c.c_int64), ("period_kernel", c.c_int64),
                ("faults", w.DWORD), ("total_processes", w.DWORD), ("active_processes", w.DWORD), ("terminated_processes", w.DWORD)]

class JobIO(c.Structure):
    _fields_ = [("accounting", Accounting), ("io", IO)]

class Memory(c.Structure):
    _fields_ = [("cb", w.DWORD), ("faults", w.DWORD)] + [(name, c.c_size_t) for name in
        ("peak_working_set", "working_set", "peak_paged", "paged", "peak_nonpaged", "nonpaged", "pagefile", "peak_pagefile", "private_bytes")]

class Supervisor:
    def __init__(self):
        self.k = c.WinDLL("kernel32", use_last_error=True)
        self.p = c.WinDLL("psapi", use_last_error=True)
        self.k.CreateJobObjectW.argtypes = [c.c_void_p, w.LPCWSTR]; self.k.CreateJobObjectW.restype = w.HANDLE
        self.k.SetInformationJobObject.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD]
        self.k.AssignProcessToJobObject.argtypes = [w.HANDLE, w.HANDLE]
        self.k.TerminateJobObject.argtypes = [w.HANDLE, w.UINT]
        self.k.CloseHandle.argtypes = [w.HANDLE]
        self.k.QueryInformationJobObject.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)]
        self.k.OpenProcess.argtypes = [w.DWORD, w.BOOL, w.DWORD]; self.k.OpenProcess.restype = w.HANDLE
        self.k.GetProcessTimes.argtypes = [w.HANDLE] + [c.POINTER(w.FILETIME)] * 4
        self.k.GetProcessIoCounters.argtypes = [w.HANDLE, c.POINTER(IO)]
        self.p.GetProcessMemoryInfo.argtypes = [w.HANDLE, c.POINTER(Memory), w.DWORD]
        self.job = self.k.CreateJobObjectW(None, None)
        if not self.job: raise c.WinError(c.get_last_error())
        limit = ExtendedLimit(); limit.Basic.Flags = 0x2000  # KILL_ON_JOB_CLOSE
        if not self.k.SetInformationJobObject(self.job, 9, c.byref(limit), c.sizeof(limit)):
            self.close(); raise c.WinError(c.get_last_error())

    def attach(self, process):
        if not self.k.AssignProcessToJobObject(self.job, w.HANDLE(int(process._handle))):
            process.kill(); process.wait(); raise c.WinError(c.get_last_error())

    def metrics(self, process):
        # Windows venv python.exe is a redirector. Its own counters omit the
        # numerical child, so measure every member of the assigned Job Object.
        ids=c.create_string_buffer(8+1024*c.sizeof(c.c_size_t)); accounting=JobIO(); limits=ExtendedLimit()
        for kind,buffer in [(3,ids),(8,accounting),(9,limits)]:
            if not self.k.QueryInformationJobObject(self.job,kind,c.byref(buffer),c.sizeof(buffer),None): raise c.WinError(c.get_last_error())
        count=c.c_uint32.from_buffer(ids,4).value; processes=[]
        for pid in (c.c_size_t*count).from_buffer(ids,8):
            handle=self.k.OpenProcess(0x1000|0x10,False,pid)
            if not handle: continue  # A member can exit between enumeration and sampling.
            try:
                memory=Memory();memory.cb=c.sizeof(memory)
                if self.p.GetProcessMemoryInfo(handle,c.byref(memory),memory.cb):
                    processes.append(dict(pid=pid,working_set_bytes=memory.working_set,peak_working_set_bytes=memory.peak_working_set,private_bytes=memory.private_bytes))
            finally: self.k.CloseHandle(handle)
        return dict(processes=processes,tree_working_set_bytes=sum(row["working_set_bytes"] for row in processes),
                    largest_process_peak_working_set_bytes=max((row["peak_working_set_bytes"] for row in processes),default=0),
                    tree_private_bytes=sum(row["private_bytes"] for row in processes),job_peak_commit_bytes=limits.PeakJobMemory,
                    job_peak_process_commit_bytes=limits.PeakProcessMemory,total_processes=accounting.accounting.total_processes,
                    cpu_user_seconds=accounting.accounting.user/1e7,cpu_kernel_seconds=accounting.accounting.kernel/1e7,
                    io_read_bytes=accounting.io.read_bytes,io_write_bytes=accounting.io.write_bytes,io_write_operations=accounting.io.write_ops)

    def close(self):
        if self.job: self.k.CloseHandle(self.job); self.job = None

def launch(root, worker):
    output = root / "output"; supervisor = Supervisor(); started = time.monotonic(); process = None
    records = []; stopped = False; final = None
    try:
        with (root / "stdout.log").open("wb") as stdout, (root / "stderr.log").open("wb") as stderr:
            process = subprocess.Popen([sys.executable, str(worker), "--input", str(root / "input.json"), "--output", str(output)],
                                       cwd=root, stdout=stdout, stderr=stderr, creationflags=subprocess.CREATE_NO_WINDOW)
            supervisor.attach(process)
            with (root / "resources.jsonl").open("w", encoding="utf-8") as telemetry:
                while True:
                    elapsed = time.monotonic() - started
                    sample = dict(wall_seconds=elapsed, **supervisor.metrics(process))
                    try: sample["progress"] = read(output / "progress.json")
                    except (OSError, ValueError): pass
                    records.append(sample); telemetry.write(encode(sample).decode() + "\n"); telemetry.flush()
                    if len(records) == 1 or len(records) % 15 == 0:
                        print(json.dumps(dict(event="worker_progress", **sample)), flush=True)
                    code = process.poll()
                    if code is not None: final = sample; break
                    if elapsed >= 470 and not stopped:
                        (output / "cancel.request").write_text("extended component wall limit", encoding="utf-8"); stopped = True
                    if elapsed >= 480:
                        supervisor.k.TerminateJobObject(supervisor.job, 124); process.wait(timeout=5); final = sample; break
                    time.sleep(min(2, max(.05, 480 - elapsed)))
        return dict(exit_code=process.returncode, elapsed_seconds=time.monotonic()-started,
                    cooperative_timeout_requested=stopped, final=final,
                    peak_sampled_tree_working_set_bytes=max(x["tree_working_set_bytes"] for x in records),
                    largest_process_peak_working_set_bytes=max(x["largest_process_peak_working_set_bytes"] for x in records),
                    peak_sampled_tree_private_bytes=max(x["tree_private_bytes"] for x in records),
                    peak_job_commit_bytes=max(x["job_peak_commit_bytes"] for x in records),
                    supervision="Windows Job Object KILL_ON_JOB_CLOSE; cooperative stop at 470 s, hard stop at 480 s")
    finally:
        supervisor.close()
        if process is not None and process.poll() is None: process.wait(timeout=5)

def verify(root, reference, existing_output=None):
    output=Path(existing_output) if existing_output is not None else root/"output"; manifest=read(output/"manifest.json"); index=read(output/"trajectory/index.json")
    measured=read(output/"measurements.json"); result=read(output/"result.json"); rows=measured["series"]
    expected=CRITERIA["expected_frames"]; selected=set(np.linspace(0, expected-1, 65, dtype=int).tolist()) | {19999, 20000, 20001, 24999, 25000}
    errors=[]; sampled=[]; count=0; previous=-1; initial=None; observed_chunks={}; box=manifest["model"]["box_nm"][0]; cutoff=manifest["model"]["cutoff_nm"]
    def require(ok, message):
        if not bool(ok): errors.append(message)
    require(manifest["parameters"] == PARAMETERS, "Manifest parameters differ from frozen requested case")
    require(manifest["worker_sha256"] == sha(root/"source/scientific_worker.py"), "Worker source hash differs")
    require(manifest["input_sha256"] == hashlib.sha256(encode({"engine":"openmm_argon","parameters":PARAMETERS})).hexdigest(), "Normalized input hash differs")
    require(manifest["platform"] == "CPU" and manifest["platform_properties"].get("Threads") == "1", "Requested CPU/one-thread platform was not used")
    require(result["status"] == "completed" and result["completed_steps"] == 500000, "Worker did not complete requested integration")
    require(index["frame_count"] == expected and len(rows) == expected and result["frame_count"] == expected, "Declared retained counts disagree")
    require(abs(index["start_time"]) <= 1e-12 and abs(index["end_time"]-500) <= 1e-6, "Trajectory time endpoints disagree")
    with (root/"frame-hashes.jsonl").open("w",encoding="utf-8") as frame_hashes:
        for chunk in index["chunks"]:
            payload=output/chunk["path"]; arrays_path=output/chunk["arrays_path"]
            require(sha(payload)==chunk["sha256"], f"Chunk hash mismatch: {chunk['path']}")
            require(sha(arrays_path)==chunk["arrays_sha256"], f"NPZ hash mismatch: {chunk['arrays_path']}")
            frames=read(payload)["frames"]; require(chunk["start_frame"]==count and chunk["end_frame"]==count+len(frames)-1 and chunk["frame_count"]==len(frames), f"Chunk range mismatch: {chunk['path']}")
            require(len(frames)<=100, "Chunk exceeds requested retained-state bound")
            with np.load(arrays_path, allow_pickle=False) as arrays:
                pp=arrays["positions_unwrapped_nm"]; vv=arrays["velocities_nm_ps"]; ff=arrays["forces_kj_mol_nm"]
                require(pp.shape==vv.shape==ff.shape==(len(frames),32,3), "State-array shape mismatch")
                require(np.isfinite(pp).all() and np.isfinite(vv).all() and np.isfinite(ff).all(), "Nonfinite retained state")
                for local,(frame,p,v,force) in enumerate(zip(frames,pp,vv,ff,strict=True)):
                    number=count+local; step=frame["step"]; row=rows[number]; time_ps=frame["time"]
                    require(step==number*20 and step>previous and int(arrays["steps"][local])==step and row["step"]==step, f"State ordering mismatch at {number}")
                    require(time_ps==float(arrays["time_ps"][local])==row["time_ps"] and abs(time_ps-step*.001)<=1e-6, f"State time mismatch at {number}")
                    wrapped=np.array([entity["position"] for entity in frame["entities"]]); require(np.array_equal(wrapped,np.mod(p,box)),f"Wrapped state differs at {number}")
                    require([entity["id"] for entity in frame["entities"]]==[f"ar-{i:04d}" for i in range(32)],f"Entity identity mismatch at {number}")
                    frame_hash=hashlib.sha256(encode(frame)).hexdigest(); frame_hashes.write(encode(dict(frame=number,step=step,time_ps=time_ps,sha256=frame_hash)).decode()+"\n")
                    if initial is None: initial=p.copy()
                    if number in selected:
                        energy,forces,virial,_=reference.pair_reference(p,box,cutoff)
                        pressure=(2*row["kinetic_energy_kj_mol"]+virial)/(3*box**3)*(1e25/6.02214076e23)
                        checks=dict(frame=number,step=step,time_ps=time_ps,energy_error_kj_mol=abs(energy-row["potential_energy_kj_mol"]),force_error_kj_mol_nm=float(np.max(np.abs(forces-force))),pressure_error_bar=abs(pressure-row["pressure_bar"]),msd_error_nm2=abs(float(np.mean(np.sum((p-initial)**2,axis=1)))-row["msd_nm2"]))
                        sampled.append(checks)
                        for key in ["energy_error_kj_mol","force_error_kj_mol_nm","pressure_error_bar","msd_error_nm2"]: require(checks[key]<=CRITERIA[key],f"Independent {key} exceeded frozen tolerance at frame {number}: {checks[key]}")
                    previous=step
                observed_chunks[chunk["path"]]=(chunk["sha256"],hashlib.sha256(encode(frames[-1])).hexdigest(),len(frames)-1,frames[-1]["step"])
            require(chunk["start_step"]==frames[0]["step"] and chunk["end_step"]==frames[-1]["step"] and chunk["start_time"]==frames[0]["time"] and chunk["end_time"]==frames[-1]["time"], "Chunk step/time endpoints mismatch")
            count+=len(frames)
    require(count==expected and count>20000 and previous==500000, "Retained states do not exceed the old ceiling at the requested endpoint")
    observations=read(output/"observations/index.json")["images"]
    for image in observations:
        source=observed_chunks[image["source_chunk"]]
        require(sha(output/image["path"])==image["sha256"] and source==(image["source_chunk_sha256"],image["source_frame_sha256"],image["source_frame_index"],image["step"]), "Observation hash/source binding mismatch")
    checkpoint=read(output/"checkpoint.json"); require(checkpoint["step"]==500000 and sha(output/checkpoint["checkpoint_path"])==checkpoint["checkpoint_sha256"],"Final checkpoint hash or endpoint mismatch")
    inventory=[]
    for path in sorted(output.rglob("*")):
        if path.is_file(): inventory.append(dict(path=path.relative_to(output).as_posix(),bytes=path.stat().st_size,sha256=sha(path)))
    write(root/"artifact-hashes.json",inventory)
    maxima={key:max(row[key] for row in sampled) for key in ["energy_error_kj_mol","force_error_kj_mol_nm","pressure_error_bar","msd_error_nm2"]}
    return dict(passed=not errors,errors=errors,frames=count,chunks=len(index["chunks"]),observations=len(observations),sampled_independent_frames=len(sampled),sampled=sampled,max_errors=maxima,
                file_count=len(inventory),disk_bytes=sum(row["bytes"] for row in inventory),final_step=previous,simulated_time_ps=result["simulated_time_ps"],worker_elapsed_seconds=result["elapsed_seconds"],manifest=manifest)

def main():
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument("--output",type=Path,required=True); args=parser.parse_args()
    root=args.output.resolve(); root.mkdir(parents=True,exist_ok=False); source=root/"source"; source.mkdir()
    tools=Path(__file__).resolve().parent
    for name in ["scientific_worker.py","check_scientific_worker.py","check_extended_trajectory.py","requirements-science.txt"]: shutil.copyfile(tools/name,source/name)
    report=dict(scope="Source component only; not installed application acceptance or empirical argon validation",parameters=PARAMETERS,criteria=CRITERIA,
                python=sys.executable,python_version=sys.version,openmm_version=openmm.version.version,numpy_version=np.__version__,started_at_unix=time.time(),source_hashes={path.name:sha(path) for path in source.iterdir()})
    write(root/"plan.json",report); write(root/"input.json",dict(engine="openmm_argon",parameters=PARAMETERS))
    try:
        report["execution"]=launch(root,source/"scientific_worker.py"); write(root/"execution.json",report["execution"])
        if report["execution"]["exit_code"]!=0: raise RuntimeError(f"Worker exited {report['execution']['exit_code']}; outputs and logs preserved")
        spec=importlib.util.spec_from_file_location("independent_lj_reference",source/"check_scientific_worker.py"); reference=importlib.util.module_from_spec(spec); spec.loader.exec_module(reference)
        started=time.monotonic(); report["verification"]=verify(root,reference); report["verification_seconds"]=time.monotonic()-started; report["passed"]=report["verification"]["passed"]
    except Exception as error:
        report["passed"]=False; report["failure"]=f"{type(error).__name__}: {error}"
    report["completed_at_unix"]=time.time(); write(root/"report.json",report)
    print(json.dumps(dict(event="complete",passed=report["passed"],report=str(root/"report.json"),failure=report.get("failure"),execution=report.get("execution"),verification={key:value for key,value in report.get("verification",{}).items() if key not in ("sampled","manifest")}),allow_nan=False),flush=True)
    return 0 if report["passed"] else 1

if __name__=="__main__": raise SystemExit(main())
