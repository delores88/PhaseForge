//! Explicit scientific contracts for optional relativity builds.
//! A diagnostic recipe is never a calibrated relativistic collision capability.
use anyhow::{ensure, Context};
use serde_json::{json, Value};

pub const GAUGE: &str = "athenak_gauge_wave";
pub const TWO_PUNCTURES_CPU: &str = "athenak_two_punctures_serial";
pub const TWO_PUNCTURES_CUDA: &str = "athenak_two_punctures_cuda";
pub const ENGINES: [&str; 3] = [GAUGE, TWO_PUNCTURES_CPU, TWO_PUNCTURES_CUDA];
const MIB: u64 = 1024 * 1024;

pub fn supports(engine: &str) -> bool { ENGINES.contains(&engine) }
pub fn is_gauge(engine: &str) -> bool { engine == GAUGE }
pub fn output_cap(engine: &str) -> u64 { if is_gauge(engine) { 64*MIB } else { 1024*MIB } }
pub fn scope(engine: &str) -> &'static str {
    if is_gauge(engine) {
        "Harmonic gauge-wave benchmark: flat spacetime in changing coordinates; no black holes or physical gravitational radiation."
    } else {
        "Three-dimensional vacuum Einstein evolution with constrained Bowen–York puncture initial data. The fixed low-momentum diagnostic has no calibrated physical boost, validated horizon, convergence or merger claim."
    }
}

pub fn parameters(engine: &str) -> Value {
    if is_gauge(engine) { json!({"nx":{"allowed":[16,32,64],"default":32}}) }
    else { json!({"recipe":{"allowed":["head_on_diagnostic_v1"],"default":"head_on_diagnostic_v1",
        "description":"Fixed measured diagnostic: equal target puncture-end ADM masses 0.5, inward Bowen–York momentum ±0.025, zero spin, separation 6, target code time 1. Momentum is not physical speed. Its earlier CPU attempt stopped at time 0.5126953125 and was not accuracy validated."}}) }
}

pub fn parameter_text(engine: &str, parameters: &Value) -> anyhow::Result<String> {
    ensure!(supports(engine), "Unsupported numerical-relativity engine");
    if is_gauge(engine) {
        super::thermal::keys(parameters, &["nx"])?;
        let nx = super::thermal::integer(parameters,"nx",32,16,64)?;
        ensure!([16,32,64].contains(&nx),"Gauge benchmark grid must be 16, 32 or 64");
        Ok(include_str!("../../../tools/athenak_gauge_wave.athinput").replace("\r\n","\n").replace("nx1 = 32", &format!("nx1 = {nx}")))
    } else {
        super::thermal::keys(parameters, &["recipe"])?;
        let recipe = match parameters.get("recipe") { None => "head_on_diagnostic_v1", Some(value) => value.as_str().context("NR recipe must be a string")? };
        ensure!(recipe=="head_on_diagnostic_v1","Unsupported NR recipe; a calibrated high-boost collision is not yet available");
        Ok(include_str!("../../../tools/athenak_head_on_diagnostic_v1.athinput").replace("\r\n","\n"))
    }
}

pub fn validate_manifest(engine: &str, manifest: &Value) -> anyhow::Result<()> {
    ensure!(supports(engine) && manifest["schema"]=="phaseforge.nr-engine.v1" && manifest["engine_id"]==engine,"NR engine identity differs");
    let source=&manifest["source"];
    let source_pin=|canonical:&str,legacy:&str|source.get(canonical).or_else(||source.get(legacy)).and_then(Value::as_str);
    ensure!(source_pin("athenak_commit","athenak")==Some("c5a0d7f9155a70149931bf0be5a4ffb673f2532a") && source_pin("kokkos_commit","kokkos")==Some("6739bc623081648af9e752b616d9671527922cbf"),"NR source pins differ from this adapter");
    let build=&manifest["build"];
    ensure!(build["precision"]=="double","NR build must retain double-precision evolution");
    if is_gauge(engine) {
        ensure!(build["problem"]=="z4c/z4c_gauge_wave" && build["backend"]=="Serial","Gauge adapter requires the verified serial gauge build");
    } else {
        ensure!(source_pin("twopunctures_commit","twopunctures")==Some("ec563aeb672235b9443c330f9cde65f7246e8ea4") && build["problem"]=="z4c/two_punctures/z4c_two_puncture","TwoPunctures initial-data source/build identity differs");
        if engine==TWO_PUNCTURES_CUDA {
            ensure!(build["backend"]=="CUDA" && build["cuda_arch"]=="ADA89" && build["cuda_version"]=="12.9.86","CUDA adapter requires the exact admitted double-precision ADA89 build");
            let runtime=&build["cuda_runtime"];
            ensure!(runtime["schema"]=="phaseforge.nr-cuda-runtime.v1"&&runtime["driver_directory"]=="/usr/lib/wsl/lib"&&runtime["guard_source_sha256"]==super::numerical_relativity::hash(include_bytes!("../../../tools/nr_gpu.py")),"CUDA build requires the exact private runtime and GPU observer contract");
            ensure!(runtime["libraries"].as_array().is_some_and(|rows|rows.len()==1),"CUDA runtime must pin its sole private runtime library");
        }else{ensure!(build["backend"]=="Serial","CPU diagnostic requires the serial TwoPunctures build");}
    }
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn physical_request_cannot_be_silently_substituted_by_diagnostic_recipe() {
        for engine in ENGINES {
            for parameters in [json!({"speed_c":0.999}),json!({"momentum":22.34}),json!({"input":"arbitrary"})] {
                assert!(parameter_text(engine,&parameters).is_err());
            }
        }
        assert!(parameter_text(TWO_PUNCTURES_CPU,&json!({"recipe":null})).is_err());
        assert!(parameter_text(TWO_PUNCTURES_CPU,&json!({"recipe":"collision_0.999c"})).is_err());
        let text=parameter_text(TWO_PUNCTURES_CPU,&json!({})).unwrap();
        assert_eq!(super::super::numerical_relativity::hash(text.as_bytes()),"66dff1103d4f521b601efc79e57c1e3ac353222f937e4d97dc07fbff922d07df");
        assert_eq!(text,parameter_text(TWO_PUNCTURES_CUDA,&json!({})).unwrap());
    }
}
