//! Reuse a completed, frozen surrogate through the same durable stage ledger.
//! A model's refusal is evidence. Only this coordinator can schedule its fallback.
use super::{ml_study::{sha, PROPOSAL, SOLVER, WORKER}, write_json, LabJob, LaboratoryService};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use std::{collections::BTreeSet, fs};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

#[cfg(test)]
#[path = "ml_query_tests.rs"]
mod tests;

fn id_at(value: &Value, key: &str) -> anyhow::Result<Uuid> {
    Ok(Uuid::parse_str(value[key].as_str().with_context(|| format!("Missing retained {key}"))?)?)
}
fn elapsed(job: &LabJob) -> anyhow::Result<f64> {
    ensure!(job.state == "completed", "Elapsed cost requires a completed job");
    Ok((job.completed_at.context("Missing completion timestamp")? - job.created_at).num_microseconds()
        .context("Elapsed cost overflow")?.max(0) as f64 / 1e6)
}
fn real(value: &Value, name: &str, low: f64, high: f64) -> anyhow::Result<f64> {
    let number = value[name].as_f64().with_context(|| format!("{name} must be numeric"))?;
    ensure!(number.is_finite() && number >= low && number <= high, "{name} is outside supported limits [{low}, {high}]");
    Ok(number)
}
fn integer(value: &Value, name: &str, low: u64, high: u64) -> anyhow::Result<u64> {
    let number = value[name].as_u64().with_context(|| format!("{name} must be an integer"))?;
    ensure!(number >= low && number <= high, "{name} is outside supported limits [{low}, {high}]");
    Ok(number)
}

/// The fixed worker performs prediction admission. This separately validates
/// actual solver compatibility; neither an arbitrary protocol nor its model
/// refusal grants permission to reinterpret an unknown engine or endpoint.
fn fallback_parameters(query: &Value, frozen: &Value) -> anyhow::Result<Value> {
    ensure!(query["units"] == json!({"temperature_kelvin":"K","density_g_cm3":"g/cm^3"}), "Exact K and g/cm^3 feature units are required");
    let features = query["features"].as_object().context("Temperature and density are required")?;
    ensure!(features.len() == 2 && features.contains_key("temperature_kelvin") && features.contains_key("density_g_cm3"), "Exactly temperature and density are supported");
    let temperature = real(&query["features"], "temperature_kelvin", 20.0, 500.0)?;
    let density = real(&query["features"], "density_g_cm3", 0.02, 2.0)?;
    let protocol = &query["protocol"];
    let mut identity = protocol.clone();
    ensure!(identity.is_object(), "A complete numerical protocol is required for a solver fallback");
    identity["fixed_parameters"] = frozen["fixed_parameters"].clone();
    ensure!(identity == *frozen && frozen["engine"] == "openmm_argon" && frozen["worker_sha256"] == sha(SOLVER.as_bytes()),
        "Unsupported engine, potential, target, units or protocol identity; no solver was launched. Supply a separately validated supported experiment.");
    let parameters = protocol["fixed_parameters"].as_object().context("Missing fixed solver parameters")?;
    let fixed = frozen["fixed_parameters"].as_object().context("Missing frozen solver parameters")?;
    ensure!(parameters.keys().collect::<BTreeSet<_>>() == fixed.keys().collect::<BTreeSet<_>>(), "Unknown or missing solver parameters; no defaults may change the requested protocol");
    let mut output = Value::Object(parameters.clone());
    let atoms = integer(&output, "atom_count", 32, 512)?;
    let steps = integer(&output, "steps", 1, 5_000_000)?;
    let interval = integer(&output, "sample_interval", 1, 1000)?;
    integer(&output, "chunk_frames", 1, 100)?;
    let threads = integer(&output, "cpu_threads", 1, 8)?;
    let dt = real(&output, "timestep_fs", 0.1, 5.0)?;
    let friction = real(&output, "friction_per_ps", 0.01, 100.0)?;
    ensure!(output["platform"] == "CPU" && threads == 1, "This fallback has validated only CPU with one thread; another platform needs separate validation");
    ensure!(matches!(output["thermostat"].as_str(), Some("langevin" | "nve")), "Unsupported thermostat");
    ensure!(steps.div_ceil(interval) + 1 <= 50_001 && steps * atoms <= 50_000_000 && interval as f64 * dt <= 1000.0, "Solver work, retained-state or sampling limit exceeded");
    let start = integer(&frozen["target"], "step_exclusive", 0, 5_000_000)?;
    let end = integer(&frozen["target"], "step_inclusive", start + 1, 5_000_000)?;
    let count = integer(&frozen["target"], "observations_per_seed", 1, 50_001)?;
    ensure!(frozen["target"]["name"] == "mean_pressure_bar" && frozen["target"]["replicates"] == 3,
        "Unsupported pressure endpoint definition");
    // The last nonmultiple sample is also retained by the solver.
    let window_count = end.min(steps) / interval - start.min(steps) / interval
        + u64::from(steps % interval != 0 && steps > start && steps <= end);
    ensure!(steps >= end && window_count == count, "Requested sampling does not retain the exact endpoint window; no solver was launched");
    output["temperature_kelvin"] = json!(temperature);
    output["density_g_cm3"] = json!(density);
    output["timestep_fs"] = json!(dt);
    output["friction_per_ps"] = json!(friction);
    Ok(output)
}

