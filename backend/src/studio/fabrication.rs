//! Durable, bounded jobs for trusted CAD/PCB workers; generated code never executes.
use std::{collections::HashMap, path::{Path as FsPath, PathBuf}, sync::{Arc, OnceLock}, process::Stdio, time::Duration};
use anyhow::{bail, Context};
use axum::{Router, Json, extract::{State, Path, Query}, response::{Response, IntoResponse}, routing::{get, post}, http::{StatusCode, header}};
use chrono::{Utc, DateTime};
use serde::{Serialize, Deserialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use crate::app::AppState;

static GATE: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
static ACTIVE: OnceLock<parking_lot::Mutex<HashMap<Uuid, CancellationToken>>> = OnceLock::new();
static OWNER: OnceLock<Uuid> = OnceLock::new();
fn owner()->Uuid {*OWNER.get_or_init(Uuid::new_v4)}
fn active()->&'static parking_lot::Mutex<HashMap<Uuid,CancellationToken>> {ACTIVE.get_or_init(Default::default)}
fn root(state:&AppState)->PathBuf {state.config.artifacts_directory().join("fabrication")}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Job {pub id:Uuid,pub project_id:Uuid,pub status:String,pub created_at:DateTime<Utc>,pub completed_at:Option<DateTime<Utc>>,pub max_seconds:u64,pub result:Option<Value>,pub error:Option<String>,runtime_owner:Uuid}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {project_id:Uuid,design:Value,#[serde(default="duration")] max_seconds:u64}
fn duration()->u64 {300}
#[derive(Deserialize)]struct List {project_id:Uuid}
struct Error(anyhow::Error);
impl From<anyhow::Error> for Error {fn from(e:anyhow::Error)->Self {Self(e)}}
impl IntoResponse for Error {fn into_response(self)->Response {(StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"error":{"message":format!("{:#}",self.0)}}))).into_response()}}
pub fn routes()->Router<Arc<AppState>> {Router::new()
    .route("/api/studio/fabrications",get(list).post(create))
    .route("/api/studio/fabrications/:id",get(get_job))
    .route("/api/studio/fabrications/:id/cancel",post(cancel))
    .route("/api/studio/fabrications/:id/artifacts/:name",get(artifact))
    .route("/api/studio/fabrication-engines",get(engines))}

