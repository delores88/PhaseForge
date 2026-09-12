//! Optional, pinned Einstein-equation workers. Benchmarks are never merger evidence.
use std::{path::{Path, PathBuf}, process::Stdio, time::Duration};
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use super::{LaboratoryService, write_json};
use super::nr_engines;

const ENGINE: &str = "athenak_gauge_wave";
const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRegistration {
    pub schema: String,
    pub engine_id: String,
    pub transport: String,
    pub distribution: Option<String>,
    pub linux_root: String,
    pub engine_manifest_sha256: String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub gpu_device_uuid: Option<String>,
}

pub(super) fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn linux_path(path: &str) -> anyhow::Result<()> {
    ensure!(path.starts_with('/') && path.len() > 1 && !path.contains(['\\', '\0', '\r', '\n']), "Expected an absolute POSIX runtime path");
    ensure!(path.split('/').skip(1).all(|part| !part.is_empty() && part != "." && part != ".."), "Runtime path must be canonical");
    Ok(())
}

impl RuntimeRegistration {
    pub(super) fn validate(&self) -> anyhow::Result<()> {
        ensure!(self.schema == "phaseforge.nr-runtime.v1" && nr_engines::supports(&self.engine_id), "Unsupported NR runtime registration");
        linux_path(&self.linux_root)?;
        ensure!(self.engine_manifest_sha256.len() == 64 && self.engine_manifest_sha256.bytes().all(|b| b.is_ascii_hexdigit()), "An exact engine manifest SHA256 is required");
        if self.engine_id==nr_engines::TWO_PUNCTURES_CUDA {
            ensure!(self.transport=="wsl2","This CUDA transport currently requires the verified Windows WSL driver bridge");
            let device=self.gpu_device_uuid.as_deref().context("CUDA runtime must bind its inspected physical GPU UUID")?;
            let id=Uuid::parse_str(device.strip_prefix("GPU-").context("GPU UUID prefix missing")?)?;
            ensure!(device==format!("GPU-{id}"),"GPU UUID must be exact and canonical");
        } else {ensure!(self.gpu_device_uuid.is_none(),"A CPU runtime must not carry a GPU execution binding");}
        match self.transport.as_str() {
            "wsl2" => {
                ensure!(cfg!(windows), "WSL2 execution requires Windows");
                let distro = self.distribution.as_deref().context("WSL distribution is required")?;
                ensure!(!distro.is_empty() && distro.len() <= 80 && distro.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)), "Invalid fixed WSL distribution");
            },
            "linux" => ensure!(cfg!(target_os = "linux") && self.distribution.is_none(), "Native execution currently requires Linux"),
            _ => anyhow::bail!("The installed NR transport is unsupported on this host"),
        }
        Ok(())
    }
    pub(super) fn host_root(&self) -> PathBuf {
        if self.transport == "wsl2" {
            PathBuf::from(format!("\\\\wsl.localhost\\{}{}", self.distribution.as_deref().unwrap_or_default(), self.linux_root.replace('/', "\\")))
        } else { PathBuf::from(&self.linux_root) }
    }
    fn command(&self, script: &str) -> tokio::process::Command {
        let mut command = if self.transport == "wsl2" {
            // This path and argv are owned by the application, never model text.
            let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\Windows"));
            let mut cmd = tokio::process::Command::new(system.join("System32/wsl.exe"));
            cmd.args(["--distribution", self.distribution.as_deref().unwrap_or_default(), "--user", "root", "--exec", "/usr/bin/python3"]);
            cmd
        } else { tokio::process::Command::new("/usr/bin/python3") };
        command.env_clear();
        for name in ["SystemRoot", "WINDIR"] { if let Some(value) = std::env::var_os(name) { command.env(name, value); } }
        command.args(["-I", "-B"]).arg(format!("{}/tools/{script}", self.linux_root));
        command.stdin(Stdio::piped()).kill_on_drop(true);
        #[cfg(windows)] command.creation_flags(0x08000000);
        command
    }
}

pub(super) fn bounded_file(path: &Path, maximum: u64) -> anyhow::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path).context("Installed NR runtime file is unavailable")?;
    ensure!(metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= maximum, "NR file is not a bounded ordinary file");
    let bytes = std::fs::read(path)?;
    ensure!(bytes.len() as u64 <= maximum, "NR file grew beyond its limit");
    Ok(bytes)
}

fn registration(service: &LaboratoryService, engine: &str) -> anyhow::Result<(RuntimeRegistration, Value)> {
    ensure!(nr_engines::supports(engine),"Unsupported NR engine");
    let path = service.config.data_directory.join("environments/numerical-relativity").join(engine).join("runtime.json");
    let runtime: RuntimeRegistration = serde_json::from_slice(&bounded_file(&path, 64 * 1024)?)?;
    runtime.validate()?;
    ensure!(runtime.engine_id==engine,"Requested and registered NR engines differ");
    let root = runtime.host_root();
    let bytes = bounded_file(&root.join("engine.json"), 64 * 1024)?;
    ensure!(hash(&bytes) == runtime.engine_manifest_sha256, "Installed NR engine manifest changed");
    let manifest: Value = serde_json::from_slice(&bytes)?;
    nr_engines::validate_manifest(engine,&manifest)?;
    for (name, expected) in [("nr_stage.py", include_bytes!("../../../tools/nr_stage.py").as_slice()), ("nr_supervisor.py", include_bytes!("../../../tools/nr_supervisor.py").as_slice())] {
        ensure!(bounded_file(&root.join("tools").join(name), 256 * 1024)? == expected, "Installed NR helper {name} differs from this application");
    }
    if engine==nr_engines::TWO_PUNCTURES_CUDA {
        let source=include_bytes!("../../../tools/nr_gpu.py");
        ensure!(bounded_file(&root.join("tools/nr_gpu.py"),256*1024)?==source && manifest["build"]["cuda_runtime"]["guard_source_sha256"]==hash(source),"Installed GPU guard differs from the exact application and engine-manifest pin");
    }
    Ok((runtime, manifest))
}

