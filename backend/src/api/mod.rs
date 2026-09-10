use std::{sync::Arc, time::Duration};

use anyhow::Context;
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Path, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{
        HeaderValue, Method, StatusCode,
        header::{AUTHORIZATION, CONTENT_TYPE},
    },
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use uuid::Uuid;

use crate::{
    app::AppState,
    domain::{
        CampaignPlanRequest, CreateProjectRequest, ExperimentManifest, ImportManifestRequest,
        ImportStructureRequest, ProviderKind, ResearchProject, RunPriority, RunRequest,
        SaveProviderKeyRequest, SaveProviderModelRequest, SendMessageRequest, UpdateProjectRequest,
        runtime_capabilities,
    },
    sandbox::validate_manifest,
    science::{
        engines::discover_scientific_engines,
        molecular::{import_structure, plan_campaign, plan_qmmm_region},
    },
};

pub fn router(state: Arc<AppState>) -> anyhow::Result<Router> {
    let origins = state
        .config
        .allowed_frontend_origins
        .iter()
        .map(|value| HeaderValue::from_str(value))
        .collect::<Result<Vec<_>, _>>()
        .context("invalid configured frontend origin")?;

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([CONTENT_TYPE, AUTHORIZATION])
        .max_age(Duration::from_secs(3600));

    Ok(Router::new()
        .merge(crate::studio::render::routes())
        .merge(crate::studio::fabrication::routes())
        .merge(crate::research::assets::routes())
        .merge(crate::agent::studio::routes())
        .merge(crate::agent::tasks::routes())
        .merge(crate::experiment::api::routes())
        .merge(crate::research::api::routes())
        .merge(crate::discovery::api::routes())
        .merge(crate::assurance::api::routes())
        .route("/api/health", get(health))
        .route("/api/workflow", get(workflow_status))
        .route("/api/telemetry", get(telemetry_status))
        .route("/api/runs/:id/findings", get(run_findings))
        .route("/api/runs/:id/explain", post(explain_run))
        .route("/api/usage", get(usage_report))
        .route("/api/usage/settings", get(usage_settings).put(save_usage_settings))
        .route("/api/chat/cancel-all", post(cancel_all_agents))
        .route("/api/hardware", get(hardware))
        .route("/api/capabilities", get(capabilities))
        .route("/api/scientific/engines", get(scientific_engines))
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:id",
            get(get_project).patch(update_project).delete(delete_project),
        )
        .route(
            "/api/projects/:id/messages",
            get(list_messages).post(send_message),
        )
        .route("/api/chat/requests/:id/cancel", post(cancel_chat_request))
        .route(
            "/api/projects/:id/manifests",
            get(list_manifests).post(import_manifest),
        )
        .route("/api/manifests/:id", get(get_manifest))
        .route("/api/manifests/:id/run", post(run_manifest))
        .route("/api/runs", get(list_runs).post(submit_run))
        .route("/api/runs/:id", get(get_run))
        .route("/api/runs/:id/cancel", post(cancel_run))
        .route("/api/providers", get(provider_statuses))
        .route(
            "/api/providers/:provider",
            axum::routing::delete(delete_provider),
        )
        .route(
            "/api/providers/:provider/key",
            put(save_provider_key).delete(delete_provider_key),
        )
        .route("/api/providers/:provider/model", put(save_provider_model))
        .route("/api/providers/:provider/models", get(list_provider_models))
        .route("/api/providers/:provider/test", post(test_provider))
        .route("/api/molecules", get(list_molecules).post(create_molecule))
        .route(
            "/api/molecules/:id",
            get(get_molecule).delete(delete_molecule),
        )
        .route(
            "/api/molecules/:id/qmmm-plans",
            get(list_qmmm_plans).post(create_qmmm_plan),
        )
        .route(
            "/api/molecules/:id/campaigns",
            get(list_campaigns).post(create_campaign),
        )
        .route("/api/events/ws", get(events_ws))
        .layer(DefaultBodyLimit::max(16 * 1024 * 1024))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(cors)
        .with_state(state))
}

async fn usage_report(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.agent.usage_report()?))
}
async fn usage_settings(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::to_value(state.agent.usage.settings()?)?))
}
async fn save_usage_settings(State(state): State<Arc<AppState>>, Json(settings): Json<crate::usage::UsageSettings>) -> Result<Json<Value>, ApiError> {
    let updated = state.agent.save_usage_settings(settings).map_err(ApiError::unprocessable_from)?;
    Ok(Json(serde_json::to_value(updated)?))
}
async fn cancel_all_agents(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"cancelled_requests": state.agent.cancel_all()}))
}