fn query_runs(id: Uuid, parameters: &Value) -> Vec<Value> {
    // Stable on replay, separate from the preregistered 110000..142002 seeds.
    // Each query gets fresh solver job IDs; seed identity is disclosed, not a
    // claim of proof of statistical independence between all possible queries.
    let hash = sha(id.as_bytes());
    let base = 1_000_000 + (u64::from_str_radix(&hash[..14], 16).unwrap() % 700_000_000) * 3;
    (0..3).map(|replica| {
        let mut parameters = parameters.clone();
        let seed = base + replica;
        parameters["seed"] = json!(seed);
        json!({"case_id":format!("query-{id}-seed-{replica}"),"condition_index":0,"replicate_index":replica,
            "role":"query_fallback","seed":seed,"engine":"openmm_argon","parameters":parameters})
    }).collect()
}

fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut correction) = (0.0, 0.0);
    for value in values { let adjusted = value - correction; let next = total + adjusted; correction = (next - total) - adjusted; total = next; }
    total
}

/// Bounded scalar instrument, intentionally separate from the fixed-membership
/// ML reducer. It does not import the new query into the trained model or tests.
fn pressure_window(manifest: &Value, measurements: &Value, parameters: &Value, protocol: &Value) -> anyhow::Result<Value> {
    ensure!(manifest["engine"] == protocol["engine"] && manifest["engine_version"] == protocol["engine_version"]
        && manifest["worker_sha256"] == protocol["worker_sha256"] && manifest["platform"] == protocol["platform"]
        && manifest["platform_properties"] == protocol["platform_properties"] && manifest["parameters"] == *parameters,
        "Actual solver identity or parameters differ from the query");
    let units = json!({"time":"ps","position":"nm","velocity":"nm/ps","energy":"kJ/mol","force":"kJ/(mol nm)","temperature":"K","pressure":"bar","density":"g/cm^3","msd":"nm^2"});
    ensure!(measurements["units"] == units, "Measured units differ from the pressure instrument");
    let model = &manifest["model"];
    let volume = parameters["atom_count"].as_u64().context("Missing atom count")? as f64 * 39.948 / 6.02214076e23
        / parameters["density_g_cm3"].as_f64().context("Missing density")? * 1e21;
    let box_nm = model["box_nm"].as_array().context("Missing periodic box")?;
    ensure!(box_nm.len() == 3, "Invalid periodic box");
    let sides = box_nm.iter().map(|n| n.as_f64().filter(|v| v.is_finite() && *v > 0.0).context("Invalid box extent")).collect::<anyhow::Result<Vec<_>>>()?;
    let cutoff = (2.5_f64 * 0.3405).min(0.49 * volume.cbrt());
    ensure!(sides.iter().all(|side| (side - volume.cbrt()).abs() <= volume.cbrt() * 1e-12)
        && (sides.iter().product::<f64>() - volume).abs() <= volume * 1e-12,
        "Measured box does not match the requested density");
    ensure!(model["mass_dalton"] == 39.948 && model["sigma_nm"] == 0.3405 && model["epsilon_kj_mol"] == 0.997
        && model["dispersion_correction"] == false && model["charges"] == 0 && model["boundary"] == "cubic periodic"
        && (real(model,"cutoff_nm",0.0,1e6)? - cutoff).abs() <= 1e-12
        && (real(model,"switch_nm",0.0,1e6)? - 0.8 * cutoff).abs() <= 1e-12,
        "Measured potential differs from the supported argon instrument");
    let steps = parameters["steps"].as_u64().context("Missing horizon")?;
    let interval = parameters["sample_interval"].as_u64().context("Missing sample interval")?;
    let dt = parameters["timestep_fs"].as_f64().context("Missing timestep")?;
    let start = protocol["target"]["step_exclusive"].as_u64().context("Missing target start")?;
    let end = protocol["target"]["step_inclusive"].as_u64().context("Missing target end")?;
    let count = protocol["target"]["observations_per_seed"].as_u64().context("Missing window size")? as usize;
    let series = measurements["series"].as_array().context("Missing measured scalar series")?;
    ensure!(series.len() as u64 == steps.div_ceil(interval) + 1, "Incomplete retained scalar series");
    let mut pressure = vec![];
    for (index, row) in series.iter().enumerate() {
        let expected = (index as u64 * interval).min(steps);
        ensure!(row["step"].as_u64() == Some(expected), "Measured samples were reordered, duplicated or lost");
        ensure!(row.as_object().context("Invalid scalar row")?.values().all(|v| v.as_f64().is_some_and(f64::is_finite)), "Nonfinite or nonnumeric scalar sample");
        ensure!((real(row,"time_ps",0.0,1e9)? - expected as f64 * dt / 1000.0).abs() <= 1e-8, "Measured simulation time differs from the requested timestep");
        let value = row["pressure_bar"].as_f64().context("Missing measured pressure")?;
        if expected > start && expected <= end { pressure.push(value); }
    }
    ensure!(pressure.len() == count, "Incomplete measured endpoint window");
    let mean = sum(pressure.iter().copied()) / count as f64;
    ensure!(mean.is_finite(), "Pressure endpoint overflow");
    let blocks = pressure.chunks(20).map(|block| sum(block.iter().copied()) / block.len() as f64).collect::<Vec<_>>();
    Ok(json!({"pressure_bar":mean,"block_means_bar":blocks,"observations":count,"volume_nm3":volume,
        "window":{"step_exclusive":start,"step_inclusive":end,"time_exclusive_ps":start as f64*dt/1000.0,"time_inclusive_ps":end as f64*dt/1000.0},
        "scope":"Finite measured window. Correlated samples and initialization effects are retained; equilibrium is not asserted."}))
}