pub fn capability(service: &LaboratoryService, engine: &str) -> Value {
    let available = registration(service,engine);
    json!({"id":engine,"adapter_version":"0.2.0","available":available.is_ok(),
        "availability_reason":available.err().map(|e|format!("{e:#}")),
        "scope":nr_engines::scope(engine),
        "parameters":nr_engines::parameters(engine),
        "units":{"length":"geometrized code length L","time":"L/c, c=1","fields":"native metric, extrinsic curvature and constraints; no SI mapping inferred"},
        "backend":if engine==nr_engines::TWO_PUNCTURES_CUDA{"Optional pinned AthenaK CUDA ADA89 on Windows WSL2, exact device UUID and private CUDA runtime; physical RAM scope and sampled device-wide VRAM admission."}else{"Optional pinned AthenaK serial CPU on Linux; Windows uses a supervised WSL2 process."},
        "physics_request_unchanged":true,"merger_validated":false,"checkpoint_resume_supported":false,
        "limits":"Both laboratory solver slots reserved exclusively; up to 4 CPU cores and half currently available host RAM. Solver RAM capped at 4 GiB with a separate 512 MiB live monitor for puncture diagnostics. CUDA uses a dedicated kernel RAM scope plus sampled device-wide VRAM growth and reserve guards; CPU uses address-space and sampled RSS guards. No physical speed or accuracy is inferred from hardware or rendering.",
        "output_limit_bytes":nr_engines::output_cap(engine),
        "outputs":"Unmodified native AMR fields, constraints, history and restart files with exact hashes; process completion is separate from numerical validation."})
}

pub fn validate(parameters: &Value) -> anyhow::Result<u64> {
    super::thermal::keys(parameters, &["nx"])?;
    let nx = super::thermal::integer(parameters, "nx", 32, 16, 64)?;
    ensure!([16,32,64].contains(&nx), "Gauge benchmark grid must be 16, 32 or 64");
    Ok(nx)
}

#[cfg(test)]
fn parameter_text(parameters: &Value) -> anyhow::Result<String> {
    nr_engines::parameter_text(ENGINE,parameters)
}

