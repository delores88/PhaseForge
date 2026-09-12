//! Read-only completion evidence for an explicitly selected puncture diagnostic.
//! These retained fields never certify a physical boost, horizon or merger.
use super::{bytes, finite, numeric_array, AppState, LabJob};
use anyhow::{ensure, Context};
use serde_json::{json, Value};

const CHANNELS: [(&str, &str); 3] = [("chi", "1"), ("lapse", "1"), ("hamiltonian", "1/L^2")];

pub(super) fn supports(engine: &str) -> bool {
    matches!(engine, "athenak_two_punctures_serial" | "athenak_two_punctures_cuda")
}

pub(super) fn require_capability(job: &LabJob, requested: Option<&str>) -> anyhow::Result<()> {
    let engine = job.input["engine"].as_str().context("Puncture engine identity is missing")?;
    ensure!(supports(engine) && requested == Some(engine),
        "Requested capability does not match this fixed puncture diagnostic; it cannot fulfill numerical_relativity or a calibrated 0.999c collision");
    Ok(())
}

pub(super) fn evidence(state: &AppState, job: &LabJob) -> anyhow::Result<Value> {
    let diagnostic = state.laboratory.verified_black_hole_diagnostics(job)?;
    let evidence = inspect(job, &diagnostic, |descriptor| {
        let name = descriptor["path"].as_str().context("Puncture numeric artifact path is missing")?;
        let digest = descriptor["sha256"].as_str().context("Puncture numeric artifact pin is missing")?;
        let raw = bytes(state, job.id, &format!("diagnostics/{name}"), Some(digest))?;
        ensure!(descriptor["bytes"].as_u64() == Some(raw.len() as u64), "Puncture numeric artifact length differs");
        Ok(serde_json::from_slice(&raw)?)
    })?;
    // A navigation/recovery mutation cannot turn the just-read snapshot into
    // evidence for a different or no-longer-completed attempt.
    let current = state.laboratory.get(job.id)?;
    ensure!(current.state == "completed" && current.project_id == job.project_id && current.input == job.input
        && current.result == job.result && current.deadline_at == job.deadline_at, "Puncture evidence changed during inspection");
    Ok(evidence)
}

