//! Spatial field adapter sharing durable scientific jobs and the managed runtime.
use super::{generated::SourceImport, process, safe_relative, write_json, LaboratoryService};
use anyhow::{bail, ensure, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub fn capability() -> Value {
    json!({
        "id":"diffusion_2d","adapter_version":"1.0.0","runtime":"NumPy 2.4.6; Pillow 12.3.0 in the bundled science-v5 environment",
        "scope":"Two-dimensional constant-isotropic-diffusivity passive scalar on a uniform periodic rectangle; synthetic normalized concentration. No reactions, advection, variable diffusion, biological binding, or nonperiodic boundaries.",
        "equations":"dc/dt = D*(d2c/dx2 + d2c/dy2)","method":"float64 conservative five-point FTCS; explicit CFL rejection",
        "parameters":{
            "nx":{"default":64,"min":16,"max":512},"ny":{"default":64,"min":16,"max":512},
            "length_x_um":{"default":10},"length_y_um":{"default":10},"diffusivity_um2_s":{"default":0.2,"min":0},
            "dt_s":{"default":0.005,"constraint":"D*dt*(1/dx^2+1/dy^2) <= 0.5; invalid input fails without adjustment"},
            "steps":{"default":512,"min":1,"max":100000},"record_interval":{"default":8,"max_retained_frames":1001},
            "boundary":{"default":"periodic","allowed":["periodic"]},"max_output_mb":{"default":256,"min":64,"max":2048},
            "initial":{"default":{"kind":"fourier","baseline":1,"amplitude":0.2,"mode_x":1,"mode_y":2},
                "alternatives":[{"kind":"gaussian","baseline":0,"amplitude":1,"center_x_um":5,"center_y_um":5,"sigma_um":0.7},
                    {"kind":"array","path":"imports/initial.npy"}],
                "array_contract":"Finite nonnegative real numeric .npy, exact shape [ny,nx], no pickle; import a retained same-project file first"},
            "probes":{"default":[],"examples":[{"name":"center","kind":"point","x_um":5,"y_um":5},
                {"name":"region","kind":"region","x_min_um":2,"x_max_um":4,"y_min_um":2,"y_max_um":4}]}
        },
        "source_imports":"Optional job input sources:[{job_id,path,destination,sha256?}]; immutable same-project files copied to imports/<destination> before execution",
        "units":{"length":"um","time":"s","diffusivity":"um^2/s","field":"1","integral":"um^2"},
        "instruments":["spatial integral","mean","min/max","variance","Fourier mode amplitude","point/region samples"],
        "outputs":{"fields":"fields/index.json + numeric .npy and bounded JSON views","measurements":"measurements.json","observations":"observations/index.json","checkpoint":"checkpoint.json"},
        "recovery":"Cooperative checkpoint/resume with immutable-input, worker, NumPy and artifact hashes; incompatible/incomplete checkpoints fail explicitly",
        "acceleration":"CPU numerical calculation; field display rendering is separate",
        "validation":"Independent discrete/continuum Fourier, refinement, conservation, control, checkpoint and pixel checks; real-material predictive validity is untested"
    })
}

/// Early dimensional admission. The worker additionally validates every initial
/// array, instrument geometry, numeric bound and remaining parameter.
pub fn validate(parameters: &Value) -> anyhow::Result<()> {
    let values = parameters
        .as_object()
        .context("Diffusion parameters must be an object")?;
    let allowed = [
        "nx",
        "ny",
        "length_x_um",
        "length_y_um",
        "diffusivity_um2_s",
        "dt_s",
        "steps",
        "record_interval",
        "boundary",
        "initial",
        "probes",
        "max_output_mb",
    ];
    ensure!(
        values.keys().all(|key| allowed.contains(&key.as_str())),
        "Unsupported diffusion parameter"
    );
    fn integer(
        values: &serde_json::Map<String, Value>,
        name: &str,
        default: u64,
        low: u64,
        high: u64,
    ) -> anyhow::Result<u64> {
        let value = values
            .get(name)
            .map(|value| value.as_u64().context("Expected integer parameter"))
            .transpose()?
            .unwrap_or(default);
        ensure!(
            (low..=high).contains(&value),
            "{name} must be between {low} and {high}"
        );
        Ok(value)
    }
    fn number(
        values: &serde_json::Map<String, Value>,
        name: &str,
        default: f64,
        low: f64,
        high: f64,
    ) -> anyhow::Result<f64> {
        let value = values
            .get(name)
            .map(|value| value.as_f64().context("Expected numeric parameter"))
            .transpose()?
            .unwrap_or(default);
        ensure!(
            value.is_finite() && (low..=high).contains(&value),
            "{name} is outside the supported range"
        );
        Ok(value)
    }
    let nx = integer(values, "nx", 64, 16, 512)?;
    let ny = integer(values, "ny", 64, 16, 512)?;
    let lx = number(values, "length_x_um", 10., 1e-3, 1e6)?;
    let ly = number(values, "length_y_um", 10., 1e-3, 1e6)?;
    let diffusion = number(values, "diffusivity_um2_s", 0.2, 0., 1e6)?;
    let dt = number(values, "dt_s", 0.005, 1e-12, 1e6)?;
    let steps = integer(values, "steps", 512, 1, 100000)?;
    let interval = integer(values, "record_interval", 8.min(steps), 1, steps)?;
    let budget = integer(values, "max_output_mb", 256, 64, 2048)? * 1024 * 1024;
    let frames = steps.div_ceil(interval) + 1;
    ensure!(
        frames <= 1001,
        "Output would exceed 1001 retained field frames"
    );
    ensure!(
        frames * (nx * ny * 40 + 2048) + 32 * 1024 * 1024 + nx * ny * 16 <= budget,
        "Estimated diffusion output exceeds max_output_mb; explicitly change cadence or budget"
    );
    let cfl = diffusion * dt * ((nx as f64 / lx).powi(2) + (ny as f64 / ly).powi(2));
    ensure!(
        cfl.is_finite() && cfl <= 0.5,
        "Unstable diffusion timestep: CFL {cfl} exceeds 0.5; explicitly choose a smaller dt_s"
    );
    ensure!(
        values
            .get("boundary")
            .is_none_or(|value| value == "periodic"),
        "Only periodic diffusion boundaries are supported"
    );
    if let Some(initial) = values.get("initial") {
        ensure!(initial.is_object(), "initial must be an object");
        let kind = initial
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("fourier");
        ensure!(
            ["fourier", "gaussian", "array"].contains(&kind),
            "Unsupported diffusion initial condition"
        );
        if kind == "array" {
            let path = initial["path"]
                .as_str()
                .context("Array initial condition requires a retained relative path")?;
            safe_relative(path)?;
            ensure!(
                path.starts_with("imports/") && path.ends_with(".npy"),
                "Initial arrays must be copied under imports/ as .npy files"
            );
        }
    }
    Ok(())
}

impl LaboratoryService {
    async fn field_readiness(
        &self,
        id: Uuid,
        python: &Path,
        worker: &Path,
        token: &CancellationToken,
    ) -> anyhow::Result<()> {
        let directory = self
            .directory(id)
            .join("field-readiness")
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&directory)?;
        let job = self.get(id)?;
        let engine = job.input["engine"].as_str().context("Field engine missing")?;
        let parameters = match engine {
            "diffusion_2d" => json!({"nx":16,"ny":16,"steps":2,"record_interval":1,"dt_s":0.005,"diffusivity_um2_s":0.2,"initial":{"kind":"fourier","baseline":1,"amplitude":0.2,"mode_x":1,"mode_y":2}}),
            "heat_conduction_2d" => json!({"nx":16,"ny":16,"steps":2,"record_interval":1}),
            "navier_stokes_2d" => json!({"nx":16,"ny":16,"steps":2,"record_interval":1}),
            _ => bail!("Unsupported field readiness engine"),
        };
        write_json(
            &directory.join("input.json"),
            &json!({"engine":engine,"parameters":parameters}),
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
        let status = process::OwnedProcess::spawn(&mut command, 512)?
            .wait_cooperative(token, &directory.join("cancel.request"))
            .await?;
        ensure!(status.success(), "The actual {engine} readiness computation failed ({status}); its field-readiness receipts are retained");
        let result: Value = serde_json::from_slice(&fs::read(directory.join("result.json"))?)?;
        ensure!(result["status"] == "completed" && result["steps"] == 2, "Field readiness did not complete its numerical steps");
        let (amplitude, drift) = match engine {
            "diffusion_2d" => {
                let a=result["final"]["mode_amplitude"].as_f64().context("Readiness amplitude missing")?;
                let d=result["relative_integral_drift"].as_f64().context("Readiness conservation missing")?;
                ensure!(a>0.19 && a<0.2 && d.abs()<1e-12,"Diffusion readiness must decay and conserve"); (a,d)
            },
            "heat_conduction_2d" => {
                let a=result["final"]["mode_amplitude_K"].as_f64().context("Readiness temperature mode missing")?;
                let d=result["relative_energy_drift"].as_f64().context("Readiness thermal energy missing")?;
                ensure!(a>9.0 && a<10.0 && d.abs()<1e-12,"Thermal readiness must decay and conserve energy"); (a,d)
            },
            "navier_stokes_2d" => {
                let a=result["relative_kinetic_energy_change"].as_f64().context("Readiness kinetic energy missing")?;
                let d=result["final"]["divergence_max_s_inv"].as_f64().context("Readiness divergence missing")?;
                ensure!(a<0.0 && a> -0.1 && d<1e-10,"Flow readiness must dissipate kinetic energy and remain incompressible"); (a,d)
            },
            _=>unreachable!(),
        };
        let path = directory
            .strip_prefix(self.directory(id))?
            .to_string_lossy()
            .replace('\\', "/");
        write_json(
            &self.directory(id).join("field-readiness.json"),
            &json!({"schema_version":1,"status":"passed","engine":engine,"source_directory":path,"steps":2,
                "mode_amplitude":if engine=="diffusion_2d" {Some(amplitude)} else {None},
                "relative_integral_drift":if engine=="diffusion_2d" {Some(drift)} else {None},
                "numerical_result":result,"worker_sha256":format!("{:x}",Sha256::digest(fs::read(worker)?))}),
        )?;
        Ok(())
    }

    fn prepare_field_sources(&self, id: Uuid) -> anyhow::Result<()> {
        let job = self.get(id)?;
        let sources: Vec<SourceImport> = serde_json::from_value(
            job.input
                .get("sources")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )?;
        // Reuse the established source contract's precise path/count/hash validation.
        super::generated::validate_generated_input(
            &json!({"engine":"python_numpy","code":"pass","inputs":{},"sources":sources,"limits":{"memory_mb":128,"process_limit":1,"wall_seconds":null,"storage_mb":64}}),
        )?;
        let root = self.directory(id);
        for source in &sources {
            self.ensure_study_import_access(&job, source.job_id)?;
        }
        let receipt_path = root.join("field-inputs.json");
        let request_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&sources)?));
        if receipt_path.is_file() {
            let receipt = self.read_json(id, "field-inputs.json")?;
            ensure!(
                receipt["request_sha256"] == request_hash,
                "Retained diffusion source request changed"
            );
            for row in receipt["sources"]
                .as_array()
                .context("Diffusion source receipts missing")?
            {
                let path = row["retained_path"]
                    .as_str()
                    .context("Source receipt path missing")?;
                let bytes = read_plain(&root, path, 64 * 1024 * 1024)?;
                ensure!(
                    row["sha256"] == format!("{:x}", Sha256::digest(&bytes)),
                    "Retained diffusion source bytes changed"
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
                "Diffusion input must be an inactive source job in the same project"
            );
            let bytes = read_plain(
                &self.directory(source.job_id),
                &source.path,
                64 * 1024 * 1024,
            )?;
            total += bytes.len();
            ensure!(
                total <= 128 * 1024 * 1024,
                "Diffusion source imports exceed 128 MiB"
            );
            let hash = format!("{:x}", Sha256::digest(&bytes));
            ensure!(
                source
                    .sha256
                    .as_ref()
                    .is_none_or(|expected| expected.eq_ignore_ascii_case(&hash)),
                "Diffusion source SHA256 differs from requested receipt"
            );
            let relative = format!("imports/{}", source.destination);
            let destination = root.join(&relative);
            fs::create_dir_all(destination.parent().unwrap())?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(destination)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            receipts.push(json!({"job_id":source.job_id,"path":source.path,"destination":source.destination,"retained_path":relative,"sha256":hash,"bytes":bytes.len()}));
        }
        write_json(
            &receipt_path,
            &json!({"schema_version":1,"request_sha256":request_hash,"sources":receipts}),
        )?;
        Ok(())
    }

    pub(super) async fn execute_field(
        &self,
        id: Uuid,
        token: &CancellationToken,
    ) -> anyhow::Result<()> {
        self.ensure_science_attempt_identity(id)?;
        let _slot = tokio::select! {_=token.cancelled()=>bail!("Cancelled in field queue"),slot=self.solver_slots.acquire()=>slot?};
        let job = self.get(id)?;
        let engine=job.input["engine"].as_str().context("Field engine missing")?;
        match engine {
            "diffusion_2d"=>validate(&job.input["parameters"])? ,
            "heat_conduction_2d"=>super::thermal::validate(&job.input["parameters"])? ,
            "navier_stokes_2d"=>super::fluid::validate(&job.input["parameters"])? ,
            _=>bail!("Unsupported field engine"),
        }
        ensure!(job.active() && !token.is_cancelled(), "Field job stopped before source verification");
        let root = self.directory(id);
        // Both files are part of immutable continuum identity; no source is updated
        // on resume and no environment/old receipts are touched before this check.
        let worker = root.join(if engine=="diffusion_2d" {"field_worker.py"} else {"continuum_worker.py"});
        if engine!="diffusion_2d" {super::retain_worker_source(&worker,include_bytes!("../../../tools/continuum_worker.py"))?;}
        super::retain_worker_source(&root.join("field_worker.py"), include_bytes!("../../../tools/field_worker.py"))?;
        super::retain_worker_source(&root.join("requirements-science.txt"), include_bytes!("../../../tools/requirements-science.txt"))?;
        let preparing = self.update(id, |job| {
            if job.active() && !token.is_cancelled() {
                job.state = "provisioning".into();
                job.event(
                    "environment",
                    "Checking the pinned numerical field environment.",
                    json!({}),
                );
            }
        })?;
        ensure!(
            preparing.active() && !token.is_cancelled(),
            "Diffusion stopped before preparation"
        );
        let python = self.ensure_environment(id, token).await?;
        ensure!(
            !token.is_cancelled(),
            "Diffusion cancelled before source preparation"
        );
        self.prepare_field_sources(id)?;
        self.event(id, "field_readiness", "Executing a small spatial field and checking decay plus conservation before the requested study.", json!({}))?;
        self.field_readiness(id, &python, &worker, token).await?;
        ensure!(
            !token.is_cancelled(),
            "Diffusion cancelled after readiness check"
        );
        write_json(
            &root.join("worker-input.json"),
            &json!({"engine":engine,"parameters":job.input["parameters"]}),
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
            "Diffusion stopped before process launch"
        );
        let mut child = process::OwnedProcess::spawn(&mut command, 2048)?;
        self.event(id,"solver_started","Integrating spatial numerical fields and recording physical instruments.",json!({"engine":engine,"pid":child.id(),"memory_limit_mb":2048,"worker":"trusted_shipped_adapter"}))?;
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
        // Drain any synchronous read/update already in progress before publishing
        // final job state. read_json closes its file before returning; this join
        // also prevents a late polling update from replacing completion progress.
        let _ = progress.await;
        let status = status?;
        if !status.success() {
            let detail = fs::read_to_string(root.join("stderr.log")).unwrap_or_default();
            bail!(
                "Diffusion worker exited with {status}: {}",
                detail.chars().take(4000).collect::<String>()
            );
        }
        let result = self.read_json(id, "result.json")?;
        ensure!(
            result["status"] == "completed",
            "Diffusion worker did not register a completed numerical result"
        );
        let manifest = self.read_json(id, "manifest.json")?;
        self.update(id, |job| {
            if job.active() && !token.is_cancelled() {
                job.result = result;
                job.state = "completed".into();
                job.progress = json!({"fraction":1.0});
                job.event(
                    "completed",
                    "Spatial fields, instruments, checkpoints and image observations are retained.",
                    json!({"manifest":manifest}),
                );
            }
        })?;
        Ok(())
    }
}