impl LaboratoryService {
    fn ml_query_sources(&self, study_id: Uuid) -> anyhow::Result<Value> {
        let study = self.get(study_id)?;
        ensure!(study.kind == "ml_study" && study.state == "completed" && !self.executing(study_id), "Choose a completed and drained ML study; incomplete studies cannot supply a surrogate");
        ensure!(study.input["proposal_sha256"] == PROPOSAL && study.input["worker_sha256"] == sha(WORKER.as_bytes())
            && study.input["solver_worker_sha256"] == sha(SOLVER.as_bytes()), "The installed source differs from the frozen study; restore the pinned implementation before querying it");
        let freeze = id_at(&study.result,"freeze_job_id")?;
        let fit = id_at(&study.result,"fit_job_id")?;
        let calibration = id_at(&study.result,"calibration_job_id")?;
        let evaluation = id_at(&study.result,"evaluation_job_id")?;
        for source in [freeze,fit,calibration,evaluation] {
            self.verify_ml_stage_source(study_id,source)?;
            let job = self.get(source)?;
            ensure!(job.project_id == study.project_id && job.parent_id == Some(study_id), "Surrogate stage ownership changed");
        }
        let mut pins = json!({});
        for (key,source,path) in [("split",freeze,"work/split.json"),("model",fit,"work/model.npz"),("model_card",fit,"work/model-card.json"),
            ("calibration",calibration,"work/calibration.json"),("evaluation",evaluation,"work/evaluation.json")] {
            let (_,pin) = self.ml_pin(source,path,key)?;
            pins[key] = json!({"job_id":source,"path":path,"sha256":pin["sha256"]});
        }
        let model_freeze = self.read_json(study_id,"model-freeze.json")?;
        ensure!(model_freeze == study.result["model_freeze"] && pins["model"]["sha256"] == model_freeze["model_sha256"]
            && pins["model_card"]["sha256"] == model_freeze["model_card_sha256"] && pins["split"]["sha256"] == model_freeze["split_sha256"],
            "Frozen source model or split identity changed");
        let split = self.read_json(freeze,"work/split.json")?;
        ensure!(split["protocol_sha256"] == model_freeze["protocol_sha256"], "Frozen protocol identity changed");
        let calibration_value = self.read_json(calibration,"work/calibration.json")?;
        let evaluation_value = self.read_json(evaluation,"work/evaluation.json")?;
        ensure!(calibration_value["model_sha256"] == pins["model"]["sha256"] && evaluation_value["model_sha256"] == pins["model"]["sha256"]
            && evaluation_value["calibration_sha256"] == pins["calibration"]["sha256"], "Calibration or final evaluation belongs to another model");
        Ok(json!({"study_id":study_id,"study_input_sha256":sha(&serde_json::to_vec(&study.input)?),"study_result_sha256":sha(&serde_json::to_vec(&study.result)?),
            "model_freeze":model_freeze,"artifacts":pins,"protocol":split["protocol"]}))
    }