// The caller has verified the complete immutable diagnostic inventory and
// original admission with the retention verifier. Re-read interpreted JSON on
// its exact pin, then validate the shape/meaning of the retained numeric views.
fn inspect(job: &LabJob, diagnostic: &Value, read: impl Fn(&Value) -> anyhow::Result<Value>) -> anyhow::Result<Value> {
    let engine = job.input["engine"].as_str().context("Puncture engine missing")?;
    ensure!(job.kind == "solver" && job.state == "completed" && supports(engine)
        && job.result["engine"] == engine && diagnostic["engine_identity"]["engine_id"] == engine,
        "Puncture evidence requires its exact completed solver");
    let execution = &diagnostic["execution"];
    ensure!(execution["complete"] == true && execution["process_completed"] == true && execution["retained_time_target_met"] == true
        && execution["termination_reason"] == "completed" && execution["exit_code"] == 0 && execution["process_group_drained"] == true,
        "Puncture evidence is partial or its process was not drained");
    let unknown = json!({"status":"unknown","gamma":null,"speed_over_c":null,"calibration":null});
    ensure!(diagnostic["physical_boost"] == unknown && diagnostic["fulfillment"] == json!({
        "requested_0_999c_collision":false,"merger_validated":false,"horizon_properties_validated":false,"convergence_validated":false}),
        "Puncture diagnostics cannot introduce accuracy, boost or merger claims");
    let inventory = diagnostic["artifacts"].as_array().context("Puncture artifact inventory is missing")?;
    let registered = |descriptor: &Value| -> anyhow::Result<()> {
        ensure!(inventory.contains(descriptor), "Puncture evidence descriptor is outside its verified inventory"); Ok(())
    };
    let slice_pin = &diagnostic["diagnostic_slices"];
    registered(slice_pin)?;
    ensure!(slice_pin["path"] == "slices/index.json", "Puncture diagnostic slice index is missing");
    let index = read(slice_pin)?;
    let units = &diagnostic["units"];
    ensure!(units["length"] == "L" && units["time"] == "L/c" && units["mass"] == "c^2 L/G"
        && units.get("si_mapping") == Some(&Value::Null), "Puncture coordinates must retain their code units without SI conversion");
    ensure!(index["schema"] == "phaseforge.nr-amr-diagnostic-slices.v1" && index["representation"] == "nr_amr_diagnostic_slice"
        && index["engine_id"] == engine && index["units"] == *units && index["time_unit"] == "L/c"
        && index["interpolation"] == "none" && index["grid_location"] == "cell_center" && index["axis_order"] == json!(["y","x"])
        && index["execution"] == *execution && index["physical_boost"] == unknown && index["primary_channel"] == "chi"
        && index["fulfillment"] == json!({"requested_0_999c_collision":false,"merger_validated":false,"horizon_properties_validated":false}),
        "Puncture slice index changed its representation, execution or scope");
    for horizon in index["horizons"].as_array().context("Puncture horizon status list is missing")? {
        ensure!(horizon["horizon_validated"] == false, "Puncture display cannot validate a horizon");
    }
    registered(&diagnostic["measurements"])?;
    let measurements = read(&diagnostic["measurements"])?;
    ensure!(measurements["schema"] == "phaseforge.nr-black-hole-measurements.v1" && measurements["units"] == *units
        && measurements["physical_boost"] == unknown, "Puncture measurement schema or boost differs");
    let frames = index["frames"].as_array().context("Puncture saved slices are missing")?;
    let times = diagnostic["paired_times"].as_array().context("Puncture paired native times are missing")?;
    let reductions = measurements["constraints"].as_array().context("Puncture retained numerical reductions are missing")?;
    ensure!((2..=128).contains(&frames.len()) && frames.len() == times.len() && reductions.len() == times.len()
        && index["frame_count"] == json!(frames.len()) && diagnostic["paired_state_count"] == json!(times.len()),
        "Puncture evidence needs all saved paired times and at least two states");
    let mut previous = None;
    for (n, ((frame, time), reduction)) in frames.iter().zip(times).zip(reductions).enumerate() {
        let t = finite(time)?;
        ensure!(t >= 0.0 && previous.is_none_or(|p|t>p) && frame["time"] == *time && frame["scientific_time"] == *time
            && frame["time_unit"] == "L/c" && frame["index"] == json!(n) && frame["cycle"].as_u64().is_some()
            && reduction["time"] == *time && reduction["cycle"] == frame["cycle"] && reduction["units"] == *units
            && reduction.get("accuracy_accepted") == Some(&Value::Null), "Puncture time order or measurement scope differs");
        ensure!(finite(&reduction["H_rms"])? >= 0.0 && finite(&reduction["M_rms"])? >= 0.0
            && finite(&reduction["proper_volume"])? > 0.0 && reduction["included_cells"].as_u64().is_some_and(|n|n>0),
            "Puncture numerical reductions are absent or invalid");
        registered(&frame["data"])?;
        for source in [&frame["source_metric"], &frame["source_constraints"]] {
            registered(source)?;
            ensure!(source["path"].as_str().is_some_and(|p|p.starts_with("native/")), "Puncture slice source is not a native field");
        }
        previous = Some(t);
    }
    ensure!(index["initial_time"] == times[0] && index["final_time"] == times[times.len()-1]
        && finite(&times[times.len()-1])? >= finite(&execution["time_target"])?
        && execution["last_paired_native_time"] == times[times.len()-1], "Puncture saved time endpoints differ from their target");
    let mut samples = Vec::new();
    for frame in [frames.first().unwrap(), frames.last().unwrap()] {
        let view = read(&frame["data"])?;
        ensure!(view["schema"] == "phaseforge.nr-amr-diagnostic-slice.v1" && view["time"] == frame["time"]
            && view["scientific_time"] == frame["time"] && view["cycle"] == frame["cycle"] && view["time_unit"] == "L/c"
            && view["coordinate_unit"] == "L" && view["axis_order"] == json!(["y","x"])
            && view["interpolation"] == "none" && view["grid_location"] == "cell_center" && view["plane"] == frame["plane"]
            && view["source_metric"] == frame["source_metric"] && view["source_constraints"] == frame["source_constraints"],
            "Puncture numeric slice identity, time or geometry differs");
        ensure!(view["plane"]["axes"] == json!(["x","y"]) && view["plane"]["normal_axis"] == "z", "Puncture plane axes differ");
        let blocks = view["blocks"].as_array().filter(|b|!b.is_empty() && b.len()<=65536).context("Puncture AMR blocks are missing")?;
        ensure!(frame["block_count"] == json!(blocks.len()), "Puncture block count differs");
        let mut cells = 0usize;
        let mut logicals = std::collections::HashSet::new();
        for block in blocks {
            let shape = block["shape"].as_array().filter(|s|s.len()==2).context("Puncture AMR shape is missing")?;
            let ny = shape[0].as_u64().filter(|n|*n>0 && *n<=4096).context("Invalid puncture block height")? as usize;
            let nx = shape[1].as_u64().filter(|n|*n>0 && *n<=4096).context("Invalid puncture block width")? as usize;
            cells = cells.checked_add(nx*ny).context("Puncture cell count overflow")?;
            ensure!(cells<=262144, "Puncture slice exceeds its retained cell bound");
            ensure!(block["logical"].as_array().is_some_and(|v|v.len()==4 && v.iter().all(|n|n.as_i64().is_some()))
                && logicals.insert(block["logical"].to_string()), "Puncture AMR block identities are duplicated or missing");
            let geometry = block["geometry"].as_array().filter(|g|g.len()==6).context("Puncture AMR bounds are missing")?;
            let g = geometry.iter().map(finite).collect::<anyhow::Result<Vec<_>>>()?;
            ensure!(g[0]<g[1] && g[2]<g[3] && g[4]<g[5] && finite(&block["z"])? >= g[4]
                && finite(&block["z"])? <= g[5], "Puncture AMR geometry is invalid");
            for (axis, count, low, high) in [("x",nx,g[0],g[1]),("y",ny,g[2],g[3])] {
                let coordinates = block[axis].as_array().filter(|v|v.len()==count).context("Puncture AMR coordinates differ from shape")?;
                let positions = coordinates.iter().map(finite).collect::<anyhow::Result<Vec<_>>>()?;
                ensure!(positions.windows(2).all(|p|p[1]>p[0]) && positions[0]>low && positions[count-1]<high,
                    "Puncture AMR cell centres are not within their original bounds");
            }
            ensure!(block["channels"].as_object().is_some_and(|c|c.len()==3), "Puncture slice has missing numeric channels");
            for (channel, unit) in CHANNELS {
                ensure!(index["channels"][channel]["unit"] == unit, "Puncture numeric channel units differ");
                let rows = block["channels"][channel].as_array().filter(|v|v.len()==ny).context("Puncture channel height differs")?;
                for row in rows { ensure!(row.as_array().is_some_and(|v|v.len()==nx), "Puncture channel width differs"); numeric_array(row)?; }
            }
        }
        ensure!(view["cell_count"] == json!(cells) && frame["cell_count"] == json!(cells), "Puncture retained cell count differs");
        samples.push(json!({"time":frame["time"],"cycle":frame["cycle"],"block_count":blocks.len(),"numeric_values":cells*3,
            "path":format!("diagnostics/{}",frame["data"]["path"].as_str().unwrap()),"sha256":frame["data"]["sha256"],
            "source_metric":frame["source_metric"],"source_constraints":frame["source_constraints"]}));
    }
    Ok(json!({"job_id":job.id,"engine":engine,"kind":"retained_temporal_numerical_states",
        "diagnostic_result":job.result["diagnostics"],"index_path":"diagnostics/slices/index.json","index_sha256":slice_pin["sha256"],
        "measurements":{"path":"diagnostics/measurements.json","sha256":diagnostic["measurements"]["sha256"]},
        "frame_count":frames.len(),"time_unit":"L/c","units":units,"verified_endpoint_samples":samples,
        "physical_boost":unknown,"merger_validated":false,"horizon_properties_validated":false,"convergence_validated":false,
        "requested_0_999c_collision":false,"accuracy_validated":false,"scientific_scope":diagnostic["scope"],
        "verification_scope":"Pinned completed diagnostic execution, original admission, native inventory and all retained time/measurement rows; finite first/last AMR numeric blocks. Native fields are not independently decoded here. No boost calibration, convergence, horizon or merger validation."}))
}

#[cfg(test)]
#[path = "laboratory_intent_black_hole_tests.rs"]
mod tests;
