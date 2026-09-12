//! Constant-property SI heat conduction on a periodic rectangle.
use anyhow::{ensure, Context};
use serde_json::{json, Value};

pub fn capability() -> Value {
    json!({"id":"heat_conduction_2d","adapter_version":"1.0.0","runtime":"Bundled NumPy 2.4.6 and Pillow 12.3.0; CPU",
        "scope":"Actual two-dimensional constant-property heat conduction on a uniform periodic rectangle, per unit out-of-plane depth. No walls, imposed flux, sources, radiation, advection, phase change, or calibrated material response.",
        "equations":"rho*cp*dT/dt = k*laplacian(T); q = -k*grad(T)","method":"float64 conservative five-point FTCS; reject unstable inputs without adjustment",
        "parameters":{"nx":{"default":64,"min":16,"max":256},"ny":{"default":64,"min":16,"max":256},"length_x_m":{"default":0.01},"length_y_m":{"default":0.01},
            "conductivity_W_mK":{"default":0.6,"min":0},"density_kg_m3":{"default":1000},"heat_capacity_J_kgK":{"default":4184},"dt_s":{"default":0.01},
            "steps":{"default":256,"max":100000},"record_interval":{"default":8,"max_retained_frames":1001},"boundary":{"allowed":["periodic"]},"max_output_mb":{"default":256,"min":64,"max":2048},
            "initial":{"default":{"kind":"fourier","baseline_K":300,"amplitude_K":10,"mode_x":1,"mode_y":1},"alternatives":[{"kind":"gaussian","baseline_K":300,"amplitude_K":10,"center_x_m":0.005,"center_y_m":0.005,"sigma_m":0.001},{"kind":"array","path":"imports/initial.npy"}]}},
        "stability":"alpha=k/(rho*cp); alpha*dt*(1/dx^2+1/dy^2)<=0.5",
        "units":{"length":"m","time":"s","temperature":"K","heat_flux":"W/m^2","thermal_energy_per_depth":"J/m"},
        "instruments":["Temperature mean, minimum, maximum and variance","Thermal energy per unit depth (J/m)","Fourier temperature amplitude (K; Fourier initial condition only)","Computed heat-flux x/y channels (W/m^2)","Relative thermal-energy drift"],
        "outputs":{"fields":"fields/index.json; actual temperature .npy + exact JSON views; each frame also pins a .npz containing temperature and both heat-flux components","measurements":"measurements.json","observations":"observations/index.json","checkpoint":"checkpoint.json"},
        "display":"Legacy scalar coordinates explicitly converted from metres to micrometres; computation and channels remain SI",
        "source_imports":"Retained same-project sources:[{job_id,path,destination,sha256?}] copied to imports/; array must be finite nonnegative real [ny,nx] .npy, no pickle",
        "validation":"Independent Fourier discrete/continuum decay, refinement, energy conservation, conductivity intervention, raw-state hashes and checkpoint recovery. Parameter defaults are numerical examples, not validated real-material predictions."})
}

pub(super) fn integer(p: &Value, key: &str, default: u64, low: u64, high: u64) -> anyhow::Result<u64> {
    let n = match p.get(key) { None => default, Some(v) => v.as_u64().with_context(|| format!("{key} must be an integer"))? };
    ensure!((low..=high).contains(&n), "{key} is outside {low}..{high}"); Ok(n)
}
pub(super) fn number(p: &Value, key: &str, default: f64, low: f64, high: f64) -> anyhow::Result<f64> {
    let n = match p.get(key) { None => default, Some(v) => v.as_f64().with_context(|| format!("{key} must be numeric"))? };
    ensure!(n.is_finite() && n >= low && n <= high, "{key} is outside its finite supported range"); Ok(n)
}
pub(super) fn keys(p: &Value, allowed: &[&str]) -> anyhow::Result<()> {
    let object = p.as_object().context("Parameters must be an object")?;
    for key in object.keys() { ensure!(allowed.contains(&key.as_str()), "Unsupported parameter: {key}"); }
    Ok(())
}
pub(super) fn common(p: &Value, thermal: bool) -> anyhow::Result<(u64,u64,f64,f64,f64)> {
    ensure!(p.is_object(), "Parameters must be an object");
    let nx=integer(p,"nx",64,16,256)?; let ny=integer(p,"ny",64,16,256)?;
    let length=if thermal {0.01} else {1.0};
    let lx=number(p,"length_x_m",length,1e-9,1e6)?; let ly=number(p,"length_y_m",length,1e-9,1e6)?;
    let dt=number(p,"dt_s",if thermal {0.01} else {0.005},1e-12,1e6)?;
    let steps=integer(p,"steps",256,1,100000)?; let interval=integer(p,"record_interval",8,1,steps)?;
    let budget=integer(p,"max_output_mb",256,64,2048)?;
    let frames=steps.div_ceil(interval)+1;
    ensure!(frames<=1001 && frames*(nx*ny*104+4096)+32*1024*1024 <= budget*1024*1024,"Retained fields exceed frame/storage admission; explicitly change interval or budget");
    if let Some(boundary)=p.get("boundary") {ensure!(boundary=="periodic","Only periodic boundaries are supported");}
    number(p,"density_kg_m3",if thermal {1000.0} else {1.0},1e-12,1e12)?;
    Ok((nx,ny,lx,ly,dt))
}