async fn health(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "status": "ok",
        "name": "PhaseForge",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": state.started_at.elapsed().as_secs(),
        "local_only": state.config.bind_address.is_loopback(),
    }))
}

async fn hardware(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut value=serde_json::to_value(state.scheduler.hardware().profile()).unwrap_or_else(|_| json!({}));
    value["neural_accelerators"]=state.scheduler.hardware().neural_accelerators().clone();
    Json(value)
}

async fn capabilities() -> Json<Value> {
    Json(json!({"capabilities": runtime_capabilities()}))
}

async fn scientific_engines() -> Result<Json<Value>, ApiError> {
    let engines = tokio::task::spawn_blocking(discover_scientific_engines)
        .await
        .context("scientific-engine discovery task failed")?;
    Ok(Json(json!({"engines": engines})))
}

async fn list_projects(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let mut projects = state.database.list_projects()?;
    projects.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(Json(json!({"projects": projects})))
}

async fn create_project(
    State(state): State<Arc<AppState>>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if request.question.trim().len() < 8 {
        return Err(ApiError::bad_request(
            "the research question must contain at least eight characters",
        ));
    }
    let project = ResearchProject::new(request);
    state.database.put_project(&project)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(project)?)))
}

async fn get_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    state
        .database
        .get_project(id)?
        .map(|project| Json(serde_json::to_value(project).unwrap_or_else(|_| json!({}))))
        .ok_or_else(|| ApiError::not_found("research world not found"))
}

async fn update_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateProjectRequest>,
) -> Result<Json<Value>, ApiError> {
    let mut project = state
        .database
        .get_project(id)?
        .ok_or_else(|| ApiError::not_found("research world not found"))?;
    if let Some(name) = request.name {
        let name = name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("research-world name cannot be empty"));
        }
        project.name = name.chars().take(120).collect();
    }
    if let Some(question) = request.question {
        let question = question.trim();
        if question.len() < 8 {
            return Err(ApiError::bad_request(
                "research question must contain at least eight characters",
            ));
        }
        project.question = question.chars().take(30_000).collect();
    }
    if let Some(status) = request.status {
        project.status = status;
    }
    project.updated_at = chrono::Utc::now();
    state.database.put_project(&project)?;
    Ok(Json(serde_json::to_value(project)?))
}

async fn delete_project(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if state.database.get_project(id)?.is_none() {
        return Err(ApiError::not_found("research world not found"));
    }
    state.assurance.delete_project(&state.discovery,id).map_err(ApiError::unprocessable_from)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct LimitQuery {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    100
}

async fn list_messages(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Value>, ApiError> {
    if state.database.get_project(id)?.is_none() {
        return Err(ApiError::not_found("research world not found"));
    }
    Ok(Json(json!({
        "messages": state.database.list_messages(id, query.limit)?,
    })))
}

async fn send_message(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<SendMessageRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let response = state
        .agent
        .chat(id, request)
        .await
        .map_err(ApiError::unprocessable_from)?;
    Ok((StatusCode::ACCEPTED, Json(serde_json::to_value(response)?)))
}

async fn cancel_chat_request(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Json<Value> {
    Json(json!({
        "request_id": id,
        "cancelled": state.agent.cancel_request(id),
    }))
}

async fn list_manifests(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Value>, ApiError> {
    if state.database.get_project(id)?.is_none() {
        return Err(ApiError::not_found("research world not found"));
    }
    Ok(Json(json!({
        "manifests": state.database.list_manifests(id, query.limit)?,
    })))
}

async fn import_manifest(
    State(state): State<Arc<AppState>>,
    Path(project_id): Path<Uuid>,
    Json(request): Json<ImportManifestRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let mut project = state
        .database
        .get_project(project_id)?
        .ok_or_else(|| ApiError::not_found("research world not found"))?;
    let revision = state.database.next_manifest_revision(project_id)?;
    let manifest = ExperimentManifest::from_draft(
        project_id,
        project.active_manifest_id,
        revision,
        request.manifest,
        "manual import",
    );
    validate_manifest(&manifest).map_err(ApiError::unprocessable_from)?;
    state.database.put_manifest(&manifest)?;
    project.active_manifest_id = Some(manifest.id);
    project.updated_at = chrono::Utc::now();
    state.database.put_project(&project)?;

    let run = if request.auto_run {
        Some(
            state
                .scheduler
                .submit(RunRequest {
                    manifest_id: manifest.id,
                    name: None,
                    priority: RunPriority::Interactive,
                    compute: None,
                })
                .map_err(ApiError::unprocessable_from)?,
        )
    } else {
        None
    };

    Ok((
        StatusCode::CREATED,
        Json(json!({"manifest": manifest, "run": run})),
    ))
}

async fn get_manifest(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    state
        .database
        .get_manifest(id)?
        .map(|manifest| Json(serde_json::to_value(manifest).unwrap_or_else(|_| json!({}))))
        .ok_or_else(|| ApiError::not_found("manifest not found"))
}

async fn run_manifest(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let run = state
        .scheduler
        .submit(RunRequest {
            manifest_id: id,
            name: None,
            priority: RunPriority::Interactive,
            compute: None,
        })
        .map_err(ApiError::unprocessable_from)?;
    Ok((StatusCode::ACCEPTED, Json(serde_json::to_value(run)?)))
}

#[derive(Debug, Deserialize)]
struct RunQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    project_id: Option<Uuid>,
}

async fn list_runs(
    State(state): State<Arc<AppState>>,
    Query(query): Query<RunQuery>,
) -> Json<Value> {
    let mut runs = state.scheduler.list(50_000);
    if let Some(project_id) = query.project_id {
        runs.retain(|run| run.project_id == project_id);
    }
    runs.truncate(query.limit.clamp(1, 50_000));
    Json(json!({"runs": runs}))
}

async fn submit_run(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RunRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let run = state
        .scheduler
        .submit(request)
        .map_err(ApiError::unprocessable_from)?;
    Ok((StatusCode::ACCEPTED, Json(serde_json::to_value(run)?)))
}

async fn get_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    state
        .scheduler
        .get(id)
        .map(|run| Json(serde_json::to_value(run).unwrap_or_else(|_| json!({}))))
        .ok_or_else(|| ApiError::not_found("run not found"))
}

async fn cancel_run(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    let run = state
        .scheduler
        .cancel(id)
        .map_err(|error| ApiError::not_found(error.to_string()))?;
    Ok(Json(serde_json::to_value(run)?))
}

async fn provider_statuses(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({"providers": state.agent.provider_statuses()?})))
}

