//! Durable scientific execution. Solver state, presentation and agent context have separate lifetimes.
pub mod process;
mod runtime;
pub mod isolation;
pub mod generated;
pub mod field;
pub mod mechanics;
pub mod sweep;
pub mod ml_study;
pub mod ml_query;
mod ml_plot;
pub mod illustration;
pub mod observation;
mod solver_lifecycle;
pub(crate) mod exports;
pub(crate) mod api;
pub(crate) mod data;
pub(crate) mod structure;
pub use api::routes;

use std::{collections::HashMap, path::{Component, Path, PathBuf}, sync::Arc, time::Duration};
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use crate::{config::AppConfig, persistence::Database};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabEvent { pub sequence: usize, pub at: DateTime<Utc>, pub kind: String, pub message: String, pub data: Value }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabJob {
    pub id: Uuid, pub project_id: Uuid, pub parent_id: Option<Uuid>,
    pub kind: String, pub state: String, pub title: String,
    pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc>,
    pub deadline_at: Option<DateTime<Utc>>, pub seen_at: Option<DateTime<Utc>>,
    #[serde(default)] pub completed_at: Option<DateTime<Utc>>,
    pub input: Value, pub result: Value, pub progress: Value,
    pub error: Option<String>, pub events: Vec<LabEvent>,
}
impl LabJob {
    pub fn active(&self) -> bool { matches!(self.state.as_str(), "queued" | "running" | "provisioning" | "waiting") }
    fn normalize_completion(&mut self) {
        if self.state=="completed" {
            self.completed_at=self.events.iter().rev().find(|event|event.kind=="completed").map(|event|event.at).or(self.completed_at).or(Some(self.updated_at));
        }
    }
    /// Polling never carries conversation transcripts, source code or tool outputs.
    /// The full record remains available on the individual job endpoint.
    pub fn summary(&self)->Value {
        let mut input=serde_json::Map::new();
        for key in ["engine","parameters","provider","model","reasoning_effort","time_limit_seconds","source_job_id","source_session_id","context_job_id","source_run_id","width","height","fps","duration_seconds","playback_speed","restarted_from_export_id"] {
            if let Some(value)=self.input.get(key){input.insert(key.into(),value.clone());}
        }
        let events=self.events.iter().rev().take(1).map(|event|json!({"sequence":event.sequence,"at":event.at,"kind":event.kind,"message":event.message.chars().take(1200).collect::<String>(),"data":{}})).collect::<Vec<_>>();
        let result=if matches!(self.kind.as_str(),"session"|"specialist"){json!({"rounds":self.result["rounds"],"tool_count":self.result["tool_count"],"compactions":self.result["compactions"]})}else if matches!(self.kind.as_str(),"generated"|"data"|"monitor"){json!({"engine":self.result["engine"],"status":self.result["status"],"source_job_id":self.result["source_job_id"],"detected":self.result["detected"]})}else if self.kind=="sweep"{json!({"status":self.result["status"],"case_count":self.result["case_count"],"completed_cases":self.result["completed_cases"],"retained_bytes":self.result["retained_bytes"]})}else{self.result.clone()};
        json!({"id":self.id,"project_id":self.project_id,"parent_id":self.parent_id,"kind":self.kind,"state":self.state,"title":self.title,"created_at":self.created_at,"updated_at":self.updated_at,"deadline_at":self.deadline_at,"seen_at":self.seen_at,"completed_at":self.completed_at,"input":input,"result":result,"progress":self.progress,"error":self.error,"events":events,"event_count":self.events.len(),"summary_only":true})
    }
    pub fn event(&mut self, kind: &str, message: impl Into<String>, data: Value) {
        self.updated_at = Utc::now();
        self.events.push(LabEvent { sequence: self.events.len()+1, at: self.updated_at, kind:kind.into(), message:message.into(), data });
    }
}

