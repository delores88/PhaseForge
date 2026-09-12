//! Bounded, isolated-boundary Newtonian mechanics through the shared LabJob path.
use super::{generated::SourceImport, process, safe_relative, write_json, LaboratoryService};
use anyhow::{bail, ensure, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn capability() -> Value {
    json!({"id":"newtonian_nbody","adapter_version":"1.0.0",
        "runtime":"NumPy 2.4.6 and Pillow 12.3.0 in the bundled science-v5 environment",
        "scope":"2–128 interacting point masses in isolated Cartesian 3D space; unsoftened Newtonian gravity G*=1 in explicitly scaled units. No collisions/mergers, relativity, ephemeris accuracy or general closed-form three-body solution.",
        "equations":"a_i=sum(j!=i) m_j*(q_j-q_i)/|q_j-q_i|^3; every mass responds to every other mass",
        "method":"fixed-step float64 velocity-Verlet; full-step positions and velocities; explicit collision/resolution refusal",
        "parameters":{
            "body_ids":{"required":true,"count":[2,128],"description":"ordered unique nonempty IDs, each <=80 characters"},
            "masses":{"shape":"[N]","range_M0":[1e-9,1e9]},
            "positions":{"shape":"[N,3]","unit":"L0","max_absolute_component":1e6},
            "velocities":{"shape":"[N,3]","unit":"L0/T0","max_absolute_component":1e6},
            "initial_state":{"alternative_to":"masses, positions, velocities","shape":"numeric-only .npz containing exactly those three arrays","example":{"path":"imports/initial.npz","sha256":"expected 64-character digest"}},
            "timestep":{"default":0.001,"unit":"T0","min":1e-12,"max":1e6},
            "steps":{"default":1000,"min":1,"max":200000},
            "sample_interval":{"default":10,"max_retained_frames":10001},"chunk_frames":{"default":50,"max":100,"semantics":"upper bound only; version 1.0 publishes exactly one retained state per immutable chunk, independent of this value; batching is not implemented"},
            "min_separation":{"default":0.05,"unit":"L0","min":1e-12,"max":1e6},
            "boundary":{"default":"isolated","allowed":["isolated"]},
            "unit_system":{"default":"scaled_G1","description":"G*=1; L0,M0,T0 with T0=sqrt(L0^3/(Gphysical*M0)); no implicit SI assignment"}},
        "guards":"Every force endpoint and numerical drift segment respects min_separation. Endpoint h*sqrt((m_i+m_j)/r^3)<=0.03 and h*|v_i-v_j|/r<=0.03; failure preserves the last valid checkpoint without changing h or equations.",
        "budget":{"max_pair_evaluations":200000000,"max_output_bytes":536870912,"max_estimated_working_bytes":134217728},
        "source_imports":"Optional job input sources:[{job_id,path,destination,sha256?}]; immutable inactive same-project files copied to imports/<destination>",
        "instruments":["kinetic/potential/total energy","momentum","angular momentum","center of mass","minimum pair separation","all pair distances, radial velocities and xy relative angles for N<=16"],
        "outputs":{"trajectory":"one retained state per immutable JSON + float64 NPZ chunk; no wrapping or periodic box","instruments":"measurements.json with source-array hashes","images":"observations/index.json with exact stored-frame provenance","checkpoint":"checkpoint.json and verified numeric state"},
        "recovery":"Cooperative checkpoint/resume with exact input/source/worker/runtime identity. Changed or incomplete identities require a new attempt; prior published chunks stay immutable.",
        "acceleration":"CPU numerical calculation; GPU rendering is a separate presentation setting",
        "validation":"Preregistered analytic binary/equilateral references, velocity intervention, conservation/refinement and independent artifact checks; empirical astrophysical validity remains untested"})
}

fn integer(
    values: &serde_json::Map<String, Value>,
    key: &str,
    default: u64,
    low: u64,
    high: u64,
) -> anyhow::Result<u64> {
    let value = values
        .get(key)
        .map(|v| v.as_u64().context(format!("{key} must be an integer")))
        .transpose()?
        .unwrap_or(default);
    ensure!(
        (low..=high).contains(&value),
        "{key} must be between {low} and {high}"
    );
    Ok(value)
}

fn number(
    values: &serde_json::Map<String, Value>,
    key: &str,
    default: f64,
    low: f64,
    high: f64,
) -> anyhow::Result<f64> {
    let value = values
        .get(key)
        .map(|v| v.as_f64().context(format!("{key} must be numeric")))
        .transpose()?
        .unwrap_or(default);
    ensure!(
        value.is_finite() && (low..=high).contains(&value),
        "{key} is outside its finite supported range"
    );
    Ok(value)
}

fn vector_rows(value: &Value, count: usize, name: &str) -> anyhow::Result<Vec<[f64; 3]>> {
    let rows = value
        .as_array()
        .context(format!("{name} must be an N by 3 array"))?;
    ensure!(rows.len() == count, "{name} must have one row per body");
    rows.iter()
        .map(|row| {
            let components = row
                .as_array()
                .context(format!("{name} rows must be numeric vectors"))?;
            ensure!(
                components.len() == 3,
                "{name} rows require three Cartesian components"
            );
            let mut result = [0.; 3];
            for (i, value) in components.iter().enumerate() {
                result[i] = value
                    .as_f64()
                    .context(format!("{name} must contain numbers"))?;
                ensure!(
                    result[i].is_finite() && result[i].abs() <= 1e6,
                    "{name} components must be finite and at most 1e6 in magnitude"
                );
            }
            Ok(result)
        })
        .collect()
}

/// Early admission checks data-only arguments. The worker repeats validation,
/// bounds archive contents before loading, and applies guards at every step.
pub fn validate(parameters: &Value) -> anyhow::Result<()> {
    let p = parameters
        .as_object()
        .context("Mechanics parameters must be an object")?;
    let allowed = [
        "body_ids",
        "masses",
        "positions",
        "velocities",
        "initial_state",
        "timestep",
        "steps",
        "sample_interval",
        "chunk_frames",
        "min_separation",
        "boundary",
        "unit_system",
    ];
    ensure!(p.keys().all(|key|allowed.contains(&key.as_str())),"Unsupported mechanics parameter; softening, fixed central masses and periodic boundaries are not this model");
    let ids = p
        .get("body_ids")
        .and_then(Value::as_array)
        .context("body_ids is required")?;
    let count = ids.len();
    ensure!(
        (2..=128).contains(&count),
        "Mechanics requires 2–128 bodies"
    );
    let mut unique = std::collections::BTreeSet::new();
    for id in ids {
        let id = id.as_str().context("Body IDs must be strings")?;
        ensure!(
            !id.trim().is_empty() && id.chars().count() <= 80 && !id.chars().any(char::is_control),
            "Body IDs must contain 1–80 visible characters"
        );
        ensure!(unique.insert(id), "Body IDs must be unique");
    }
    let h = number(p, "timestep", 0.001, 1e-12, 1e6)?;
    let min_separation = number(p, "min_separation", 0.05, 1e-12, 1e6)?;
    let steps = integer(p, "steps", 1000, 1, 200000)?;
    let interval = integer(p, "sample_interval", 10, 1, 200000)?;
    let chunk = integer(p, "chunk_frames", 50, 1, 100)?;
    let n = count as u64;
    let pairs = n * (n - 1) / 2;
    let frames = steps.div_ceil(interval) + 1;
    ensure!(
        frames <= 10001,
        "Mechanics output would exceed 10001 retained states"
    );
    ensure!(
        (steps + 1) * pairs <= 200_000_000,
        "Mechanics pair-evaluation budget exceeded"
    );
    let instrument_pairs = if count <= 16 { pairs } else { 0 };
    let output = 32 * 1024 * 1024 + frames * (512 * n + 384 * instrument_pairs + 4096);
    let working = 16 * 1024 * 1024 + n * n * 128 + chunk * n * 9 * 8;
    ensure!(
        output <= 512 * 1024 * 1024,
        "Estimated mechanics output exceeds 512 MiB; explicitly change cadence or study size"
    );
    ensure!(
        working <= 128 * 1024 * 1024,
        "Estimated mechanics working arrays exceed 128 MiB"
    );
    ensure!(
        p.get("boundary").is_none_or(|value| value == "isolated"),
        "Only isolated nonperiodic mechanics boundaries are supported"
    );
    ensure!(
        p.get("unit_system")
            .is_none_or(|value| value == "scaled_G1"),
        "Mechanics inputs must explicitly use scaled_G1 units"
    );
    if let Some(initial) = p.get("initial_state") {
        ensure!(
            !["masses", "positions", "velocities"]
                .iter()
                .any(|key| p.contains_key(*key)),
            "Choose inline initial arrays or initial_state, not both"
        );
        let source = initial
            .as_object()
            .context("initial_state must contain path and sha256")?;
        ensure!(
            source
                .keys()
                .all(|key| ["path", "sha256"].contains(&key.as_str())),
            "Unsupported initial_state field"
        );
        let path = source
            .get("path")
            .and_then(Value::as_str)
            .context("Initial archive path missing")?;
        safe_relative(path)?;
        ensure!(
            path.starts_with("imports/") && path.ends_with(".npz"),
            "Initial archives must be retained imports/*.npz files"
        );
        let hash = source
            .get("sha256")
            .and_then(Value::as_str)
            .context("Initial archive SHA256 is required")?;
        ensure!(
            hash.len() == 64
                && hash
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "Initial archive SHA256 must be a 64-character lowercase digest"
        );
    } else {
        let masses = p
            .get("masses")
            .and_then(Value::as_array)
            .context("masses is required for inline initial state")?;
        ensure!(masses.len() == count, "masses must have one entry per body");
        let masses = masses
            .iter()
            .map(|value| {
                let mass = value.as_f64().context("Mass must be numeric")?;
                ensure!(
                    mass.is_finite() && (1e-9..=1e9).contains(&mass),
                    "Mass must be finite and between 1e-9 and 1e9 M0"
                );
                Ok(mass)
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let q = vector_rows(
            p.get("positions").context("positions missing")?,
            count,
            "positions",
        )?;
        let v = vector_rows(
            p.get("velocities").context("velocities missing")?,
            count,
            "velocities",
        )?;
        for i in 0..count {
            for j in i + 1..count {
                let distance = (0..3)
                    .map(|axis| (q[j][axis] - q[i][axis]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                let speed = (0..3)
                    .map(|axis| (v[j][axis] - v[i][axis]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                ensure!(
                    distance > min_separation,
                    "Initial bodies {} and {} violate min_separation",
                    ids[i],
                    ids[j]
                );
                ensure!(h*((masses[i]+masses[j])/distance.powi(3)).sqrt()<=0.03&&h*speed/distance<=0.03,
                "Initial pair {} / {} is unresolved at this timestep; explicitly choose a smaller timestep",ids[i],ids[j]);
            }
        }
    }
    Ok(())
}

impl LaboratoryService {
    async fn mechanics_readiness(
        &self,
        id: Uuid,
        python: &Path,
        worker: &Path,
        token: &CancellationToken,
    ) -> anyhow::Result<()> {
        ensure!(
            !token.is_cancelled() && self.get(id)?.active(),
            "Mechanics stopped before readiness"
        );
        let root = self.directory(id);
        let directory = root
            .join("mechanics-readiness")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&directory)?;
        write_json(
            &directory.join("input.json"),
            &json!({"engine":"newtonian_nbody","parameters":{
            "body_ids":["left","right"],"masses":[0.5,0.5],"positions":[[-0.5,0.,0.],[0.5,0.,0.]],
            "velocities":[[0.,-0.5,0.],[0.,0.5,0.]],"timestep":0.001,"steps":2,"sample_interval":1,
            "chunk_frames":50,"min_separation":0.05,"boundary":"isolated","unit_system":"scaled_G1"}}),
        )?;
        let mut command = process::clean_command(python, &directory);
        command
            .args(["-I", "-B"])
            .arg(worker)
            .arg("--input")
            .arg(directory.join("input.json"))
            .arg("--output")
            .arg(&directory)
            .stdout(fs::File::create(directory.join("stdout.log"))?)
            .stderr(fs::File::create(directory.join("stderr.log"))?);
        ensure!(
            !token.is_cancelled() && self.get(id)?.active(),
            "Mechanics stopped before readiness process launch"
        );
        let status = process::OwnedProcess::spawn(&mut command, 512)?
            .wait_cooperative(token, &directory.join("cancel.request"))
            .await?;
        ensure!(status.success(),"Actual mechanics readiness failed ({status}); retained mechanics-readiness receipts explain it");
        let result: Value = serde_json::from_slice(&fs::read(directory.join("result.json"))?)?;
        let measured: Value =
            serde_json::from_slice(&fs::read(directory.join("measurements.json"))?)?;
        let rows = measured["series"]
            .as_array()
            .context("Mechanics readiness instruments missing")?;
        ensure!(
            rows.len() == 3
                && result["status"] == "completed"
                && result["engine"] == "newtonian_nbody",
            "Mechanics readiness must retain three actual states"
        );
        let energy = rows[2]["total_energy"]
            .as_f64()
            .context("Readiness energy missing")?;
        let separation = rows[2]["min_separation"]
            .as_f64()
            .context("Readiness separation missing")?;
        ensure!(
            (energy + 0.125).abs() < 1e-7
                && (separation - 1.).abs() < 1e-7
                && rows[2]["time"] == json!(0.002),
            "Mechanics readiness invariant/time check failed"
        );
        let index: Value =
            serde_json::from_slice(&fs::read(directory.join("trajectory/index.json"))?)?;
        let last = index["chunks"]
            .as_array()
            .and_then(|chunks| chunks.last())
            .context("Readiness trajectory missing")?;
        let chunk: Value = serde_json::from_slice(&read_plain(
            &directory,
            last["path"].as_str().context("Readiness chunk missing")?,
            16 * 1024 * 1024,
        )?)?;
        let frame = chunk["frames"]
            .as_array()
            .and_then(|frames| frames.last())
            .context("Readiness final frame missing")?;
        let y = frame["entities"][0]["position"][1]
            .as_f64()
            .context("Readiness numerical position missing")?;
        ensure!(
            y < -0.0009 && y > -0.0011,
            "Mechanics readiness did not move the interacting bodies"
        );
        let relative = directory
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        write_json(
            &root.join("mechanics-readiness.json"),
            &json!({"schema_version":1,"status":"passed","source_directory":relative,
            "steps":2,"total_energy":energy,"min_separation":separation,"first_body_y":y,"worker_sha256":format!("{:x}",Sha256::digest(fs::read(worker)?))}),
        )?;
        Ok(())
    }

    fn prepare_mechanics_sources(&self, id: Uuid) -> anyhow::Result<()> {
        let job = self.get(id)?;
        let sources: Vec<SourceImport> = serde_json::from_value(
            job.input
                .get("sources")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )?;
        super::generated::validate_generated_input(
            &json!({"engine":"python_numpy","code":"pass","inputs":{},"sources":sources,
            "limits":{"memory_mb":128,"process_limit":1,"wall_seconds":null,"storage_mb":64}}),
        )?;
        for source in &sources {
            self.ensure_study_import_access(&job, source.job_id)?;
        }
        let root = self.directory(id);
        let receipt_path = root.join("mechanics-inputs.json");
        let request_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&sources)?));
        if receipt_path.is_file() {
            let receipt = self.read_json(id, "mechanics-inputs.json")?;
            ensure!(
                receipt["request_sha256"] == request_hash,
                "Retained mechanics source request changed"
            );
            for row in receipt["sources"]
                .as_array()
                .context("Mechanics source receipts missing")?
            {
                let bytes = read_plain(
                    &root,
                    row["retained_path"]
                        .as_str()
                        .context("Source path missing")?,
                    64 * 1024 * 1024,
                )?;
                ensure!(
                    row["sha256"] == format!("{:x}", Sha256::digest(bytes)),
                    "Retained mechanics input bytes changed"
                );
            }
            return Ok(());
        }
        let mut total = 0;
        let mut receipts = vec![];
        for source in sources {
            let origin = self.get(source.job_id)?;
            ensure!(
                origin.project_id == job.project_id
                    && !origin.active()
                    && !self.executing(origin.id),
                "Mechanics input must be an inactive source job in the same project"
            );
            let bytes = read_plain(&self.directory(origin.id), &source.path, 64 * 1024 * 1024)?;
            total += bytes.len();
            ensure!(
                total <= 128 * 1024 * 1024,
                "Mechanics source imports exceed 128 MiB"
            );
            let hash = format!("{:x}", Sha256::digest(&bytes));
            ensure!(
                source
                    .sha256
                    .as_ref()
                    .is_none_or(|expected| expected.eq_ignore_ascii_case(&hash)),
                "Mechanics source SHA256 differs from requested receipt"
            );
            let relative = format!("imports/{}", source.destination);
            retain_immutable(&root, &relative, &bytes)?;
            receipts.push(json!({"job_id":source.job_id,"path":source.path,"retained_path":relative,"sha256":hash,"bytes":bytes.len()}));
        }
        write_json(
            &receipt_path,
            &json!({"schema_version":1,"request_sha256":request_hash,"sources":receipts}),
        )?;
        Ok(())
    }

    pub(super) async fn execute_mechanics(
        &self,
        id: Uuid,
        token: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.ensure_science_attempt_identity(id)?;
        let _slot = tokio::select! {_=token.cancelled()=>bail!("Cancelled in mechanics queue"),slot=self.solver_slots.acquire()=>slot?};
        let job = self.get(id)?;
        validate(&job.input["parameters"])?;
        ensure!(job.active() && !token.is_cancelled(), "Mechanics stopped before source verification");
        let root = self.directory(id);
        let worker = root.join("mechanics_worker.py");
        super::retain_worker_source(&worker, include_bytes!("../../../tools/mechanics_worker.py"))?;
        super::retain_worker_source(&root.join("requirements-science.txt"), include_bytes!("../../../tools/requirements-science.txt"))?;
        let preparing = self.update(id, |job| {
            if job.active() && !token.is_cancelled() {
                job.state = "provisioning".into();
                job.event(
                    "environment",
                    "Checking the pinned mechanics environment.",
                    json!({}),
                );
            }
        })?;
        ensure!(
            preparing.active() && !token.is_cancelled(),
            "Mechanics stopped before preparation"
        );
        let python = self.ensure_environment(id, token).await?;
        ensure!(
            !token.is_cancelled(),
            "Mechanics cancelled before source preparation"
        );
        self.prepare_mechanics_sources(id)?;
        ensure!(
            !token.is_cancelled() && self.get(id)?.active(),
            "Mechanics stopped during source preparation"
        );
        self.event(
            id,
            "mechanics_readiness",
            "Executing interacting bodies and checking their actual movement plus invariants.",
            json!({}),
        )?;
        self.mechanics_readiness(id, &python, &worker, token)
            .await?;
        ensure!(
            !token.is_cancelled(),
            "Mechanics cancelled after readiness check"
        );
        write_json(
            &root.join("worker-input.json"),
            &json!({"engine":"newtonian_nbody","parameters":job.input["parameters"]}),
        )?;
        let mut command = process::clean_command(&python, &root);
        command
            .args(["-I", "-B"])
            .arg(&worker)
            .arg("--input")
            .arg(root.join("worker-input.json"))
            .arg("--output")
            .arg(&root)
            .stdout(fs::File::create(root.join("stdout.log"))?)
            .stderr(fs::File::create(root.join("stderr.log"))?);
        let starting = self.update(id, |job| {
            if job.active() && !token.is_cancelled() {
                job.state = "running".into();
            }
        })?;
        ensure!(
            starting.active() && !token.is_cancelled(),
            "Mechanics stopped before process launch"
        );
        let mut child = process::OwnedProcess::spawn(&mut command, 512)?;
        self.event(id,"solver_started","Integrating interacting Newtonian bodies and retaining measured states.",json!({"engine":"newtonian_nbody","pid":child.id(),"memory_limit_mb":512,"worker":"trusted_shipped_adapter"}))?;
        let progress_service = self.clone();
        let progress_token = token.clone();
        let progress = tokio::spawn(async move {
            loop {
                tokio::select! {_=progress_token.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(250))=>{}}
                if let Ok(value) = progress_service.read_json(id, "progress.json") {
                    let _ = progress_service.update(id, |job| job.progress = value);
                }
            }
        });
        let status = child
            .wait_cooperative(token, &root.join("cancel.request"))
            .await;
        progress.abort();
        let _ = progress.await;
        let status = status?;
        if !status.success() {
            let detail = fs::read_to_string(root.join("stderr.log")).unwrap_or_default();
            bail!(
                "Mechanics worker exited with {status}: {}",
                detail.chars().take(4000).collect::<String>()
            );
        }
        let result = self.read_json(id, "result.json")?;
        ensure!(
            result["status"] == "completed" && result["engine"] == "newtonian_nbody",
            "Mechanics did not register a completed numerical result"
        );
        let manifest = self.read_json(id, "manifest.json")?;
        self.update(id,|job|{if job.active()&&!token.is_cancelled(){job.result=result;job.state="completed".into();job.progress=json!({"fraction":1.0});
            job.event("completed","Computed trajectories, invariants, checkpoints and stored-state images are retained.",json!({"manifest":manifest}));}})?;
        Ok(())
    }
}

fn retain_immutable(root: &Path, relative: &str, bytes: &[u8]) -> anyhow::Result<()> {
    safe_relative(relative)?;
    let destination = root.join(relative);
    if destination.exists() {
        ensure!(read_plain(root, relative, bytes.len() as u64)? == bytes,
            "Retained mechanics file {relative} differs; use a new attempt instead of overwriting provenance");
        return Ok(());
    }
    fs::create_dir_all(
        destination
            .parent()
            .context("Retained file parent missing")?,
    )?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    output.write_all(bytes)?;
    output.sync_all()?;
    Ok(())
}

fn read_plain(root: &Path, relative: &str, limit: u64) -> anyhow::Result<Vec<u8>> {
    safe_relative(relative)?;
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for component in relative.split('/') {
        path.push(component);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Mechanics source links are forbidden"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Mechanics source reparse points are forbidden"
            );
        }
    }
    ensure!(
        path.is_file() && path.canonicalize()?.starts_with(&root),
        "Mechanics source escapes its job"
    );
    let file = fs::File::open(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        ensure!(
            unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } != 0
                && info.nNumberOfLinks == 1,
            "Mechanics source hardlinks are forbidden"
        );
    }
    ensure!(
        file.metadata()?.len() <= limit,
        "Mechanics source exceeds its read limit"
    );
    let mut bytes = vec![];
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Mechanics source grew beyond its read limit"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AppConfig,
        domain::{CreateProjectRequest, ResearchProject},
        persistence::Database,
    };
    use std::path::PathBuf;
    fn binary() -> Value {
        json!({"body_ids":["a","b"],"masses":[0.5,0.5],"positions":[[-0.5,0.,0.],[0.5,0.,0.]],"velocities":[[0.,-0.5,0.],[0.,0.5,0.]]})
    }
    fn service(root: &Path) -> (LaboratoryService, Uuid) {
        let config = AppConfig {
            data_directory: root.into(),
            ..Default::default()
        };
        let db = Database::open(&config.database_path()).unwrap();
        let project = ResearchProject::new(CreateProjectRequest {
            name: None,
            question: "Mechanics adapter validation".into(),
        });
        db.put_project(&project).unwrap();
        (LaboratoryService::new(db, config).unwrap(), project.id)
    }
    #[test]
    fn mechanics_admission_rejects_changed_equations_unresolved_pairs_and_bad_shapes() {
        validate(&binary()).unwrap();
        for (key, value) in [
            ("masses", json!([0., 0.5])),
            ("positions", json!([[0., 0.], [1., 0.]])),
            ("body_ids", json!(["a", "a"])),
            ("boundary", json!("periodic")),
            ("unit_system", json!("nm_ps")),
            ("softening", json!(0.01)),
            ("timestep", json!(0.1)),
            ("steps", json!(200001)),
            ("sample_interval", json!(0)),
        ] {
            let mut p = binary();
            p[key] = value;
            assert!(validate(&p).is_err(), "{key}: {p}");
        }
        let mut overlapping = binary();
        overlapping["positions"] = json!([[0., 0., 0.], [0.01, 0., 0.]]);
        assert!(validate(&overlapping).is_err());
        let mut oversized = binary();
        oversized["steps"] = json!(10001);
        oversized["sample_interval"] = json!(1);
        assert!(validate(&oversized).is_err());
        let archive = json!({"body_ids":["a","b"],"initial_state":{"path":"imports/initial.npz","sha256":"0".repeat(64)}});
        validate(&archive).unwrap();
        let mut uppercase = archive.clone();
        uppercase["initial_state"]["sha256"] = json!("A".repeat(64));
        assert!(validate(&uppercase).is_err());
        let mut escaped = archive;
        escaped["initial_state"]["path"] = json!("../outside.npz");
        assert!(validate(&escaped).is_err());
    }
    #[test]
    fn mechanics_imports_reuse_retained_bytes_and_fail_on_changed_pins() {
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let source = service
            .create(
                Uuid::new_v4(),
                project,
                None,
                "data",
                "source",
                json!({}),
                None,
            )
            .unwrap();
        let bytes = b"retained numerical source bytes";
        fs::write(service.directory(source.id).join("state.npz"), bytes).unwrap();
        service
            .update(source.id, |job| job.state = "completed".into())
            .unwrap();
        let input = json!({"engine":"newtonian_nbody","parameters":binary(),"sources":[{"job_id":source.id,"path":"state.npz","destination":"initial.npz","sha256":format!("{:x}",Sha256::digest(bytes))}]});
        let job = service
            .create(
                Uuid::new_v4(),
                project,
                None,
                "solver",
                "import",
                input,
                None,
            )
            .unwrap();
        // Emulate a stopped copy before its aggregate receipt was published.
        retain_immutable(&service.directory(job.id), "imports/initial.npz", bytes).unwrap();
        service.prepare_mechanics_sources(job.id).unwrap();
        fs::write(
            service.directory(source.id).join("state.npz"),
            b"changed upstream",
        )
        .unwrap();
        service.prepare_mechanics_sources(job.id).unwrap();
        assert_eq!(
            fs::read(service.directory(job.id).join("imports/initial.npz")).unwrap(),
            bytes
        );
        fs::write(
            service.directory(job.id).join("imports/initial.npz"),
            b"changed retained input",
        )
        .unwrap();
        assert!(service.prepare_mechanics_sources(job.id).is_err());
    }
    #[test]
    fn mechanics_resume_preserves_original_worker_and_dependency_sources() {
        let temp = tempfile::tempdir().unwrap();
        retain_immutable(temp.path(), "mechanics_worker.py", b"original worker").unwrap();
        retain_immutable(temp.path(), "mechanics_worker.py", b"original worker").unwrap();
        assert!(retain_immutable(temp.path(), "mechanics_worker.py", b"changed worker").is_err());
        assert_eq!(
            fs::read(temp.path().join("mechanics_worker.py")).unwrap(),
            b"original worker"
        );
    }
    #[tokio::test]
    async fn mechanics_readiness_executes_actual_interacting_state() {
        let Some(python) = std::env::var_os("PHASEFORGE_TEST_PYTHON").map(PathBuf::from) else {
            eprintln!("SKIP actual mechanics readiness: PHASEFORGE_TEST_PYTHON unavailable");
            return;
        };
        let temp = tempfile::tempdir().unwrap();
        let (service, project) = service(temp.path());
        let job = service
            .create(
                Uuid::new_v4(),
                project,
                None,
                "solver",
                "readiness",
                json!({"engine":"newtonian_nbody","parameters":binary()}),
                None,
            )
            .unwrap();
        let root = service.directory(job.id);
        let worker = root.join("mechanics_worker.py");
        fs::write(&worker, include_str!("../../../tools/mechanics_worker.py")).unwrap();
        fs::write(
            root.join("requirements-science.txt"),
            include_str!("../../../tools/requirements-science.txt"),
        )
        .unwrap();
        service
            .mechanics_readiness(job.id, &python, &worker, &CancellationToken::new())
            .await
            .unwrap();
        let receipt = service
            .read_json(job.id, "mechanics-readiness.json")
            .unwrap();
        assert_eq!(receipt["status"], "passed");
        assert_eq!(receipt["steps"], 2);
        assert!(receipt["first_body_y"].as_f64().unwrap() < -0.0009);
        let stopped = CancellationToken::new();
        stopped.cancel();
        assert!(service
            .mechanics_readiness(job.id, &python, &worker, &stopped)
            .await
            .is_err());
        assert_eq!(
            fs::read_dir(root.join("mechanics-readiness"))
                .unwrap()
                .count(),
            1,
            "A cancelled readiness request must create no second execution attempt"
        );
    }
}
