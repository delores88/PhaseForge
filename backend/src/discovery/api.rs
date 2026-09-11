//! Local HTTP surface for studies, author-owned research records, and export.
use std::sync::Arc;
use axum::{extract::{Path,Query,State},http::{header,StatusCode},response::{IntoResponse,Response},routing::{get,post},Json,Router};
use serde::Deserialize;
use serde_json::{json,Value};
use uuid::Uuid;
use crate::app::AppState;
use super::{types::*,math,notebook,publication,signals,service};

pub fn routes()->Router<Arc<AppState>> {
    Router::new()
        .route("/api/discovery/studies",get(list).post(create))
        .route("/api/discovery/studies/:id",get(detail))
        .route("/api/discovery/studies/:id/start",post(start))
        .route("/api/discovery/studies/:id/pause",post(pause))
        .route("/api/discovery/studies/:id/cancel",post(cancel))
        .route("/api/discovery/studies/:id/review",post(review))
        .route("/api/discovery/studies/:id/next-proposal",post(next_proposal))
        .route("/api/discovery/studies/:id/fork",post(fork))
        .route("/api/discovery/studies/:id/export",get(export))
        .route("/api/research/notebooks/:id",get(load_notebook).put(save_notebook))
        .route("/api/research/literature",post(literature))
        .route("/api/runs/:id/signals",get(inspect_signals))
}
#[derive(Debug)]struct Error(anyhow::Error);
impl From<anyhow::Error> for Error {fn from(e:anyhow::Error)->Self{Self(e)}}
impl IntoResponse for Error {fn into_response(self)->Response {
    let text=format!("{:#}",self.0);let status=if text.contains("changed in another window"){StatusCode::CONFLICT}else{StatusCode::UNPROCESSABLE_ENTITY};
    (status,Json(json!({"error":{"message":text,"code":"research_request_failed"}}))).into_response()
}}
#[derive(Deserialize)]struct ProjectQuery{project_id:Option<Uuid>}
async fn list(State(state):State<Arc<AppState>>,Query(q):Query<ProjectQuery>)->Result<Json<Value>,Error>{
    let rows=state.discovery.list(q.project_id)?;
    Ok(Json(json!({"studies":rows.iter().map(|s|json!({"id":s.id,"project_id":s.project_id,"title":s.recipe.title,"state":s.state,"stage":s.stage,"created_at":s.created_at,"updated_at":s.updated_at,"recipe_hash":s.recipe_hash,"summary":math::summary(s)})).collect::<Vec<_>>()})))
}
async fn create(State(state):State<Arc<AppState>>,Json(recipe):Json<StudyRecipe>)->Result<Json<Value>,Error>{
    let study=state.discovery.create(recipe)?;Ok(Json(json!({"study":study,"summary":math::summary(&study)})))
}
async fn detail(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{
    let study=state.discovery.get(id)?;let book=notebook::load(&state.database,study.project_id)?;
    Ok(Json(json!({"study":study,"summary":math::summary(&study),"report":service::report(&study),"readiness":notebook::checklist(&state.database,&book,&study)})))
}
async fn start(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(json!({"study":state.discovery.start(id)?})))}
async fn pause(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(json!({"study":state.discovery.stop(id,false)?})))}
async fn cancel(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(json!({"study":state.discovery.stop(id,true)?})))}
async fn review(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(selection):Json<StudyModelSelection>)->Result<Json<Value>,Error>{Ok(Json(state.discovery.review(id,selection).await?))}
async fn fork(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{
    let old=state.discovery.get(id)?;let mut recipe=old.recipe.clone();recipe.title=format!("{} / follow-up",recipe.title.chars().take(175).collect::<String>());recipe.seed=recipe.seed.wrapping_add(1);
    let study=state.discovery.create(recipe)?;Ok(Json(json!({"study":study,"source_study_id":id})))
}
async fn export(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Response,Error>{
    let study=state.discovery.get(id)?;
    if matches!(study.state.as_str(),"running"|"pausing"){return Err(Error(anyhow::anyhow!("pause or finish the study before exporting a consistent evidence snapshot")));}
    let db=state.database.clone();
    let bytes=tokio::task::spawn_blocking(move||publication::build(&db,&study)).await.map_err(|e|Error(anyhow::anyhow!(e)))??;
    Ok(([(header::CONTENT_TYPE,"application/zip".to_owned()),(header::CONTENT_DISPOSITION,format!("attachment; filename=\"PhaseForge-study-{id}.zip\""))],bytes).into_response())
}
async fn load_notebook(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(json!({"notebook":notebook::load(&state.database,id)?})))}
async fn save_notebook(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(record):Json<Notebook>)->Result<Json<Value>,Error>{Ok(Json(json!({"notebook":notebook::save(&state.database,id,record)?})))}
#[derive(Deserialize)]struct LiteratureQuery{query:String,#[serde(default)]allow_public_query:bool}
async fn literature(Json(q):Json<LiteratureQuery>)->Result<Json<Value>,Error>{
    if !q.allow_public_query{return Err(Error(anyhow::anyhow!("public Crossref query requires explicit consent; the query text leaves this machine")));}
    Ok(Json(json!({"references":notebook::literature(&q.query).await?,"scope":"Bibliographic metadata only. No full-paper review or novelty adjudication was performed."})))
}
async fn inspect_signals(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{
    let run=state.database.get_run(id)?.ok_or_else(||Error(anyhow::anyhow!("run not found")))?;
    let result=run.result.ok_or_else(||Error(anyhow::anyhow!("run has no completed evidence")))?;
    let value=tokio::task::spawn_blocking(move||signals::inspect(&result)).await.map_err(|e|Error(anyhow::anyhow!(e)))?;Ok(Json(value))
}

async fn next_proposal(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(selection):Json<StudyModelSelection>)->Result<Json<Value>,Error>{Ok(Json(state.discovery.propose_next(id,selection).await?))}