impl LaboratoryService {
    pub(crate) async fn execute_numerical_relativity(&self, id: Uuid, token: &CancellationToken) -> anyhow::Result<()> {
        let _slots = tokio::select! { _ = token.cancelled() => anyhow::bail!("NR request stopped in queue"), slots = self.solver_slots.acquire_many(2) => slots? };
        let job = self.get(id)?;
        ensure!(job.active() && !token.is_cancelled(), "NR request stopped before admission");
        let engine=job.input["engine"].as_str().context("NR engine is missing")?;
        let is_gauge=nr_engines::is_gauge(engine);
        let output_cap=nr_engines::output_cap(engine);
        let (runtime, manifest) = registration(self,engine)?;
        let input = nr_engines::parameter_text(engine,&job.input["parameters"])?;
        let monitor_python=if is_gauge{None}else{Some(self.ensure_environment(id,token).await?)};
        let (hardware,telemetry)=self.resource_context.as_ref().context("NR admission requires the application's live resource context")?;
        let plan=crate::compute::resource_plan::plan(hardware,&telemetry.snapshot(),&self.config.data_directory,job.deadline_at,None)?;
        ensure!(plan["budget"]["status"]=="proposal_available","Current resource plan holds NR execution: {}",plan["budget"]["reasons"]);
        let monitor_memory=if is_gauge{0}else{super::nr_live_guard::MEMORY_MIB*MIB};
        let memory = plan["budget"]["memory_bytes_max"].as_u64().context("Fresh host memory budget unavailable")?.saturating_sub(monitor_memory).min(4096*MIB);
        let minimum_memory=if is_gauge{256*MIB}else{4096*MIB};
        ensure!(memory >= minimum_memory, "Current RAM budget cannot admit the fixed NR input; the app will not silently coarsen its numerical grid");
        let cpus = plan["budget"]["cpu_threads_max"].as_u64().context("Fresh host CPU budget unavailable")?.clamp(1,4);
        let total_storage_proposal=output_cap*4; // runtime + native import + derived output and bounded overhead
        ensure!(plan["budget"]["output_bytes_max"].as_u64().is_some_and(|bytes|bytes>=total_storage_proposal),"Host workspace budget cannot hold the runtime, retained native data and derived diagnostics");
        let job_root = format!("{}/jobs", runtime.linux_root);
        let mut request = json!({"schema":"phaseforge.nr-request.v1","engine_id":engine,"job_id":id,
            "engine_manifest_sha256":runtime.engine_manifest_sha256,"input":{"path":"input.athinput","sha256":hash(input.as_bytes())},
            "output_dir":format!("{job_root}/{id}/work"),"cpu_threads":cpus,"memory_limit_bytes":memory,"output_limit_bytes":output_cap,
            "deadline_at":job.deadline_at});
        if engine==nr_engines::TWO_PUNCTURES_CUDA {
            request["schema"]=json!("phaseforge.nr-request.v2");
            request["execution_policy"]=json!({"backend":"cuda","memory_boundary":"systemd_cgroup_v2","address_space_limit_bytes":null,"tasks_max":128,
                "gpu_vram_policy":{"mode":"device_wide_soft_guard","device_uuid":runtime.gpu_device_uuid,"maximum_device_used_growth_bytes":4096*MIB,"minimum_free_bytes":2048*MIB,"poll_interval_seconds":1}});
        }
        let directory = self.directory(id);
        write_json(&directory.join("nr-request.json"), &request)?;
        let manifest_bytes=bounded_file(&runtime.host_root().join("engine.json"),64*1024)?;
        ensure!(hash(&manifest_bytes)==runtime.engine_manifest_sha256,"NR engine manifest changed during admission");
        std::fs::write(directory.join("nr-engine.json"),&manifest_bytes)?;
        write_json(&directory.join("nr-runtime-registration.json"),&serde_json::to_value(&runtime)?)?;
        std::fs::write(directory.join("input.athinput"), &input)?;
        let runtime_tools=directory.join("nr-runtime-tools");std::fs::create_dir_all(&runtime_tools)?;
        for (name,source) in [("nr_stage.py",include_bytes!("../../../tools/nr_stage.py").as_slice()),("nr_supervisor.py",include_bytes!("../../../tools/nr_supervisor.py").as_slice())] {
            super::retain_worker_source(&runtime_tools.join(name),source)?;
        }
        if engine==nr_engines::TWO_PUNCTURES_CUDA {super::retain_worker_source(&runtime_tools.join("nr_gpu.py"),include_bytes!("../../../tools/nr_gpu.py"))?;}
        self.event(id, "resource_plan", format!("NR diagnostic reserves both solver slots with a cap of {cpus} CPU cores, {} MiB solver RAM and {} MiB monitor RAM. Runtime admission rechecks available memory and storage.", memory/MIB,monitor_memory/MIB), json!({"proposal":plan,"request":request,"monitor_memory_limit_bytes":monitor_memory,"combined_memory_proposal_bytes":memory+monitor_memory,"combined_storage_proposal_bytes":total_storage_proposal,"monitor_final_scan_grace_seconds":20,"slot_reservation":{"solver_slots":2},"host_budget_checked":true,"execution_runtime_admitted":false,"scientific_scope":nr_engines::scope(engine),"merger_validated":false}))?;
        let mut stage = runtime.command("nr_stage.py");
        stage.args(["--job-root", &job_root, "--job-id", &id.to_string()]);
        stage.stdout(std::fs::File::create(directory.join("nr-staging.log"))?).stderr(std::fs::File::create(directory.join("nr-staging-stderr.log"))?);
        let mut child = stage.spawn().context("Could not start the trusted NR staging helper")?;
        let packet = serde_json::to_vec(&json!({"request":request,"input_utf8":input}))?;
        child.stdin.take().context("NR staging input pipe unavailable")?.write_all(&packet).await?;
        let stage_status = tokio::select! {
            _ = token.cancelled() => anyhow::bail!("NR request stopped during staging; staged attempt will not be silently reused"),
            status = tokio::time::timeout(Duration::from_secs(15), child.wait()) => status.context("NR staging timed out")??,
        };
        ensure!(stage_status.success(), "Trusted NR staging rejected the request; see retained staging log");
        let staging:Value=serde_json::from_slice(&bounded_file(&directory.join("nr-staging.log"),64*1024)?)?;
        ensure!(staging["schema"]=="phaseforge.nr-staging.v1" && staging["job_id"]==json!(id) && staging["input_sha256"]==request["input"]["sha256"] && staging["request_sha256"].as_str().is_some_and(|pin|pin.len()==64),"NR staging receipt does not bind this exact request");
        let staged_bytes=bounded_file(&runtime.host_root().join("jobs").join(id.to_string()).join("request.json"),64*1024)?;
        ensure!(staging["request_sha256"]==hash(&staged_bytes) && serde_json::from_slice::<Value>(&staged_bytes)?==request,"NR staged request bytes differ from this admitted request");
        std::fs::write(directory.join("nr-staged-request.json"),&staged_bytes)?;
        ensure!(!token.is_cancelled() && self.get(id)?.active(), "NR request stopped before execution");
        let mut monitor=if let Some(python)=monitor_python{
            let guard=super::nr_live_guard::LiveGuard::start(&python,&directory,&runtime.host_root().join("jobs").join(id.to_string()).join("work"),token).await?;
            self.event(id,"numerical_monitor_started","A separate read-only monitor will check complete saved native fields for finite values, positive physical metric and the original 100× constraint-growth stop threshold. These guards do not establish accuracy.",json!({"pid":guard.id(),"memory_limit_bytes":monitor_memory,"accuracy_validated":false}))?;
            Some(guard)
        }else{None};
        ensure!(!token.is_cancelled()&&self.get(id)?.active(),"NR request stopped during monitor readiness; solver was not launched");
        let mut command = runtime.command("nr_supervisor.py");
        command.args(["--engine-manifest", &format!("{}/engine.json", runtime.linux_root), "--request", &format!("{job_root}/{id}/request.json"), "--job-root", &job_root]);
        command.stdout(std::fs::File::create(directory.join("nr-supervisor.jsonl"))?).stderr(std::fs::File::create(directory.join("nr-supervisor-stderr.log"))?);
        let mut child = command.spawn().context("Could not start the trusted NR supervisor")?;
        let mut control = child.stdin.take().context("NR supervision control pipe unavailable")?;
        self.update(id, |record| { if record.active() { record.state="running".into(); record.event("worker_started", "Supervised Einstein-equation benchmark started; numerical validity is checked separately.", json!({"bridge_pid":child.id(),"request":request})); } })?;
        let monitor_token=CancellationToken::new();let mut last_observed=0;let mut monitor_stop_reason:Option<String>=None;
        let status = loop {
            let stopped=tokio::select! {
                status = child.wait() => break status?,
                _ = token.cancelled() => true,
                status=async{match monitor.as_mut(){Some(guard)=>guard.wait(&monitor_token).await,None=>std::future::pending().await}}=>{
                    monitor_stop_reason=Some(format!("Live numerical monitor stopped before solver completion: {status:?}"));true
                },
                _=tokio::time::sleep(Duration::from_secs(1))=>{
                    match monitor.as_ref().map(|guard|guard.state()).transpose(){
                        Ok(Some(Some(state)))=>{
                            let count=state["observed_pair_count"].as_u64().unwrap_or(0);
                            if count>last_observed {last_observed=count;self.event(id,"numerical_observation",format!("Observed {count} complete native field pairs through code time {}. Accuracy remains unvalidated.",state["last_time"]),state.clone())?;}
                            if state["status"]=="failed" {monitor_stop_reason=Some(format!("Live numerical monitor stopped: {}",state["reason"]));true}else{false}
                        },
                        Ok(_)=>false,
                        Err(error)=>{monitor_stop_reason=Some(format!("Live numerical monitor state could not be verified: {error:#}"));true}
                    }
                }
            };
            if stopped{
                let action=if self.get(id)?.state=="paused"{"pause"}else{"cancel"};
                if let Some(reason)=&monitor_stop_reason{self.event(id,"numerical_monitor_stop",reason,json!({"action":action,"accuracy_validated":false}))?;}
                let _=control.write_all(format!("{{\"action\":\"{action}\"}}\n").as_bytes()).await;
                break match tokio::time::timeout(Duration::from_secs(8),child.wait()).await{
                    Ok(status)=>status?,
                    Err(_)=>{drop(control);let _=child.kill().await;anyhow::bail!("NR supervisor did not acknowledge stop; its ownership guardian must drain the process group; inspect retained runtime receipt")}
                };
            }
        };
        drop(control);
        let monitor_result=if let Some(guard)=monitor{Some(guard.finish().await)}else{None};
        // The engine is drained. Await bounded blocking I/O so large UNC
        // imports do not occupy a Tokio worker; cancellation never abandons a
        // partially publishing writer or resets the scientific deadline.
        let mut result=super::nr_retention::retain_async(runtime,id,directory.clone(),manifest,request,staging,output_cap).await?;
        if let Some(observation)=monitor_result{result["live_monitor"]=observation;}
        // Even a timed-out attempt has a discoverable record of its exact saved
        // fields. Processing after its deadline requires a separate user action.
        self.update(id,|record|{record.result=result.clone();})?;
        ensure!(is_gauge||(monitor_stop_reason.is_none()&&result["live_monitor"]["passed"]==true),"NR live numerical monitor did not accept every saved field pair: {}; exact native files and monitor evidence are retained",monitor_stop_reason.as_deref().unwrap_or("see retained monitor receipt"));
        ensure!(status.success() && result["execution_completed"]==true, "NR attempt stopped with reason {}; its exact artifacts and process receipt are retained", result["termination_reason"]);
        ensure!(!token.is_cancelled(), "NR request stopped; completed worker artifacts remain retained");
        let diagnostics=if is_gauge {self.postprocess_nr_gauge(id,token).await?} else {self.postprocess_nr_black_hole(id,token).await?};
        result["diagnostics"]=diagnostics;
        let valid=if is_gauge {result["diagnostics"]["gauge_reference_validated"]==true} else {result["diagnostics"]["execution"]["complete"]==true};
        self.update(id, |record| { if record.active() && !token.is_cancelled() { record.state=if valid{"completed"}else{"failed"}.into(); record.result=result; if !valid{record.error=Some(if is_gauge{"NR execution finished, but its retained analytic checks failed. Original outputs and unchanged tolerances are available."}else{"The NR process finished without retaining its complete time target. Its partial numerical output remains available."}.into());} record.event(if valid{"completed"}else{"numerical_check_failed"}, if is_gauge{"Gauge-wave output and independent analytic checks are retained. This does not validate a black-hole collision."}else{"The constrained puncture diagnostic and its measurements are retained. Numerical convergence, physical boost, horizons and merger remain unvalidated."}, json!({"gauge_reference_validated":is_gauge&&valid,"time_target_met":valid,"merger_validated":false})); } })?;
        Ok(())
    }