fn read_plain(root: &Path, relative: &str, limit: u64) -> anyhow::Result<Vec<u8>> {
    safe_relative(relative)?;
    let root = root.canonicalize()?;
    let mut path: PathBuf = root.clone();
    for part in relative.split('/') {
        path.push(part);
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Diffusion source links are forbidden"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Diffusion source reparse points are forbidden"
            );
        }
    }
    ensure!(
        path.is_file() && path.canonicalize()?.starts_with(root),
        "Invalid diffusion source file"
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
            "Diffusion source hardlinks are forbidden"
        );
    }
    ensure!(
        file.metadata()?.len() <= limit,
        "Diffusion source exceeds bounded read limit"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Diffusion source grew beyond bounded read limit"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Requires explicitly supplied pinned science interpreter; executes two real continuum computations"]
    async fn continuum_readiness_runs_actual_temperature_and_velocity_channels() {
        use crate::{config::AppConfig,domain::{CreateProjectRequest,ResearchProject},persistence::Database};
        let python=PathBuf::from(std::env::var_os("PHASEFORGE_TEST_PYTHON").expect("Pinned science interpreter required"));
        let temp=tempfile::tempdir().unwrap();
        let config=AppConfig {data_directory:temp.path().into(),..Default::default()};
        let database=Database::open(&config.database_path()).unwrap();
        let project=ResearchProject::new(CreateProjectRequest {name:None,question:"Actual thermal and incompressible readiness".into()});
        database.put_project(&project).unwrap();
        let service=LaboratoryService::new(database,config).unwrap();
        for engine in ["heat_conduction_2d","navier_stokes_2d"] {
            let job=service.create(Uuid::new_v4(),project.id,None,"solver","continuum readiness",json!({"engine":engine,"parameters":{}}),None).unwrap();
            let worker=service.directory(job.id).join("continuum_worker.py");
            fs::write(&worker,include_bytes!("../../../tools/continuum_worker.py")).unwrap();
            fs::write(service.directory(job.id).join("field_worker.py"),include_bytes!("../../../tools/field_worker.py")).unwrap();
            service.field_readiness(job.id,&python,&worker,&CancellationToken::new()).await.unwrap();
            let receipt=service.read_json(job.id,"field-readiness.json").unwrap();
            assert_eq!(receipt["status"],"passed");assert_eq!(receipt["engine"],engine);
            let directory=service.directory(job.id).join(receipt["source_directory"].as_str().unwrap());
            let index:Value=serde_json::from_slice(&fs::read(directory.join("fields/index.json")).unwrap()).unwrap();
            assert_eq!(index["frame_count"],3);
            assert!(index["frames"][2]["time"].as_f64().unwrap()>index["frames"][1]["time"].as_f64().unwrap());
            let state=&index["frames"][2];
            let bytes=fs::read(directory.join(state["state_path"].as_str().unwrap())).unwrap();
            assert_eq!(state["state_sha256"],format!("{:x}",Sha256::digest(bytes)));
            if engine=="navier_stokes_2d" {
                assert_eq!(index["channels"]["velocity_x_m_s"],"m/s");
                assert_eq!(index["channels"]["pressure_Pa"],"Pa");
                assert!(receipt["numerical_result"]["relative_kinetic_energy_change"].as_f64().unwrap()<0.0);
            } else {
                assert_eq!(index["channels"]["temperature_K"],"K");
                assert_eq!(index["channels"]["heat_flux_x_W_m2"],"W/m^2");
            }
        }
    }
    #[tokio::test]
    async fn diffusion_readiness_runs_actual_field_and_instruments() {
        let Some(python) = std::env::var_os("PHASEFORGE_TEST_PYTHON").map(PathBuf::from) else {
            eprintln!("SKIP actual diffusion readiness: PHASEFORGE_TEST_PYTHON is unavailable");
            return;
        };
        use crate::{
            config::AppConfig,
            domain::{CreateProjectRequest, ResearchProject},
            persistence::Database,
        };
        let temp = tempfile::tempdir().unwrap();
        let config = AppConfig {
            data_directory: temp.path().into(),
            ..Default::default()
        };
        let database = Database::open(&config.database_path()).unwrap();
        let project = ResearchProject::new(CreateProjectRequest {
            name: None,
            question: "Actual field readiness acceptance".into(),
        });
        database.put_project(&project).unwrap();
        let service = LaboratoryService::new(database, config).unwrap();
        let job = service
            .create(
                Uuid::new_v4(),
                project.id,
                None,
                "solver",
                "readiness",
                json!({"engine":"diffusion_2d","parameters":{}}),
                None,
            )
            .unwrap();
        let worker = service.directory(job.id).join("field_worker.py");
        fs::write(&worker, include_str!("../../../tools/field_worker.py")).unwrap();
        service
            .field_readiness(job.id, &python, &worker, &CancellationToken::new())
            .await
            .unwrap();
        let receipt = service.read_json(job.id, "field-readiness.json").unwrap();
        assert_eq!(receipt["status"], "passed");
        let source = service
            .directory(job.id)
            .join(receipt["source_directory"].as_str().unwrap());
        let index: Value =
            serde_json::from_slice(&fs::read(source.join("fields/index.json")).unwrap()).unwrap();
        assert_eq!(index["frame_count"], 3);
        assert!(source.join("observations/field-00000002.png").is_file());
        assert!(receipt["mode_amplitude"].as_f64().unwrap() < 0.2);
    }

    #[test]
    fn diffusion_admission_validates_dimensions_stability_and_retention() {
        validate(&json!({})).unwrap();
        validate(&json!({"nx":32,"ny":32,"dt_s":0.02,"steps":128})).unwrap();
        for parameters in [
            json!({"dt_s":1}),
            json!({"boundary":"reflecting"}),
            json!({"nx":2}),
            json!({"steps":10000,"record_interval":1}),
            json!({"initial":{"kind":"array","path":"C:/source.npy"}}),
            json!({"nx":512,"ny":512,"dt_s":0.00001,"record_interval":1}),
        ] {
            assert!(validate(&parameters).is_err(), "{parameters}");
        }
    }
    #[test]
    fn diffusion_sources_are_pinned_and_reused_without_reacquiring_upstream() {
        use crate::{
            config::AppConfig,
            domain::{CreateProjectRequest, ResearchProject},
            persistence::Database,
        };
        let temp = tempfile::tempdir().unwrap();
        let config = AppConfig {
            data_directory: temp.path().into(),
            ..Default::default()
        };
        let database = Database::open(&config.database_path()).unwrap();
        let project = ResearchProject::new(CreateProjectRequest {
            name: None,
            question: "Field source acceptance".into(),
        });
        database.put_project(&project).unwrap();
        let service = LaboratoryService::new(database, config).unwrap();
        let source = service
            .create(
                Uuid::new_v4(),
                project.id,
                None,
                "data",
                "source",
                json!({}),
                None,
            )
            .unwrap();
        service
            .update(source.id, |job| job.state = "completed".into())
            .unwrap();
        fs::write(
            service.directory(source.id).join("source.npy"),
            b"retained numerical input bytes",
        )
        .unwrap();
        let hash = format!("{:x}", Sha256::digest(b"retained numerical input bytes"));
        let job=service.create(Uuid::new_v4(),project.id,None,"solver","field",json!({"engine":"diffusion_2d","parameters":{},"sources":[{"job_id":source.id,"path":"source.npy","destination":"initial.npy","sha256":hash}]}),None).unwrap();
        service.prepare_field_sources(job.id).unwrap();
        fs::write(
            service.directory(source.id).join("source.npy"),
            b"changed upstream",
        )
        .unwrap();
        service.prepare_field_sources(job.id).unwrap();
        assert_eq!(
            fs::read(service.directory(job.id).join("imports/initial.npy")).unwrap(),
            b"retained numerical input bytes"
        );
        fs::write(
            service.directory(job.id).join("imports/initial.npy"),
            b"tampered local input",
        )
        .unwrap();
        assert!(service.prepare_field_sources(job.id).is_err());
    }
}
