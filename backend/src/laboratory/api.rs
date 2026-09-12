use std::sync::Arc;
use anyhow::Context;
use axum::{Router,Json,body::Body,extract::{State,Path,Query},routing::{get,post},response::{IntoResponse,Response},http::{StatusCode,header}};
use serde::Deserialize;
use serde_json::{json,Value};
use uuid::Uuid;
use crate::app::AppState;
use super::{LabJob,write_json};

pub fn routes()->Router<Arc<AppState>>{
    Router::new()
        .merge(super::exports::routes())
        .merge(super::data::routes())
        .merge(crate::agent::laboratory::steering_routes())
        .route("/api/laboratory/capabilities",get(capabilities))
        .route("/api/laboratory/jobs",get(list))
        .route("/api/laboratory/jobs/:id",get(get_job))
        .route("/api/laboratory/jobs/:id/control",post(control))
        .route("/api/laboratory/jobs/:id/seen",post(seen))
        .route("/api/laboratory/jobs/:id/artifacts/*path",get(artifact))
        .route("/api/laboratory/jobs/:id/presentation",get(presentation).put(save_presentation))
        .route("/api/laboratory/jobs/:id/observations",post(observe))
        .route("/api/laboratory/jobs/:id/review",post(review_result))
        .route("/api/projects/:id/laboratory/chat",post(chat))
        .route("/api/projects/:id/laboratory/seen",post(seen_project))
        .route("/api/projects/:id/laboratory/jobs",post(create_standalone_job))
}
/// Direct numerical clients use the same retained jobs and supervision as chat.
/// This route never selects a provider, creates synthetic model turns or bypasses
/// the generated-code boundary.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StandaloneJobRequest { request_id:Uuid, kind:String, title:String, input:Value, time_limit_seconds:Value }
fn standalone_deadline(request:&StandaloneJobRequest,now:chrono::DateTime<chrono::Utc>)->anyhow::Result<Option<chrono::DateTime<chrono::Utc>>>{
    anyhow::ensure!(!request.title.trim().is_empty()&&request.title.len()<=8000,"Provide a bounded job title");
    match request.kind.as_str(){
        "solver"=>{
            let input=request.input.as_object().context("Solver input must be an object")?;
            anyhow::ensure!(input.keys().all(|key|["engine","parameters","sources","question","hypothesis"].contains(&key.as_str())),"Unsupported standalone solver input field");
            anyhow::ensure!(request.input["parameters"].is_object(),"Solver parameters must be an object");
            match request.input["engine"].as_str(){
                Some("openmm_argon")=>{},
                Some("diffusion_2d")=>super::field::validate(&request.input["parameters"])?,
                Some("heat_conduction_2d")=>super::thermal::validate(&request.input["parameters"])?,
                Some("navier_stokes_2d")=>super::fluid::validate(&request.input["parameters"])?,
                Some("newtonian_nbody")=>super::mechanics::validate(&request.input["parameters"])?,
                _=>anyhow::bail!("Choose an executable scientific engine"),
            }
        },
        "generated"=>{super::generated::validate_generated_input(&request.input)?;},
        _=>anyhow::bail!("Standalone creation supports solver or generated jobs"),
    }
    if request.time_limit_seconds.is_null(){return Ok(None);}
    let seconds=request.time_limit_seconds.as_u64().context("Choose explicit seconds or null for Off")?;
    anyhow::ensure!((10..=604800).contains(&seconds),"Job budget must be 10 seconds to 7 days");
    Ok(Some(now+chrono::Duration::seconds(seconds as i64)))
}
async fn create_standalone_job(State(state):State<Arc<AppState>>,Path(project):Path<Uuid>,Json(request):Json<StandaloneJobRequest>)->Result<Json<LabJob>,Error>{
    let deadline=standalone_deadline(&request,chrono::Utc::now())?;
    let job=state.laboratory.create(request.request_id,project,None,&request.kind,&request.title,request.input,deadline)?;
    if job.state=="queued"&&!state.laboratory.executing(job.id){
        if job.kind=="solver"{state.laboratory.start_solver(job.id)?;}else{state.laboratory.start_generated(job.id)?;}
    }
    Ok(Json(state.laboratory.get(job.id)?))
}
#[derive(Deserialize)]struct ProjectQuery{project_id:Option<Uuid>}
async fn list(State(state):State<Arc<AppState>>,Query(query):Query<ProjectQuery>)->Result<Json<Value>,Error>{
    Ok(Json(json!({"jobs":state.laboratory.list(query.project_id)?.iter().map(LabJob::summary).collect::<Vec<_>>()})))
}
async fn capabilities()->Json<Value>{Json(super::catalog::capabilities())}
async fn get_job(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<LabJob>,Error>{Ok(Json(state.laboratory.get(id)?))}
async fn chat(State(state):State<Arc<AppState>>,Path(project):Path<Uuid>,Json(request):Json<crate::agent::laboratory::SessionRequest>)->Result<Json<LabJob>,Error>{
    let job=state.agent.start_lab_session(state.clone(),project,request,None)?;Ok(Json(job))
}
#[derive(Deserialize)]struct Control{action:String,#[serde(flatten)]options:serde_json::Map<String,Value>}
fn resumed_deadline(job:&LabJob,options:&serde_json::Map<String,Value>,now:chrono::DateTime<chrono::Utc>)->anyhow::Result<Option<chrono::DateTime<chrono::Utc>>>{
    match options.get("time_limit_seconds"){
        Some(Value::Null)=>Ok(None),
        Some(value)=>{let seconds=value.as_u64().context("Resume budget must be seconds or null for Off")?;anyhow::ensure!((10..=604800).contains(&seconds),"Resume budget must be 10 seconds to 7 days");Ok(Some(now+chrono::Duration::seconds(seconds as i64)))},
        None=>{anyhow::ensure!(job.deadline_at.is_none_or(|at|at>now),"This job's original deadline expired. Choose an explicit new time limit or Off to resume; the budget is never silently reset.");Ok(job.deadline_at)},
    }
}
async fn control(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<Control>)->Result<Json<LabJob>,Error>{
    let job=state.laboratory.get(id)?;
    match request.action.as_str(){
        "cancel"=>{state.laboratory.stop(id,"cancelled")?;}
        "pause"=>{state.laboratory.stop(id,"paused")?;}
        "resume"=>{
            if !matches!(job.state.as_str(),"paused"|"failed"|"timed_out"){return Err(anyhow::anyhow!("Only paused, failed or timed-out jobs can resume").into());}
            if job.kind=="published_simulation"{return Err(anyhow::anyhow!("Resume the parent agent to continue this retained-data publication; it is not a solver restart").into());}
            if job.kind=="specialist"{
                if request.options.contains_key("time_limit_seconds"){return Err(anyhow::anyhow!("Specialists inherit their parent's exact deadline. Extend and resume the parent session instead.").into());}
                state.agent.resume_lab_session(state.clone(),id)?;
                return Ok(Json(state.laboratory.get(id)?));
            }
            // A render retry creates an immutable new attempt. Do not rewrite
            // the original attempt's budget before admission succeeds.
            if matches!(job.kind.as_str(),"illustration"|"observation"){
                let deadline=render_restart_deadline(&state.laboratory,&job,&request.options,chrono::Utc::now())?;
                return Ok(Json(if job.kind=="illustration"{state.laboratory.restart_illustration(id,deadline)?}else{state.laboratory.restart_observation(id,deadline)?}));
            }
            if job.kind=="solver"{
                let (deadline,independent)=solver_resume_request(&state.laboratory,&job,&request.options,chrono::Utc::now())?;
                state.laboratory.resume_solver_with_mode(id,deadline,independent)?;
                return Ok(Json(state.laboratory.get(id)?));
            }
            if matches!(job.kind.as_str(),"sweep"|"ml_study"|"ml_query"){
                let deadline=render_restart_deadline(&state.laboratory,&job,&request.options,chrono::Utc::now())?;
                if job.kind=="sweep"{state.laboratory.resume_sweep(id,deadline)?;}else{state.laboratory.resume_ml_study(id,deadline)?;}
                return Ok(Json(state.laboratory.get(id)?));
            }
            let deadline=resumed_deadline(&job,&request.options,chrono::Utc::now())?;
            if request.options.contains_key("time_limit_seconds"){
                state.laboratory.update(id,|j|{j.deadline_at=deadline;j.event("budget_changed","An explicit new work budget was selected for continuation.",json!({"deadline_at":deadline,"time_limit_seconds":request.options["time_limit_seconds"]}));})?;
            }
            if job.kind=="session"{state.agent.resume_lab_session(state.clone(),id)?;}
            else if job.kind=="export"{
                return Ok(Json(state.laboratory.resume_export(id,deadline)?));
            }else if job.kind=="generated"{
                return Ok(Json(state.laboratory.restart_generated(id,deadline)?));
            }else{return Err(anyhow::anyhow!("This job type cannot resume").into());}
        }
        _=>return Err(anyhow::anyhow!("Choose pause, resume or cancel").into()),
    }
    Ok(Json(state.laboratory.get(id)?))
}
#[derive(Default,Deserialize)]
#[serde(deny_unknown_fields)]
struct SeenRequest { completed_at:Option<chrono::DateTime<chrono::Utc>> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeenJob { id:Uuid,completed_at:chrono::DateTime<chrono::Utc> }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeenProjectRequest { jobs:Vec<SeenJob> }
fn acknowledge_seen(service:&super::LaboratoryService,id:Uuid,expected:Option<chrono::DateTime<chrono::Utc>>)->anyhow::Result<LabJob>{
    let current=service.get(id)?;
    let observed=expected.or(current.completed_at);
    if current.state!="completed"||observed.is_none()||current.completed_at!=observed||current.seen_at.is_some_and(|seen|Some(seen)>=observed){return Ok(current);}
    service.update(id,|job|{
        // A completion after the user's click remains unread, including a
        // restart that finished while this acknowledgement was in flight.
        if job.state=="completed"&&job.completed_at==observed{
            job.seen_at=Some(job.seen_at.map_or(observed.unwrap(),|seen|seen.max(observed.unwrap())));
        }
    })
}
async fn seen(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<SeenRequest>)->Result<Json<LabJob>,Error>{
    Ok(Json(acknowledge_seen(&state.laboratory,id,request.completed_at)?))
}
fn acknowledge_project(service:&super::LaboratoryService,project:Uuid,request:&SeenProjectRequest)->anyhow::Result<Vec<Value>>{
    anyhow::ensure!(request.jobs.len()<=1000,"Acknowledge at most 1,000 observed jobs at once");
    let mut unique=std::collections::HashSet::new();
    for item in &request.jobs{
        anyhow::ensure!(unique.insert(item.id)&&service.get(item.id)?.project_id==project,"Each observed job must be unique and belong to this project");
    }
    request.jobs.iter().map(|item|acknowledge_seen(service,item.id,Some(item.completed_at)).map(|job|job.summary())).collect()
}
async fn seen_project(State(state):State<Arc<AppState>>,Path(project):Path<Uuid>,Json(request):Json<SeenProjectRequest>)->Result<Json<Value>,Error>{
    Ok(Json(json!({"jobs":acknowledge_project(&state.laboratory,project,&request)?})))
}
fn render_restart_deadline(service:&super::LaboratoryService,job:&LabJob,options:&serde_json::Map<String,Value>,now:chrono::DateTime<chrono::Utc>)->anyhow::Result<Option<chrono::DateTime<chrono::Utc>>>{
    if let Some(parent)=job.parent_id.map(|id|service.get(id)).transpose()?{
        anyhow::ensure!(parent.project_id==job.project_id,"Render parent belongs to another project");
        if parent.active()&&parent.deadline_at.is_none_or(|at|at>now){
            anyhow::ensure!(!options.contains_key("time_limit_seconds"),"An active session's render inherits its exact deadline. Change the parent session's budget instead.");
            return Ok(parent.deadline_at);
        }
    }
    resumed_deadline(job,options,now)
}
fn solver_resume_request(service:&super::LaboratoryService,job:&LabJob,options:&serde_json::Map<String,Value>,now:chrono::DateTime<chrono::Utc>)->anyhow::Result<(Option<chrono::DateTime<chrono::Utc>>,bool)>{
    let mut independent=false;
    if let Some(parent)=job.parent_id.map(|id|service.get(id)).transpose()?{
        anyhow::ensure!(parent.project_id==job.project_id,"Solver parent belongs to another project");
        if parent.active()&&parent.deadline_at.is_none_or(|at|at>now){
            anyhow::ensure!(!options.contains_key("time_limit_seconds"),"A solver under an active parent inherits its exact deadline. Change the parent budget instead.");
            return Ok((parent.deadline_at,false));
        }
        anyhow::ensure!(options.contains_key("time_limit_seconds"),"Resume the parent, or choose an explicit time limit or Off to continue this solver independently after its former execution tree has stopped");
        independent=true;
    }
    Ok((resumed_deadline(job,options,now)?,independent))
}
/// Classify the safely resolved file, including Windows case and path aliases,
/// before exposing either HTTP bytes or a native model image.
pub(crate) fn published_artifact_path(service:&super::LaboratoryService,id:Uuid,relative:&str)->anyhow::Result<std::path::PathBuf>{
    let path=service.path(id,relative)?;
    let root=service.directory(id).canonicalize()?;
    let name=path.strip_prefix(root)?.to_string_lossy().replace('\\',"/").to_ascii_lowercase();
    let job=service.get(id)?;
    if job.kind=="illustration"&&["render.png","scene.blend","scene.glb"].contains(&name.as_str()){
        anyhow::ensure!(job.state=="completed","Illustration files are available after the render completes");
    }
    if job.kind=="observation"&&["first-frame.png","scene.blend"].contains(&name.as_str()){
        anyhow::ensure!(job.state=="completed","The requested view is available after rendering completes");
    }
    if job.kind=="study_plot"&&name=="plot/plot.png"{anyhow::ensure!(job.state=="completed","The scientific plot is available after source validation and rendering complete");}
    super::exports::ensure_artifact_available(&job,&name)?;
    Ok(path)
}
async fn artifact(State(state):State<Arc<AppState>>,Path((id,relative)):Path<(Uuid,String)>)->Result<Response,Error>{
    let job=state.laboratory.get(id)?;
    if job.kind=="generated"{
        let bytes=state.laboratory.read_generated_artifact(id,&relative)?;
        let mime=match std::path::Path::new(&relative).extension().and_then(|value|value.to_str()).unwrap_or(""){"json"=>"application/json","png"=>"image/png","csv"=>"text/csv",_=>"application/octet-stream"};
        return Ok(([(header::CONTENT_TYPE,mime),(header::CACHE_CONTROL,"no-store"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff")],bytes).into_response());
    }
    let path=published_artifact_path(&state.laboratory,id,&relative)?;
    let content_type=match path.extension().and_then(|v|v.to_str()).unwrap_or("").to_ascii_lowercase().as_str(){"json"=>"application/json","png"=>"image/png","mp4"=>"video/mp4","csv"=>"text/csv",_=>"application/octet-stream"};
    let file=tokio::fs::File::open(path).await?;
    let stream=futures_util::stream::try_unfold(file,|mut file|async move{
        use tokio::io::AsyncReadExt;
        let mut bytes=vec![0;64*1024];let count=file.read(&mut bytes).await?;
        if count==0{Ok::<_,std::io::Error>(None)}else{bytes.truncate(count);Ok(Some((bytes,file)))}
    });
    Ok(([(header::CONTENT_TYPE,content_type),(header::CACHE_CONTROL,"no-store"),(header::X_CONTENT_TYPE_OPTIONS,"nosniff")],Body::from_stream(stream)).into_response())
}
pub fn get_presentation(state:&AppState,id:Uuid)->anyhow::Result<Value>{read_presentation(&state.laboratory,id)}
fn read_presentation(service:&super::LaboratoryService,id:Uuid)->anyhow::Result<Value>{
    service.get(id)?;
    match std::fs::read(service.directory(id).join("presentation.json")) {
        Ok(bytes)=>Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>Ok(json!({"revision":0,"settings":{},"applied_operations":[]})),
        Err(error)=>Err(error.into()),
    }
}
#[derive(Debug)]struct PresentationConflict{current:Value}
impl std::fmt::Display for PresentationConflict{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{write!(f,"Presentation changed concurrently")}}
impl std::error::Error for PresentationConflict{}
/// Agent tools patch the current state under the same lock as browser revision checks.
pub fn put_presentation(state:&AppState,id:Uuid,patch:Value,operation:Option<&str>)->anyhow::Result<Value>{
    patch_presentation(&state.laboratory,id,patch,None,operation)
}
fn patch_presentation(service:&super::LaboratoryService,id:Uuid,patch:Value,base:Option<u64>,operation:Option<&str>)->anyhow::Result<Value>{
    use sha2::{Digest,Sha256};
    anyhow::ensure!(patch.is_object()&&patch.to_string().len()<128000,"Presentation patch must be a bounded object");
    anyhow::ensure!(operation.is_none_or(|op|!op.is_empty()&&op.len()<=128),"Invalid presentation operation ID");
    let digest=format!("{:x}",Sha256::digest(serde_json::to_vec(&patch)?));
    let _guard=service.gate.lock();
    let previous=read_presentation(service,id)?;
    let mut operations=previous["applied_operations"].as_array().cloned().unwrap_or_default();
    if let Some(receipt)=operation.and_then(|op|operations.iter().find(|entry|entry["id"]==op)) {
        anyhow::ensure!(receipt["sha256"]==digest,"Operation ID is bound to a different presentation patch");
        return Ok(previous);
    }
    let revision=previous["revision"].as_u64().unwrap_or(0);
    if base.is_some_and(|expected|expected!=revision){return Err(PresentationConflict{current:previous}.into());}
    let mut settings=previous["settings"].as_object().cloned().unwrap_or_default();
    for (key,value) in patch.as_object().unwrap(){settings.insert(key.clone(),value.clone());}
    anyhow::ensure!(serde_json::to_vec(&settings)?.len()<128000,"Combined presentation is too large");
    if let Some(op)=operation{operations.push(json!({"id":op,"sha256":digest}));}
    if operations.len()>256{operations.drain(..operations.len()-256);}
    let next=json!({"revision":revision+1,"settings":settings,"operation_id":operation,"applied_operations":operations,"updated_at":chrono::Utc::now()});
    write_json(&service.directory(id).join("presentation.json"),&next)?;
    let mut job=service.get(id)?;
    job.event("presentation_changed","Presentation updated using the saved numerical result.",json!({"revision":next["revision"],"operation_id":operation}));
    service.database.put_lab_record(&job)?;
    Ok(next)
}
async fn presentation(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(get_presentation(&state,id)?))}
async fn save_presentation(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(value):Json<Value>)->Result<Json<Value>,Error>{
    let revision=value["base_revision"].as_u64().context("A presentation base_revision is required; reload the viewer")?;
    let operation=value["operation_id"].as_str().context("A presentation operation_id is required")?;
    Ok(Json(patch_presentation(&state.laboratory,id,value["patch"].clone(),Some(revision),Some(operation))?))
}
async fn observe(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(mut value):Json<Value>)->Result<Json<LabJob>,Error>{
    use base64::Engine;
    let job=state.laboratory.get(id)?;
    let data=value["data_url"].as_str().or(value["dataUrl"].as_str()).context("A captured PNG is required")?;
    let encoded=data.strip_prefix("data:image/png;base64,").context("Only PNG observations are accepted")?;
    if encoded.len()>12*1024*1024{return Err(anyhow::anyhow!("Observation exceeds 9 MiB").into());}
    let bytes=base64::engine::general_purpose::STANDARD.decode(encoded)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n"){return Err(anyhow::anyhow!("Invalid PNG").into());}
    let observation_id=Uuid::new_v4();let name=format!("observation-{observation_id}.png");
    std::fs::write(state.laboratory.directory(id).join(&name),bytes)?;
    value.as_object_mut().context("Observation must be an object")?.remove("data_url");value.as_object_mut().unwrap().remove("dataUrl");
    write_json(&state.laboratory.directory(id).join(format!("observation-{observation_id}.json")),&value)?;
    let request=crate::agent::laboratory::SessionRequest{
        request_id:Some(observation_id),content:format!("Inspect the captured view of job {id}, image {name}. Cross-check any inference against this run's actual measurements. Capture metadata: {value}"),
        provider:serde_json::from_value(value["provider"].clone()).ok(),model:value["model"].as_str().map(str::to_owned),reasoning_effort:value["reasoning_effort"].as_str().map(str::to_owned),
        time_limit_seconds:match value.get("time_limit_seconds"){Some(Value::Null)=>None,Some(v)=>Some(v.as_u64().context("Invalid observation time limit")?),None=>Some(900)},context_job_id:Some(id),..Default::default()
    };
    Ok(Json(state.agent.start_lab_session(state.clone(),job.project_id,request,None)?))
}

/// A dedicated endpoint fails closed against an older engine. Never fall back
/// to ordinary chat if this evidence-only operation is unavailable.
async fn review_result(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<crate::agent::laboratory::SessionRequest>)->Result<Json<LabJob>,Error>{
    let review=request.result_review.as_ref().context("Choose a saved-result review action")?;
    if review.source_job_id!=id{return Err(anyhow::anyhow!("The review path and selected source do not match").into());}
    let request_id=request.request_id.context("A saved-result review needs an immutable request ID")?;
    let source=state.laboratory.get(id)?;
    let existing=state.laboratory.list(Some(source.project_id))?;
    if existing.iter().any(|job|job.id!=request_id&&job.kind=="session"&&job.active()){return Err(anyhow::anyhow!("Finish or stop this conversation's active session before starting a result review").into());}
    Ok(Json(state.agent.start_lab_session(state.clone(),source.project_id,request,None)?))
}
pub struct Error(anyhow::Error);
impl<E:Into<anyhow::Error>> From<E> for Error{fn from(error:E)->Self{Self(error.into())}}
impl IntoResponse for Error{fn into_response(self)->Response{
    if let Some(conflict)=self.0.downcast_ref::<PresentationConflict>(){return (StatusCode::CONFLICT,Json(json!({"error":{"message":conflict.to_string(),"current":conflict.current}}))).into_response();}
    let missing=self.0.chain().any(|error|error.downcast_ref::<std::io::Error>().is_some_and(|io|io.kind()==std::io::ErrorKind::NotFound));
    (if missing{StatusCode::NOT_FOUND}else{StatusCode::UNPROCESSABLE_ENTITY},Json(json!({"error":{"message":format!("{:#}",self.0)}}))).into_response()
}}

#[cfg(test)]mod tests{
    #[test]fn standalone_jobs_require_explicit_budget_and_cannot_forge_agent_lineage(){
        use super::*;
        let mut value=json!({"request_id":Uuid::new_v4(),"kind":"solver","title":"Direct numerical calculation","input":{"engine":"openmm_argon","parameters":{}},"time_limit_seconds":null});
        let request:StandaloneJobRequest=serde_json::from_value(value.clone()).unwrap();
        assert_eq!(standalone_deadline(&request,chrono::Utc::now()).unwrap(),None);
        value.as_object_mut().unwrap().remove("time_limit_seconds");assert!(serde_json::from_value::<StandaloneJobRequest>(value.clone()).is_err());
        value["time_limit_seconds"]=json!(9);assert!(standalone_deadline(&serde_json::from_value(value.clone()).unwrap(),chrono::Utc::now()).is_err());
        value["time_limit_seconds"]=json!(60);value["input"]["source_session_id"]=json!(Uuid::new_v4());assert!(standalone_deadline(&serde_json::from_value(value.clone()).unwrap(),chrono::Utc::now()).is_err());
        value["input"].as_object_mut().unwrap().remove("source_session_id");value["kind"]=json!("session");assert!(standalone_deadline(&serde_json::from_value(value).unwrap(),chrono::Utc::now()).is_err());
    }
    use super::*;
    fn fixture()->(tempfile::TempDir,super::super::LaboratoryService,Uuid){
        let dir=tempfile::tempdir().unwrap();let config=crate::config::AppConfig{data_directory:dir.path().into(),..Default::default()};
        let db=crate::persistence::Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Presentation".into()),question:"test".into()});db.put_project(&project).unwrap();
        let service=super::super::LaboratoryService::new(db,config).unwrap();let id=Uuid::new_v4();service.create(id,project.id,None,"solver","test",json!({}),None).unwrap();(dir,service,id)
    }
    #[test]fn viewed_completion_stays_read_but_a_new_completion_does_not(){
        let (_dir,service,id)=fixture();
        let done=service.update(id,|job|{job.state="completed".into();job.event("completed","First result",json!({}));}).unwrap();
        let first=done.completed_at.unwrap();
        let seen=acknowledge_seen(&service,id,Some(first)).unwrap();assert_eq!(seen.seen_at,Some(first));
        service.update(id,|job|job.event("presentation_updated","Camera moved",json!({}))).unwrap();
        assert_eq!(service.get(id).unwrap().completed_at,Some(first));
        let later=first+chrono::Duration::seconds(10);
        service.update(id,|job|{job.events.push(super::super::LabEvent{sequence:job.events.len()+1,at:later,kind:"completed".into(),message:"New result".into(),data:json!({})});}).unwrap();
        let stale=acknowledge_seen(&service,id,Some(first)).unwrap();assert_eq!(stale.seen_at,Some(first));assert_eq!(stale.completed_at,Some(later));
        let viewed=acknowledge_seen(&service,id,Some(later)).unwrap();assert_eq!(viewed.seen_at,Some(later));
    }
    #[test]fn project_read_validates_all_members_before_changing_any_job(){
        let (_dir,service,id)=fixture();let job=service.update(id,|job|{job.state="completed".into();job.event("completed","Saved",json!({}));}).unwrap();
        let request=SeenProjectRequest{jobs:vec![SeenJob{id,completed_at:job.completed_at.unwrap()}]};
        assert!(acknowledge_project(&service,Uuid::new_v4(),&request).is_err());assert!(service.get(id).unwrap().seen_at.is_none());
        let result=acknowledge_project(&service,job.project_id,&request).unwrap();assert_eq!(result.len(),1);assert!(service.get(id).unwrap().seen_at.is_some());
    }
    #[test]fn concurrent_camera_save_rebases_without_losing_agent_color(){
        let (_dir,service,id)=fixture();
        patch_presentation(&service,id,json!({"color":"blue"}),None,Some("agent-color")).unwrap();
        let error=patch_presentation(&service,id,json!({"camera":{"position":[1,2,3]}}),Some(0),Some("camera")).unwrap_err();
        assert_eq!(error.downcast_ref::<PresentationConflict>().unwrap().current["settings"]["color"],"blue");
        let saved=patch_presentation(&service,id,json!({"camera":{"position":[1,2,3]}}),Some(1),Some("camera")).unwrap();
        assert_eq!(saved["settings"]["color"],"blue");assert_eq!(saved["settings"]["camera"]["position"],json!([1,2,3]));
    }
    #[test]fn uncertain_retry_after_other_edit_does_not_reapply_or_duplicate(){
        let (_dir,service,id)=fixture();
        patch_presentation(&service,id,json!({"color":"blue"}),Some(0),Some("first")).unwrap();
        patch_presentation(&service,id,json!({"color":"red"}),Some(1),Some("second")).unwrap();
        let retry=patch_presentation(&service,id,json!({"color":"blue"}),Some(0),Some("first")).unwrap();
        assert_eq!(retry["revision"],2);assert_eq!(retry["settings"]["color"],"red");
        assert!(patch_presentation(&service,id,json!({"color":"gold"}),None,Some("first")).is_err());
        assert_eq!(service.get(id).unwrap().events.iter().filter(|e|e.kind=="presentation_changed").count(),2);
    }
    #[test]fn parallel_patch_writes_preserve_each_distinct_setting(){
        let (_dir,service,id)=fixture();let barrier=std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads:Vec<_>=(0..8).map(|n|{let service=service.clone();let barrier=barrier.clone();std::thread::spawn(move||{barrier.wait();patch_presentation(&service,id,json!({format!("field{n}"):n}),None,Some(&format!("op{n}"))).unwrap();})}).collect();
        for thread in threads{thread.join().unwrap();}let result=read_presentation(&service,id).unwrap();assert_eq!(result["revision"],8);assert_eq!(result["settings"].as_object().unwrap().len(),8);
    }
    #[test]fn resume_preserves_deadline_unless_user_explicitly_changes_budget(){
        let (_dir,service,id)=fixture();let now=chrono::Utc::now();let mut job=service.get(id).unwrap();job.deadline_at=Some(now+chrono::Duration::seconds(30));
        assert_eq!(resumed_deadline(&job,&Default::default(),now).unwrap(),job.deadline_at);
        assert!(resumed_deadline(&job,&Default::default(),now+chrono::Duration::seconds(31)).is_err());
        let off=json!({"time_limit_seconds":null});assert_eq!(resumed_deadline(&job,off.as_object().unwrap(),now).unwrap(),None);
        let extension=json!({"time_limit_seconds":60});assert_eq!(resumed_deadline(&job,extension.as_object().unwrap(),now).unwrap(),Some(now+chrono::Duration::seconds(60)));
    }
    #[test]fn resolved_render_publication_guards_case_aliases_and_invalid_relative_paths(){
        let (_dir,service,id)=fixture();
        for (kind,names) in [("illustration",vec!["render.png","scene.blend","scene.glb"]),("observation",vec!["first-frame.png","scene.blend"])]{
            service.update(id,|job|{job.kind=kind.into();job.state="running".into();}).unwrap();
            for name in names{
                std::fs::write(service.directory(id).join(name),b"incomplete renderer bytes").unwrap();
                // These are the same file on Windows. Creating both names also
                // exercises the classifier on case-sensitive test hosts.
                std::fs::write(service.directory(id).join(name.to_ascii_uppercase()),b"incomplete renderer bytes").unwrap();
                for alias in [name.to_owned(),name.to_ascii_uppercase()]{
                    assert!(published_artifact_path(&service,id,&alias).unwrap_err().to_string().contains("complet"),"{kind} {alias}");
                }
                for invalid in [format!("./{name}"),format!("../{name}"),format!("folder\\{name}")]{assert!(published_artifact_path(&service,id,&invalid).is_err(),"{invalid}");}
            }
            service.update(id,|job|job.state="completed".into()).unwrap();
            let name=if kind=="illustration"{"RENDER.PNG"}else{"FIRST-FRAME.PNG"};
            assert!(published_artifact_path(&service,id,name).unwrap().is_file());
        }
        // Accepted repeated separators and interior dots still resolve to the
        // canonical file, rather than changing which publication rule applies.
        std::fs::create_dir_all(service.directory(id).join("data")).unwrap();
        std::fs::write(service.directory(id).join("data/values.json"),b"{}").unwrap();
        assert_eq!(published_artifact_path(&service,id,"data//./values.json").unwrap(),published_artifact_path(&service,id,"data/values.json").unwrap());
    }
    #[test]fn resolved_export_publication_blocks_partial_video_even_after_completion(){
        let (_dir,service,id)=fixture();
        service.update(id,|job|{job.kind="export".into();job.state="running".into();job.result=json!({"filename":"simulation.mp4"});}).unwrap();
        for name in ["SIMULATION.MP4","encoding0001-0005.MP4"]{std::fs::write(service.directory(id).join(name),b"video").unwrap();assert!(published_artifact_path(&service,id,name).is_err());}
        service.update(id,|job|job.state="completed".into()).unwrap();
        assert!(published_artifact_path(&service,id,"SIMULATION.MP4").is_ok());
        assert!(published_artifact_path(&service,id,"encoding0001-0005.MP4").is_err());
    }
    #[test]fn render_retry_budget_uses_live_parent_without_mutating_original_attempt(){
        let (_dir,service,id)=fixture();let now=chrono::Utc::now();
        let original=service.update(id,|job|{job.kind="illustration".into();job.state="timed_out".into();job.deadline_at=Some(now-chrono::Duration::seconds(1));}).unwrap();
        let parent=service.create(Uuid::new_v4(),original.project_id,None,"session","parent",json!({}),Some(now+chrono::Duration::seconds(75))).unwrap();
        let original=service.update(id,|job|job.parent_id=Some(parent.id)).unwrap();let encoded=serde_json::to_value(&original).unwrap();
        assert_eq!(render_restart_deadline(&service,&original,&Default::default(),now).unwrap(),parent.deadline_at);
        assert!(render_restart_deadline(&service,&original,json!({"time_limit_seconds":null}).as_object().unwrap(),now).is_err());
        service.update(parent.id,|job|job.state="completed".into()).unwrap();
        assert!(render_restart_deadline(&service,&original,&Default::default(),now).is_err());
        assert_eq!(render_restart_deadline(&service,&original,json!({"time_limit_seconds":null}).as_object().unwrap(),now).unwrap(),None);
        assert_eq!(serde_json::to_value(service.get(id).unwrap()).unwrap(),encoded);
    }
    #[test]fn solver_resume_api_inherits_parent_or_requires_an_explicit_independent_budget(){
        let (_dir,service,id)=fixture();let now=chrono::Utc::now();let child=service.get(id).unwrap();
        let parent=service.create(Uuid::new_v4(),child.project_id,None,"session","parent",json!({}),Some(now+chrono::Duration::seconds(90))).unwrap();
        let child=service.update(id,|job|{job.parent_id=Some(parent.id);job.state="paused".into();job.deadline_at=Some(now-chrono::Duration::seconds(1));}).unwrap();
        let snapshot=serde_json::to_value(&child).unwrap();let off=json!({"time_limit_seconds":null});
        assert_eq!(solver_resume_request(&service,&child,&Default::default(),now).unwrap(),(parent.deadline_at,false));
        assert!(solver_resume_request(&service,&child,off.as_object().unwrap(),now).is_err());
        service.update(parent.id,|job|job.state="completed".into()).unwrap();
        assert!(solver_resume_request(&service,&child,&Default::default(),now).is_err());
        assert_eq!(solver_resume_request(&service,&child,off.as_object().unwrap(),now).unwrap(),(None,true));
        assert_eq!(solver_resume_request(&service,&child,json!({"time_limit_seconds":60}).as_object().unwrap(),now).unwrap(),(Some(now+chrono::Duration::seconds(60)),true));
        assert!(solver_resume_request(&service,&child,json!({"time_limit_seconds":"Off"}).as_object().unwrap(),now).is_err());
        assert_eq!(serde_json::to_value(service.get(id).unwrap()).unwrap(),snapshot);
    }
}