    pub fn create_ml_query(&self, id: Uuid, parent: Uuid, source_study_id: Uuid, query: Value, storage_mb: u64) -> anyhow::Result<LabJob> {
        ensure!(query.is_object() && serde_json::to_vec(&query)?.len() <= 32*1024, "Provide a bounded numerical query");
        ensure!(matches!(query["intent"].as_str(),Some("scientific" | "explanation")), "Choose intent scientific or explanation explicitly");
        ensure!((256..=16384).contains(&storage_mb), "Query storage must be 256–16384 MiB");
        if let Ok(existing) = self.get(id) {
            ensure!(existing.kind == "ml_query" && existing.input["source_session_id"] == json!(parent)
                && existing.input["source_study_id"] == json!(source_study_id) && existing.input["original_query"] == query
                && existing.input["storage_mb"] == storage_mb, "Query receipt ID was already reserved for another request");
            return Ok(existing);
        }
        let parent_job = self.get(parent)?;
        ensure!(self.get(source_study_id)?.project_id == parent_job.project_id, "The source study belongs to another project");
        let sources = self.ml_query_sources(source_study_id)?;
        let mut resolved = query.clone();
        if resolved.get("protocol").is_none() { resolved["protocol"] = sources["protocol"].clone(); }
        self.create_active_child(id,parent,"ml_query","Query a frozen pressure surrogate",json!({
            "source_study_id":source_study_id,"original_query":query,"query":resolved,"source_pins":sources,"storage_mb":storage_mb,
            "proposal_sha256":PROPOSAL,"worker_sha256":sha(WORKER.as_bytes()),"solver_worker_sha256":sha(SOLVER.as_bytes()),"source_session_id":parent}))
    }