async fn save_provider_key(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Json(request): Json<SaveProviderKeyRequest>,
) -> Result<Json<Value>, ApiError> {
    let status = state
        .agent
        .save_provider_key(parse_provider(&provider)?, request)
        .map_err(ApiError::unprocessable_from)?;
    Ok(Json(serde_json::to_value(status)?))
}

async fn delete_provider_key(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let status = state.agent.delete_provider_key(parse_provider(&provider)?)?;
    Ok(Json(serde_json::to_value(status)?))
}

async fn save_provider_model(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Json(request): Json<SaveProviderModelRequest>,
) -> Result<Json<Value>, ApiError> {
    let status = state
        .agent
        .save_provider_model(parse_provider(&provider)?, request)
        .map_err(ApiError::unprocessable_from)?;
    Ok(Json(serde_json::to_value(status)?))
}

async fn list_provider_models(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let models = state
        .agent
        .list_provider_models(parse_provider(&provider)?)
        .await
        .map_err(ApiError::bad_gateway_from)?;
    Ok(Json(serde_json::to_value(models)?))
}

async fn delete_provider(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.agent.delete_provider(parse_provider(&provider)?)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_provider(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let message = state
        .agent
        .test_provider(parse_provider(&provider)?)
        .await
        .map_err(ApiError::bad_gateway_from)?;
    Ok(Json(json!({"ok": true, "message": message})))
}

#[derive(Debug, Deserialize)]
struct MoleculeQuery {
    #[serde(default)]
    project_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    limit: usize,
}

async fn list_molecules(
    State(state): State<Arc<AppState>>,
    Query(query): Query<MoleculeQuery>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(json!({
        "structures": state.database.list_molecules(query.project_id, query.limit)?,
    })))
}

async fn create_molecule(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ImportStructureRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if let Some(project_id) = request.project_id {
        if state.database.get_project(project_id)?.is_none() {
            return Err(ApiError::not_found("research world not found"));
        }
    }
    let structure = import_structure(request).map_err(ApiError::unprocessable_from)?;
    state.database.put_molecule(&structure)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(structure)?)))
}

async fn get_molecule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    state
        .database
        .get_molecule(id)?
        .map(|value| Json(serde_json::to_value(value).unwrap_or_else(|_| json!({}))))
        .ok_or_else(|| ApiError::not_found("molecular structure not found"))
}