    async fn postprocess_nr_black_hole(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<Value>{
        let directory=self.directory(id);
        let engine=self.get(id)?.input["engine"].as_str().context("NR engine is missing")?.to_owned();
        ensure!(nr_engines::supports(&engine)&&!nr_engines::is_gauge(&engine),"Black-hole reader requires its explicit puncture engine");
        let python=self.ensure_environment(id,token).await?;
        let worker_dir=directory.join("nr-tools");std::fs::create_dir_all(&worker_dir)?;
        for(name,bytes)in[("nr_black_hole_result.py",include_bytes!("../../../tools/nr_black_hole_result.py").as_slice()),("athenak_decode.py",include_bytes!("../../../tools/athenak_decode.py").as_slice())]{super::retain_worker_source(&worker_dir.join(name),bytes)?;}
        let receipt=directory.join("nr-process-receipt.json");let receipt_pin=hash(&bounded_file(&receipt,8*MIB)?);
        self.event(id,"numerical_measurement","Reading the saved AMR fields, constraints, initial-data residuals and current horizon-search outcomes. Physical boost and numerical accuracy require separate calibration and convergence evidence.",json!({"execution_receipt_sha256":receipt_pin,"units":"G=c=1, no SI mapping inferred","merger_validated":false}))?;
        let mut command=super::process::clean_command(&python,&directory);
        command.args(["-I","-B","-c","import runpy,sys; sys.path.insert(0,sys.argv.pop(1)); sys.argv=sys.argv[1:]; runpy.run_path(sys.argv[0],run_name='__main__')"])
            .arg(&worker_dir).arg(worker_dir.join("nr_black_hole_result.py")).arg("--run").arg(directory.join("native"))
            .arg("--execution-receipt").arg(&receipt).arg("--execution-receipt-sha256").arg(&receipt_pin)
            .arg("--engine-manifest").arg(directory.join("nr-engine.json")).arg("--input").arg(directory.join("input.athinput"))
            .arg("--request").arg(directory.join("nr-staged-request.json")).arg("--output").arg(directory.join("diagnostics"))
            .stdout(std::fs::File::create(directory.join("nr-diagnostics.log"))?).stderr(std::fs::File::create(directory.join("nr-diagnostics-stderr.log"))?);
        let status=super::process::OwnedProcess::spawn(&mut command,1024)?.wait(token).await?;
        ensure!(matches!(status.code(),Some(0|3)),"NR measurement reader rejected its input; see retained diagnostic logs");
        let bytes=bounded_file(&directory.join("diagnostics/result.json"),8*MIB)?;let diagnostics:Value=serde_json::from_slice(&bytes)?;
        ensure!(diagnostics["schema"]=="phaseforge.nr-black-hole-result.v1" && diagnostics["job_id"]==json!(id) && diagnostics["engine_identity"]["engine_id"]==engine && diagnostics["sources"]["receipt_sha256"]==receipt_pin,"Black-hole diagnostics differ from the exact execution receipt");
        ensure!(diagnostics["execution"]["complete"]==json!(status.success()),"NR measurement exit status and time-target result disagree");
        for key in ["requested_0_999c_collision","merger_validated","horizon_properties_validated","convergence_validated"]{ensure!(diagnostics["fulfillment"][key]==false,"Unvalidated puncture diagnostics cannot certify {key}");}
        Ok(json!({"path":"diagnostics/result.json","sha256":hash(&bytes),"execution":diagnostics["execution"],"frame_inventory":diagnostics["frame_inventory"],"measurements":diagnostics["measurements"],"paired_times":diagnostics["paired_times"],"paired_state_count":diagnostics["paired_state_count"],"physical_boost":diagnostics["physical_boost"],"fulfillment":diagnostics["fulfillment"],"scope":diagnostics["scope"]}))
    }

    async fn postprocess_nr_gauge(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<Value>{
        let directory=self.directory(id);
        let python=self.ensure_environment(id,token).await?;
        let worker_dir=directory.join("nr-tools");std::fs::create_dir_all(&worker_dir)?;
        for(name,bytes)in[("nr_gauge_result.py",include_bytes!("../../../tools/nr_gauge_result.py").as_slice()),("athenak_decode.py",include_bytes!("../../../tools/athenak_decode.py").as_slice()),("check_athenak_gauge.py",include_bytes!("../../../tools/check_athenak_gauge.py").as_slice())]{super::retain_worker_source(&worker_dir.join(name),bytes)?;}
        let receipt=directory.join("nr-process-receipt.json");let receipt_pin=hash(&bounded_file(&receipt,8*MIB)?);
        self.event(id,"numerical_validation","Checking the saved spacetime fields against the analytic gauge-wave solution and retaining exact cell-centre diagnostic slices. No solver is rerun.",json!({"execution_receipt_sha256":receipt_pin,"units":"L=1,c=1; time L/c","black_hole_collision":false}))?;
        let mut command=super::process::clean_command(&python,&directory);
        command.args(["-I","-B","-c","import runpy,sys; sys.path.insert(0,sys.argv.pop(1)); sys.argv=sys.argv[1:]; runpy.run_path(sys.argv[0],run_name='__main__')"])
            .arg(&worker_dir).arg(worker_dir.join("nr_gauge_result.py")).arg("--run").arg(directory.join("native"))
            .arg("--execution-receipt").arg(&receipt).arg("--execution-receipt-sha256").arg(&receipt_pin).arg("--output").arg(directory.join("diagnostics"))
            .stdout(std::fs::File::create(directory.join("nr-diagnostics.log"))?).stderr(std::fs::File::create(directory.join("nr-diagnostics-stderr.log"))?);
        let status=super::process::OwnedProcess::spawn(&mut command,1024)?.wait(token).await?;
        ensure!(matches!(status.code(),Some(0|3)),"NR diagnostic processing rejected its input; see retained diagnostic logs");
        let bytes=bounded_file(&directory.join("diagnostics/result.json"),8*MIB)?;let diagnostics:Value=serde_json::from_slice(&bytes)?;
        ensure!(diagnostics["schema"]=="phaseforge.nr-gauge-result.v1" && diagnostics["engine"]==ENGINE && diagnostics["sources"]["execution_receipt"]["sha256"]==receipt_pin,"NR diagnostic result differs from the exact execution receipt");
        ensure!(diagnostics["fulfillment"]["black_hole_collision_validated"]==false && diagnostics["fulfillment"]["physical_radiation"]==false,"Gauge diagnostic cannot be a black-hole collision or physical radiation");
        ensure!(diagnostics["analytic"]["passed"]==json!(status.success()),"NR diagnostic exit status and analytic result disagree");
        Ok(json!({"path":"diagnostics/result.json","sha256":hash(&bytes),"gauge_reference_validated":diagnostics["fulfillment"]["gauge_reference_validated"],"analytic":diagnostics["analytic"],"slice_index":diagnostics["slice_index"],"frame_count":diagnostics["frame_count"],"initial_time":diagnostics["initial_time"],"final_time":diagnostics["final_time"],"scope":diagnostics["scientific_scope"]}))
    }
}

fn validate_gpu_receipt(receipt:&Value,request:&Value)->anyhow::Result<bool>{
    let policy=&request["execution_policy"]["gpu_vram_policy"];
    ensure!(request["engine_id"]==nr_engines::TWO_PUNCTURES_CUDA && receipt["build"]["backend"]=="CUDA","GPU policy requires the exact CUDA engine identity");
    ensure!(policy.as_object().is_some_and(|v|v.len()==5) && policy["mode"]=="device_wide_soft_guard" && policy["poll_interval_seconds"]==1,"Unsupported GPU memory policy");
    let device=policy["device_uuid"].as_str().context("GPU device identity is missing")?;
    let id=Uuid::parse_str(device.strip_prefix("GPU-").context("GPU UUID prefix missing")?)?;
    ensure!(device==format!("GPU-{id}"),"GPU device identity is not canonical");
    let reserve=policy["minimum_free_bytes"].as_u64().filter(|v|*v>=2048*MIB).context("GPU reserve must be at least 2 GiB")?;
    let growth=policy["maximum_device_used_growth_bytes"].as_u64().filter(|v|*v>0&&*v<=1024*1024*MIB).context("GPU growth budget is invalid")?;
    let gpu=&receipt["gpu_vram"];
    ensure!(gpu["policy"]==*policy && gpu["hard_limit_enforced"]==false && gpu.get("attributable_process_usage")==Some(&Value::Null) && gpu.get("kernel_execution_observed")==Some(&Value::Null),"Sampled GPU admission cannot assert a hard quota or attributed kernel execution");
    let runtime=&receipt["build"]["cuda_runtime"];
    let guard=runtime["guard_source_sha256"].as_str().filter(|v|v.len()==64&&v.bytes().all(|b|b.is_ascii_hexdigit())).context("GPU guard source pin missing")?;
    ensure!(gpu["guard_source_sha256"]==guard,"GPU guard differs from the pinned engine runtime");
    let admitted=gpu["execution_admitted"].as_bool().context("GPU execution admission status is missing")?;
    if !admitted {
        ensure!(receipt["termination_reason"]!="completed" && receipt["exit_code"]!=0,"A refused GPU request cannot be a completed execution");
        ensure!(receipt["files"].as_array().is_some_and(|files|files.iter().all(|row|matches!(row["path"].as_str(),Some("solver.stdout.log"|"solver.stderr.log")))),"An unadmitted GPU attempt cannot retain numerical engine output");
        if receipt.get("memory_boundary").is_none() {
            ensure!(receipt.get("engine_pid").is_none() && receipt["exit_code"].is_null() && receipt["process_group_drained"]==true,"An absent RAM scope must be an explicitly unlaunched, drained attempt");
            return Ok(true);
        }
        return Ok(false);
    }
    let baseline=&gpu["baseline"];
    for (sample_name,decision_name) in [("baseline","admission_guard"),("before_launch","before_launch_guard")] {
        let sample=&gpu[sample_name];let decision=&gpu[decision_name];
        ensure!(sample["schema"]=="phaseforge.nr-gpu-sample.v1" && sample["status"]=="known" && sample["device_uuid"]==device && sample["returncode"]==0,"GPU admission requires a known successful observation of the exact device");
        let total=sample["total_bytes"].as_u64().filter(|v|*v>0).context("GPU total memory is unknown")?;
        for field in ["used_bytes","free_bytes","reserved_bytes"] {ensure!(sample[field].as_u64().is_some_and(|v|v<=total),"GPU memory observation is unknown or outside capacity");}
        ensure!(sample["free_bytes"].as_u64().unwrap()>=reserve && sample["total_bytes"]==baseline["total_bytes"],"GPU free reserve or device capacity changed during admission");
        let observed=sample["observed_monotonic"].as_f64().filter(|v|v.is_finite()).context("GPU observation clock is missing")?;
        let duration=sample["query_duration_seconds"].as_f64().filter(|v|v.is_finite()&&*v>=0.0&&*v<=1.0).context("GPU query exceeded its acquisition budget")?;
        ensure!(observed>=baseline["observed_monotonic"].as_f64().context("GPU baseline clock missing")? && duration<=1.0,"GPU observation precedes its baseline");
        let delta=sample["used_bytes"].as_u64().unwrap() as i128-baseline["used_bytes"].as_u64().context("GPU baseline memory missing")? as i128;
        ensure!(delta<=growth as i128 && decision["allowed"]==true && decision["scope"]=="device_wide_soft_guard" && decision["hard_vram_quota"]==false && decision["process_attribution"]==false && decision["device_used_growth_bytes"].as_i64().map(i128::from)==Some(delta),"GPU admission decision differs from its actual device-wide measurements");
    }
    ensure!(baseline["free_bytes"].as_u64().unwrap()>=reserve.checked_add(growth).context("GPU proposal overflow")?,"Initial GPU free memory cannot hold its complete proposed growth plus reserve");
    let snapshot=&receipt["cuda_runtime_snapshot"];
    let job_root=request["output_dir"].as_str().and_then(|path|path.strip_suffix("/work")).context("GPU job output path is invalid")?;
    let directory=format!("{job_root}/cuda-runtime");
    ensure!(snapshot["directory"]==directory && snapshot["source_directory"]==runtime["library_directory"] && snapshot["driver_directory"]=="/usr/lib/wsl/lib" && snapshot["driver_directory"]==runtime["driver_directory"] && snapshot["library"]==runtime["libraries"][0] && snapshot["build_receipt"]==runtime["build_receipt"] && snapshot["ld_library_path"]==format!("{directory}:/usr/lib/wsl/lib"),"GPU execution did not use the exact private library snapshot and WSL driver contract");
    Ok(false)
}

pub(super) fn validate_receipt(receipt:&Value,request:&Value,staging:&Value)->anyhow::Result<()>{
    let v2=request["schema"]=="phaseforge.nr-request.v2";
    ensure!((v2||request["schema"]=="phaseforge.nr-request.v1") && receipt["schema"]==if v2{"phaseforge.nr-process-receipt.v2"}else{"phaseforge.nr-process-receipt.v1"} && receipt["request_sha256"]==staging["request_sha256"],"NR terminal receipt differs from the staged request bytes or schema");
    for key in ["job_id","engine_id","engine_manifest_sha256","deadline_at","cpu_threads","memory_limit_bytes","output_limit_bytes"]{
        ensure!(receipt[key]==request[key],"NR terminal receipt changed admitted {key}");
    }
    ensure!(receipt["input"]["sha256"]==request["input"]["sha256"] && receipt["input"]["path"]==request["input"]["path"],"NR terminal receipt changed admitted input");
    if v2 {
        let policy=&request["execution_policy"];
        ensure!(receipt["execution_policy"]==*policy && policy["memory_boundary"]=="systemd_cgroup_v2" && policy.get("address_space_limit_bytes")==Some(&Value::Null),"NR physical-memory policy differs from admission");
        ensure!(policy.as_object().is_some_and(|v|v.len()==5),"Unexpected NR execution policy fields");
        if policy["backend"]=="cuda" {if validate_gpu_receipt(receipt,request)? {return Ok(());}}
        else {ensure!(policy["backend"]=="cpu" && policy["gpu_vram_policy"]==json!({"mode":"not_admitted"}),"Unsupported NR backend policy");}
        let tasks=policy["tasks_max"].as_u64().context("NR task cap is missing")?;
        ensure!((8..=128).contains(&tasks),"NR kernel task cap is outside admission");
        let id=Uuid::parse_str(request["job_id"].as_str().context("NR job UUID missing")?)?;
        let unit=format!("phaseforge-nr-{id}.scope");let path=format!("/system.slice/{unit}");
        let boundary=&receipt["memory_boundary"];
        ensure!(boundary["kind"]=="systemd_cgroup_v2" && boundary["unit"]==unit && boundary["path"]==path && boundary["child_cgroup_before_exec"]==path && boundary["inode"].as_u64().is_some_and(|v|v>0),"NR kernel boundary is not the unique admitted child scope");
        for name in ["supervisor_cgroup","guardian_cgroup"] {ensure!(boundary[name].as_str().is_some_and(|value|value.starts_with('/')&&value!=path&&!value.starts_with(&format!("{path}/"))),"NR owner must remain outside the workload scope");}
        ensure!(boundary["effective_kernel_limits"]==json!({"memory.max":request["memory_limit_bytes"],"memory.swap.max":0,"pids.max":tasks,"memory.oom.group":1}) && boundary["cgroup_drained"]==true && boundary.get("address_space_limit_bytes")==Some(&Value::Null),"NR physical-memory limits or workload drainage differ from admission");
        if policy["backend"]=="cpu" {ensure!(receipt["gpu_vram"]["execution_admitted"]==false && receipt["gpu_vram"]["hard_limit_enforced"]==false && receipt["gpu_vram"]["policy"]==policy["gpu_vram_policy"],"CPU scope receipt cannot assert GPU admission");}
    } else {
        ensure!(request.get("execution_policy").is_none() && receipt.get("execution_policy").is_none() && receipt["build"]["backend"]!="CUDA","Legacy NR receipts do not admit GPU execution");
    }
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn fixed_benchmark_cannot_accept_boost_or_generated_parameters() {
        assert!(validate(&json!({"speed_c":0.999})).is_err());
        assert!(validate(&json!({"input":"<problem> arbitrary"})).is_err());
        assert!(validate(&json!({"nx":63})).is_err());
        for nx in [16,32,64] { let text=parameter_text(&json!({"nx":nx})).unwrap(); assert_eq!(text.matches(&format!("nx1 = {nx}\n")).count(),2); assert!(text.contains("tlim = 0.1\n")); }
    }
    #[test] fn runtime_path_does_not_admit_argument_or_parent_escapes() {
        for path in ["relative", "/", "/root/../tmp", "/root//jobs", "/root/jobs/", "/root/jobs\n--exec", "/root\\jobs"] { assert!(linux_path(path).is_err(),"{path}"); }
        linux_path("/root/phaseforge-nr-runtime").unwrap();
    }
    #[test] fn exact_staged_receipt_binds_budget_and_off_without_silent_widening(){
        let request=json!({"schema":"phaseforge.nr-request.v1","job_id":Uuid::new_v4(),"engine_id":ENGINE,"engine_manifest_sha256":"engine","deadline_at":null,"cpu_threads":2,"memory_limit_bytes":1024*MIB,"output_limit_bytes":64*MIB,"input":{"path":"input.athinput","sha256":"input"}});
        let staging=json!({"request_sha256":"staged"});
        let mut receipt=request.clone();receipt["schema"]=json!("phaseforge.nr-process-receipt.v1");receipt["request_sha256"]=json!("staged");
        validate_receipt(&receipt,&request,&staging).unwrap();
        for (key,value) in [("deadline_at",json!("2026-09-13T00:00:00Z")),("cpu_threads",json!(8)),("memory_limit_bytes",json!(8*1024*MIB)),("input",json!({"path":"input.athinput","sha256":"changed"})),("request_sha256",json!("changed"))]{let mut altered=receipt.clone();altered[key]=value;assert!(validate_receipt(&altered,&request,&staging).is_err(),"{key}");}
    }
    #[test] fn physical_ram_receipt_requires_owned_scope_and_exact_kernel_limits(){
        let id=Uuid::new_v4();let unit=format!("phaseforge-nr-{id}.scope");let path=format!("/system.slice/{unit}");
        let request=json!({"schema":"phaseforge.nr-request.v2","job_id":id,"engine_id":nr_engines::TWO_PUNCTURES_CPU,"engine_manifest_sha256":"engine","deadline_at":null,"cpu_threads":2,"memory_limit_bytes":4096*MIB,"output_limit_bytes":1024*MIB,"input":{"path":"input.athinput","sha256":"input"},
            "execution_policy":{"backend":"cpu","memory_boundary":"systemd_cgroup_v2","address_space_limit_bytes":null,"tasks_max":128,"gpu_vram_policy":{"mode":"not_admitted"}}});
        let staging=json!({"request_sha256":"staged"});let mut receipt=request.clone();
        receipt["schema"]=json!("phaseforge.nr-process-receipt.v2");receipt["request_sha256"]=json!("staged");
        receipt["memory_boundary"]=json!({"kind":"systemd_cgroup_v2","unit":unit,"path":path,"inode":123,
            "child_cgroup_before_exec":path,"supervisor_cgroup":"/init.scope","guardian_cgroup":"/init.scope","address_space_limit_bytes":null,
            "effective_kernel_limits":{"memory.max":4096*MIB,"memory.swap.max":0,"pids.max":128,"memory.oom.group":1},"cgroup_drained":true,
            "after_cleanup":{"values":{},"unavailable":["memory.current"],"last_observed_values":{"memory.current":12345}}});
        receipt["gpu_vram"]=json!({"execution_admitted":false,"hard_limit_enforced":false,"policy":{"mode":"not_admitted"}});
        validate_receipt(&receipt,&request,&staging).unwrap(); // vanished counters stay unknown
        for (name,value) in [("unit",json!("init.scope")),("path",json!("/init.scope")),("guardian_cgroup",json!(path)),("cgroup_drained",json!(false)),("effective_kernel_limits",json!({"memory.max":"max"}))] {
            let mut changed=receipt.clone();changed["memory_boundary"][name]=value;
            assert!(validate_receipt(&changed,&request,&staging).is_err(),"{name}");
        }
    }
    #[test] fn cuda_receipt_binds_device_measurements_and_private_runtime(){
        let id=Uuid::new_v4();let device=format!("GPU-{}",Uuid::new_v4());
        let output=format!("/root/private/jobs/{id}/work");
        let runtime=json!({"guard_source_sha256":"a".repeat(64),"library_directory":"/root/cuda/lib","driver_directory":"/usr/lib/wsl/lib","libraries":[{"name":"libcudart.so.12.9.79","bytes":741088,"sha256":"b".repeat(64),"aliases":{"libcudart.so.12":"libcudart.so.12.9.79"}}],"build_receipt":{"path":"/root/build.json","sha256":"c".repeat(64)}});
        let policy=json!({"mode":"device_wide_soft_guard","device_uuid":device,"maximum_device_used_growth_bytes":4096*MIB,"minimum_free_bytes":2048*MIB,"poll_interval_seconds":1});
        let request=json!({"engine_id":nr_engines::TWO_PUNCTURES_CUDA,"output_dir":output,"execution_policy":{"gpu_vram_policy":policy}});
        let sample=json!({"schema":"phaseforge.nr-gpu-sample.v1","status":"known","device_uuid":device,"returncode":0,"total_bytes":16376*MIB,"reserved_bytes":256*MIB,"used_bytes":1024*MIB,"free_bytes":15096*MIB,"observed_monotonic":10.0,"query_duration_seconds":0.1});
        let decision=json!({"allowed":true,"scope":"device_wide_soft_guard","hard_vram_quota":false,"process_attribution":false,"device_used_growth_bytes":0});
        let directory=format!("/root/private/jobs/{id}/cuda-runtime");
        let mut receipt=json!({"build":{"backend":"CUDA","cuda_runtime":runtime},"gpu_vram":{"execution_admitted":true,"hard_limit_enforced":false,"attributable_process_usage":null,"kernel_execution_observed":null,"policy":policy,"guard_source_sha256":"a".repeat(64),"baseline":sample,"before_launch":sample,"admission_guard":decision,"before_launch_guard":decision},"cuda_runtime_snapshot":{"directory":directory,"source_directory":runtime["library_directory"],"driver_directory":runtime["driver_directory"],"library":runtime["libraries"][0],"build_receipt":runtime["build_receipt"],"ld_library_path":format!("{directory}:/usr/lib/wsl/lib")}});
        assert!(!validate_gpu_receipt(&receipt,&request).unwrap());
        for (field,value) in [("status",json!("unknown")),("device_uuid",json!(format!("GPU-{}",Uuid::new_v4()))),("free_bytes",json!(1024*MIB)),("query_duration_seconds",json!(1.1)),("used_bytes",json!(6*1024*MIB))]{let mut changed=receipt.clone();changed["gpu_vram"]["before_launch"][field]=value;assert!(validate_gpu_receipt(&changed,&request).is_err(),"{field}");}
        for(field,value)in[("hard_limit_enforced",json!(true)),("kernel_execution_observed",json!(true)),("guard_source_sha256",json!("d".repeat(64)))]{let mut changed=receipt.clone();changed["gpu_vram"][field]=value;assert!(validate_gpu_receipt(&changed,&request).is_err(),"{field}");}
        let mut changed=receipt.clone();changed["cuda_runtime_snapshot"]["ld_library_path"]=json!("/root/cuda/stubs");assert!(validate_gpu_receipt(&changed,&request).is_err());
        receipt["gpu_vram"]["execution_admitted"]=json!(false);receipt["termination_reason"]=json!("gpu_admission:unknown");receipt["exit_code"]=Value::Null;receipt["process_group_drained"]=json!(true);receipt["files"]=json!([]);
        assert!(validate_gpu_receipt(&receipt,&request).unwrap());
        receipt["files"]=json!([{"path":"bin/frame.bin"}]);assert!(validate_gpu_receipt(&receipt,&request).is_err());
    }
}
