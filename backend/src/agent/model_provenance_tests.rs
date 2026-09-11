//! Local HTTP fixtures inspect the actual provider payload. No credentials, network
//! services, installed application data or paid model calls are used by these tests.
use super::*;
use axum::{routing::post, http::StatusCode, Json, Router};
use serde_json::Value;
use crate::{discovery::{DiscoveryService,types::{StudyRecipe,StudyModelSelection}},domain::{CreateProjectRequest,RunStatus}};

const CHOSEN:&str="gpt-6-astra";
const OTHER:&str="claude-sonnet-4-6";
struct Fixture {
    agent:AgentService,
    project:Uuid,
    calls:Arc<Mutex<Vec<Value>>>,
    server:tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {fn drop(&mut self){self.server.abort();}}
fn scene()->Value {
    json!({"message":"Conceptual source for the model selection test.","design":{"version":"1","title":"Selection fixture","kind":"scene",
        "scene":{"schema_version":"1.0","title":"Selection fixture","units":"m","provenance":{"kind":"conceptual","description":"Test only"},
            "nodes":[{"id":"sphere","type":"sphere","label":"Test sphere","description":null,"entity_id":null,"position":[0,0,0],"rotation":null,"scale":null,"color":null,"parameters":null}],"bonds":[],"camera":null},
        "fabrication":null,"source_refs":[],"asset_bindings":[]}})
}
fn response_text(body:&Value, malformed:bool)->String {
    let prompt=body["input"].as_str().or_else(||body["messages"][0]["content"].as_str()).unwrap_or_default();
    let schema=body.pointer("/text/format/schema").or_else(||body.pointer("/output_config/format/schema"));
    if malformed {return "{malformed local fixture".into();}
    if prompt.contains("MEASURED COMPUTATIONAL EVIDENCE:") {
        return json!({"summary":"The local control measured final_x=1.","next_step":"Review the recorded control.","continue_research":false}).to_string();
    }
    if let Some(schema)=schema {
        if schema.pointer("/properties/design").is_some(){return scene().to_string();}
        let mut proposal:Value=serde_json::from_str(include_str!("../../tests/fixtures/experiment-response.json")).unwrap();
        if prompt.contains("Review this computational discovery campaign") {
            proposal["action"]=json!("explain");proposal["manifest"]=Value::Null;
        }
        return proposal.to_string();
    }
    "The recorded numerical control does not establish an empirical claim.".into()
}
async fn fixture(repair_first:bool)->Fixture {
    let database=Database::open(Path::new(":memory:")).unwrap();
    let config=crate::config::AppConfig{gpu_enabled:false,..Default::default()};
    let hardware=crate::compute::HardwareManager::discover(&config).await.unwrap();
    let scheduler=Scheduler::new(database.clone(),hardware).unwrap();
    let mut agent=AgentService::new(database.clone(),scheduler).unwrap();
    agent.test_key=Some("local-model-provenance-fixture-never-persisted".into());
    let calls=Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut app=Router::new();
    for (path,provider) in [("/v1/responses","open_ai"),("/v1/messages","anthropic")] {
        let recorded=calls.clone();
        app=app.route(path,post(move |Json(body):Json<Value>| {
            let recorded=recorded.clone();
            async move {
                let index={let mut records=recorded.lock();records.push(json!({"provider":provider,"body":body}));records.len()};
                if body["model"]=="unavailable-selection" {
                    return (StatusCode::NOT_FOUND,Json(json!({"error":{"message":"The explicitly selected model is not available to this account","code":"model_not_found"}})));
                }
                let text=response_text(&body,repair_first&&index==1);
                (StatusCode::OK,Json(if provider=="open_ai" {
                    json!({"id":"local-response","status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":text}]}],"usage":{"input_tokens":17,"output_tokens":21}})
                } else {
                    json!({"id":"local-message","type":"message","stop_reason":"end_turn","content":[{"type":"text","text":text}],"usage":{"input_tokens":17,"output_tokens":21}})
                }))
            }
        }));
    }
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url=format!("http://{}/v1",listener.local_addr().unwrap());
    let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    // Former Settings defaults deliberately disagree with every selected model.
    for (provider,model) in [(ProviderKind::OpenAi,"gpt-4.1"),(ProviderKind::Anthropic,"claude-haiku-4-5")] {
        database.put_provider_status(&ProviderStatus{provider,model:model.into(),configured:true,key_configured:true,model_configured:true,base_url:base_url.clone()}).unwrap();
    }
    let project=ResearchProject::new(CreateProjectRequest{name:Some("Model provenance fixture".into()),question:"Check dx/dt=1 as a local software control".into()});
    database.put_project(&project).unwrap();
    let mut limits=agent.usage.settings().unwrap();limits.max_parallel_calls=2;limits.repair_attempts=1;agent.usage.save_settings(limits).unwrap();
    Fixture{agent,project:project.id,calls,server}
}
fn assert_call(call:&Value,provider:&str,model:&str,effort:Option<&str>) {
    assert_eq!(call["provider"],provider);
    assert_eq!(call["body"]["model"],model,"Saved Settings model must never override the captured choice");
    let actual=if provider=="open_ai" {&call["body"]["reasoning"]["effort"]} else {&call["body"]["output_config"]["effort"]};
    assert_eq!(actual.as_str(),effort,"The transmitted reasoning effort must equal the selected effort");
}
fn request()->SendMessageRequest {
    serde_json::from_value(json!({"content":"Build a bounded numerical control.","provider":"open_ai","model":CHOSEN,"reasoning_effort":"low","study_intent":"experiment","auto_run":false})).unwrap()
}
fn save_manifest(f:&Fixture,discovery:bool)->ExperimentManifest {
    let draft=if discovery {
        serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap()
    } else {
        let proposal:Value=serde_json::from_str(include_str!("../../tests/fixtures/experiment-response.json")).unwrap();
        serde_json::from_value(proposal["manifest"].clone()).unwrap()
    };
    let manifest=ExperimentManifest::from_draft(f.project,None,1,draft,"model provenance test");
    f.agent.database.put_manifest(&manifest).unwrap();manifest
}
async fn completed_run(f:&Fixture,manifest_id:Uuid)->RunRecord {
    let run=f.agent.scheduler.submit(RunRequest{manifest_id,name:None,priority:RunPriority::Background,compute:None}).unwrap();
    tokio::time::timeout(Duration::from_secs(10),async {
        loop {
            let saved=f.agent.scheduler.get(run.id).unwrap();
            if saved.status==RunStatus::Completed{return saved;}
            assert!(matches!(saved.status,RunStatus::Queued|RunStatus::Running),"{:?}",saved.error);
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("Tiny deterministic solver fixture should finish")
}
#[tokio::test]
async fn chat_and_automatic_repair_transmit_selected_model_and_effort() {
    let f=fixture(true).await;
    let response=f.agent.chat(f.project,request()).await.unwrap();
    assert!(response.manifest.is_some());
    let calls=f.calls.lock();assert_eq!(calls.len(),2);
    for call in calls.iter(){assert_call(call,"open_ai",CHOSEN,Some("low"));}
    assert!(calls[1]["body"]["input"].as_str().unwrap().contains("REPAIR ONLY"));
    let usage=f.agent.database.list_usage_records().unwrap();
    assert!(usage.iter().all(|row|row.model==CHOSEN));
    assert!(usage.iter().any(|row|row.purpose=="repair"));
}
#[tokio::test]
async fn unavailable_selected_model_records_failure_without_a_default_retry() {
    let f=fixture(false).await;
    let mut selected=request();selected.model=Some("unavailable-selection".into());selected.reasoning_effort=None;
    let response=f.agent.chat(f.project,selected).await.unwrap();
    assert_eq!(response.assistant_message.kind,MessageKind::Error);
    assert!(response.manifest.is_none());assert!(f.agent.database.list_runs(10).unwrap().is_empty());
    let calls=f.calls.lock();assert_eq!(calls.len(),1,"Unavailable selection must never trigger a substitute model");
    assert_call(&calls[0],"open_ai","unavailable-selection",None);
    let usage=f.agent.database.list_usage_records().unwrap();assert_eq!(usage.len(),1);
    assert_eq!(usage[0].model,"unavailable-selection");assert_eq!(usage[0].status,"provider_error");
}
#[tokio::test]
async fn missing_or_blank_legacy_chat_model_fails_before_messages_usage_or_provider_requests(){
    let f=fixture(false).await;
    for provider in [None,Some(ProviderKind::OpenAi),Some(ProviderKind::Anthropic)]{
        for model in [None,Some(String::new()),Some(" \t ".into())]{
            let mut selected=request();selected.provider=provider;selected.model=model;
            let error=f.agent.chat(f.project,selected).await.unwrap_err();
            assert!(error.to_string().contains("Choose a model in this conversation's chat"));
        }
    }
    assert!(f.calls.lock().is_empty());assert!(f.agent.database.list_usage_records().unwrap().is_empty());
    assert!(f.agent.database.list_messages(f.project,20).unwrap().is_empty());assert!(f.agent.database.list_runs(20).unwrap().is_empty());
    assert_eq!(f.agent.provider_status(ProviderKind::OpenAi).unwrap().model,"gpt-4.1");assert_eq!(f.agent.provider_status(ProviderKind::Anthropic).unwrap().model,"claude-haiku-4-5");
}
#[tokio::test]
async fn explain_cache_and_verification_review_follow_model_provider_and_effort() {
    let f=fixture(false).await;
    let manifest=save_manifest(&f,false);let run=completed_run(&f,manifest.id).await;
    let first=f.agent.explain_run(run.id,Uuid::new_v4(),Some(ProviderKind::OpenAi),Some(CHOSEN.into()),Some("low".into())).await.unwrap();
    assert_eq!(first["model"],CHOSEN);assert_eq!(first["reasoning_effort"],"low");
    f.agent.explain_run(run.id,Uuid::new_v4(),Some(ProviderKind::OpenAi),Some(CHOSEN.into()),Some("low".into())).await.unwrap();
    assert_eq!(f.calls.lock().len(),1,"Identical evidence and selection may reuse the explanation");
    f.agent.explain_run(run.id,Uuid::new_v4(),Some(ProviderKind::OpenAi),Some(CHOSEN.into()),None).await.unwrap();
    assert_eq!(f.calls.lock().len(),2,"A changed effort must not return old model provenance");
    let review=f.agent.review_verification(f.project,run.id,Uuid::new_v4(),json!({"dossier_id":Uuid::new_v4(),"measurements":{"final_x":1}}),Some(ProviderKind::Anthropic),Some(OTHER.into()),Some("high".into())).await.unwrap();
    assert_eq!(review["assistant_message"]["metadata"]["model"],OTHER);
    let calls=f.calls.lock();assert_eq!(calls.len(),3);
    assert_call(&calls[0],"open_ai",CHOSEN,Some("low"));
    assert_call(&calls[1],"open_ai",CHOSEN,None);
    assert_call(&calls[2],"anthropic",OTHER,Some("high"));
}
fn recipe(manifest_id:Uuid,automatic:bool)->StudyRecipe {
    serde_json::from_value(json!({"title":"Selection snapshot control","hypothesis":"Software test only","base_manifest_id":manifest_id,
        "strategy":"latin_hypercube","parameters":[{"name":"initial x","target":"initial:x","minimum":0.1,"maximum":1.0}],
        "objectives":[{"metric":"state","goal":"minimize"}],"descriptors":[],"exploration_trials":4,"validation_finalists":0,
        "wall_seconds":30,"per_trial_seconds":10,"seed":17,"absolute_tolerance":1e-6,"relative_tolerance":1e-3,
        "auto_review":automatic,"review_model":{"provider":"open_ai","model":CHOSEN,"reasoning_effort":"low"}})).unwrap()
}
fn chosen()->StudyModelSelection {StudyModelSelection{provider:ProviderKind::OpenAi,model:CHOSEN.into(),reasoning_effort:Some("low".into())}}
#[tokio::test]
async fn discovery_manual_review_next_proposal_and_delayed_review_use_captured_selection() {
    let f=fixture(false).await;
    let manifest=save_manifest(&f,true);
    let discovery=DiscoveryService::new(f.agent.database.clone(),f.agent.scheduler.clone(),f.agent.clone()).unwrap();
    let study=discovery.create(recipe(manifest.id,false)).unwrap();
    discovery.review(study.id,chosen()).await.unwrap();
    discovery.propose_next(study.id,chosen()).await.unwrap();
    let automatic=discovery.create(recipe(manifest.id,true)).unwrap();
    discovery.start(automatic.id).unwrap();
    tokio::time::timeout(Duration::from_secs(15),async {
        loop {
            let saved=discovery.get(automatic.id).unwrap();
            assert_ne!(saved.state,"failed","{:?}",saved.events);
            if saved.events.iter().any(|event|event.kind=="ai_review_result"){break;}
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    }).await.expect("Automatic local study review should complete");
    let calls=f.calls.lock();assert_eq!(calls.len(),3);
    for call in calls.iter(){assert_call(call,"open_ai",CHOSEN,Some("low"));}
    assert_eq!(discovery.get(automatic.id).unwrap().recipe.review_model.unwrap().model,CHOSEN);
    let mut legacy=recipe(manifest.id,true);legacy.review_model=None;
    assert!(discovery.create(legacy).is_err(),"Automatic review may not inherit Settings model");
}
async fn task_stopped(f:&Fixture,id:Uuid)->tasks::ResearchTask {
    tokio::time::timeout(Duration::from_secs(15),async {
        loop {
            let task=f.agent.research_task(id).unwrap();
            if task.state!=tasks::TaskState::Running {
                // Worker persists its terminal state before releasing its admission slot.
                tokio::time::sleep(Duration::from_millis(30)).await;return task;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }).await.expect("Bounded fixture task should stop")
}
#[tokio::test]
async fn research_specialist_builder_and_resumed_reviewer_keep_selection_and_clear_effort() {
    let f=fixture(false).await;
    let request=serde_json::from_value(json!({"objective":"Verify dx/dt=1","duration_minutes":2,"max_cycles":1,"specialist_count":1,"auto_run":false,
        "provider":"open_ai","model":CHOSEN,"reasoning_effort":"low"})).unwrap();
    let task=f.agent.start_research_task(f.project,request).unwrap();
    let waiting=task_stopped(&f,task.id).await;
    assert_eq!(waiting.state,tasks::TaskState::NeedsInput,"{:?}",waiting.failure);
    completed_run(&f,waiting.current_manifest_id.unwrap()).await;
    let control=serde_json::from_value(json!({"action":"resume","provider":"open_ai","model":CHOSEN,"reasoning_effort":null})).unwrap();
    let resumed=f.agent.control_research_task(task.id,control).unwrap();
    assert_eq!(resumed.reasoning_effort,None,"Explicit provider default must clear the previously saved effort");
    let completed=task_stopped(&f,task.id).await;
    assert_eq!(completed.state,tasks::TaskState::Completed,"{:?}",completed.failure);
    let calls=f.calls.lock();assert_eq!(calls.len(),3);
    assert_call(&calls[0],"open_ai",CHOSEN,Some("low"));
    assert_call(&calls[1],"open_ai",CHOSEN,Some("low"));
    assert_call(&calls[2],"open_ai",CHOSEN,None);
}
#[tokio::test]
async fn studio_repair_keeps_selected_model_and_missing_selections_never_use_settings() {
    let f=fixture(true).await;
    let request=serde_json::from_value(json!({"prompt":"Create a conceptual sphere","kind":"scene","provider":"open_ai","model":CHOSEN,"reasoning_effort":"low"})).unwrap();
    f.agent.design_studio(f.project,request).await.unwrap();
    {
        let calls=f.calls.lock();assert_eq!(calls.len(),2);
        for call in calls.iter(){assert_call(call,"open_ai",CHOSEN,Some("low"));}
    }
    let request=serde_json::from_value(json!({"prompt":"Create a conceptual sphere","kind":"scene","provider":"open_ai"})).unwrap();
    assert!(f.agent.design_studio(f.project,request).await.is_err());
    let request=serde_json::from_value(json!({"objective":"Verify dx/dt=1","provider":"open_ai"})).unwrap();
    assert!(f.agent.start_research_task(f.project,request).is_err());
    assert_eq!(f.calls.lock().len(),2,"Missing choices must fail before a provider request");
}