#[derive(Clone)]
pub struct LaboratoryService {
    pub database: Database, pub config: AppConfig,
    active: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
    gate: Arc<Mutex<()>>, provision: Arc<tokio::sync::Mutex<()>>,
    solver_slots: Arc<tokio::sync::Semaphore>,
    render_slots: Arc<tokio::sync::Semaphore>,
}
impl LaboratoryService {
    pub fn new(database: Database, config: AppConfig) -> anyhow::Result<Self> {
        std::fs::create_dir_all(config.artifacts_directory().join("laboratory"))?;
        // Never auto-resubmit an uncertain paid request after a process restart.
        for mut job in database.lab_records()? {
            if job.active() {
                job.state = "paused".into();
                job.event("interrupted", "The runtime restarted. Completed artifacts are retained. Resume reconciles tool receipts before continuing.", json!({"recovery":"explicit_resume"}));
                database.put_lab_record(&job)?;
            }
        }
        Ok(Self {database,config,active:Default::default(),gate:Default::default(),provision:Default::default(),solver_slots:Arc::new(tokio::sync::Semaphore::new(2)),render_slots:Arc::new(tokio::sync::Semaphore::new(1))})
    }
    pub fn directory(&self, id: Uuid) -> PathBuf { self.config.artifacts_directory().join("laboratory").join(id.to_string()) }
    pub fn get(&self,id:Uuid) -> anyhow::Result<LabJob> { let mut job:LabJob=self.database.lab_record(id)?.context("Laboratory job not found")?;job.normalize_completion();Ok(job) }
    pub fn list(&self, project: Option<Uuid>) -> anyhow::Result<Vec<LabJob>> {
        Ok(self.database.lab_records()?.into_iter().filter(|j|project.is_none_or(|id|j.project_id==id)).map(|mut job|{job.normalize_completion();job}).collect())
    }
    pub fn create(&self, id:Uuid, project:Uuid, parent:Option<Uuid>, kind:&str, title:&str, input:Value, deadline:Option<DateTime<Utc>>) -> anyhow::Result<LabJob> {
        let _guard=self.gate.lock();
        self.create_locked(id,project,parent,kind,title,input,deadline)
    }
    fn create_locked(&self, id:Uuid, project:Uuid, parent:Option<Uuid>, kind:&str, title:&str, input:Value, deadline:Option<DateTime<Utc>>) -> anyhow::Result<LabJob> {
        self.database.get_project(project)?.context("Project not found")?;
        if let Some(existing)=self.database.lab_record(id)? {
            anyhow::ensure!(existing.project_id==project && existing.input==input && existing.kind==kind && existing.parent_id==parent,"Request ID is already bound to a different request");
            return Ok(existing);
        }
        let now=Utc::now();
        let mut job=LabJob {id,project_id:project,parent_id:parent,kind:kind.into(),state:"queued".into(),title:title.into(),created_at:now,updated_at:now,deadline_at:deadline,seen_at:None,completed_at:None,input,result:Value::Null,progress:json!({}),error:None,events:vec![]};
        job.event("queued","Job saved; execution is independent of the selected view.",json!({}));
        std::fs::create_dir_all(self.directory(id))?;
        write_json(&self.directory(id).join("input.json"),&job.input)?;
        self.database.put_lab_record(&job)?;
        Ok(job)
    }
    /// Admission and tree limits share the creation lock, including concurrent
    /// delegation by siblings. The caller cannot choose another model or budget.
    pub(crate) fn create_monitor(&self,id:Uuid,parent_id:Uuid,title:&str,input:Value,token:&CancellationToken)->anyhow::Result<LabJob>{
        let _guard=self.gate.lock();let parent=self.get(parent_id)?;
        anyhow::ensure!(matches!(parent.kind.as_str(),"session"|"specialist")&&parent.active()&&!token.is_cancelled()&&parent.deadline_at.is_none_or(|at|at>Utc::now()),"The parent observation session is paused, stopped or out of time");
        self.create_locked(id,parent.project_id,Some(parent_id),"monitor",title,input,parent.deadline_at)
    }
    pub fn create_specialist(&self,id:Uuid,parent_id:Uuid,task:&str,evidence:Vec<Uuid>)->anyhow::Result<LabJob>{
        let _guard=self.gate.lock();
        let parent=self.get(parent_id)?;
        anyhow::ensure!(matches!(parent.kind.as_str(),"session"|"specialist"),"Only an agent session can assign a specialist");
        anyhow::ensure!(!task.trim().is_empty()&&task.len()<=8000&&evidence.len()<=12,"Assign a concrete task of at most 8,000 bytes and at most 12 evidence jobs");
        let mut evidence=evidence;evidence.sort();evidence.dedup();
        for evidence_id in &evidence{
            let source=self.get(*evidence_id)?;anyhow::ensure!(source.project_id==parent.project_id,"Specialist evidence belongs to another project");
            if parent.kind=="specialist"{anyhow::ensure!(parent.input["delegation"]["evidence_job_ids"].as_array().is_some_and(|ids|ids.contains(&json!(evidence_id))),"Nested specialists may only receive their parent's scoped evidence");}
        }
        let mut ancestor=parent.clone();let mut depth=1;let mut seen=vec![parent.id];
        while let Some(ancestor_id)=ancestor.parent_id{
            anyhow::ensure!(!seen.contains(&ancestor_id),"Agent ancestry contains a cycle");seen.push(ancestor_id);
            ancestor=self.get(ancestor_id)?;anyhow::ensure!(ancestor.project_id==parent.project_id&&matches!(ancestor.kind.as_str(),"session"|"specialist"),"Invalid specialist ancestry");depth+=1;
        }
        anyhow::ensure!(depth<=2,"Specialist nesting is limited to two levels");
        let root_id=ancestor.id;
        let input=json!({"request_id":id,"content":task.trim(),"provider":parent.input["provider"],"model":parent.input["model"],"reasoning_effort":parent.input["reasoning_effort"],
            "time_limit_seconds":parent.input["time_limit_seconds"],"context_job_id":Value::Null,"attachments":[],
            "delegation":{"parent_job_id":parent.id,"root_job_id":root_id,"depth":depth,"objective":task.trim(),"evidence_job_ids":evidence,"max_model_turns":8,"parent_objective":parent.input["content"]}});
        if let Some(existing)=self.database.lab_record(id)?{
            anyhow::ensure!(existing.input==input&&existing.parent_id==Some(parent_id)&&existing.kind=="specialist","Specialist request ID is bound to a different assignment");return Ok(existing);
        }
        anyhow::ensure!(parent.active()&&parent.deadline_at.is_none_or(|at|at>Utc::now()),"The parent must be active with remaining time before delegation");
        let jobs=self.database.lab_records()?;
        if let Some(existing)=jobs.iter().find(|job|job.kind=="specialist"&&job.parent_id==Some(parent_id)&&job.input["delegation"]["objective"]==input["delegation"]["objective"]&&job.input["delegation"]["evidence_job_ids"]==input["delegation"]["evidence_job_ids"]){return Ok(existing.clone());}
        anyhow::ensure!(jobs.iter().filter(|job|job.kind=="specialist"&&job.parent_id==Some(parent_id)).count()<3,"A parent may assign at most three specialists; inspect the existing assignments");
        anyhow::ensure!(jobs.iter().filter(|job|job.kind=="specialist"&&job.input["delegation"]["root_job_id"]==json!(root_id)).count()<8,"The session has reached its eight-specialist tree limit");
        self.create_locked(id,parent.project_id,Some(parent_id),"specialist",&task.trim().chars().take(100).collect::<String>(),input,parent.deadline_at)
    }
    pub fn update(&self, id:Uuid, f:impl FnOnce(&mut LabJob)) -> anyhow::Result<LabJob> {
        let _guard=self.gate.lock(); let mut job=self.get(id)?; f(&mut job); job.updated_at=Utc::now();
        job.normalize_completion();
        self.database.put_lab_record(&job)?; Ok(job)
    }
    pub fn event(&self,id:Uuid,kind:&str,message:impl Into<String>,data:Value) -> anyhow::Result<()> {
        self.update(id,|j|j.event(kind,message,data))?; Ok(())
    }
    pub fn update_with_message(&self,id:Uuid,f:impl FnOnce(&mut LabJob)->anyhow::Result<Option<crate::domain::ConversationMessage>>)->anyhow::Result<LabJob>{
        let _guard=self.gate.lock();let mut job=self.get(id)?;let message=f(&mut job)?;
        job.updated_at=Utc::now();if job.state=="completed"&&job.completed_at.is_none(){job.completed_at=Some(job.updated_at);}
        self.database.put_lab_record_with_message(&job,message.as_ref())?;Ok(job)
    }
    pub fn acquire(&self,id:Uuid) -> anyhow::Result<CancellationToken> {
        let mut active=self.active.lock();
        anyhow::ensure!(!active.contains_key(&id),"Job is already executing");
        let token=CancellationToken::new(); active.insert(id,token.clone()); Ok(token)
    }
    pub fn acquire_child(&self,id:Uuid,parent:&CancellationToken)->anyhow::Result<CancellationToken>{
        let mut active=self.active.lock();
        anyhow::ensure!(!active.contains_key(&id),"Job is already executing");
        anyhow::ensure!(!parent.is_cancelled(),"Parent stopped before child execution");
        let token=parent.child_token();active.insert(id,token.clone());Ok(token)
    }
    pub fn executing(&self,id:Uuid)->bool{self.active.lock().contains_key(&id)}
    pub fn execution_token(&self,id:Uuid)->Option<CancellationToken>{self.active.lock().get(&id).cloned()}
    pub fn release(&self,id:Uuid) { self.active.lock().remove(&id); }
    pub fn stop(&self,id:Uuid,state:&str) -> anyhow::Result<LabJob> {
        // Mark the complete tree before waking any worker. Otherwise a linked
        // child can win the race and turn an explicit pause into cancellation.
        // The creation gate also prevents new delegation under a stopped parent.
        let _guard=self.gate.lock();
        self.get(id)?;
        let mut jobs=self.database.lab_records()?;
        let mut tree=vec![id];let mut cursor=0;
        while cursor<tree.len(){
            let parent=tree[cursor];cursor+=1;
            for child in jobs.iter().filter(|job|job.parent_id==Some(parent)){
                if !tree.contains(&child.id){tree.push(child.id);}
            }
        }
        let marking=(||->anyhow::Result<LabJob>{
            for job in jobs.iter_mut().filter(|job|tree.contains(&job.id)&&job.active()){
                job.state=state.into();job.event(state,"Stop requested. Completed outputs and checkpoints remain available.",json!({}));
                self.database.put_lab_record(job)?;
            }
            self.get(id)
        })();
        // Even a persistence failure must not leave paid or compute work running.
        let active=self.active.lock();
        for target in tree{if let Some(token)=active.get(&target){token.cancel();}}
        marking
    }
    pub fn path(&self,id:Uuid,relative:&str) -> anyhow::Result<PathBuf> {
        safe_relative(relative)?;
        let root=self.directory(id).canonicalize()?;
        let path=root.join(relative).canonicalize().context("Artifact is not yet available")?;
        anyhow::ensure!(path.starts_with(&root)&&path.is_file(),"Artifact is outside the job");
        Ok(path)
    }
    pub fn read_json(&self,id:Uuid,name:&str) -> anyhow::Result<Value> {
        let path=self.path(id,name)?;
        anyhow::ensure!(std::fs::metadata(&path)?.len()<=16*1024*1024,"Artifact is too large; request a bounded range");
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    pub fn artifact_inventory(&self,id:Uuid) -> anyhow::Result<Value> {
        fn walk(root:&Path,dir:&Path,rows:&mut Vec<Value>) -> anyhow::Result<()> {
            for entry in std::fs::read_dir(dir)? { let entry=entry?; let path=entry.path(); let meta=match std::fs::symlink_metadata(&path){Ok(meta)=>meta,Err(error) if error.kind()==std::io::ErrorKind::NotFound=>continue,Err(error)=>return Err(error.into())};if meta.file_type().is_symlink(){continue;}
                if meta.is_dir(){walk(root,&path,rows)?;}else if meta.is_file(){rows.push(json!({"path":path.strip_prefix(root)?.to_string_lossy().replace('\\',"/"),"bytes":meta.len()}));}
            } Ok(())
        }
        let root=self.directory(id);let mut rows=vec![];walk(&root,&root,&mut rows)?;Ok(json!(rows))
    }
    pub fn start_solver(&self,id:Uuid) -> anyhow::Result<()> {
        let token={let _guard=self.gate.lock();let job=self.get(id)?;
            anyhow::ensure!(job.kind=="solver"&&job.state=="queued"&&job.deadline_at.is_none_or(|at|at>Utc::now()),"Solver is stopped, already started or out of time");
            self.ensure_science_attempt_identity(id)?;
            self.acquire(id)?};
        self.spawn_solver(id,token)
    }
    fn spawn_solver(&self,id:Uuid,token:CancellationToken)->anyhow::Result<()>{
        let service=self.clone();
        tokio::spawn(async move {
            let result=service.run_solver(id,&token).await;
            if let Err(error)=result {
                let _=service.update(id,|j|{
                    if j.active(){j.state=if token.is_cancelled(){"cancelled"}else{"failed"}.into();}
                    j.error=Some(format!("{error:#}")); j.event("worker_stopped",format!("{error:#}"),json!({}));
                });
            }
            service.release(id);
        }); Ok(())
    }
    pub fn resume_solver(&self,id:Uuid,deadline:Option<DateTime<Utc>>)->anyhow::Result<()>{
        self.resume_solver_with_mode(id,deadline,false)
    }
    async fn run_solver(&self,id:Uuid,token:&CancellationToken) -> anyhow::Result<()> {
        let job=self.get(id)?;
        let deadline_token=token.clone();
        let deadline_task=job.deadline_at.map(|deadline| tokio::spawn(async move {
            let duration=(deadline-Utc::now()).to_std().unwrap_or_default();
            tokio::time::sleep(duration).await;deadline_token.cancel();
        }));
        let result=self.execute_solver(id,token).await;
        if let Some(task)=deadline_task{task.abort();}
        if token.is_cancelled() && job.deadline_at.is_some_and(|at|at<=Utc::now()) {
            self.update(id,|j|{j.state="timed_out".into();j.event("deadline","The selected time limit expired.",json!({}));})?;
        }
        result
    }
    async fn execute_solver(&self,id:Uuid,token:&CancellationToken) -> anyhow::Result<()> {
        self.ensure_science_attempt_identity(id)?;
        if self.get(id)?.input["engine"]=="diffusion_2d" { return self.execute_field(id,token).await; }
        if self.get(id)?.input["engine"]=="newtonian_nbody" { return self.execute_mechanics(id,token).await; }
        let _slot=tokio::select! { _=token.cancelled()=>bail!("Cancelled in queue"), slot=self.solver_slots.acquire()=>slot? };
        let job=self.get(id)?;
        anyhow::ensure!(job.input["engine"]=="openmm_argon","This engine is not executable in the current managed laboratory");
        anyhow::ensure!(job.active()&&!token.is_cancelled(),"Solver stopped before source verification");
        let directory=self.directory(id);
        let worker=directory.join("scientific_worker.py");
        retain_worker_source(&worker,include_bytes!("../../../tools/scientific_worker.py"))?;
        retain_worker_source(&directory.join("requirements-science.txt"),include_bytes!("../../../tools/requirements-science.txt"))?;
        let preparing=self.update(id,|j|{if j.active()&&!token.is_cancelled(){j.state="provisioning".into();j.event("environment","Checking the versioned scientific environment.",json!({}));}})?;
        anyhow::ensure!(preparing.active()&&!token.is_cancelled(),"Solver stopped before environment preparation");
        let python=self.ensure_environment(id,token).await?;
        if token.is_cancelled(){bail!("Cancelled before solver launch");}
        write_json(&directory.join("worker-input.json"),&json!({"engine":job.input["engine"],"parameters":job.input["parameters"]}))?;
        let stdout=std::fs::File::create(directory.join("stdout.log"))?;
        let stderr=std::fs::File::create(directory.join("stderr.log"))?;
        let mut command=process::clean_command(&python,&directory);
        command.args(["-I","-B"]).arg(&worker).arg("--input").arg(directory.join("worker-input.json")).arg("--output").arg(&directory).stdout(stdout).stderr(stderr);
        let starting=self.update(id,|j|{if j.active()&&!token.is_cancelled(){j.state="running".into();}})?;
        anyhow::ensure!(starting.active()&&!token.is_cancelled(),"Solver stopped before process launch");
        let mut child=process::OwnedProcess::spawn(&mut command,4096)?;
        self.event(id,"solver_started","OpenMM is computing forces and integrating numerical states.",json!({"pid":child.id(),"python":python,"worker":"trusted_shipped_adapter","memory_limit_mb":4096}))?;
        let progress_service=self.clone();let progress_token=token.clone();
        let progress_task=tokio::spawn(async move {
            let mut last=Value::Null;
            loop {
                tokio::select!{_=progress_token.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(500))=>{}}
                if let Ok(progress)=progress_service.read_json(id,"progress.json") {
                    if progress!=last {let _=progress_service.update(id,|j|j.progress=progress.clone());last=progress;}
                }
            }
        });
        let status=child.wait_cooperative(token,&directory.join("cancel.request")).await;
        progress_task.abort();
        let status=status?;
        if !status.success() {
            let detail=std::fs::read_to_string(directory.join("stderr.log")).unwrap_or_default();
            bail!("Scientific worker exited with {status}: {}",detail.chars().rev().take(4000).collect::<String>().chars().rev().collect::<String>());
        }
        let result=self.read_json(id,"result.json")?;
        let manifest=self.read_json(id,"manifest.json")?;
        self.update(id,|j|{j.result=result;if j.active()&&!token.is_cancelled(){j.state="completed".into();j.progress=json!({"fraction":1.0});j.event("completed","Scientific output and provenance are saved.",json!({"manifest":manifest}));}})?;
        Ok(())
    }
    pub fn environment_python(&self)->PathBuf {
        runtime::RuntimeKind::Science.directory(&self.config.data_directory).join("python.exe")
    }
    async fn ensure_environment(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<PathBuf> {
        let _guard=tokio::select!{_=token.cancelled()=>bail!("Cancelled while waiting for environment"),guard=self.provision.lock()=>guard};
        self.ensure_science_attempt_identity(id)?;
        self.event(id,"provisioning","Verifying and copying the bundled scientific runtime. No host Python or network installation is used.",json!({"runtime":"science-v3"}))?;
        let data=self.config.data_directory.clone();let copy_token=token.clone();
        let verified=tokio::task::spawn_blocking(move||runtime::provision(&data,runtime::RuntimeKind::Science,&copy_token)).await??;
        self.event(id,"runtime_verified","The runtime matches the compiled full inventory and upstream source pins.",serde_json::to_value(&verified)?)?;
        anyhow::ensure!(self.environment_smoke(id,&verified.python,token).await?,"Verified bundled engine failed an actual integration smoke test; see environment-smoke-error.log. No host/network fallback is permitted.");
        Ok(verified.python)
    }
    fn ensure_science_attempt_identity(&self,id:Uuid)->anyhow::Result<()> {
        let saved=self.get(id)?;
        for event in saved.events.iter().filter(|event|event.kind=="runtime_verified") {
            anyhow::ensure!(event.data["kind"]==runtime::RuntimeKind::Science.name()
                &&event.data["manifest_sha256"]==runtime::RuntimeKind::Science.manifest_sha256(),
                "Original scientific runtime differs; new immutable run required. Existing numerical artifacts and runtime receipts are preserved.");
        }
        if self.directory(id).join("manifest.json").exists() {
            anyhow::ensure!(saved.events.iter().any(|event|event.kind=="runtime_verified"),
                "Original scientific runtime identity is unavailable; new immutable run required. Existing numerical artifacts are preserved.");
        }
        Ok(())
    }
    async fn environment_smoke(&self,id:Uuid,python:&Path,token:&CancellationToken)->anyhow::Result<bool>{
        let directory=self.directory(id);
        let mut smoke=process::clean_command(python,&directory);
        smoke.args(["-I","-B","-c",runtime::SCIENCE_SMOKE]).stdout(std::fs::File::create(directory.join("environment-smoke.log"))?).stderr(std::fs::File::create(directory.join("environment-smoke-error.log"))?);
        Ok(process::OwnedProcess::spawn(&mut smoke,2048)?.wait(token).await?.success())
    }
}

pub fn safe_relative(value:&str)->anyhow::Result<()> {
    anyhow::ensure!(!value.is_empty()&&!value.contains('\\')&&!value.contains(':')&&Path::new(value).components().all(|c|matches!(c,Component::Normal(_))),"Invalid artifact path");Ok(())
}
/// Shipped code is never replaced inside an existing scientific attempt. A new
/// build may continue only byte-identical worker/dependency sources; otherwise
/// the original source remains available and a fresh immutable run is required.
pub(super) fn retain_worker_source(path:&Path,bytes:&[u8])->anyhow::Result<()> {
    use std::io::{Read,Write};
    if path.try_exists()? {
        let metadata=std::fs::symlink_metadata(path)?;
        anyhow::ensure!(metadata.is_file()&&!metadata.file_type().is_symlink(),"Retained worker source must be a regular unlinked file");
        #[cfg(windows)]{
            use std::os::windows::{fs::MetadataExt,io::AsRawHandle};
            use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle,BY_HANDLE_FILE_INFORMATION};
            anyhow::ensure!(metadata.file_attributes()&0x400==0,"Retained worker source reparse points are forbidden");
            let file=std::fs::File::open(path)?;let mut info:BY_HANDLE_FILE_INFORMATION=unsafe{std::mem::zeroed()};
            anyhow::ensure!(unsafe{GetFileInformationByHandle(file.as_raw_handle(),&mut info)}!=0&&info.nNumberOfLinks==1,"Retained worker source hardlinks are forbidden");
        }
        let mut original=Vec::new();std::fs::File::open(path)?.take(bytes.len() as u64+1).read_to_end(&mut original)?;
        anyhow::ensure!(original==bytes,"Original worker differs; new immutable run required. The saved source was preserved and was not executed.");
        return Ok(());
    }
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;file.sync_all()?;Ok(())
}
pub fn write_json(path:&Path,value:&impl Serialize)->anyhow::Result<()> {
    use std::io::Write;
    let temp=path.with_extension(format!("{}.tmp",Uuid::new_v4()));
    let mut file=std::fs::File::create(&temp)?;file.write_all(&serde_json::to_vec_pretty(value)?)?;file.sync_all()?;drop(file);
    let until=std::time::Instant::now()+Duration::from_secs(2);
    loop{match std::fs::rename(&temp,path){
        Ok(())=>return Ok(()),
        Err(error) if cfg!(windows)&&matches!(error.raw_os_error(),Some(5|32|33))&&std::time::Instant::now()<until=>std::thread::sleep(Duration::from_millis(10)),
        Err(error)=>return Err(error).context("Atomic journal publication failed; previous and temporary bytes remain available"),
    }}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn retained_worker_source_reuses_identical_bytes_and_preserves_changed_source(){
        let temp=tempfile::tempdir().unwrap();let path=temp.path().join("worker.py");
        retain_worker_source(&path,b"original shipped worker\n").unwrap();
        let modified=std::fs::metadata(&path).unwrap().modified().unwrap();
        retain_worker_source(&path,b"original shipped worker\n").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(),modified);
        let error=retain_worker_source(&path,b"different shipped worker\n").unwrap_err();
        assert!(error.to_string().contains("Original worker differs; new immutable run required"));
        assert_eq!(std::fs::read(&path).unwrap(),b"original shipped worker\n");
        let alias=temp.path().join("alias.py");std::fs::hard_link(&path,&alias).unwrap();
        #[cfg(windows)]assert!(retain_worker_source(&alias,b"original shipped worker\n").is_err());
    }
    #[tokio::test]async fn changed_solver_sources_fail_before_environment_preparation(){
        let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().into(),..Default::default()};
        let database=Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:None,question:"Immutable worker admission".into()});database.put_project(&project).unwrap();
        let service=LaboratoryService::new(database,config).unwrap();
        for (engine,filename) in [("openmm_argon","scientific_worker.py"),("diffusion_2d","field_worker.py")]{
            let job=service.create(Uuid::new_v4(),project.id,None,"solver","Preserved prior worker",json!({"engine":engine,"parameters":{}}),None).unwrap();
            let path=service.directory(job.id).join(filename);std::fs::write(&path,b"previous worker version\n").unwrap();
            let error=service.execute_solver(job.id,&CancellationToken::new()).await.unwrap_err();
            assert!(error.to_string().contains("Original worker differs; new immutable run required"),"{error:#}");
            assert_eq!(std::fs::read(path).unwrap(),b"previous worker version\n");
            assert!(!service.get(job.id).unwrap().events.iter().any(|event|event.kind=="environment"));
            assert!(!service.directory(job.id).join("stdout.log").exists());
            assert!(!service.directory(job.id).join("field-readiness").exists());
        }
        assert!(!temp.path().join("environments").exists());
    }
    #[cfg(windows)]
    #[test]fn atomic_journal_retries_a_real_reader_lock_and_preserves_permanent_denial(){
        use std::os::windows::fs::OpenOptionsExt;
        let temp=tempfile::tempdir().unwrap();let path=temp.path().join("journal.json");write_json(&path,&json!({"old":true})).unwrap();
        let handle=std::fs::OpenOptions::new().read(true).share_mode(1).open(&path).unwrap();
        let held=std::thread::spawn(move||{std::thread::sleep(Duration::from_millis(120));drop(handle);});
        let started=std::time::Instant::now();write_json(&path,&json!({"new":true})).unwrap();held.join().unwrap();
        assert!(started.elapsed()>=Duration::from_millis(100));assert_eq!(serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap(),json!({"new":true}));
        let before=std::fs::read(&path).unwrap();let mut permissions=std::fs::metadata(&path).unwrap().permissions();permissions.set_readonly(true);std::fs::set_permissions(&path,permissions).unwrap();
        let denied=write_json(&path,&json!({"must_not_replace":true}));
        let mut permissions=std::fs::metadata(&path).unwrap().permissions();permissions.set_readonly(false);std::fs::set_permissions(&path,permissions).unwrap();
        assert!(denied.is_err());assert_eq!(std::fs::read(&path).unwrap(),before);assert!(std::fs::read_dir(temp.path()).unwrap().any(|item|item.unwrap().path().extension().is_some_and(|ext|ext=="tmp")));
    }
    #[test] fn completion_and_polling_do_not_change_with_later_presentation_activity(){
        let time=Utc::now()-chrono::Duration::hours(1);
        let mut job=LabJob{id:Uuid::new_v4(),project_id:Uuid::new_v4(),parent_id:None,kind:"session".into(),state:"completed".into(),title:"Review".into(),created_at:time-chrono::Duration::seconds(10),updated_at:Utc::now(),deadline_at:None,seen_at:None,completed_at:Some(Utc::now()),input:json!({"content":"private long transcript","model":"chosen-model","attachments":["large data"]}),result:json!({"answer":"very large answer"}),progress:json!({}),error:None,events:vec![LabEvent{sequence:1,at:time,kind:"completed".into(),message:"Saved".into(),data:json!({"tool_output":"large scientific arrays"})}]};
        job.event("presentation_changed","New color",json!({}));job.normalize_completion();
        assert_eq!(job.completed_at,Some(time));
        let summary=job.summary();assert_eq!(summary["event_count"],2);assert_eq!(summary["input"]["model"],"chosen-model");assert!(summary["input"]["content"].is_null());assert!(summary["result"]["answer"].is_null());assert_eq!(summary["events"].as_array().unwrap().len(),1);assert_eq!(summary["events"][0]["data"],json!({}));
        assert_eq!(job.events[0].data["tool_output"],"large scientific arrays");assert_eq!(job.result["answer"],"very large answer");
    }
    #[test] fn artifact_paths_cannot_escape_or_use_windows_streams() {
        for path in ["../secret","/root/file","C:/secret","a\\b","file:stream",""]{assert!(safe_relative(path).is_err(),"{path}");}
        assert!(safe_relative("trajectory/chunk-0000.json").is_ok());
    }
    #[test] fn restart_pauses_work_without_reissuing_science() {
        let dir=tempfile::tempdir().unwrap(); let config=AppConfig{data_directory:dir.path().into(),..Default::default()};
        let db=Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Test".into()),question:"test".into()});db.put_project(&project).unwrap();
        let service=LaboratoryService::new(db.clone(),config.clone()).unwrap();
        let id=Uuid::new_v4();service.create(id,project.id,None,"solver","test",json!({"engine":"openmm_argon"}),None).unwrap();
        let restored=LaboratoryService::new(db,config).unwrap();let job=restored.get(id).unwrap();
        assert_eq!(job.state,"paused");assert!(job.events.iter().any(|e|e.kind=="interrupted"));
        assert_eq!(restored.list(None).unwrap().len(),1);
    }
    #[test] fn pause_is_durable_for_the_whole_tree_before_cancellation_is_observable(){
        use std::{future::Future,task::{Wake,Waker,Context as TaskContext,Poll}};
        struct InspectCancellation{service:LaboratoryService,ids:Vec<Uuid>,observed:Arc<Mutex<Vec<String>>>}
        impl Wake for InspectCancellation{
            fn wake(self:Arc<Self>){self.wake_by_ref();}
            fn wake_by_ref(self:&Arc<Self>){*self.observed.lock()=self.ids.iter().map(|id|self.service.get(*id).unwrap().state).collect();}
        }
        let dir=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:dir.path().into(),..Default::default()};
        let db=Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:None,question:"Cancellation ordering".into()});db.put_project(&project).unwrap();
        let service=LaboratoryService::new(db,config).unwrap();
        let root=Uuid::new_v4();let child=Uuid::new_v4();let grandchild=Uuid::new_v4();let complete=Uuid::new_v4();let unrelated=Uuid::new_v4();
        for (id,parent) in [(root,None),(child,Some(root)),(grandchild,Some(child)),(complete,Some(root)),(unrelated,None)]{
            service.create(id,project.id,parent,"session","Pause fixture",json!({}),None).unwrap();
        }
        service.update(complete,|job|{job.state="completed".into();job.result=json!({"saved":true});}).unwrap();
        let token=service.acquire(root).unwrap();let child_token=service.acquire_child(child,&token).unwrap();let grand_token=service.acquire_child(grandchild,&child_token).unwrap();let other_token=service.acquire(unrelated).unwrap();
        let observed=Arc::new(Mutex::new(vec![]));
        let waker=Waker::from(Arc::new(InspectCancellation{service:service.clone(),ids:vec![root,child,grandchild],observed:observed.clone()}));
        let mut waiting=Box::pin(token.cancelled());
        assert!(matches!(waiting.as_mut().poll(&mut TaskContext::from_waker(&waker)),Poll::Pending));
        service.stop(root,"paused").unwrap();
        // This waker runs at cancellation itself, before stop() returns. A mere
        // post-stop assertion misses the race with a fast finishing worker.
        assert_eq!(*observed.lock(),vec!["paused","paused","paused"]);
        assert!(token.is_cancelled()&&child_token.is_cancelled()&&grand_token.is_cancelled());assert!(!other_token.is_cancelled());
        assert_eq!(service.get(unrelated).unwrap().state,"queued");assert_eq!(service.get(complete).unwrap().result,json!({"saved":true}));assert_eq!(service.get(complete).unwrap().state,"completed");
        service.stop(root,"paused").unwrap();
        for id in [root,child,grandchild]{assert_eq!(service.get(id).unwrap().events.iter().filter(|event|event.kind=="paused").count(),1);}
    }
}
