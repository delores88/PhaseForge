//! Verification, prior-art records and selective evidence inspection. All writes explicit.
use std::sync::Arc;
use axum::{Router,Json,extract::{Path,Query,State},http::StatusCode,response::{IntoResponse,Response},routing::{get,post}};
use serde::Deserialize;
use serde_json::{json,Value};
use uuid::Uuid;
use crate::{app::AppState,domain::RunStatus};
use super::{comparison,service,types::*};

pub fn routes()->Router<Arc<AppState>>{
    Router::new()
        .route("/api/verification/dossiers",get(list).post(create))
        .route("/api/verification/dossiers/:id",get(detail))
        .route("/api/verification/dossiers/:id/start",post(start))
        .route("/api/verification/dossiers/:id/cancel",post(cancel))
        .route("/api/verification/dossiers/:id/search",post(search))
        .route("/api/verification/dossiers/:id/review",post(review))
        .route("/api/verification/dossiers/:id/agent-review",post(agent_review))
        .route("/api/verification/catalog",get(catalog).post(import))
        .route("/api/verification/probe",post(probe))
        .route("/api/verification/runs/:id/window",get(window))
}
#[derive(Debug)]struct Error(anyhow::Error);
impl From<anyhow::Error> for Error{fn from(e:anyhow::Error)->Self{Self(e)}}
impl IntoResponse for Error{fn into_response(self)->Response{(StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"error":{"message":format!("{:#}",self.0),"code":"verification_request_failed"}}))).into_response()}}
fn missing(s:&str)->Error{Error(anyhow::anyhow!("{s}"))}
#[derive(Deserialize)]struct Scope{project_id:Option<Uuid>,study_id:Option<Uuid>}
async fn list(State(state):State<Arc<AppState>>,Query(q):Query<Scope>)->Result<Json<Value>,Error>{
    let rows=state.assurance.list(q.project_id,q.study_id)?;
    Ok(Json(json!({"dossiers":rows.iter().map(|d|json!({"id":d.id,"project_id":d.project_id,"study_id":d.protocol.study_id,"trial_id":d.protocol.trial_id,
        "question":d.protocol.question,"state":d.state,"stage":d.stage,"completed_tasks":d.completed_tasks,"total_tasks":d.total_tasks,"created_at":d.created_at})).collect::<Vec<_>>()})))
}
async fn create(State(state):State<Arc<AppState>>,Json(p):Json<VerificationRequest>)->Result<Json<Value>,Error>{Ok(Json(json!({"dossier":state.assurance.create(p)?})))}
async fn detail(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{
    let d=state.assurance.get(id)?;let a=state.assurance.assessment(&d)?;
    // Polling returns metadata/results, not a repeated multi-megabyte source trajectory.
    let mut visible=d;visible.source_run.result=None;for c in &mut visible.controls{c.run.result=None;}
    Ok(Json(json!({"dossier":visible,"assessment":a,"snapshot_notice":"Exact source/control outputs remain frozen in storage and the research bundle; use recorded-window inspection for bounded samples."})))
}
async fn start(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(json!({"dossier":state.assurance.start(id)?})))}
async fn cancel(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{Ok(Json(json!({"dossier":state.assurance.cancel(id)?})))}
async fn probe()->Json<Value>{match service::probe_python().await{
    Ok((path,prefix))=>Json(json!({"available":true,"interpreter":path.to_string_lossy(),"arguments":prefix,"worker_source_hash":comparison::text_hash(service::WORKER),"capability":"Independent ODE verification only; no arbitrary execution"})),
    Err(e)=>Json(json!({"available":false,"error":format!("{e:#}"),"capability":"Python 3.10+ required"})),
}}
async fn catalog(State(state):State<Arc<AppState>>,Query(q):Query<Scope>)->Result<Json<Value>,Error>{
    let mut rows=state.database.list_catalog()?;rows.retain(|r|q.project_id.map(|id|id==r.project_id).unwrap_or(true));
    Ok(Json(json!({"references":rows,"boundary":"Numeric references are immutable local snapshots. Imported external measurements are author supplied, not source-verified."})))
}
async fn import(State(state):State<Arc<AppState>>,Json(p):Json<CatalogImport>)->Result<Json<Value>,Error>{Ok(Json(json!({"reference":state.assurance.import_reference(p)?})))}
#[derive(Deserialize)]struct SearchInput{query:String,#[serde(default)]allow_public_query:bool}
async fn search(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(q):Json<SearchInput>)->Result<Json<Value>,Error>{
    let d=state.assurance.get(id)?;
    if matches!(d.state.as_str(),"running"|"stopping"){return Err(missing("wait for a stable verification snapshot before literature review"));}
    if !q.allow_public_query{return Err(missing("explicit consent is required; only this query text is sent to public Crossref"));}
    if !(3..=300).contains(&q.query.trim().len()){return Err(missing("literature query must contain 3-300 bytes"));}
    if state.database.list_literature_searches(id)?.len()>=20{return Err(missing("20 recorded searches reached; create a follow-up dossier for further work"));}
    let mut record=SearchRecord{id:Uuid::new_v4(),dossier_id:id,query:q.query.trim().to_owned(),provider:"Crossref".into(),status:"running".into(),error:None,references:vec![],retrieved_at:chrono::Utc::now(),
        scope:"First 12 Crossref bibliographic matches only; no full-text review, exhaustive search, or novelty determination.".into()};
    state.database.put_literature_search(&record)?;
    match crate::discovery::notebook::literature(&record.query).await{
        Ok(items)=>{record.references=items;record.status="completed".into();},
        Err(e)=>{record.status="failed".into();record.error=Some(format!("{e:#}"));},
    }
    record.retrieved_at=chrono::Utc::now();state.database.put_literature_search(&record)?;
    Ok(Json(json!({"search":record})))
}
async fn review(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(r):Json<ReviewRequest>)->Result<Json<Value>,Error>{Ok(Json(json!({"review":state.assurance.save_review(id,r)?})))}
async fn agent_review(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{
    let d=state.assurance.get(id)?;
    if matches!(d.state.as_str(),"running"|"stopping"){return Err(missing("stop or finish verification before an evidence review"));}
    let assessment=state.assurance.assessment(&d)?;
    let signals=d.source_run.result.as_ref().map(crate::discovery::signals::inspect);
    let no_result=Value::Null;let result=d.result.as_ref().unwrap_or(&no_result);
    let methods=["same_method","different_method"].into_iter().map(|name|json!({
        "name":name,"method":result[name]["method"],"status":result[name]["status"],
        "reached_end_time":result[name]["reached_end_time"],"comparisons":result[name]["comparisons"],
    })).collect::<Vec<_>>();
    let controls=result["controls"].as_array().into_iter().flatten().take(4).map(|v|json!({"spec":v["spec"],"observed":v["observed"],"status":v["status"]})).collect::<Vec<_>>();
    let holdouts=result["holdouts"].as_array().into_iter().flatten().take(16).map(|v|json!({"index":v["index"],"values":v["values"],"status":v["status"],"comparisons":v["comparisons"]})).collect::<Vec<_>>();
    let numerical=json!({"methods":methods,"controls":controls,"holdouts":holdouts,"state":result["state"],"error":result["error"],"partial":result["partial"],"checkpoint_notice":result["checkpoint_notice"],"rhs_evaluations":result["rhs_evaluations"],"wall_seconds":result["wall_seconds"]});
    let compact=json!({"dossier_id":d.id,"evidence_hash":assessment["evidence_hash"],"independent_numerical_evidence":numerical,"frozen_refinement":d.frozen_refinement,"recorded_signal_diagnostics":signals,"protocol":d.protocol,"source_run_hash":d.source_run_hash,"state":d.state,"gaps":assessment["gaps"],"catalog":assessment["catalog"],
        "within_study":d.within_study,"independent_agreement":assessment["independent_agreement"],"robustness":assessment["robustness"],"controls_passed":assessment["controls_passed"],
        "searches":assessment["searches"],"reviews":assessment["reviews"]});
    let response=state.agent.review_verification(d.project_id,d.source_run.id,Uuid::new_v4(),compact).await?;
    Ok(Json(json!({"response":response,"notice":"Metered advisory stored in project chat; no new experiment approved or scientific claim certified."})))
}
#[derive(Deserialize)]struct WindowQuery{
    series:Option<String>,start:Option<f64>,end:Option<f64>,#[serde(default="point_limit")]max_points:usize,
}
fn point_limit()->usize{512}
async fn window(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Query(q):Query<WindowQuery>)->Result<Json<Value>,Error>{
    if !(2..=2048).contains(&q.max_points)||q.start.map(|x|!x.is_finite()).unwrap_or(false)||q.end.map(|x|!x.is_finite()).unwrap_or(false){return Err(missing("finite window limits and 2-2048 points required"));}
    if q.start.zip(q.end).map(|(a,b)|a>b).unwrap_or(false){return Err(missing("window start must not exceed end"));}
    let run=state.database.get_run(id)?.ok_or_else(||missing("run not found"))?;
    if run.status!=RunStatus::Completed{return Err(missing("select completed evidence, not partial live output"));}
    let result=run.result.as_ref().ok_or_else(||missing("run has no recorded evidence"))?;
    let mut out=Vec::new();
    for s in result["series"].as_array().into_iter().flatten().take(32){
        let name=s["name"].as_str().unwrap_or("");if q.series.as_ref().map(|v|v!=name).unwrap_or(false){continue;}
        let filtered=s["points"].as_array().into_iter().flatten().filter(|p|p[0].as_f64().map(|t|q.start.map(|a|t>=a).unwrap_or(true)&&q.end.map(|b|t<=b).unwrap_or(true)).unwrap_or(false)).collect::<Vec<_>>();
        let n=filtered.len();let indices=if n<=q.max_points{(0..n).collect::<Vec<_>>()}else{(0..q.max_points).map(|i|i*(n-1)/(q.max_points-1)).collect::<Vec<_>>()};
        let points=indices.into_iter().map(|i|filtered[i].clone()).collect::<Vec<_>>();
        out.push(json!({"name":name,"unit":s["unit"],"matching_recorded_points":n,"returned_points":points.len(),"sampled":n>q.max_points,"points":points}));
    }
    Ok(Json(json!({"run_id":run.id,"manifest_id":run.manifest_id,"series":out,"boundary":"Recorded samples only; no invented frames or interpolated scientific measurements. Display subsampling can miss extrema; full evidence remains in the run/export."})))
}
