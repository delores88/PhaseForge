//! Resolved unforced periodic incompressible two-dimensional Newtonian flow.
use super::thermal::{common,integer,keys,number};
use anyhow::{ensure,Context};
use serde_json::{json,Value};

pub fn capability() -> Value {
    json!({"id":"navier_stokes_2d","adapter_version":"1.0.0","runtime":"Bundled NumPy 2.4.6 and Pillow 12.3.0; CPU",
        "scope":"Actual unforced 2D periodic incompressible Newtonian flow, constant density/viscosity, zero mean flow, resolved Fourier modes. No walls, inlets, obstacles, free surfaces, compressibility, 3D turbulence or calibrated real-world flow.",
        "equations":"omega_t+u*omega_x+v*omega_y=nu*laplacian(omega); -laplacian(psi)=omega; u=psi_y; v=-psi_x; div(u)=0",
        "method":"float64 Fourier vorticity-streamfunction pseudospectral RK4 with strict 2/3 dealiasing; actual velocity CFL checked each stage; diagnostic pressure Poisson solve in retained modes with mean-zero gauge",
        "parameters":{"nx":{"default":64,"min":16,"max":256},"ny":{"default":64,"min":16,"max":256},"length_x_m":{"default":1},"length_y_m":{"default":1},
            "kinematic_viscosity_m2_s":{"default":0.001,"min":0},"density_kg_m3":{"default":1},"dt_s":{"default":0.005},"steps":{"default":256,"max":100000},
            "record_interval":{"default":8,"max_retained_frames":1001},"boundary":{"allowed":["periodic"]},"max_output_mb":{"default":256,"min":64,"max":2048},
            "initial":{"default":{"kind":"taylor_green","velocity_m_s":0.1,"mode":1},"alternatives":[{"kind":"streamfunction_modes","modes":[{"mode_x":1,"mode_y":2,"amplitude_m2_s":0.01},{"mode_x":2,"mode_y":1,"amplitude_m2_s":0.006}]}],"modes":"1..16 sine streamfunction terms; positive integer mode_x/mode_y strictly below N/3; signed amplitude in m^2/s. Taylor-Green requires a square."}},
        "stability":"dt*(max(abs(u))/dx+max(abs(v))/dy)<=0.5 at every RK stage; nu*dt*(kx_max^2+ky_max^2)<=0.5; fail without changing timestep",
        "units":{"length":"m","time":"s","velocity":"m/s","vorticity":"1/s","pressure":"Pa","kinematic_viscosity":"m^2/s"},
        "instruments":["Kinetic energy per mass (m^2/s^2)","Enstrophy (1/s^2)","Maximum velocity divergence (1/s)","Circulation (m^2/s)","Mean x/y velocity (m/s)","Mean-zero pressure (Pa)","Actual advective CFL","Vorticity mean, minimum, maximum and variance"],
        "outputs":{"fields":"fields/index.json; default vorticity scalar with exact .npy/JSON; each retained frame pins actual .npz velocity_x,velocity_y,vorticity,pressure,divergence","measurements":"measurements.json: energy, enstrophy, divergence, circulation, mean velocity/pressure, CFL","observations":"observations/index.json","checkpoint":"checkpoint.json"},
        "display":"Metre solver coordinates explicitly converted to micrometres for the legacy scalar viewer; velocity/pressure remain retained SI channels",
        "validation":"Independent Taylor-Green velocity/vorticity/pressure decay, RK4 refinement, nonlinear advection derivative, divergence, inviscid energy/enstrophy, viscosity intervention, hashes and restart. Numerical tests do not establish experimental predictive validity."})
}

pub fn validate(p:&Value)->anyhow::Result<()> {
    keys(p,&["nx","ny","length_x_m","length_y_m","dt_s","steps","record_interval","boundary","max_output_mb","initial","kinematic_viscosity_m2_s","density_kg_m3"])?;
    let (nx,ny,lx,ly,dt)=common(p,false)?;
    let nu=number(p,"kinematic_viscosity_m2_s",0.001,0.0,1e6)?;
    let mx=(nx-1)/3; let my=(ny-1)/3;
    let kx=2.0*std::f64::consts::PI*mx as f64/lx; let ky=2.0*std::f64::consts::PI*my as f64/ly;
    let stability=nu*dt*(kx*kx+ky*ky);
    ensure!(stability.is_finite() && stability<=0.5,"Flow requires nu*dt*(kx_max^2+ky_max^2)<=0.5");
    if let Some(initial)=p.get("initial") {
        let kind=initial.get("kind").and_then(Value::as_str).unwrap_or("taylor_green");
        match kind {
            "taylor_green"=>{
                keys(initial,&["kind","velocity_m_s","mode"])?;
                ensure!(lx==ly,"Taylor-Green requires a square; streamfunction_modes supports rectangles");
                number(initial,"velocity_m_s",0.1,0.0,1e6)?; integer(initial,"mode",1,1,mx.min(my))?;
            },
            "streamfunction_modes"=>{
                keys(initial,&["kind","modes"])?;
                let modes=initial["modes"].as_array().context("Streamfunction modes array required")?;
                ensure!((1..=16).contains(&modes.len()),"Provide 1..16 streamfunction modes");
                for mode in modes {
                    keys(mode,&["mode_x","mode_y","amplitude_m2_s"])?;
                    ensure!(mode.get("mode_x").is_some() && mode.get("mode_y").is_some() && mode.get("amplitude_m2_s").is_some(),"Every mode requires both indices and amplitude");
                    integer(mode,"mode_x",1,1,mx)?; integer(mode,"mode_y",1,1,my)?; number(mode,"amplitude_m2_s",0.0,-1e12,1e12)?;
                }
            },
            _=>anyhow::bail!("Unsupported flow initial condition"),
        }
    } else {ensure!(lx==ly,"Default Taylor-Green requires a square; explicitly select streamfunction_modes on a rectangle");}
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn flow_admits_resolved_interventions_and_rejects_unimplemented_or_unstable_inputs() {
        validate(&json!({})).unwrap(); validate(&json!({"kinematic_viscosity_m2_s":0})).unwrap();
        validate(&json!({"length_y_m":2,"initial":{"kind":"streamfunction_modes","modes":[{"mode_x":1,"mode_y":2,"amplitude_m2_s":0.01}]}})).unwrap();
        for invalid in [json!({"dt_s":1}),json!({"boundary":"no_slip"}),json!({"length_y_m":2}),json!({"nx":16,"initial":{"kind":"taylor_green","mode":6}}),json!({"initial":{"kind":"streamfunction_modes","modes":[]}}),json!({"pressure_Pa":2})] {assert!(validate(&invalid).is_err(),"{invalid}");}
    }
}
