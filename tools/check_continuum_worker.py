"""Independent retained-array numerical checks; never imports worker code.

Run with the pinned science interpreter and -I -B. Small synthetic analytic
references exercise actual supervised worker processes, not models or an app.
All subprocess inputs/stdout/stderr and numerical artifacts remain in --output.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path
import shutil
import subprocess
import sys
import time
import numpy as np
from PIL import Image

REPO = Path(__file__).resolve().parents[1]
WORKER = REPO/"tools/continuum_worker.py"
HEAT, FLOW = "heat_conduction_2d", "navier_stokes_2d"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, value):
    path.write_text(json.dumps(value, sort_keys=True, indent=2, allow_nan=False), encoding="utf-8")


def checked(condition, message):
    if not condition:
        raise AssertionError(message)


def run(root, name, engine, parameters, expect=0):
    directory = root/name
    directory.mkdir(parents=True, exist_ok=False)
    write(directory/"input.json", {"engine": engine, "parameters": parameters})
    execute(directory, expect)
    return directory


def execute(directory, expect=0, suffix=""):
    before = time.perf_counter()
    with (directory/f"stdout{suffix}.log").open("wb") as out, (directory/f"stderr{suffix}.log").open("wb") as err:
        result = subprocess.run([sys.executable, "-I", "-B", str(WORKER), "--input", str(directory/"input.json"), "--output", str(directory)], stdout=out, stderr=err, timeout=120)
    write(directory/f"execution{suffix}.json", {"returncode": result.returncode, "wall_seconds": time.perf_counter()-before, "expected_returncode": expect, "worker_sha256": digest(WORKER)})
    checked(result.returncode == expect, f"{directory.name}: exit {result.returncode}, expected {expect}: {(directory/f'stderr{suffix}.log').read_text()}")


def audit(directory, *, completed=True):
    index, result, manifest = (read(directory/name) for name in ("fields/index.json", "result.json", "manifest.json"))
    p = manifest["input"]["parameters"]
    checked(manifest["worker_sha256"] == digest(WORKER), "worker source pin")
    checked(manifest["shared_io_sha256"] == digest(REPO/"tools/field_worker.py"), "shared I/O source pin")
    checked(manifest["units"]["length"] == "m" and index["length_unit"] == "um" and index["time_unit"] == "s", "explicit physical/display units")
    checked(index["lengths_um"] == [p["length_x_m"]*1e6,p["length_y_m"]*1e6], "SI/display length conversion")
    checked(index["frames"][0]["step"] == 0, "initial physical state retained")
    previous = -1
    all_states = []
    for row in index["frames"]:
        checked(row["step"] > previous and row["time"] == row["step"]*p["dt_s"], "strict integer-step physical time ordering")
        previous = row["step"]
        for key, pin in (("path","sha256"),("view_path","view_sha256"),("state_path","state_sha256")):
            checked(digest(directory/row[key]) == row[pin], f"retained {key} SHA256")
        field = np.load(directory/row["path"],allow_pickle=False)
        view = read(directory/row["view_path"])
        checked(view["field_unit"] == index["field_unit"] and view["step"] == row["step"] and view["time"] == row["time"], "view identity/units")
        checked(np.array_equal(field,np.array(view["values"])), "JSON values equal full numeric NPY")
        with np.load(directory/row["state_path"],allow_pickle=False) as archive:
            state = {name:archive[name].copy() for name in archive.files}
        checked(set(state)==set(index["channels"]) and all(a.shape==(p["ny"],p["nx"]) and a.dtype==np.float64 and np.isfinite(a).all() for a in state.values()), "all actual channel shapes/dtypes/values")
        checked(np.array_equal(field,state[index["field_name"]]), "primary scalar equals NPZ channel")
        all_states.append((row,state))
    for row in read(directory/"observations/index.json")["images"]:
        checked(digest(directory/row["path"])==row["sha256"], "actual image SHA256")
        checked(digest(directory/row["field_source"]["path"])==row["field_source"]["sha256"],"image source SHA256")
        source_path = directory / row["field_source"]["path"]
        field=np.load(source_path,allow_pickle=False)
        x0,y0,width,height=row["camera"]["plot_bbox_pixels"]
        pixels=Image.open(directory/row["path"]).convert("RGB")
        low,high=np.array([20,38,80]),np.array([255,190,60])
        for iy,ix in ((0,0),(p["ny"]//3,p["nx"]//4),(p["ny"]-1,p["nx"]-1)):
            x=x0+math.floor((ix+.5)*width/p["nx"]);y=y0+math.floor((p["ny"]-iy-.5)*height/p["ny"])
            fraction=max(0,min(1,(field[iy,ix]-row["color_scale"]["min"])/(row["color_scale"]["max"]-row["color_scale"]["min"])))
            expected=np.rint(low+fraction*(high-low)).astype(int)
            checked(np.array_equal(pixels.getpixel((x,y)),expected),"actual rendered scalar pixel value/orientation")
    if completed:
        checked(result["status"]=="completed" and previous==p["steps"], "completed physical endpoint")
    return p,index,result,all_states


def grid(p):
    x=(np.arange(p["nx"])+.5)*p["length_x_m"]/p["nx"]
    y=(np.arange(p["ny"])+.5)*p["length_y_m"]/p["ny"]
    return np.meshgrid(x,y)


def heat_checks(root):
    errors=[]; receipts=[]
    for n in (16,32,64):
        dt=.1*(16/n)**2
        steps=round(.4/dt)
        directory=run(root,f"heat-refinement-{n}",HEAT,{"nx":n,"ny":n,"dt_s":dt,"steps":steps,"record_interval":steps,"initial":{"kind":"fourier","baseline_K":300,"amplitude_K":10,"mode_x":1,"mode_y":2}})
        p,_,result,states=audit(directory)
        X,Y=grid(p); mx,my=1,2
        # Scalar standard-library trig reference, independent of NumPy FFT/stencils.
        basis=np.array([[math.cos(2*math.pi*mx*x/p["length_x_m"])*math.cos(2*math.pi*my*y/p["length_y_m"]) for x,y in zip(xs,ys)] for xs,ys in zip(X,Y)])
        alpha=p["conductivity_W_mK"]/(p["density_kg_m3"]*p["heat_capacity_J_kgK"])
        dx,dy=p["length_x_m"]/n,p["length_y_m"]/n
        multiplier=1-4*alpha*dt*(math.sin(math.pi*mx/n)**2/dx**2+math.sin(math.pi*my/n)**2/dy**2)
        for row,state in states:
            expected=300+10*multiplier**row["step"]*basis
            checked(np.max(np.abs(state["temperature_K"]-expected))<=1e-10,"heat discrete Fourier solution")
            amplitude=10*multiplier**row["step"]
            qx=p["conductivity_W_mK"]*amplitude*math.sin(2*math.pi*mx/n)/dx*np.sin(2*math.pi*mx*X/p["length_x_m"])*np.cos(2*math.pi*my*Y/p["length_y_m"])
            qy=p["conductivity_W_mK"]*amplitude*math.sin(2*math.pi*my/n)/dy*np.cos(2*math.pi*mx*X/p["length_x_m"])*np.sin(2*math.pi*my*Y/p["length_y_m"])
            checked(np.max(np.abs(state["heat_flux_x_W_m2"]-qx))<1e-7 and np.max(np.abs(state["heat_flux_y_W_m2"]-qy))<1e-7,"independently derived physical heat-flux channels")
            energy=math.fsum(float(v) for v in state["temperature_K"].flat)*dx*dy*p["density_kg_m3"]*p["heat_capacity_J_kgK"]
            checked(abs(energy-result["initial"]["thermal_energy_per_depth_J_m"])/energy<=1e-12,"independent heat energy conservation")
        continuum=300+10*math.exp(-alpha*((2*math.pi*mx/p["length_x_m"])**2+(2*math.pi*my/p["length_y_m"])**2)*.4)*basis
        error=float(np.sqrt(np.mean((states[-1][1]["temperature_K"]-continuum)**2)))
        errors.append(error); receipts.append({"directory":directory.name,"continuum_L2_K":error,"discrete_multiplier":multiplier})
    checked(errors[0]/errors[1]>3.5 and errors[1]/errors[2]>3.5,"heat second-order refinement")
    control=run(root,"heat-zero-conductivity",HEAT,{"nx":16,"ny":16,"conductivity_W_mK":0,"steps":8,"record_interval":4})
    _,_,_,states=audit(control)
    checked(np.array_equal(states[0][1]["temperature_K"],states[-1][1]["temperature_K"]),"zero conductivity exact control")
    intervention=run(root,"heat-conductivity-intervention",HEAT,{"nx":16,"ny":16,"conductivity_W_mK":1.2,"steps":40,"record_interval":40,"dt_s":.01})
    p,_,result,states=audit(intervention)
    alpha=1.2/(1000*4184); multiplier=1-8*alpha*.01*math.sin(math.pi/16)**2/(.01/16)**2
    checked(abs(result["final"]["mode_amplitude_K"]-10*multiplier**40)<1e-10,"conductivity intervention exact discrete response")
    return {"refinement":receipts,"refinement_ratios":[errors[0]/errors[1],errors[1]/errors[2]],"control":"exactly unchanged","conductivity_intervention_amplitude_K":result["final"]["mode_amplitude_K"]}


def retained_temperature_checks(root):
    directory=root/"heat-retained-array";directory.mkdir();(directory/"imports").mkdir()
    original=280+np.arange(16*16,dtype=np.float64).reshape(16,16)/10
    np.save(directory/"imports/initial.npy",original,allow_pickle=False)
    write(directory/"input.json",{"engine":HEAT,"parameters":{"nx":16,"ny":16,"steps":2,"record_interval":1,"conductivity_W_mK":0,"initial":{"kind":"array","path":"imports/initial.npy"}}})
    execute(directory);_,_,_,states=audit(directory)
    checked(np.array_equal(states[0][1]["temperature_K"],original),"retained array exact initial identity")
    checked(np.array_equal(states[-1][1]["temperature_K"],original),"retained asymmetric temperature control")
    for _,state in states:
        checked(np.count_nonzero(state["heat_flux_x_W_m2"])==0 and np.count_nonzero(state["heat_flux_y_W_m2"])==0,"zero-conductivity actual flux channels")
    invalid=root/"heat-invalid-array";invalid.mkdir();(invalid/"imports").mkdir()
    np.save(invalid/"imports/initial.npy",np.ones((15,16)),allow_pickle=False)
    write(invalid/"input.json",read(directory/"input.json"));execute(invalid,expect=2)
    checked("shape" in read(invalid/"error.json")["message"],"imported array wrong shape rejected")
    return {"initial_sha256":digest(directory/"imports/initial.npy"),"wrong_shape_rejected":True,"asymmetric_render_pixels":"passed"}


def taylor_reference(p,t):
    X,Y=grid(p); ini=p["initial"]; k=2*math.pi*ini["mode"]/p["length_x_m"]
    U=ini["velocity_m_s"]; nu=p["kinematic_viscosity_m2_s"]; decay=math.exp(-2*nu*k*k*t)
    sx,cx=np.sin(k*X),np.cos(k*X); sy,cy=np.sin(k*Y),np.cos(k*Y)
    return {"velocity_x_m_s":U*sx*cy*decay,"velocity_y_m_s":-U*cx*sy*decay,"vorticity_s_inv":2*U*k*sx*sy*decay,
            "pressure_Pa":p["density_kg_m3"]*U*U/4*(np.cos(2*k*X)+np.cos(2*k*Y))*decay**2}


def flow_checks(root):
    directory=run(root,"flow-taylor-green",FLOW,{"nx":32,"ny":32,"steps":40,"record_interval":10,"dt_s":.005})
    p,_,result,states=audit(directory); maxima={key:0. for key in ("velocity_x_m_s","velocity_y_m_s","vorticity_s_inv","pressure_Pa")}
    for row,state in states:
        reference=taylor_reference(p,row["time"])
        for key,expected in reference.items():
            maxima[key]=max(maxima[key],float(np.max(np.abs(state[key]-expected))))
            checked(maxima[key]<=1e-8,f"Taylor-Green {key}")
        checked(np.max(np.abs(state["divergence_s_inv"]))<=1e-10,"incompressibility")
        for key in ("velocity_x_m_s","velocity_y_m_s","pressure_Pa"):
            checked(abs(math.fsum(float(v) for v in state[key].flat)/state[key].size)<=1e-12,"zero mean velocity/pressure gauge")
    errors=[]
    for dt in (.02,.01,.005):
        steps=round(.1/dt)
        directory=run(root,f"flow-rk4-{steps}",FLOW,{"nx":16,"ny":16,"steps":steps,"record_interval":steps,"dt_s":dt,"kinematic_viscosity_m2_s":.01,"initial":{"kind":"taylor_green","mode":3,"velocity_m_s":.1}})
        p,_,_,states=audit(directory)
        errors.append(float(np.sqrt(np.mean((states[-1][1]["vorticity_s_inv"]-taylor_reference(p,.1)["vorticity_s_inv"])**2))))
    checked(errors[0]/errors[1]>12 and errors[1]/errors[2]>12,"fourth-order temporal convergence")
    # Distinct Laplacian eigenvalues ensure a genuinely nonzero nonlinear RHS.
    modes=[{"mode_x":1,"mode_y":2,"amplitude_m2_s":.01},{"mode_x":2,"mode_y":3,"amplitude_m2_s":.004}]
    dt=1e-6
    directory=run(root,"flow-nonlinear-derivative",FLOW,{"nx":32,"ny":32,"steps":1,"record_interval":1,"dt_s":dt,"initial":{"kind":"streamfunction_modes","modes":modes}})
    p,_,_,states=audit(directory); X,Y=grid(p)
    u=np.zeros_like(X);v=u.copy();wx=u.copy();wy=u.copy();lap=u.copy()
    for mode in modes:
        kx,ky=2*math.pi*mode["mode_x"],2*math.pi*mode["mode_y"]
        a=mode["amplitude_m2_s"]; k2=kx*kx+ky*ky
        u+=a*ky*np.sin(kx*X)*np.cos(ky*Y); v-=a*kx*np.cos(kx*X)*np.sin(ky*Y)
        wx+=a*k2*kx*np.cos(kx*X)*np.sin(ky*Y); wy+=a*k2*ky*np.sin(kx*X)*np.cos(ky*Y)
        lap-=a*k2*k2*np.sin(kx*X)*np.sin(ky*Y)
    nonlinear=-u*wx-v*wy; expected=nonlinear+p["kinematic_viscosity_m2_s"]*lap
    observed=(states[-1][1]["vorticity_s_inv"]-states[0][1]["vorticity_s_inv"])/dt
    relative=float(np.linalg.norm(observed-expected)/np.linalg.norm(expected))
    checked(np.linalg.norm(nonlinear)>1,"nonlinear fixture has genuine advection")
    checked(relative<1e-4,"actual nonlinear advection matches independently differentiated analytic RHS")
    directory=run(root,"flow-inviscid-interactions",FLOW,{"nx":32,"ny":32,"steps":100,"record_interval":25,"dt_s":.001,"kinematic_viscosity_m2_s":0,"initial":{"kind":"streamfunction_modes","modes":modes}})
    _,_,_,states=audit(directory)
    invariants=[]
    for _,state in states:
        u,v,w=(state[key] for key in ("velocity_x_m_s","velocity_y_m_s","vorticity_s_inv"))
        invariants.append((math.fsum(float(x*x+y*y) for x,y in zip(u.flat,v.flat))/(2*u.size),math.fsum(float(x*x) for x in w.flat)/(2*w.size)))
    drifts=[max(abs(row[i]-invariants[0][i])/invariants[0][i] for row in invariants) for i in (0,1)]
    checked(max(drifts)<=1e-8,"inviscid nonlinear energy and enstrophy conservation")
    checked(not np.allclose(states[0][1]["vorticity_s_inv"],states[-1][1]["vorticity_s_inv"],atol=1e-6),"inviscid interacting state actually evolves")
    return {"Taylor_Green_max_errors":maxima,"RK4_errors":errors,"RK4_ratios":[errors[0]/errors[1],errors[1]/errors[2]],"nonlinear_RHS_relative_error":relative,"inviscid_relative_energy_enstrophy_drifts":drifts}


def recovery_checks(root):
    receipts=[]
    for engine in (HEAT,FLOW):
        params={"nx":32,"ny":32,"steps":180,"record_interval":10,"dt_s":.005}
        baseline=run(root,engine+"-uninterrupted",engine,params)
        directory=root/(engine+"-resume");directory.mkdir()
        write(directory/"input.json",{"engine":engine,"parameters":params})
        with (directory/"stdout.log").open("wb") as out,(directory/"stderr.log").open("wb") as err:
            process=subprocess.Popen([sys.executable,"-I","-B",str(WORKER),"--input",str(directory/"input.json"),"--output",str(directory)],stdout=out,stderr=err)
            end=time.monotonic()+30
            while time.monotonic()<end:
                if (directory/"checkpoint.json").exists():
                    cp=read(directory/"checkpoint.json")
                    if cp["step"]>=10:
                        (directory/"cancel.request").write_text("requested by independent recovery checker",encoding="utf-8")
                        break
                if process.poll() is not None:
                    raise AssertionError("Recovery fixture finished before interruption")
                time.sleep(.005)
            checked(process.wait(timeout=30)==3,"actual cooperative cancellation")
        cp=read(directory/"checkpoint.json");stopped=cp["step"]
        checked(10<=stopped<180,"interruption retained an intermediate state")
        # Deliberately stale public aliases emulate a crash between checkpoint and aliases.
        write(directory/"fields/index.json",{"stale":True})
        (directory/"cancel.request").unlink()
        execute(directory,suffix="-resumed")
        _,_,_,a=audit(directory);_,_,_,b=audit(baseline)
        maximum=max(float(np.max(np.abs(a[-1][1][key]-b[-1][1][key]))) for key in a[-1][1])
        checked(maximum<=(1e-12 if engine==HEAT else 1e-10),"recovery equals uninterrupted physical channels")
        # Corrupt only a copied job's retained bytes; original evidence remains intact.
        corrupt=root/(engine+"-corrupt");shutil.copytree(directory,corrupt)
        first=read(corrupt/"fields/index.json")["frames"][0]["state_path"]
        with (corrupt/first).open("ab") as stream:stream.write(b"corrupted-checker-fixture")
        execute(corrupt,expect=2,suffix="-rejected")
        checked("hash changed" in read(corrupt/"error.json")["message"],"corrupt retained channel rejected")
        changed=root/(engine+"-changed-input");shutil.copytree(directory,changed)
        request=read(changed/"input.json");request["parameters"]["dt_s"]*=.5;write(changed/"input.json",request)
        execute(changed,expect=2,suffix="-rejected")
        checked("immutable" in read(changed/"error.json")["message"],"changed physics cannot reuse retained checkpoint")
        receipts.append({"engine":engine,"stopped_step":stopped,"max_recovery_channel_error":maximum,"corruption_rejected":True})
    return receipts


def main():
    parser=argparse.ArgumentParser();parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args();root=args.output.resolve();root.mkdir(parents=True,exist_ok=False)
    report={"scope":"source-level small analytic component computations; not installed application or real-material validation","worker_sha256":digest(WORKER),"checker_sha256":digest(Path(__file__)),"shared_io_sha256":digest(REPO/"tools/field_worker.py"),"reference_document_sha256":digest(REPO/"docs/validation/continuum-coverage.md"),"python":sys.version,"numpy":np.__version__,"passed":False}
    started=time.perf_counter()
    try:
        report["heat"]=heat_checks(root);write(root/"report.json",report)
        report["flow"]=flow_checks(root);write(root/"report.json",report)
        report["retained_temperature"]=retained_temperature_checks(root)
        report["recovery"]=recovery_checks(root)
        invalid=[(HEAT,{"dt_s":10}),(FLOW,{"dt_s":1}),(FLOW,{"initial":{"kind":"taylor_green","velocity_m_s":1e5}}),(HEAT,{"boundary":"insulated"}),(FLOW,{"boundary":"no_slip"}),(HEAT,{"nx":15}),(HEAT,{"steps":1.5}),(FLOW,{"initial":{"kind":"taylor_green","mode":30}})]
        for i,(engine,p) in enumerate(invalid):
            directory=run(root,f"invalid-{i}",engine,p,expect=2)
            checked(not (directory/"result.json").exists(),"invalid parameters cannot become completed experiment")
        report["invalid_requests_rejected"]=len(invalid);report["passed"]=True
    except Exception as error:
        report["error"]={"type":type(error).__name__,"message":str(error)}
        raise
    finally:
        report["wall_seconds"]=time.perf_counter()-started
        report["retained_bytes"]=sum(p.stat().st_size for p in root.rglob("*") if p.is_file())
        write(root/"report.json",report)
        print(json.dumps(report,sort_keys=True))


if __name__=="__main__":main()
