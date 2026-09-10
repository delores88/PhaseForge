//! User-driven, provider-free repeat/refine/extend. Receipts make resubmission
//! idempotent; no old results, credentials or research records are overwritten.
use std::sync::Arc;
use anyhow::{bail,Context};
use axum::{Router,Json,extract::{State,Path},routing::post,response::{Response,IntoResponse},http::StatusCode};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use uuid::Uuid;
use crate::{app::AppState,domain::{ExperimentManifest,RunRequest,RunPriority,RunStatus}};
use super::{NextOperation,next_draft};
static ACTION_GATE:parking_lot::Mutex<()>=parking_lot::Mutex::new(());
#[derive(Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextRequest {pub request_id:Uuid,pub source_run_id:Uuid,pub operation:NextOperation,pub run:bool}
struct Error(anyhow::Error);
impl From<anyhow::Error> for Error {fn from(e:anyhow::Error)->Self{Self(e)}}
impl IntoResponse for Error {fn into_response(self)->Response{(StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"error":{"message":format!("{:#}",self.0)}}))).into_response()}}
pub fn routes()->Router<Arc<AppState>>{Router::new().route("/api/projects/:id/experiments/next",post(next))}
async fn next(State(s):State<Arc<AppState>>,Path(project):Path<Uuid>,Json(request):Json<NextRequest>)->Result<Json<Value>,Error> {Ok(Json(perform(&s,project,request)?))}
fn perform(s:&AppState,project_id:Uuid,q:NextRequest)->anyhow::Result<Value>{
    let _lock=ACTION_GATE.lock();
    let hash=format!("{:x}",Sha256::digest(serde_json::to_vec(&json!({"project_id":project_id,"request":q}))?));
    if let Some(receipt)=s.database.experiment_receipt(q.request_id)? {
        if receipt["request_hash"]!=hash||receipt["project_id"]!=project_id.to_string(){bail!("Request ID was already used for a different action");}
        return Ok(receipt);
    }
    let mut project=s.database.get_project(project_id)?.context("Project not found")?;
    let run=s.database.get_run(q.source_run_id)?.context("Source run not found")?;
    if run.project_id!=project_id {bail!("Source run belongs to another project");}
    if !matches!(run.status,RunStatus::Completed|RunStatus::Failed|RunStatus::Cancelled){bail!("Stop the source run before preparing another numerical pass");}
    let base=s.database.get_manifest(run.manifest_id)?.context("Source manifest missing")?;
    if base.project_id!=project_id {bail!("Source manifest belongs to another project");}
    let manifest:ExperimentManifest=if q.operation==NextOperation::Replay {base} else {
        let draft=next_draft(&base,q.operation)?;
        let revision=s.database.next_manifest_revision(project_id)?;
        ExperimentManifest::from_draft(project_id,Some(base.id),revision,draft,format!("User direct {:?} from run {}",q.operation,run.id))
    };
    crate::sandbox::validate_manifest(&manifest)?;
    if q.operation!=NextOperation::Replay {
        s.database.put_manifest(&manifest)?;
        project.active_manifest_id=Some(manifest.id);project.updated_at=chrono::Utc::now();s.database.put_project(&project)?;
    }
    let mut result=json!({"request_id":q.request_id,"project_id":project_id,"request_hash":hash,"source_run_id":q.source_run_id,
        "operation":q.operation,"manifest":manifest,"run":null,"execution_error":null,
        "status":if q.run {"reserved"}else{"prepared"},"created_at":chrono::Utc::now(),
        "note":"User-requested operation; no model tokens. Re-evaluates the whole seeded experiment, not continuation from a selected finalist. All original compute caps and numerical validation remain active."});
    // Reserve BEFORE enqueue. A crash cannot make replay of the same request
    // enqueue twice. A reserved receipt needs queue inspection, not blind retry.
    s.database.put_experiment_receipt(q.request_id,&result)?;
    if q.run {
        match s.scheduler.submit(RunRequest{manifest_id:manifest.id,name:None,priority:RunPriority::Interactive,compute:None}) {
            Ok(next)=>{result["status"]=json!("queued");result["run"]=json!(next);},
            Err(error)=>{result["status"]=json!("prepared_not_queued");result["execution_error"]=json!(format!("{error:#}"));},
        }
        s.database.put_experiment_receipt(q.request_id,&result)?;
    }
    Ok(result)
}