pub fn schema()->Value {serde_json::from_str(include_str!("../../../docs/fabrication.schema.json")).expect("fabrication schema")}
pub fn validate_design(value:&Value)->anyhow::Result<()> {
    if serde_json::to_vec(value)?.len()>2*1024*1024 {bail!("Fabrication design exceeds 2 MiB");}
    fn check(value:&Value,s:&Value,path:&str)->anyhow::Result<()> {
        if let Some(choices)=s["anyOf"].as_array(){if choices.iter().any(|choice|check(value,choice,path).is_ok()){return Ok(());}bail!("{path}: invalid object variant");}
        let valid=match s["type"].as_str().unwrap_or(""){"object"=>value.is_object(),"array"=>value.is_array(),"string"=>value.is_string(),"number"=>value.as_f64().is_some_and(|n|n.is_finite()&&n.abs()<=100000.),"null"=>value.is_null(),_=>false};
        if !valid {bail!("{path}: wrong data type or nonfinite/unbounded number");}
        if let Some(options)=s["enum"].as_array(){if !options.contains(value){bail!("{path}: unsupported option");}}
        if let Some(text)=value.as_str(){if text.len()>16000||text.chars().any(char::is_control){bail!("{path}: text is too long or contains control characters");}}
        if let Some(obj)=value.as_object(){let props=s["properties"].as_object().context("object schema missing")?;
            for key in s["required"].as_array().into_iter().flatten().filter_map(Value::as_str){if !obj.contains_key(key){bail!("{path}.{key}: required");}}
            for (key,v) in obj {check(v,props.get(key).with_context(||format!("{path}.{key}: unsupported field"))?,&format!("{path}.{key}"))?;}}
        if let Some(items)=value.as_array(){if items.len()<s["minItems"].as_u64().unwrap_or(0) as usize||items.len()>s["maxItems"].as_u64().unwrap_or(2048) as usize {bail!("{path}: array length exceeds supported bounds");}for (i,item) in items.iter().enumerate(){check(item,&s["items"],&format!("{path}[{i}]"))?;}}
        Ok(())
    }
    check(value,&schema(),"fabrication")?;
    match value["kind"].as_str(){Some("cad") if value["cad"].is_object()&&value["pcb"].is_null()=>{},Some("pcb") if value["pcb"].is_object()&&value["cad"].is_null()=>{},_=>bail!("Specify exactly one CAD or PCB design")};
    Ok(())
}
fn worker()->anyhow::Result<PathBuf> {
    let exe=std::env::current_exe()?;
    let candidates=[std::env::current_dir()?.join("tools/fabrication_worker.py"),exe.parent().unwrap_or(FsPath::new(".")).join("../tools/fabrication_worker.py"),PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools/fabrication_worker.py")];
    candidates.into_iter().find(|p|p.is_file()).context("Trusted fabrication worker missing; reinstall PhaseForge")
}
fn python()->PathBuf {
    if let Some(path)=std::env::var_os("PHASEFORGE_CAD_PYTHON"){return path.into();}
    if let Some(local)=std::env::var_os("LOCALAPPDATA") {for environment in ["cadquery-cpython","cadquery"] {let p=PathBuf::from(&local).join("PhaseForge/engines").join(environment).join("Scripts/python.exe");if p.is_file(){return p;}}}
    if let Some(home)=std::env::var_os("HOME"){for environment in ["cadquery-cpython","cadquery"] {let p=PathBuf::from(&home).join(".local/share/PhaseForge/engines").join(environment).join("bin/python");if p.is_file(){return p;}}}
    if cfg!(windows){"python".into()}else{"python3".into()}
}
fn save(dir:&FsPath,job:&Job)->anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let temp=dir.join(format!("job-{}.tmp",Uuid::new_v4()));std::fs::write(&temp,serde_json::to_vec_pretty(job)?)?;
    let target=dir.join("job.json");
    std::fs::rename(temp,target)?;Ok(())
}
static RECORDS:parking_lot::Mutex<()>=parking_lot::Mutex::new(());
fn read(state:&AppState,id:Uuid)->anyhow::Result<Job> {
    let _guard=RECORDS.lock();let dir=root(state).join(id.to_string());
    let mut job:Job=serde_json::from_slice(&std::fs::read(dir.join("job.json")).context("Fabrication job not found")?)?;
    if job.runtime_owner!=owner()&&["queued","running"].contains(&job.status.as_str()) {job.status="interrupted".into();job.error=Some("The application restarted. Retained design can be fabricated again.".into());save(&dir,&job)?;}
    Ok(job)
}
fn persist(dir:&FsPath,job:&Job)->anyhow::Result<()> {let _guard=RECORDS.lock();save(dir,job)}
async fn engines()->Result<Json<Value>,Error> {
    let mut command=tokio::process::Command::new(python());command.arg(worker()?).arg("--probe").kill_on_drop(true);
    #[cfg(windows)] command.creation_flags(0x08000000);
    let result=tokio::time::timeout(Duration::from_secs(12),command.output()).await;
    Ok(Json(match result {Ok(Ok(out)) if out.status.success()=>serde_json::from_slice::<Value>(&out.stdout).unwrap_or(json!({"cadquery_available":false,"kicad_available":false})),_=>json!({"cadquery_available":false,"kicad_available":false,"notice":"Install the optional research engines or configure PHASEFORGE_CAD_PYTHON."})}))
}
async fn create(State(state):State<Arc<AppState>>,Json(q):Json<Request>)->Result<Json<Job>,Error> {
    state.database.get_project(q.project_id)?.context("Project not found")?;validate_design(&q.design)?;
    if active().lock().len()>=8 {return Err(anyhow::anyhow!("Fabrication queue is full; wait for an existing job or cancel it").into());}
    if !(5..=1800).contains(&q.max_seconds){return Err(anyhow::anyhow!("Fabrication deadline must be 5–1800 seconds").into());}
    let id=Uuid::new_v4();let dir=root(&state).join(id.to_string());std::fs::create_dir_all(&dir).map_err(anyhow::Error::from)?;
    std::fs::write(dir.join("request.json"),serde_json::to_vec_pretty(&q.design).map_err(anyhow::Error::from)?).map_err(anyhow::Error::from)?;
    std::fs::write(dir.join("schema.json"),serde_json::to_vec(&schema()).map_err(anyhow::Error::from)?).map_err(anyhow::Error::from)?;
    let job=Job{id,project_id:q.project_id,status:"queued".into(),created_at:Utc::now(),completed_at:None,max_seconds:q.max_seconds,result:None,error:None,runtime_owner:owner()};persist(&dir,&job)?;
    let token=CancellationToken::new();active().lock().insert(id,token.clone());let running=job.clone();
    tokio::spawn(async move {let mut job=running;let deadline=tokio::time::Instant::now()+Duration::from_secs(job.max_seconds);
        let outcome=execute(&dir,&mut job,deadline,token.clone()).await;
        job.status=if token.is_cancelled(){"cancelled"}else if outcome.is_ok(){"completed"}else{"failed"}.into();job.error=outcome.err().map(|e|format!("{e:#}"));job.completed_at=Some(Utc::now());let _=persist(&dir,&job);active().lock().remove(&id);
    });Ok(Json(job))
}
async fn execute(dir:&FsPath,job:&mut Job,deadline:tokio::time::Instant,token:CancellationToken)->anyhow::Result<()> {
    let _permit=tokio::select!{_ = token.cancelled()=>bail!("Cancelled"),_ = tokio::time::sleep_until(deadline)=>bail!("Fabrication queue time limit reached"),p=GATE.acquire()=>p?};
    job.status="running".into();persist(dir,job)?;
    let dir=dir.to_owned();let python=python();let worker=worker()?;let deadline=deadline.into_std();
    job.result=Some(tokio::task::spawn_blocking(move||->anyhow::Result<Value>{
        const MEMORY_BUDGET:u64=4*1024*1024*1024;
        if token.is_cancelled(){bail!("Cancelled");}
        let remaining=deadline.saturating_duration_since(std::time::Instant::now());if remaining.is_zero(){bail!("Fabrication time limit reached");}
        let log=std::fs::File::create(dir.join("worker.log"))?;
        let mut cmd=std::process::Command::new(python);cmd.arg(worker).arg("--input").arg(dir.join("request.json")).arg("--output").arg(&dir).arg("--schema").arg(dir.join("schema.json")).arg("--max-seconds").arg(remaining.as_secs().max(1).to_string()).stdin(Stdio::null()).stdout(Stdio::from(log.try_clone()?)).stderr(Stdio::from(log));
        #[cfg(windows)] {use std::os::windows::process::CommandExt;cmd.creation_flags(0x08000000);}
        #[cfg(unix)] {use std::os::unix::process::CommandExt;cmd.process_group(0);}
        let mut child=cmd.spawn().context("Could not launch CAD/PCB worker; install the optional research engines")?;
        let tree=match super::render::ProcessTree::attach(&child,MEMORY_BUDGET){Ok(tree)=>tree,Err(error)=>{let _=child.kill();let _=child.wait();return Err(error);}};
        let pid=sysinfo::Pid::from_u32(child.id());let mut system=sysinfo::System::new();let mut checked=std::time::Instant::now()-Duration::from_secs(2);
        let status=loop {
            if token.is_cancelled(){stop(&mut child,tree);bail!("Cancelled");}
            if std::time::Instant::now()>=deadline{stop(&mut child,tree);bail!("Fabrication time limit reached");}
            if checked.elapsed()>=Duration::from_secs(1){
                system.refresh_processes();system.refresh_memory();checked=std::time::Instant::now();
                let mut owned=std::collections::HashSet::from([pid]);
                loop {let before=owned.len();for (id,process) in system.processes(){if process.parent().is_some_and(|p|owned.contains(&p)){owned.insert(*id);}}if owned.len()==before{break;}}
                let used:u64=owned.iter().filter_map(|id|system.process(*id)).map(|p|p.memory()).sum();
                if used>MEMORY_BUDGET||system.available_memory()<128*1024*1024 {stop(&mut child,tree);bail!("Fabrication stopped at its 4 GiB process-tree RAM limit or because host memory was exhausted");}
            }
            match child.try_wait(){Ok(Some(status))=>break status,Ok(None)=>{},Err(error)=>{stop(&mut child,tree);return Err(error.into());}}
            std::thread::sleep(Duration::from_millis(100));
        };
        drop(tree);
        if !status.success(){let log=std::fs::read_to_string(dir.join("worker.log")).unwrap_or_default();let tail=log.chars().rev().take(2500).collect::<String>().chars().rev().collect::<String>();bail!("Fabrication worker failed: {tail}");}
        Ok(serde_json::from_slice(&std::fs::read(dir.join("result.json"))?)?)
    }).await.context("Fabrication supervisor failed")??);Ok(())
}
fn stop(child:&mut std::process::Child,tree:super::render::ProcessTree) {
    drop(tree);let _=child.kill();let _=child.wait();
}
async fn list(State(state):State<Arc<AppState>>,Query(q):Query<List>)->Result<Json<Vec<Job>>,Error> {
    state.database.get_project(q.project_id)?.context("Project not found")?;let mut jobs=vec![];
    if let Ok(entries)=std::fs::read_dir(root(&state)){for entry in entries.flatten(){if let Ok(id)=entry.file_name().to_string_lossy().parse(){if let Ok(job)=read(&state,id){if job.project_id==q.project_id{jobs.push(job);}}}}}
    jobs.sort_by_key(|j|std::cmp::Reverse(j.created_at));Ok(Json(jobs))
}
async fn get_job(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Job>,Error>{Ok(Json(read(&state,id)?))}
async fn cancel(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Job>,Error>{let mut job=read(&state,id)?;if let Some(token)=active().lock().get(&id){token.cancel();job.status="cancelling".into();}Ok(Json(job))}
async fn artifact(State(state):State<Arc<AppState>>,Path((id,name)):Path<(Uuid,String)>)->Result<Response,Error> {
    let job=read(&state,id)?;state.database.get_project(job.project_id)?.context("Project not found")?;
    let allowed=["model.step","model.stl","board.kicad_pcb","board.glb","board.svg","bom.csv","drc.json","manufacturing.zip","editable-project.zip","result.json","request.json"];
    if !allowed.contains(&name.as_str()){return Err(anyhow::anyhow!("Artifact not found").into());}
    let bytes=tokio::fs::read(root(&state).join(id.to_string()).join(&name)).await.map_err(anyhow::Error::from)?;
    let mime=if name.ends_with(".glb"){"model/gltf-binary"}else if name.ends_with(".svg"){"image/svg+xml"}else if name.ends_with(".json"){"application/json"}else if name.ends_with(".stl"){"model/stl"}else{"application/octet-stream"};
    Ok(([(header::CONTENT_TYPE,mime),(header::CONTENT_DISPOSITION,&format!("attachment; filename=\"{name}\""))],bytes).into_response())
}

#[cfg(test)]mod tests {use super::*;
    #[test]fn rejects_code_unknown_fields_unbounded_geometry_and_wrong_modes(){
        let mut d=json!({"kind":"cad","units":"mm","description":"test","cad":{"parts":[],"fillet_radius":0},"pcb":null});assert!(validate_design(&d).is_ok());d["script"]=json!("import os");assert!(validate_design(&d).is_err());d.as_object_mut().unwrap().remove("script");d["cad"]["fillet_radius"]=json!(1e100);assert!(validate_design(&d).is_err());d["cad"]["fillet_radius"]=json!(0);d["kind"]=json!("pcb");assert!(validate_design(&d).is_err());
    }
}