    pub(super) async fn execute_ml_query(&self, id: Uuid, token: &CancellationToken) -> anyhow::Result<()> {
        self.initialize_ml_ledger(id)?;
        let job = self.get(id)?;
        ensure!(job.kind == "ml_query" && job.active() && !token.is_cancelled(), "Query was stopped before admission");
        let source_id = id_at(&job.input,"source_study_id")?;
        let source = self.ml_query_sources(source_id)?;
        ensure!(source == job.input["source_pins"], "Pinned source evidence changed after query admission; no new calculation was launched");
        self.update(id,|job| { if job.active() { job.state = "running".into(); } })?;
        let pins = &source["artifacts"];
        let (mut request,mut imports) = self.ml_request(source_id,"infer",id_at(&pins["split"],"job_id")?)?;
        for key in ["model","model_card","calibration","evaluation"] {
            self.ml_add(&mut request,&mut imports,key,id_at(&pins[key],"job_id")?,pins[key]["path"].as_str().context("Missing pinned source path")?)?;
            ensure!(request[key]["sha256"] == pins[key]["sha256"], "Model source changed during query preparation");
        }
        ensure!(request["split"]["sha256"] == pins["split"]["sha256"], "Split changed during query preparation");
        request["query"] = job.input["query"].clone();
        let inference = self.ml_compute(id,"query-inference",request,imports,token).await?;
        let decision = self.read_json(inference,"work/inference.json")?;
        ensure!(matches!(decision["status"].as_str(),Some("invalid_input" | "prediction" | "requires_solver")), "Unknown inference decision; no solver was launched");
        let reference = self.read_json(id_at(&pins["evaluation"],"job_id")?,"work/evaluation.json")?;
        if decision["status"] == "prediction" {
            ensure!(reference["useful_acceleration"] == true && decision["model_sha256"] == pins["model"]["sha256"],
                "A prediction requires the pinned model and its actual completed usefulness evaluation");
        }
        let mut result = json!({"status":decision["status"],"query_job_id":id,"source_study_id":source_id,"query":job.input["query"],
            "source_pins":source,"inference_job_id":inference,"decision":decision,"solver_launched":false,
            "timing":{"inference_job_wall_seconds":elapsed(&self.get(inference)?)?,"source_study_benchmark":reference["benchmark"],
                "scope":"Query wall time includes isolated process startup, model loading and queueing. The source study benchmark is historical; it is not this query's measured warm latency."}});
        if decision["status"] == "requires_solver" && job.input["query"]["intent"] == "scientific" {
            match fallback_parameters(&job.input["query"],&source["protocol"]) {
                Err(error) => { result["fallback"] = json!({"status":"unsupported_protocol","reason":format!("{error:#}"),"solver_launched":false}); }
                Ok(parameters) => {
                    let runs = query_runs(id,&parameters);
                    let sweep = self.ml_sweep(id,"query-solver-fallback",&runs,&job.input["query"]["protocol"],token).await?;
                    let rows = self.ml_solver_rows(&[sweep])?;
                    let mut seeds = vec![];
                    for run in &runs {
                        let case = rows.get(run["case_id"].as_str().unwrap()).context("Missing actual query solver case")?;
                        let solver_id = id_at(&case["completion"],"job_id")?;
                        let solver = self.get(solver_id)?;
                        ensure!(solver.state == "completed" && !self.executing(solver_id) && solver.project_id == job.project_id
                            && solver.parent_id == Some(sweep) && solver.input["engine"] == "openmm_argon" && solver.input["parameters"] == run["parameters"],
                            "Actual fallback solver lineage or input changed");
                        let raw_manifest = fs::read(self.path(solver_id,"manifest.json")?)?;
                        let raw_measurements = fs::read(self.path(solver_id,"measurements.json")?)?;
                        ensure!(raw_manifest.len() <= 2*1024*1024 && raw_measurements.len() <= 64*1024*1024, "Fallback evidence exceeds bounded scalar reader");
                        for (path,hash) in [("manifest.json",sha(&raw_manifest)),("measurements.json",sha(&raw_measurements))] {
                            ensure!(case["completion"]["artifacts"].as_array().context("Missing retained artifact receipt")?.iter().any(|pin|pin["path"] == path && pin["sha256"] == hash), "Fallback evidence differs from completed solver receipts");
                        }
                        let mut measured = pressure_window(&serde_json::from_slice(&raw_manifest)?,&serde_json::from_slice(&raw_measurements)?,&run["parameters"],&job.input["query"]["protocol"])?;
                        measured["solver_job_id"] = json!(solver_id); measured["seed"] = run["seed"].clone();
                        measured["attempt_lineage"] = case["attempts"].clone(); measured["parameters"] = run["parameters"].clone();
                        measured["manifest_sha256"] = json!(sha(&raw_manifest)); measured["measurements_sha256"] = json!(sha(&raw_measurements));
                        measured["solver_wall_seconds"] = json!(elapsed(&solver)?); seeds.push(measured);
                    }
                    ensure!(seeds.len() == 3, "A scientific fallback needs all three actual seeds");
                    let means = seeds.iter().map(|seed|seed["pressure_bar"].as_f64().unwrap()).collect::<Vec<_>>();
                    let mean = sum(means.iter().copied()) / 3.0;
                    let sd = (sum(means.iter().map(|value|(value-mean).powi(2))) / 2.0).sqrt();
                    ensure!(mean.is_finite() && sd.is_finite(), "Replicate endpoint overflow");
                    let endpoint = json!({"source_study_id":source_id,"query_job_id":id,"sweep_job_id":sweep,"seeds":seeds,
                        "mean_pressure_bar":mean,"replicate_sd_bar":sd,"replicate_se_bar":sd/3.0_f64.sqrt(),"window":seeds[0]["window"],
                        "scope":"Actual three-seed finite-window solver result. This does not refit, validate or rehabilitate the rejected surrogate; replicate SE is descriptive, not proof of equilibrium or population coverage."});
                    let reduction = self.ml_metadata(id,"query-measured-endpoint",endpoint.clone(),token).await?;
                    result["solver_launched"] = json!(true); result["fallback"] = endpoint; result["fallback"]["reduction_job_id"] = json!(reduction);
                    result["status"] = json!("measured_solver_fallback");
                    result["timing"]["solver_sweep_wall_seconds"] = json!(elapsed(&self.get(sweep)?)?);
                }
            }
        }
        result["retained_bytes"] = json!(self.check_ml_storage(id)?);
        write_json(&self.directory(id).join("result.json"),&result)?;
        self.update(id,|job| { if job.active() && !token.is_cancelled() {
            job.state = "completed".into(); job.result = result; job.progress = json!({"fraction":1.0});
            job.event("completed","Frozen surrogate query completed; decision and any actual solver evidence are retained.",json!({"inference_job_id":inference}));
        } })?;
        Ok(())
    }
}
