#!/usr/bin/env python3
"""Independent numerical acceptance checks for the trusted argon lab.

Criteria are frozen in docs/validation/molecular-lab.md. Reference energy/forces,
pressure, RDF and MSD do not import worker implementations. Calls execute real
OpenMM and worker subprocesses; there are no mocked scientific outcomes.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys
import time

import numpy as np
import openmm as mm
from openmm import unit
from PIL import Image

SIGMA, EPSILON, MASS = .3405, .997, 39.948
NA = 6.02214076e23


def encode(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=True, allow_nan=False).encode()


def sha(v):
    return hashlib.sha256(v).hexdigest()


def read(path):
    return json.loads(Path(path).read_text(), parse_constant=lambda s: (_ for _ in ()).throw(ValueError(s)))


def pair_reference(points, box, cutoff):
    """Scalar independent implementation, full force vector and pair virial."""
    energy = 0.0
    virial = 0.0
    forces = np.zeros_like(points)
    distances = []
    for i in range(len(points)):
        for j in range(i):
            d = points[i] - points[j]
            d -= box * np.round(d/box)
            r = float(np.linalg.norm(d))
            distances.append(r)
            if r >= cutoff:
                continue
            q = SIGMA/r
            u = 4*EPSILON*(q**12-q**6)
            dudr = 24*EPSILON*(q**6-2*q**12)/r
            switch, derivative = 1.0, 0.0
            if r > .8*cutoff:
                x = (r-.8*cutoff)/(.2*cutoff)
                switch = 1-6*x**5+15*x**4-10*x**3
                derivative = (-30*x**4+60*x**3-30*x**2)/(.2*cutoff)
            f = -(switch*dudr + u*derivative)*d/r
            forces[i] += f
            forces[j] -= f
            energy += u*switch
            virial += float(np.dot(d, f))
    return energy, forces, virial, distances


def frozen_test():
    points = np.array([[.05,.5,.5], [.43,.5,.5], [1.18,.5,.5], [2.72,.5,.5]], dtype=float)
    box, cutoff = 3.0, 2.5*SIGMA
    expected_energy, expected_force, virial, _ = pair_reference(points, box, cutoff)
    h = 1e-6
    fd = np.zeros_like(points)
    for i in range(len(points)):
        for a in range(3):
            plus, minus = points.copy(), points.copy()
            plus[i,a] += h; minus[i,a] -= h
            fd[i,a] = -(pair_reference(plus,box,cutoff)[0]-pair_reference(minus,box,cutoff)[0])/(2*h)
    finite_error = float(np.max(np.abs(fd-expected_force)))
    results = {"coordinates_nm": points.tolist(), "box_nm": box, "cutoff_nm": cutoff,
               "independent_energy_kj_mol": expected_energy, "independent_virial_kj_mol": virial,
               "finite_difference_max_force_error": finite_error,
               "finite_difference_tolerance": 1e-5, "platforms": []}
    passed = finite_error <= 1e-5
    for name, etol, ftol in [("Reference",1e-8,1e-7), ("CPU",2e-4,3e-3)]:
        system, force = mm.System(), mm.NonbondedForce()
        system.setDefaultPeriodicBoxVectors(mm.Vec3(box,0,0),mm.Vec3(0,box,0),mm.Vec3(0,0,box))
        force.setNonbondedMethod(mm.NonbondedForce.CutoffPeriodic)
        force.setCutoffDistance(cutoff)
        force.setUseSwitchingFunction(True); force.setSwitchingDistance(.8*cutoff)
        force.setUseDispersionCorrection(False)
        for _ in points:
            system.addParticle(MASS); force.addParticle(0,SIGMA,EPSILON)
        system.addForce(force)
        integrator = mm.VerletIntegrator(.001)
        properties = {"Threads":"1", "DeterministicForces":"true"} if name=="CPU" else {}
        context = mm.Context(system,integrator,mm.Platform.getPlatformByName(name),properties)
        context.setPositions(points)
        state = context.getState(getEnergy=True,getForces=True)
        energy = float(state.getPotentialEnergy().value_in_unit(unit.kilojoule_per_mole))
        forces = state.getForces(asNumpy=True).value_in_unit(unit.kilojoule_per_mole/unit.nanometer)
        ee, fe = abs(energy-expected_energy), float(np.max(np.abs(forces-expected_force)))
        ok = ee <= etol and fe <= ftol
        results["platforms"].append({"platform":name,"energy_error":ee,"force_error":fe,
                                      "energy_tolerance":etol,"force_tolerance":ftol,"passed":ok})
        passed &= ok
        del context, integrator
    results["passed"] = bool(passed)
    return results


def launch(worker, root, label, parameters, expected=0):
    folder = root/label
    folder.mkdir()
    inp = folder/"input.json"
    inp.write_bytes(encode({"engine":"openmm_argon","parameters":parameters}))
    run_dir = folder/"output"
    proc = subprocess.run([sys.executable,str(worker),"--input",str(inp),"--output",str(run_dir)],
                          capture_output=True,text=True,timeout=240)
    if proc.returncode != expected:
        raise RuntimeError(f"{label}: exit {proc.returncode}: {proc.stderr[-1600:]}")
    return run_dir, proc


def artifact_test(run_dir):
    manifest, index, m = [read(run_dir/p) for p in ["manifest.json","trajectory/index.json","measurements.json"]]
    box = manifest["model"]["box_nm"][0]
    cutoff = manifest["model"]["cutoff_nm"]
    frames, positions, velocities = [], [], []
    hash_ok, coordinates_ok = True, True
    for chunk in index["chunks"]:
        data, arrays_data = (run_dir/chunk["path"]).read_bytes(), (run_dir/chunk["arrays_path"]).read_bytes()
        hash_ok &= sha(data)==chunk["sha256"] and sha(arrays_data)==chunk["arrays_sha256"]
        chunk_frames = json.loads(data)["frames"]
        with np.load(run_dir/chunk["arrays_path"], allow_pickle=False) as arrays:
            pp, vv = arrays["positions_unwrapped_nm"], arrays["velocities_nm_ps"]
            for f, p, v in zip(chunk_frames,pp,vv,strict=True):
                wrapped = np.array([x["position"] for x in f["entities"]])
                coordinates_ok &= bool(np.array_equal(wrapped,np.mod(p,box)))
                frames.append(f); positions.append(p.copy()); velocities.append(v.copy())
    pressure_errors, msd_errors, engine_energy_errors = [], [], []
    rdf_counts = np.zeros(len(m["rdf"]["g_r"]),dtype=int)
    # All saved samples participate, independent scalar pair loop.
    for p,row in zip(positions,m["series"],strict=True):
        energy,_,virial,distances = pair_reference(p,box,cutoff)
        pressure = (2*row["kinetic_energy_kj_mol"]+virial)/(3*box**3)*(1e25/NA)
        pressure_errors.append(abs(pressure-row["pressure_bar"]))
        engine_energy_errors.append(abs(energy-row["potential_energy_kj_mol"]))
        msd_errors.append(abs(float(np.mean(np.sum((p-positions[0])**2,axis=1)))-row["msd_nm2"]))
        rdf_counts += np.histogram(distances,bins=m["rdf"]["edges_nm"])[0]
    images_ok = True
    for img in read(run_dir/"observations/index.json")["images"]:
        path = run_dir/img["path"]
        source = read(run_dir/img["source_chunk"])["frames"][img["source_frame_index"]]
        images_ok &= sha(path.read_bytes())==img["sha256"] and sha(encode(source))==img["source_frame_sha256"]
        with Image.open(path) as image:
            images_ok &= image.size==(1024,1024) and image.format=="PNG"
    momentum = np.asarray(velocities).sum(axis=1)*MASS
    momentum_drift = float(np.max(np.linalg.norm(momentum-momentum[0],axis=1)))
    checks = {"chunk_hashes":hash_ok,"wrapped_positions_exact":coordinates_ok,"projected_images":images_ok,
              "frame_count":len(frames),"declared_frame_count":index["frame_count"],
              "pressure_max_error_bar":max(pressure_errors),"msd_max_error_nm2":max(msd_errors),
              "independent_energy_max_error_kj_mol":max(engine_energy_errors),
              "rdf_counts_equal":bool(np.array_equal(rdf_counts,m["rdf"]["pair_counts"])),
              "momentum_max_drift_dalton_nm_ps":momentum_drift}
    checks["passed"] = bool(hash_ok and coordinates_ok and images_ok and len(frames)==index["frame_count"]
                            and max(pressure_errors)<1e-8 and max(msd_errors)<1e-12 and checks["rdf_counts_equal"])
    return checks


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output",required=True,type=Path)
    args=parser.parse_args()
    root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
    worker=Path(__file__).with_name("scientific_worker.py").resolve()
    plan=Path(__file__).parents[1]/"docs/validation/molecular-lab.md"
    report={"schema_version":1,"worker_sha256":sha(worker.read_bytes()),"criteria_sha256":sha(plan.read_bytes()),
            "openmm_version":mm.version.version,"tests":{},"started_at_unix":time.time()}
    try:
        report["tests"]["frozen_reference"]=frozen_test()
        baseline={"atom_count":32,"temperature_kelvin":100,"density_g_cm3":.8,
                  "seed":712,"platform":"Reference","thermostat":"nve","sample_interval":10,"chunk_frames":20}
        coarse,_=launch(worker,root,"nve-2fs",{**baseline,"timestep_fs":2,"steps":250})
        fine,_=launch(worker,root,"nve-1fs",{**baseline,"timestep_fs":1,"steps":500})
        c,f=[read(x/"measurements.json") for x in (coarse,fine)]
        errors=[x["summary"]["max_energy_deviation_kj_mol"] for x in (c,f)]
        relative=[error/max(x["series"][0]["kinetic_energy_kj_mol"],1) for error,x in zip(errors,(c,f))]
        integrity=artifact_test(fine)
        report["tests"]["nve_refinement"]={"coarse_max_energy_error":errors[0],"fine_max_energy_error":errors[1],
          "relative_errors":relative,"relative_limit":.005,"fine_to_coarse_limit":.6,
          "momentum_max_drift":integrity["momentum_max_drift_dalton_nm_ps"],
          "passed":max(relative)<.005 and (errors[1]<=.6*errors[0] or max(errors)<1e-7)
                    and integrity["momentum_max_drift_dalton_nm_ps"]<1e-8}
        report["tests"]["artifacts_instruments"]=integrity
        means=[]
        for temp,seed in [(90,812),(180,813)]:
            folder,_=launch(worker,root,f"temperature-{temp}",{"atom_count":108,"temperature_kelvin":temp,
                  "density_g_cm3":.8,"seed":seed,"platform":"CPU","thermostat":"langevin",
                  "friction_per_ps":5,"timestep_fs":1,"steps":10000,"sample_interval":100,"chunk_frames":50})
            rows=read(folder/"measurements.json")["series"]
            means.append(float(np.mean([r["temperature_kelvin"] for r in rows if r["time_ps"]>=5-1e-9])))
        report["tests"]["controlled_temperature"]={"targets":[90,180],"late_mean_kelvin":means,
            "relative_tolerance":.15,"minimum_difference_kelvin":60,
            "passed":all(abs(a-b)/b<.15 for a,b in zip(means,(90,180))) and means[1]-means[0]>60}
        invalid_results=[]
        for i,p in enumerate([{"temperature_kelvin":-1},{"temperature_kelvin":float("inf")},
                              {"unrecognized":1},{"atom_count":513},{"steps":200001},{"platform":"missing"}]):
            directory=root/f"invalid-{i}";directory.mkdir()
            inp=directory/"input.json";inp.write_text(json.dumps({"engine":"openmm_argon","parameters":p}))
            result=subprocess.run([sys.executable,str(worker),"--input",str(inp),"--output",str(directory/"output")],
                                  capture_output=True,text=True,timeout=30)
            invalid_results.append({"parameter":p,"exit_code":result.returncode,"passed":result.returncode!=0})
        # Replace the deliberately invalid infinity for standards-compliant reporting.
        invalid_results[1]["parameter"]={"temperature_kelvin":"Infinity (invalid JSON numeric)"}
        report["tests"]["invalid_inputs"]={"cases":invalid_results,"passed":all(x["passed"] for x in invalid_results)}
        installed=[mm.Platform.getPlatform(i).getName() for i in range(mm.Platform.getNumPlatforms())]
        unavailable=next((name for name in ["CUDA","OpenCL"] if name not in installed),None)
        if unavailable:
            _,proc=launch(worker,root,"unavailable-platform",{"platform":unavailable},expected=2)
            report["tests"]["unavailable_platform"]={"platform":unavailable,"exit_code":proc.returncode,"passed":True}
        # Interrupt after observed numerical progress, preserving already integrated frames.
        folder=root/"resume";folder.mkdir();out=folder/"output";out.mkdir()
        resume_parameters={**baseline,"steps":1000,"timestep_fs":1,"chunk_frames":10}
        inp=folder/"input.json";inp.write_bytes(encode({"engine":"openmm_argon","parameters":resume_parameters}))
        first=subprocess.Popen([sys.executable,str(worker),"--input",str(inp),"--output",str(out)],stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        deadline=time.monotonic()+30
        observed_step=0
        while time.monotonic()<deadline and first.poll() is None:
            try:
                observed_step=read(out/"progress.json").get("step",0)
                if observed_step>=50:
                    (out/"cancel.request").touch();break
            except (FileNotFoundError,ValueError,PermissionError):
                pass
            time.sleep(.005)
        first_stdout,first_stderr=first.communicate(timeout=30)
        if first.returncode not in (0,3):
            raise RuntimeError(f"Checkpoint pause failed: {first_stderr[-1600:]}")
        first_result=read(out/"result.json");initial_hash=sha((out/"trajectory/chunk-00000.json").read_bytes())
        (out/"cancel.request").unlink(missing_ok=True)
        second=subprocess.run([sys.executable,str(worker),"--input",str(inp),"--output",str(out)],capture_output=True,text=True,timeout=30)
        second_result=read(out/"result.json")
        intact,_=launch(worker,root,"resume-uninterrupted",resume_parameters)
        def final_positions(folder):
            last=read(folder/"trajectory/index.json")["chunks"][-1]
            with np.load(folder/last["arrays_path"],allow_pickle=False) as arrays:
                return arrays["positions_unwrapped_nm"][-1].copy()
        continuation_error=float(np.max(np.abs(final_positions(out)-final_positions(intact))))
        bad={"engine":"openmm_argon","parameters":{**resume_parameters,"steps":1001}}
        inp.write_bytes(encode(bad))
        third=subprocess.run([sys.executable,str(worker),"--input",str(inp),"--output",str(out)],capture_output=True,text=True,timeout=30)
        report["tests"]["checkpoint_resume"]={"pause_exit":first.returncode,"resume_exit":second.returncode,
            "changed_input_exit":third.returncode,"paused_step":first_result["completed_steps"],
            "resumed_step":second_result["completed_steps"],"uninterrupted_max_position_error_nm":continuation_error,
            "passed":first.returncode==3 and second.returncode==0 and 0<first_result["completed_steps"]<1000
            and third.returncode!=0 and first_result["status"]=="paused" and second_result["completed_steps"]==1000
            and continuation_error<1e-12
            and initial_hash==sha((out/"trajectory/chunk-00000.json").read_bytes())}
        report["passed"]=all(x["passed"] for x in report["tests"].values())
    except Exception as error:
        report["passed"]=False;report["error"]=str(error)
    report["elapsed_seconds"]=time.time()-report["started_at_unix"]
    (root/"report.json").write_bytes(encode(report))
    print(json.dumps(report,indent=2,allow_nan=False))
    return 0 if report["passed"] else 1


if __name__=="__main__":
    raise SystemExit(main())
