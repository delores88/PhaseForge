//! Local, data-only Blender jobs. Numerical evidence remains in the solver runtime.
use std::{collections::HashMap, fs, path::{Path as FilePath, PathBuf}, process::{Command, Stdio}, sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};
use anyhow::{bail, Context};
use axum::{Router, Json, body::Body, extract::{DefaultBodyLimit, Path, Query, State}, http::{StatusCode, header}, response::{IntoResponse, Response}, routing::{get, post}};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{io::AsyncReadExt, sync::Semaphore};
use uuid::Uuid;
use crate::app::AppState;

const WORKER: &str = include_str!("../../../tools/blender_render.py");
static GATE: Mutex<()> = Mutex::new(());
static ACTIVE: OnceLock<Mutex<HashMap<Uuid, Arc<AtomicBool>>>> = OnceLock::new();
static RENDERS: Semaphore = Semaphore::const_new(1);
static SESSION: OnceLock<Uuid> = OnceLock::new();
fn active() -> &'static Mutex<HashMap<Uuid, Arc<AtomicBool>>> { ACTIVE.get_or_init(Default::default) }
fn session() -> Uuid { *SESSION.get_or_init(Uuid::new_v4) }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderCamera { pub position: [f64; 3], pub target: [f64; 3], #[serde(default)] pub up: Option<[f64;3]>, #[serde(default="lens")] pub focal_length_mm: f64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderBinding { pub node_id:String, pub structure_id:Uuid }
fn lens() -> f64 { 50.0 }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RenderRequest {
    pub project_id: Uuid,
    #[serde(default)] pub scene: Option<Value>,
    #[serde(default)] pub structure_id: Option<Uuid>,
    #[serde(default)] pub bindings: Vec<RenderBinding>,
    #[serde(default="style")] pub style: String,
    #[serde(default="resolution")] pub width: u32,
    #[serde(default="resolution")] pub height: u32,
    #[serde(default="samples")] pub samples: u32,
    #[serde(default="seconds")] pub max_seconds: u64,
    #[serde(default="memory")] pub max_memory_mb: u64,
    #[serde(default)] pub camera: Option<RenderCamera>,
}
fn style()->String { "microscopy".into() } fn resolution()->u32 { 1024 } fn samples()->u32 { 64 }
fn seconds()->u64 { 300 } fn memory()->u64 { 4096 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RenderJob {
    pub id: Uuid, pub project_id: Uuid, pub state: String, pub session_id: Uuid,
    pub created_at: DateTime<Utc>, pub updated_at: DateTime<Utc>, pub deadline_at: DateTime<Utc>,
    pub settings: Value, pub message: String, pub error: Option<String>, pub artifacts: Vec<Value>,
    pub renderer: Option<Value>,
}
fn root(s: &AppState)->PathBuf { s.config.artifacts_directory().join("studio") }
fn save(folder: &FilePath, job: &RenderJob)->anyhow::Result<()> {
    let temporary=folder.join("job.tmp"); fs::write(&temporary,serde_json::to_vec_pretty(job)?)?;
    fs::rename(temporary,folder.join("job.json"))?; Ok(())
}
fn load(folder:&FilePath)->anyhow::Result<RenderJob> {
    let mut job:RenderJob=serde_json::from_slice(&fs::read(folder.join("job.json"))?)?;
    if matches!(job.state.as_str(),"queued"|"rendering") && job.session_id!=session() {
        job.state="interrupted".into(); job.message="The app restarted. Saved inputs and completed files remain available; submit a new render to restart.".into(); job.updated_at=Utc::now(); save(folder,&job)?;
    }
    Ok(job)
}
struct Error(anyhow::Error);
impl From<anyhow::Error> for Error { fn from(e:anyhow::Error)->Self { Self(e) } }
impl IntoResponse for Error { fn into_response(self)->Response { (StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"error":{"message":format!("{:#}",self.0)}}))).into_response() } }
pub fn routes()->Router<Arc<AppState>> {
    Router::new().route("/api/studio/capabilities",get(capabilities))
        .route("/api/studio/renders",post(create).get(list))
        .route("/api/studio/renders/:id",get(detail))
        .route("/api/studio/renders/:id/cancel",post(cancel))
        .route("/api/studio/renders/:id/artifacts/:name",get(artifact))
        .layer(DefaultBodyLimit::max(8*1024*1024))
}
async fn capabilities()->Json<Value> { Json(json!({"available":find_blender().is_some(),"engine":"Blender Cycles","source":"Local Blender installation; set BLENDER_PATH to its executable.","gpu":"OptiX, CUDA, HIP, oneAPI or Metal when Cycles initializes a compatible device; CPU fallback.","formats":["png","blend","glb"],"max_parallel_jobs":1,"max_atoms":50000,"max_atoms_per_surface":12000,"max_resolution":2048})) }
fn validate(q:&RenderRequest)->anyhow::Result<()> {
    if q.scene.is_some()==q.structure_id.is_some() { bail!("Supply exactly one of scene or structure_id"); }
    if !q.bindings.is_empty() && q.scene.is_none() { bail!("Source bindings require a scene and cannot accompany a standalone structure_id"); }
    if q.bindings.len()>16 { bail!("At most 16 molecular source bindings are allowed"); }
    if !["microscopy","studio"].contains(&q.style.as_str()) { bail!("style must be microscopy or studio"); }
    if !(256..=2048).contains(&q.width)||!(256..=2048).contains(&q.height)||!(16..=256).contains(&q.samples)||!(15..=1800).contains(&q.max_seconds)||!(512..=16384).contains(&q.max_memory_mb) { bail!("Render limits: dimensions 256–2048, samples 16–256, seconds 15–1800, memory 512–16384 MiB"); }
    if let Some(scene)=&q.scene { crate::sandbox::validate_scene(scene)?; if scene["nodes"].as_array().is_none_or(|n|n.is_empty()) { bail!("Render scene must contain geometry"); } }
    if let Some(c)=&q.camera {
        if c.position.iter().chain(&c.target).any(|n|!n.is_finite()||n.abs()>1e15)||!(20.0..=120.0).contains(&c.focal_length_mm)||c.position.iter().zip(c.target).map(|(a,b)|(a-b).powi(2)).sum::<f64>()<1e-12 { bail!("Camera needs distinct finite position/target and a 20–120 mm lens"); }
        if let Some(up)=c.up {
            let direction=std::array::from_fn::<_,3,_>(|i|c.target[i]-c.position[i]);
            let up_norm=up.iter().map(|x|x*x).sum::<f64>();let direction_norm=direction.iter().map(|x|x*x).sum::<f64>();
            let dot=up.iter().zip(direction).map(|(a,b)|a*b).sum::<f64>();
            anyhow::ensure!(up.iter().all(|n|n.is_finite()&&n.abs()<=1e15)&&up_norm>1e-12&&1.0-dot*dot/(up_norm*direction_norm)>1e-10,"Camera up must be finite, nonzero and not parallel to its viewing direction");
        }
    }
    Ok(())
}
fn validate_structure(value:&crate::domain::MolecularStructure,project:Uuid)->anyhow::Result<()> {
    if value.project_id!=Some(project) { bail!("Structure must belong to the selected project"); }
    if value.atoms.is_empty()||value.atoms.len()>50000||value.atoms.iter().any(|a|a.position.iter().any(|p|!p.is_finite()||p.abs()>1e9)) { bail!("Structure must contain 1–50000 atoms with finite coordinates"); }
    // Match the trusted worker's chain/ligand grouping before admitting work.
    let mut groups=HashMap::<String,usize>::new();
    for atom in &value.atoms {
        if atom.hetero && ["HOH","WAT","DOD","H2O"].contains(&atom.residue_name.to_uppercase().as_str()) { continue; }
        let mut group=if atom.hetero {format!("Ligand {} {}",if atom.residue_name.is_empty(){&atom.element}else{&atom.residue_name},atom.residue_id)}else{if atom.chain_id.is_empty(){"structure".into()}else{atom.chain_id.clone()}};
        if !groups.contains_key(&group)&&groups.len()>=16 {group="other chains".into();}
        let count=groups.entry(group).or_default();*count+=1;
        if *count>12000 {bail!("A molecular chain/group exceeds the 12000-atom surface budget; select a smaller source assembly");}
    }
    if groups.is_empty(){bail!("Structure contains only solvent; no molecular surface can be rendered");}
    Ok(())
}
fn resolve_bindings(q:&RenderRequest,mut fetch:impl FnMut(Uuid)->anyhow::Result<Option<crate::domain::MolecularStructure>>)->anyhow::Result<HashMap<String,crate::domain::MolecularStructure>> {
    let mut resolved=HashMap::new();
    if q.bindings.is_empty(){return Ok(resolved);}
    let nodes=q.scene.as_ref().context("Source bindings require a scene")?["nodes"].as_array().context("Scene needs nodes")?;
    let mut total=nodes.iter().map(|node|node["parameters"]["atoms"].as_array().map_or(0,Vec::len)).sum::<usize>();
    for binding in &q.bindings {
        if resolved.contains_key(&binding.node_id){bail!("Duplicate source binding for node {}",binding.node_id);}
        let node=nodes.iter().find(|node|node["id"].as_str()==Some(&binding.node_id)).context("Source binding refers to a missing scene node")?;
        if !["molecule","protein","virus"].contains(&node["type"].as_str().unwrap_or("")){bail!("Source bindings support molecule, protein, or virus nodes only");}
        for field in ["atoms","bonds","points","vertices","indices"] {
            if node["parameters"][field].as_array().is_some_and(|values|!values.is_empty()){bail!("Bound node {} mixes a saved source with inline {field}; remove the inline geometry",binding.node_id);}
        }
        let structure=fetch(binding.structure_id)?.context("Bound molecular structure not found")?;
        validate_structure(&structure,q.project_id)?;
        total+=structure.atoms.len();if total>50000 {bail!("Scene sources exceed the shared 50000-atom budget (repeated bindings count separately)");}
        resolved.insert(binding.node_id.clone(),structure);
    }
    Ok(resolved)
}
/// Resolve data-only geometry once before admitting a durable laboratory render.
pub(crate) fn resolved_input(state:&AppState,request:&RenderRequest)->anyhow::Result<Value>{
    validate(request)?;state.database.get_project(request.project_id)?.context("Project not found")?;
    let structure=if let Some(id)=request.structure_id{let value=state.database.get_molecule(id)?.context("Molecular structure not found")?;validate_structure(&value,request.project_id)?;Some(value)}else{None};
    let bound_structures=resolve_bindings(request,|id|state.database.get_molecule(id))?;
    let value=json!({"request":request,"structure":structure,"bound_structures":bound_structures,"gpu_enabled":state.config.gpu_enabled});
    anyhow::ensure!(serde_json::to_vec(&value)?.len()<=48*1024*1024,"Resolved render input exceeds 48 MiB");Ok(value)
}
async fn create(State(s):State<Arc<AppState>>,Json(q):Json<RenderRequest>)->Result<(StatusCode,Json<RenderJob>),Error> {
    validate(&q)?;
    let blender=find_blender().context("Blender is not installed. Install Blender and set BLENDER_PATH to its executable, then restart PhaseForge.")?;
    s.database.get_project(q.project_id)?.context("Project not found")?;
    let structure=if let Some(id)=q.structure_id {
        let value=s.database.get_molecule(id)?.context("Molecular structure not found")?;
        validate_structure(&value,q.project_id)?;
        Some(value)
    } else { None };
    let bound_structures=resolve_bindings(&q,|id|s.database.get_molecule(id))?;
    let input=serde_json::to_vec_pretty(&json!({"request":q,"structure":structure,"bound_structures":bound_structures,"parent_pid":std::process::id(),"gpu_enabled":s.config.gpu_enabled})).map_err(anyhow::Error::from)?;
    if input.len()>48*1024*1024 {return Err(anyhow::anyhow!("Resolved render input exceeds the 48 MiB data budget").into());}
    let _gate=GATE.lock();
    if active().lock().len()>=8 { return Err(anyhow::anyhow!("The render queue is full (8 jobs). Wait for or cancel an existing job.").into()); }
    let id=Uuid::new_v4(); let folder=root(&s).join(id.to_string()); fs::create_dir_all(&folder).map_err(anyhow::Error::from)?;
    let now=Utc::now(); let token=Arc::new(AtomicBool::new(false));
    let job=RenderJob{id,project_id:q.project_id,state:"queued".into(),session_id:session(),created_at:now,updated_at:now,deadline_at:now+chrono::Duration::seconds(q.max_seconds as i64),settings:json!({"style":q.style,"width":q.width,"height":q.height,"samples":q.samples,"max_seconds":q.max_seconds,"max_memory_mb":q.max_memory_mb,"structure_id":q.structure_id,"bindings":q.bindings,"camera":q.camera}),message:"Queued for the local Cycles renderer. The time budget includes queue time.".into(),error:None,artifacts:vec![],renderer:None};
    fs::write(folder.join("input.json"),input).map_err(anyhow::Error::from)?;
    fs::write(folder.join("worker.py"),WORKER).map_err(anyhow::Error::from)?; save(&folder,&job)?;
    active().lock().insert(id,token.clone());
    let pending=job.clone(); tokio::spawn(async move { run_job(folder,blender,pending,token).await; });
    Ok((StatusCode::ACCEPTED,Json(job)))
}
#[derive(Deserialize)] struct ListQuery { project_id:Uuid }
async fn list(State(s):State<Arc<AppState>>,Query(q):Query<ListQuery>)->Result<Json<Value>,Error> {
    let _gate=GATE.lock(); let mut jobs=Vec::new();
    if root(&s).exists() { for entry in fs::read_dir(root(&s)).map_err(anyhow::Error::from)?.take(10000) {
        let path=entry.map_err(anyhow::Error::from)?.path(); if !path.join("job.json").is_file() { continue; }
        let job=load(&path)?; if job.project_id==q.project_id {jobs.push(job);}
    }}
    jobs.sort_by_key(|job|std::cmp::Reverse(job.created_at)); jobs.truncate(100); Ok(Json(json!({"jobs":jobs})))
}
async fn detail(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<RenderJob>,Error> { let _gate=GATE.lock(); Ok(Json(load(&root(&s).join(id.to_string()))?)) }
async fn cancel(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<RenderJob>,Error> {
    let _gate=GATE.lock(); let folder=root(&s).join(id.to_string()); let mut job=load(&folder)?;
    if matches!(job.state.as_str(),"queued"|"rendering") { if let Some(token)=active().lock().get(&id) { token.store(true,Ordering::SeqCst); } job.message="Cancellation requested. Waiting for the local renderer to stop.".into(); job.updated_at=Utc::now(); save(&folder,&job)?; }
    Ok(Json(job))
}
fn artifact_type(name:&str)->Option<&'static str> { match name { "render.png"=>Some("image/png"),"scene.glb"=>Some("model/gltf-binary"),"scene.blend"=>Some("application/octet-stream"),"renderer.json"|"input.json"=>Some("application/json"),_=>None } }
async fn artifact(State(s):State<Arc<AppState>>,Path((id,name)):Path<(Uuid,String)>)->Result<Response,Error> {
    let mime=artifact_type(&name).context("Unknown render artifact")?; let folder=root(&s).join(id.to_string());
    { let _gate=GATE.lock(); let job=load(&folder)?; if job.state!="completed" { return Err(anyhow::anyhow!("Artifacts become available after a completed render").into()); } }
    let file=tokio::fs::File::open(folder.join(&name)).await.map_err(anyhow::Error::from)?;
    let stream=futures_util::stream::try_unfold(file,|mut file|async move { let mut chunk=vec![0u8;64*1024]; let count=file.read(&mut chunk).await?; chunk.truncate(count); Ok::<_,std::io::Error>(if count==0 {None} else {Some((chunk,file))}) });
    Ok(([(header::CONTENT_TYPE,mime),(header::CONTENT_DISPOSITION,if name=="render.png" {"inline"}else{"attachment"})],Body::from_stream(stream)).into_response())
}
async fn run_job(folder:PathBuf,blender:PathBuf,mut job:RenderJob,token:Arc<AtomicBool>) {
    let outcome: anyhow::Result<Value>=async {
        let permit=loop {
            if token.load(Ordering::SeqCst) { bail!("Cancelled by the researcher"); }
            if Utc::now()>=job.deadline_at { bail!("Render deadline expired while queued"); }
            if let Ok(permit)=RENDERS.try_acquire() { break permit; }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        job.state="rendering".into(); job.message="Building surfaces and rendering locally in Blender Cycles.".into(); job.updated_at=Utc::now(); {let _gate=GATE.lock();save(&folder,&job)?;}
        let f=folder.clone(); let j=job.clone(); let t=token.clone();
        let result=tokio::task::spawn_blocking(move||render_process(&f,&blender,&j,&t)).await.context("Render supervisor failed")?;
        drop(permit); result
    }.await;
    job.updated_at=Utc::now();
    match outcome {
        Ok(metadata) if !token.load(Ordering::SeqCst)=>{
            job.state="completed".into();job.message="Cycles render and editable 3D files are ready.".into();job.renderer=Some(metadata);
            for name in ["render.png","scene.blend","scene.glb","renderer.json","input.json"] {if let Ok(info)=fs::metadata(folder.join(name)) {job.artifacts.push(json!({"name":name,"bytes":info.len(),"url":format!("/api/studio/renders/{}/artifacts/{name}",job.id)}));}}
        },
        result=>{job.state=if token.load(Ordering::SeqCst){"cancelled"}else{"failed"}.into();job.error=Some(result.err().map(|e|format!("{e:#}")).unwrap_or_else(||"Cancelled by the researcher".into()));job.message=job.error.clone().unwrap_or_default();}
    }
    {let _gate=GATE.lock();if let Err(error)=save(&folder,&job) {tracing::error!(%error,job_id=%job.id,"Unable to persist render completion");} active().lock().remove(&job.id);}
}
fn render_process(folder:&FilePath,blender:&FilePath,job:&RenderJob,token:&AtomicBool)->anyhow::Result<Value> {
    let remaining=(job.deadline_at-Utc::now()).to_std().context("Render deadline expired")?;
    let mut system=sysinfo::System::new(); system.refresh_memory();
    let budget=(job.settings["max_memory_mb"].as_u64().unwrap_or(4096)*1024*1024).min(system.available_memory()*3/5);
    if budget<512*1024*1024 { bail!("Insufficient available RAM for a bounded Blender job (512 MiB minimum)"); }
    let log=fs::File::create(folder.join("blender.log"))?; let stderr=log.try_clone()?;
    let mut command=Command::new(blender); command.args(["--background","--factory-startup","--disable-autoexec","--python-exit-code","2","--python"]).arg(folder.join("worker.py")).arg("--").arg(folder.join("input.json")).arg(folder).arg(remaining.as_secs().max(1).to_string()).arg(budget.to_string());
    command.stdin(Stdio::null()).stdout(Stdio::from(log)).stderr(Stdio::from(stderr));
    #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
    #[cfg(unix)] {use std::os::unix::process::CommandExt;command.process_group(0);}
    let mut child=command.spawn().context("Unable to start Blender")?;
    let tree=match ProcessTree::attach(&child,budget) {Ok(tree)=>tree,Err(error)=>{let _=child.kill();let _=child.wait();return Err(error);}};
    let start=Instant::now(); let pid=sysinfo::Pid::from_u32(child.id()); let mut last_memory=Instant::now()-Duration::from_secs(2);
    let status=loop {
        let reason=if token.load(Ordering::SeqCst) {Some("Cancelled by the researcher")} else if start.elapsed()>=remaining {Some("Render exceeded the authorized time budget")} else {None};
        if let Some(reason)=reason {drop(tree);let _=child.kill();let _=child.wait();bail!("{reason}");}
        if last_memory.elapsed()>=Duration::from_secs(1) {
            system.refresh_processes();system.refresh_memory();last_memory=Instant::now();
            let mut owned=std::collections::HashSet::from([pid]);loop {let previous=owned.len();for (id,process) in system.processes(){if process.parent().is_some_and(|p|owned.contains(&p)){owned.insert(*id);}}if owned.len()==previous{break;}}
            let used:u64=owned.iter().filter_map(|id|system.process(*id)).map(|p|p.memory()).sum();
            if used>budget||system.available_memory()<128*1024*1024 {drop(tree);let _=child.kill();let _=child.wait();bail!("Render stopped at its RAM budget; lower dimensions or simplify the geometry before restarting");}
        }
        match child.try_wait(){Ok(Some(status))=>break status,Ok(None)=>{},Err(error)=>{drop(tree);let _=child.kill();let _=child.wait();return Err(error.into());}}
        std::thread::sleep(Duration::from_millis(100));
    };
    drop(tree);
    if !status.success() {let log=fs::read_to_string(folder.join("blender.log")).unwrap_or_default(); let tail=log.chars().rev().take(3000).collect::<String>().chars().rev().collect::<String>();bail!("Blender exited with {status}: {tail}");}
    for name in ["render.png","scene.blend","scene.glb"] {let info=fs::metadata(folder.join(name)).with_context(||format!("Blender did not produce {name}"))?;let limit=if name=="scene.glb" {128} else {256};if info.len()==0||info.len()>limit*1024*1024 {bail!("{name} is empty or exceeds its {limit} MiB artifact budget");}}
    Ok(serde_json::from_slice(&fs::read(folder.join("renderer.json"))?)?)
}
pub(crate) fn find_blender()->Option<PathBuf> {
    if let Some(path)=std::env::var_os("BLENDER_PATH").map(PathBuf::from) {if path.is_file(){return Some(path);}}
    let name=if cfg!(windows){"blender.exe"}else{"blender"};
    if let Some(paths)=std::env::var_os("PATH") {for directory in std::env::split_paths(&paths){let path=directory.join(name);if path.is_file(){return Some(path);}}}
    let mut locations=vec![PathBuf::from("/usr/bin/blender"),PathBuf::from("/usr/local/bin/blender"),PathBuf::from("/snap/bin/blender"),PathBuf::from("/Applications/Blender.app/Contents/MacOS/Blender")];
    for base in [std::env::var_os("ProgramFiles").map(PathBuf::from),std::env::var_os("LOCALAPPDATA").map(|v|PathBuf::from(v).join("Programs"))].into_iter().flatten(){
        for vendor in [base.join("Blender Foundation"),base.join("PhaseForge").join("blender")] {locations.push(vendor.join(name));if let Ok(entries)=fs::read_dir(vendor){let mut entries=entries.filter_map(Result::ok).map(|e|e.path().join(name)).collect::<Vec<_>>();entries.sort();entries.reverse();locations.extend(entries);}}
    }
    if let Some(base)=std::env::var_os("LOCALAPPDATA") {let directory=PathBuf::from(base).join("PhaseForge/engines/blender");locations.push(directory.join(name));if let Ok(entries)=fs::read_dir(directory){let mut paths=entries.filter_map(Result::ok).map(|e|e.path().join(name)).collect::<Vec<_>>();paths.sort();paths.reverse();locations.extend(paths);}}
    if let Some(base)=std::env::var_os("LOCALAPPDATA") {let directory=PathBuf::from(base).join("PhaseForge/engines");if let Ok(entries)=fs::read_dir(directory){let mut paths=entries.filter_map(Result::ok).filter(|e|e.file_name().to_string_lossy().starts_with("blender-")).map(|e|e.path().join(name)).collect::<Vec<_>>();paths.sort();paths.reverse();locations.extend(paths);}}
    locations.into_iter().find(|p|p.is_file())
}

#[cfg(unix)] pub(crate) struct ProcessTree(i32);
#[cfg(unix)] impl ProcessTree {pub(crate) fn attach(child:&std::process::Child,_budget:u64)->anyhow::Result<Self>{Ok(Self(child.id() as i32))}}
#[cfg(unix)] impl Drop for ProcessTree {fn drop(&mut self){unsafe {unsafe extern "C" {fn kill(pid:i32,signal:i32)->i32;} kill(-self.0,9);}}}

#[cfg(windows)] mod windows_job {
    use std::{ffi::c_void,os::windows::io::AsRawHandle};
    #[repr(C)]#[derive(Default)]struct Basic {process_time:i64,job_time:i64,flags:u32,min_working:usize,max_working:usize,active:u32,affinity:usize,priority:u32,scheduling:u32}
    #[repr(C)]#[derive(Default)]struct Io {read:u64,write:u64,other:u64,read_bytes:u64,write_bytes:u64,other_bytes:u64}
    #[repr(C)]#[derive(Default)]struct Extended {basic:Basic,io:Io,process_memory:usize,job_memory:usize,peak_process:usize,peak_job:usize}
    #[link(name="kernel32")]unsafe extern "system" {fn CreateJobObjectW(attributes:*const c_void,name:*const u16)->*mut c_void;fn SetInformationJobObject(job:*mut c_void,class:i32,info:*const c_void,size:u32)->i32;fn AssignProcessToJobObject(job:*mut c_void,process:*mut c_void)->i32;fn CloseHandle(handle:*mut c_void)->i32;}
    pub(crate) struct ProcessTree(*mut c_void);
    impl ProcessTree {pub(crate) fn attach(child:&std::process::Child,budget:u64)->anyhow::Result<Self>{unsafe {
        let handle=CreateJobObjectW(std::ptr::null(),std::ptr::null());if handle.is_null(){return Err(std::io::Error::last_os_error().into());}
        let tree=Self(handle);let mut limits=Extended::default();limits.basic.flags=0x2000|0x200;limits.job_memory=budget as usize;
        if SetInformationJobObject(handle,9,&limits as *const _ as *const c_void,std::mem::size_of::<Extended>() as u32)==0||AssignProcessToJobObject(handle,child.as_raw_handle())==0 {return Err(std::io::Error::last_os_error().into());}Ok(tree)
    }}}
    impl Drop for ProcessTree {fn drop(&mut self){unsafe{CloseHandle(self.0);}}}
}
#[cfg(windows)] pub(crate) use windows_job::ProcessTree;

#[cfg(test)]mod tests {
    use super::*;
    fn request()->RenderRequest {serde_json::from_value(json!({"project_id":Uuid::nil(),"structure_id":Uuid::new_v4()})).unwrap()}
    fn binding_fixture()->(RenderRequest,crate::domain::MolecularStructure) {
        let structure=crate::science::molecular::import_structure(crate::domain::ImportStructureRequest{project_id:Some(Uuid::nil()),name:"Saved coordinates".into(),format:crate::domain::MolecularFormat::Xyz,content:"3\nsource coordinates\nC 0 0 0\nO 1.3 0 0\nN 0 1.4 0\n".into()}).unwrap();
        let q=serde_json::from_value(json!({"project_id":Uuid::nil(),"scene":{"schema_version":"1.0","provenance":{"kind":"conceptual","description":"Source structure placed in an authored scene"},"nodes":[{"id":"target","type":"protein","parameters":{"radius":2}}]},"bindings":[{"node_id":"target","structure_id":structure.id}]})).unwrap();(q,structure)
    }
    #[test]fn scene_bindings_resolve_exact_saved_coordinates_without_mutating_scene(){
        let (q,structure)=binding_fixture();validate(&q).unwrap();let original=q.scene.clone();
        let resolved=resolve_bindings(&q,|id|{assert_eq!(id,structure.id);Ok(Some(structure.clone()))}).unwrap();
        assert_eq!(serde_json::to_value(&resolved["target"]).unwrap(),serde_json::to_value(&structure).unwrap());assert_eq!(q.scene,original);
    }
    #[test]fn scene_bindings_reject_wrong_ownership_missing_nodes_and_mixed_geometry(){
        let (mut q,mut structure)=binding_fixture();structure.project_id=Some(Uuid::new_v4());assert!(resolve_bindings(&q,|_|Ok(Some(structure.clone()))).is_err());structure.project_id=Some(q.project_id);
        q.bindings[0].node_id="missing".into();assert!(resolve_bindings(&q,|_|Ok(Some(structure.clone()))).is_err());q.bindings[0].node_id="target".into();
        q.scene.as_mut().unwrap()["nodes"][0]["parameters"]["atoms"]=json!([{"position":[0,0,0]}]);assert!(resolve_bindings(&q,|_|Ok(Some(structure.clone()))).is_err());
        q.scene.as_mut().unwrap()["nodes"][0]["parameters"]=json!({});q.scene.as_mut().unwrap()["nodes"][0]["type"]=json!("planet");assert!(resolve_bindings(&q,|_|Ok(Some(structure.clone()))).is_err());
        q.scene.as_mut().unwrap()["nodes"][0]["type"]=json!("protein");q.bindings.push(q.bindings[0].clone());assert!(resolve_bindings(&q,|_|Ok(Some(structure.clone()))).is_err());
    }
    #[test]fn source_admission_checks_surface_and_aggregate_atom_budgets(){
        let (mut q,mut structure)=binding_fixture();structure.atoms=vec![structure.atoms[0].clone();12001];assert!(validate_structure(&structure,q.project_id).is_err());
        for (i,atom) in structure.atoms.iter_mut().enumerate(){atom.hetero=false;atom.chain_id=(i/10000).to_string();}assert!(validate_structure(&structure,q.project_id).is_ok());
        let first=q.scene.as_ref().unwrap()["nodes"][0].clone();let mut nodes=vec![];q.bindings.clear();
        for i in 0..5 {let id=format!("part-{i}");let mut node=first.clone();node["id"]=json!(id);nodes.push(node);q.bindings.push(RenderBinding{node_id:id,structure_id:structure.id});}
        q.scene.as_mut().unwrap()["nodes"]=json!(nodes);assert!(resolve_bindings(&q,|_|Ok(Some(structure.clone()))).unwrap_err().to_string().contains("50000"));
    }
    #[test]fn enforces_exclusive_source_and_resource_caps(){let mut q=request();assert!(validate(&q).is_ok());q.scene=Some(json!({}));assert!(validate(&q).is_err());q.scene=None;q.width=16384;assert!(validate(&q).is_err());q=request();q.max_seconds=0;assert!(validate(&q).is_err());q=request();q.camera=Some(RenderCamera{position:[0.;3],target:[0.;3],up:None,focal_length_mm:50.});assert!(validate(&q).is_err());}
    #[test]fn artifact_names_cannot_escape_job_directory(){assert!(artifact_type("render.png").is_some());for name in ["../job.json","worker.py","C:\\secret","blender.log"]{assert!(artifact_type(name).is_none());}}
    #[test]fn restart_marks_active_job_interrupted_and_preserves_completed_files(){let dir=tempfile::tempdir().unwrap();let now=Utc::now();let job=RenderJob{id:Uuid::new_v4(),project_id:Uuid::nil(),state:"rendering".into(),session_id:Uuid::new_v4(),created_at:now,updated_at:now,deadline_at:now,settings:json!({}),message:String::new(),error:None,artifacts:vec![],renderer:None};save(dir.path(),&job).unwrap();fs::write(dir.path().join("scene.blend"),b"saved").unwrap();let restored=load(dir.path()).unwrap();assert_eq!(restored.state,"interrupted");assert_eq!(fs::read(dir.path().join("scene.blend")).unwrap(),b"saved");}
    #[cfg(windows)]#[test]fn windows_job_drop_stops_the_owned_process_tree(){
        use std::os::windows::process::CommandExt;
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("child.log");
        let mut child=Command::new("cmd.exe").args(["/D","/C","ping -n 30 127.0.0.1 >NUL"]).creation_flags(0x08000000).stdout(Stdio::from(fs::File::create(&path).unwrap())).stderr(Stdio::null()).spawn().unwrap();
        let tree=ProcessTree::attach(&child,512*1024*1024).unwrap();std::thread::sleep(Duration::from_millis(150));assert!(child.try_wait().unwrap().is_none());
        let stopped=Instant::now();drop(tree);child.wait().unwrap();assert!(stopped.elapsed()<Duration::from_secs(5));fs::remove_file(path).unwrap();
    }
    #[test]#[ignore="Requires a local Blender installation and PHASEFORGE_RENDER_TEST_PDB / PHASEFORGE_RENDER_TEST_OUT; executes a real Cycles render"]
    fn real_local_blender_writes_all_artifacts(){
        let source=PathBuf::from(std::env::var_os("PHASEFORGE_RENDER_TEST_PDB").expect("Supply an actual PDB file"));let folder=PathBuf::from(std::env::var_os("PHASEFORGE_RENDER_TEST_OUT").expect("Supply an isolated output directory"));fs::create_dir_all(&folder).unwrap();
        let name=format!("PDB {} validation structure",source.file_stem().unwrap().to_string_lossy());
        let project=Uuid::new_v4();let structure=crate::science::molecular::import_structure(crate::domain::ImportStructureRequest{project_id:Some(project),name,format:crate::domain::MolecularFormat::Pdb,content:fs::read_to_string(source).unwrap()}).unwrap();
        let mut q=request();q.project_id=project;q.structure_id=Some(structure.id);q.width=768;q.height=768;q.samples=32;q.max_seconds=240;
        fs::write(folder.join("input.json"),serde_json::to_vec_pretty(&json!({"request":q,"structure":structure,"parent_pid":std::process::id()})).unwrap()).unwrap();fs::write(folder.join("worker.py"),WORKER).unwrap();
        let now=Utc::now();let job=RenderJob{id:Uuid::new_v4(),project_id:project,state:"rendering".into(),session_id:session(),created_at:now,updated_at:now,deadline_at:now+chrono::Duration::seconds(240),settings:json!({"max_memory_mb":4096}),message:String::new(),error:None,artifacts:vec![],renderer:None};
        let metadata=render_process(&folder,&find_blender().expect("Install Blender"),&job,&AtomicBool::new(false)).unwrap();
        assert!(fs::read(folder.join("render.png")).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));assert!(fs::read(folder.join("scene.glb")).unwrap().starts_with(b"glTF"));assert!(fs::metadata(folder.join("scene.blend")).unwrap().len()>10000);
        assert_eq!(metadata["source"]["kind"],"imported_structure");assert!(metadata["source"]["atom_count"].as_u64().unwrap()>100);assert!(metadata["actual_compute"]["backend"].is_string());
        println!("Actual Blender verification: {}",serde_json::to_string_pretty(&metadata).unwrap());
    }
}
