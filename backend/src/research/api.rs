use std::sync::Arc;use axum::{Router,Json,extract::{State,Path},routing::{get,post},http::StatusCode,response::{Response,IntoResponse}};
use anyhow::Context;use serde::Deserialize;use serde_json::{Value,json};use uuid::Uuid;use crate::{app::AppState,domain::RunStatus};use super::*;
static SEARCH_GATE:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(1);
pub fn routes()->Router<Arc<AppState>>{Router::new().route("/api/projects/:id/research",get(workspace)).route("/api/projects/:id/research/search",post(search)).route("/api/projects/:id/research/tasks",post(task)).route("/api/projects/:id/research/data",post(data)).route("/api/manifests/:id/compute-advice",get(advice))}
struct Error(anyhow::Error);impl From<anyhow::Error> for Error{fn from(e:anyhow::Error)->Self{Self(e)}}impl IntoResponse for Error{fn into_response(self)->Response{(StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"error":{"message":format!("{:#}",self.0)}}))).into_response()}}
async fn workspace(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{s.database.get_project(id)?.context("research world missing")?;Ok(Json(json!({"plans":s.database.research_plans(id)?,"searches":s.database.research_searches(id)?,"tasks":s.database.research_tasks(id)?,"data":s.database.research_data(id)?}))) }
#[derive(Deserialize)]struct Query{query:String,#[serde(default)]consent:bool}
async fn search(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(q):Json<Query>)->Result<Json<Value>,Error>{
    s.database.get_project(id)?.context("research world missing")?;
    if !q.consent||q.query.trim().len()<3||q.query.len()>1000{return Err(Error(anyhow::anyhow!("explicit public-query consent and a 3–1000 character query required")));}
    let _permit=SEARCH_GATE.try_acquire().map_err(|_|Error(anyhow::anyhow!("one public research search is already running")))?;
    let mut record=Search{id:Uuid::new_v4(),project_id:id,query:q.query.trim().into(),created_at:Utc::now(),status:"requesting".into(),error:None,response_sha256:None,total_hits:None,results:vec![]};s.database.put_research_search(&record)?;
    match retrieve(&record.query).await{Ok((rows,hash,hits))=>{record.status="completed".into();record.results=rows;record.response_sha256=Some(hash);record.total_hits=hits;},Err(e)=>{record.status="failed".into();record.error=Some(e.to_string());}}
    s.database.put_research_search(&record)?;Ok(Json(json!({"search":record})))
}
#[derive(Deserialize)]struct Update{plan_id:Uuid,task_id:String,status:String,notes:String,#[serde(default)]evidence_run_ids:Vec<Uuid>}
async fn task(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(q):Json<Update>)->Result<Json<Value>,Error>{
    s.database.get_project(id)?.context("research world missing")?;
    let plan=s.database.research_plans(id)?.into_iter().find(|p|p.id==q.plan_id).context("plan not found in this world")?;
    if !plan.plan.tasks.iter().any(|t|t.id==q.task_id)||!["planned","in_progress","blocked","completed"].contains(&q.status.as_str())||q.notes.len()>20000||q.evidence_run_ids.len()>50{return Err(Error(anyhow::anyhow!("invalid task update")));}
    for rid in &q.evidence_run_ids{let r=s.database.get_run(*rid)?.context("evidence run missing")?;if r.project_id!=id||r.status!=RunStatus::Completed{return Err(Error(anyhow::anyhow!("evidence runs must be completed in this world")));}}
    let r=TaskRecord{id:Uuid::new_v4(),project_id:id,plan_id:q.plan_id,task_id:q.task_id,status:q.status,notes:q.notes,evidence_run_ids:q.evidence_run_ids,created_at:Utc::now()};s.database.put_research_task(&r)?;Ok(Json(json!({"task":r})))
}
#[derive(Deserialize)]struct DataInput{name:String,content:String,provenance:String}
async fn data(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(q):Json<DataInput>)->Result<Json<Value>,Error>{s.database.get_project(id)?.context("research world missing")?;if q.name.trim().is_empty()||q.name.len()>160||q.provenance.len()>2000{return Err(Error(anyhow::anyhow!("bounded dataset name/provenance required")));}let mut p=super::data::profile(&q.content)?;let key=Uuid::new_v4();p["id"]=json!(key);p["project_id"]=json!(id);p["name"]=json!(q.name);p["provenance"]=json!(q.provenance);p["created_at"]=json!(Utc::now());s.database.put_research_data(key,&p)?;Ok(Json(p))}
async fn advice(State(s):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{let m=s.database.get_manifest(id)?.context("manifest missing")?;Ok(Json(json!(crate::compute::advisor::advise(&m,s.scheduler.hardware())?)))}