pub fn validate(p: &Value) -> anyhow::Result<()> {
    keys(p,&["nx","ny","length_x_m","length_y_m","dt_s","steps","record_interval","boundary","max_output_mb","initial","conductivity_W_mK","density_kg_m3","heat_capacity_J_kgK"])?;
    let (nx,ny,lx,ly,dt)=common(p,true)?;
    let k=number(p,"conductivity_W_mK",0.6,0.0,1e12)?;
    let rho=number(p,"density_kg_m3",1000.0,1e-12,1e12)?;
    let cp=number(p,"heat_capacity_J_kgK",4184.0,1e-12,1e12)?;
    let cfl=k/(rho*cp)*dt*((nx as f64/lx).powi(2)+(ny as f64/ly).powi(2));
    ensure!(cfl.is_finite() && cfl<=0.5,"Heat FTCS requires alpha*dt*(1/dx^2+1/dy^2)<=0.5");
    if let Some(initial)=p.get("initial") {
        let kind=initial.get("kind").and_then(Value::as_str).unwrap_or("fourier");
        match kind {
            "fourier" => {
                keys(initial,&["kind","baseline_K","amplitude_K","mode_x","mode_y"])?;
                let base=number(initial,"baseline_K",300.0,0.0,1e9)?; let amp=number(initial,"amplitude_K",10.0,0.0,1e9)?;
                let mx=integer(initial,"mode_x",1,0,nx/4)?; let my=integer(initial,"mode_y",1,0,ny/4)?;
                ensure!(base>=amp && mx+my>0,"Fourier temperature requires baseline >= amplitude and nonzero mode");
            },
            "gaussian" => {
                keys(initial,&["kind","baseline_K","amplitude_K","center_x_m","center_y_m","sigma_m"])?;
                number(initial,"baseline_K",300.0,0.0,1e9)?; number(initial,"amplitude_K",10.0,0.0,1e9)?;
                number(initial,"center_x_m",lx/2.0,0.0,lx)?; number(initial,"center_y_m",ly/2.0,0.0,ly)?;
                number(initial,"sigma_m",lx.min(ly)/10.0,(lx/nx as f64).min(ly/ny as f64),lx.max(ly))?;
            },
            "array" => {
                keys(initial,&["kind","path"])?;
                let path=initial["path"].as_str().context("Retained initial array path required")?;
                super::safe_relative(path)?;
                ensure!(path.starts_with("imports/") && path.ends_with(".npy"),"Initial array must be retained under imports/ as .npy");
            },
            _=>anyhow::bail!("Unsupported heat initial condition"),
        }
    }
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn thermal_admits_defaults_and_rejects_dimensional_or_stability_errors() {
        validate(&json!({})).unwrap(); validate(&json!({"conductivity_W_mK":0})).unwrap();
        for invalid in [json!({"dt_s":10}),json!({"density_kg_m3":0}),json!({"boundary":"insulated"}),json!({"steps":100000,"record_interval":1}),json!({"initial":{"kind":"array","path":"../data.npy"}}),json!({"initial":{"kind":"fourier","baseline_K":1,"amplitude_K":5}})] {assert!(validate(&invalid).is_err(),"{invalid}");}
    }
}