async fn delete_molecule(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    if state.database.get_molecule(id)?.is_none() {
        return Err(ApiError::not_found("molecular structure not found"));
    }
    state.database.delete_molecule(id)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_qmmm_plans(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if state.database.get_molecule(id)?.is_none() {
        return Err(ApiError::not_found("molecular structure not found"));
    }
    Ok(Json(json!({"plans": state.database.list_qmmm_plans(id)?})))
}

async fn create_qmmm_plan(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<crate::domain::QmmmRegionRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let structure = state
        .database
        .get_molecule(id)?
        .ok_or_else(|| ApiError::not_found("molecular structure not found"))?;
    let plan = plan_qmmm_region(&structure, request).map_err(ApiError::unprocessable_from)?;
    state.database.put_qmmm_plan(&plan)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(plan)?)))
}

async fn list_campaigns(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    if state.database.get_molecule(id)?.is_none() {
        return Err(ApiError::not_found("molecular structure not found"));
    }
    Ok(Json(json!({"campaigns": state.database.list_campaigns(id)?})))
}

async fn create_campaign(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(request): Json<CampaignPlanRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let structure = state
        .database
        .get_molecule(id)?
        .ok_or_else(|| ApiError::not_found("molecular structure not found"))?;
    if let Some(plan_id) = request.qmmm_plan_id {
        let plan = state
            .database
            .get_qmmm_plan(plan_id)?
            .ok_or_else(|| ApiError::not_found("QM/MM plan not found"))?;
        if plan.structure_id != id {
            return Err(ApiError::bad_request(
                "QM/MM plan belongs to another structure",
            ));
        }
    }
    let engines = tokio::task::spawn_blocking(discover_scientific_engines)
        .await
        .context("scientific-engine discovery task failed")?;
    let campaign =
        plan_campaign(&structure, request, &engines).map_err(ApiError::unprocessable_from)?;
    state.database.put_campaign(&campaign)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(campaign)?)))
}

async fn events_ws(
    State(state): State<Arc<AppState>>,
    upgrade: WebSocketUpgrade,
) -> impl IntoResponse {
    let receiver = state.scheduler.subscribe();
    upgrade.on_upgrade(move |socket| stream_events(socket, receiver))
}

async fn stream_events(
    mut socket: WebSocket,
    mut receiver: tokio::sync::broadcast::Receiver<crate::domain::ServerEvent>,
) {
    while let Ok(event) = receiver.recv().await {
        let Ok(text) = serde_json::to_string(&event) else {
            continue;
        };
        if socket.send(Message::Text(text)).await.is_err() {
            break;
        }
    }
}

fn parse_provider(value: &str) -> Result<ProviderKind, ApiError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "openai" | "open_ai" => Ok(ProviderKind::OpenAi),
        "anthropic" | "claude" => Ok(ProviderKind::Anthropic),
        _ => Err(ApiError::bad_request("unknown provider")),
    }
}

struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn unprocessable_from(error: anyhow::Error) -> Self {
        tracing::warn!(error = %error, "request could not be executed");
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "unprocessable_entity",
            message: format!("{error:#}"),
        }
    }

    fn bad_gateway_from(error: anyhow::Error) -> Self {
        tracing::warn!(error = %error, "AI provider request failed");
        Self {
            status: StatusCode::BAD_GATEWAY,
            code: "provider_error",
            message: format!("{error:#}"),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        tracing::error!(error = %error, "API request failed");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            message: "The backend encountered an internal error. See the backend log for details."
                .to_owned(),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self::from(anyhow::Error::new(error))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": {
                    "code": self.code,
                    "message": self.message,
                    "status": self.status.as_u16()
                }
            })),
        )
            .into_response()
    }
}

async fn workflow_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({"sampled_at":chrono::Utc::now(),"requests":state.agent.workflow_requests(),"runs":state.scheduler.workflow_status()}))
}
async fn telemetry_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    Json(state.telemetry.snapshot())
}
async fn run_findings(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>) -> Result<Json<Value>, ApiError> {
    let run = state.database.get_run(id)?.ok_or_else(||ApiError::not_found("Run not found"))?;
    let manifest=state.database.get_manifest(run.manifest_id)?.ok_or_else(||ApiError::not_found("Run manifest is missing"))?;
    let mut report=crate::science::findings::build(&run,&manifest);
    report["ai_analysis"]=state.database.get_run_analysis(id)?.unwrap_or(Value::Null);
    Ok(Json(report))
}
#[derive(Deserialize)]
struct ExplainRequest { request_id: Uuid, #[serde(default)] provider: Option<ProviderKind> }
async fn explain_run(State(state): State<Arc<AppState>>, Path(id): Path<Uuid>, Json(request): Json<ExplainRequest>) -> Result<Json<Value>, ApiError> {
    Ok(Json(state.agent.explain_run(id,request.request_id,request.provider).await.map_err(ApiError::unprocessable_from)?))
}
