//! Loopback-only Anthropic native continuation and saved-compaction recovery.
use super::*;
use axum::{Router,Json,routing::post};
use parking_lot::Mutex;
use crate::{config::AppConfig,compute::{HardwareManager,Scheduler},domain::{CreateProjectRequest,ProviderStatus,ResearchProject},persistence::Database};

const MODEL:&str="claude-sonnet-4-6";
struct Fixture{state:Arc<AppState>,job:LabJob,request:SessionRequest,calls:Arc<Mutex<Vec<Value>>>,dispatch:Arc<Mutex<Option<Value>>>,native_dispatch:Arc<Mutex<Option<Value>>>,server:tokio::task::JoinHandle<()>,_temp:tempfile::TempDir}
impl Drop for Fixture{fn drop(&mut self){self.server.abort();}}
fn answer(text:&str)->Value{json!({"id":"local-anthropic-response","type":"message","stop_reason":"end_turn","content":[{"type":"text","text":text}],"usage":{"input_tokens":23,"output_tokens":11}})}
async fn fixture()->Fixture{
    let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().into(),gpu_enabled:false,..Default::default()};let database=Database::open(&config.database_path()).unwrap();
    let scheduler=Scheduler::new(database.clone(),HardwareManager::discover(&config).await.unwrap()).unwrap();let mut agent=AgentService::new(database.clone(),scheduler.clone()).unwrap();agent.test_key=Some("local-anthropic-recovery-fixture".into());
    let project=ResearchProject::new(CreateProjectRequest{name:None,question:"Inspect retained numerical evidence only".into()});database.put_project(&project).unwrap();let id=Uuid::new_v4();
    let calls=Arc::new(Mutex::new(vec![]));let captured=calls.clone();let dispatch=Arc::new(Mutex::new(None));let captured_dispatch=dispatch.clone();let journal_path=config.artifacts_directory().join("laboratory").join(id.to_string()).join("journal.json");
    let native_dispatch=Arc::new(Mutex::new(None));let captured_native=native_dispatch.clone();
    let app=Router::new().route("/v1/messages",post(move|Json(body):Json<Value>|{let summary=body.get("tools").is_none();captured.lock().push(body);let snapshot=Some(serde_json::from_slice(&std::fs::read(&journal_path).unwrap()).unwrap());if summary{*captured_dispatch.lock()=snapshot;}else{*captured_native.lock()=snapshot;}async move{Json(answer(if summary{"Goal: inspect retained evidence only. The measured value is 42 m; no empirical claim is established. Next: answer within the original objective."}else{"Recovered Anthropic continuation inspected the measured evidence; no new experiment was run."}))}}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base_url=format!("http://{}/v1",listener.local_addr().unwrap());let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    database.put_provider_status(&ProviderStatus{provider:ProviderKind::Anthropic,configured:true,key_configured:true,model_configured:true,model:"claude-haiku-4-5".into(),base_url}).unwrap();
    let state=Arc::new(AppState{config:config.clone(),discovery:crate::discovery::DiscoveryService::new(database.clone(),scheduler.clone(),agent.clone()).unwrap(),assurance:crate::assurance::AssuranceService::new(database.clone()).unwrap(),laboratory:crate::laboratory::LaboratoryService::new(database.clone(),config).unwrap(),database,scheduler,agent,started_at:std::time::Instant::now(),telemetry:crate::compute::telemetry::Telemetry::start()});
    let request=SessionRequest{request_id:Some(id),content:"Inspect the retained measured value and its limitations. Do not run another experiment.".into(),provider:Some(ProviderKind::Anthropic),model:Some(MODEL.into()),reasoning_effort:Some("high".into()),time_limit_seconds:None,..Default::default()};
    let job=state.laboratory.create(id,project.id,None,"session","Anthropic recovery fixture",serde_json::to_value(&request).unwrap(),None).unwrap();
    Fixture{state,job,request,calls,dispatch,native_dispatch,server,_temp:temp}
}
fn source()->Journal{Journal{items:vec![json!({"role":"user","content":"Retained numerical evidence with units and limitations. ".repeat(4000)})],..Default::default()}}
fn path(f:&Fixture)->std::path::PathBuf{f.state.laboratory.directory(f.job.id).join("journal.json")}
fn chosen(body:&Value){assert_eq!(body["model"],MODEL);assert_eq!(body["output_config"]["effort"],"high");assert_eq!(body["thinking"]["type"],"adaptive");}

#[tokio::test]async fn anthropic_cached_native_tools_compact_and_finish_with_captured_selection(){
    let mut f=fixture().await;write_json(&f.state.laboratory.directory(f.job.id).join("measurements.json"),&json!({"value":42,"units":"m"})).unwrap();
    let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j3f8AAAAASUVORK5CYII=";
    for name in ["camera-a.png","camera-b.png"]{std::fs::write(f.state.laboratory.directory(f.job.id).join(name),base64::engine::general_purpose::STANDARD.decode(png).unwrap()).unwrap();}
    let usage=f.state.agent.usage.begin(f.job.id,Some(f.job.project_id),ProviderKind::Anthropic,MODEL,"laboratory_tool_turn",0,100,128).unwrap();
    let mut journal=source();journal.pending=Some(json!({"usage_id":usage.id,"http_status":200,"state":"requesting"}));write_json(&path(&f),&journal).unwrap();
    let body=json!({"id":"saved-anthropic-tools","type":"message","stop_reason":"tool_use","content":[
        {"type":"tool_use","id":"keep-goal","name":"remember","input":{"goal":"Inspect existing evidence; do not run a new experiment.","constraints":["retain units"],"evidence":[f.job.id],"next_actions":["read measured data"]}},
        {"type":"tool_use","id":"read-evidence","name":"read_artifact","input":{"job_id":f.job.id,"path":"measurements.json"}},
        {"type":"tool_use","id":"view-a","name":"observe_frame","input":{"job_id":f.job.id,"path":"camera-a.png","question":"Inspect view A"}},
        {"type":"tool_use","id":"view-b","name":"observe_frame","input":{"job_id":f.job.id,"path":"camera-b.png","question":"Inspect view B"}}],"usage":{"input_tokens":13,"output_tokens":7}});
    write_json(&f.state.laboratory.directory(f.job.id).join("provider-00000.json"),&body).unwrap();
    f.state.agent.run_lab_session(f.state.clone(),f.job.id,&CancellationToken::new()).await.unwrap();
    let saved:Journal=serde_json::from_slice(&std::fs::read(path(&f)).unwrap()).unwrap();assert_eq!(saved.tools["keep-goal"]["state"],"completed");assert_eq!(saved.tools["read-evidence"]["state"],"completed");assert!(saved.tools["read-evidence"]["output"].to_string().contains("42"));
    assert_eq!(saved.compactions.len(),1);assert_eq!(saved.compactions[0]["model"],MODEL);assert_eq!(saved.compactions[0]["reasoning_effort"],"high");
    let job=f.state.laboratory.get(f.job.id).unwrap();assert_eq!(job.state,"completed");assert_eq!(job.deadline_at,None);assert_eq!(job.events.iter().filter(|event|event.kind=="completed").count(),1);
    {let calls=f.calls.lock();assert_eq!(calls.len(),2,"Only one summary and one continuation may be generated; the saved tool response must not be regenerated");for call in calls.iter(){chosen(call);}assert!(calls[0]["messages"][0]["content"].as_str().unwrap().contains("42"));assert!(calls[1]["tools"].is_array());
        let images=calls[1]["messages"].as_array().unwrap().iter().flat_map(|message|message["content"].as_array().into_iter().flatten()).filter(|part|part["type"]=="image").collect::<Vec<_>>();assert_eq!(images.len(),2,"Compaction must not erase newly acquired images before a vision request receives them");assert!(images.iter().all(|image|image["source"]["data"]==png));}
    assert!(saved.image_outbox.is_empty());let event=job.events.iter().find(|event|event.kind=="vision_input_acknowledged").unwrap();assert_eq!(event.data["image_count"],2);assert_eq!(event.data["images"][0]["path"],"camera-a.png");assert_eq!(event.data["images"][1]["path"],"camera-b.png");
    let receipt=f.state.laboratory.read_json(job.id,event.data["request_path"].as_str().unwrap()).unwrap();assert_eq!(receipt["input_images"].as_array().unwrap().len(),2);assert_eq!(receipt["receipt"]["model"],MODEL);assert_eq!(receipt["receipt"]["images"][0]["sha256"],event.data["images"][0]["sha256"]);
    f.state.agent.resume_lab_session(f.state.clone(),job.id).unwrap();assert_eq!(f.calls.lock().len(),2);assert_eq!(f.state.database.list_usage_records().unwrap().len(),3);
    assert!(f.state.database.list_usage_records().unwrap().iter().all(|row|row.provider==ProviderKind::Anthropic&&row.model==MODEL&&row.status=="completed"));
    // Recover a process lost after native response persistence but before delivery.
    write_json(&path(&f),&f.native_dispatch.lock().clone().unwrap()).unwrap();f.state.laboratory.update(job.id,|job|job.state="paused".into()).unwrap();
    Arc::get_mut(&mut f.state).unwrap().agent.test_key=None;let mut status=f.state.database.provider_status(ProviderKind::Anthropic,false).unwrap();status.base_url="invalid endpoint must not be contacted".into();f.state.database.put_provider_status(&status).unwrap();
    f.state.agent.run_lab_session(f.state.clone(),job.id,&CancellationToken::new()).await.unwrap();assert_eq!(f.calls.lock().len(),2);
    let replay:Journal=serde_json::from_slice(&std::fs::read(path(&f)).unwrap()).unwrap();assert!(replay.image_outbox.is_empty());assert_eq!(f.state.laboratory.get(job.id).unwrap().events.iter().filter(|event|event.kind=="vision_input_acknowledged").count(),1);
}

#[tokio::test]async fn anthropic_saved_compaction_recovers_without_credentials_or_another_summary(){
    let mut f=fixture().await;let mut journal=source();let original=journal.items.clone();
    let png="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j3f8AAAAASUVORK5CYII=";
    record_image(&mut journal,"pending-image",&json!({"image_data_url":format!("data:image/png;base64,{png}"),"evidence":{"job_id":f.job.id,"path":"retained.png"}}));
    f.state.agent.prepare_lab_compaction(&f.job,&f.request,&path(&f),&mut journal).unwrap();
    f.state.agent.reconcile_lab_compaction(&f.state,&f.job,&f.request,&path(&f),&mut journal,&CancellationToken::new()).await.unwrap();assert_eq!(f.calls.lock().len(),1);chosen(&f.calls.lock()[0]);
    let snapshot=f.dispatch.lock().clone().unwrap();let mut interrupted:Journal=serde_json::from_value(snapshot).unwrap();write_json(&path(&f),&interrupted).unwrap();
    Arc::get_mut(&mut f.state).unwrap().agent.test_key=None;let mut status=f.state.database.provider_status(ProviderKind::Anthropic,false).unwrap();status.base_url="invalid endpoint must not be contacted".into();f.state.database.put_provider_status(&status).unwrap();
    f.state.agent.reconcile_lab_compaction(&f.state,&f.job,&f.request,&path(&f),&mut interrupted,&CancellationToken::new()).await.unwrap();
    assert_eq!(f.calls.lock().len(),1);assert_eq!(interrupted.compactions.len(),1);assert_eq!(&interrupted.items[..original.len()],&original);assert_eq!(interrupted.compactions[0]["model"],MODEL);assert!(interrupted.compaction_pending.is_none()&&interrupted.compaction_delivery.is_none());
    assert_eq!(interrupted.image_outbox.len(),1,"A summary response never acknowledges image delivery");assert!(context(&interrupted).unwrap().iter().any(|item|item["content"].as_array().is_some_and(|parts|parts.iter().any(|part|part["type"]=="input_image"))));
    assert_eq!(f.state.database.list_usage_records().unwrap().len(),1);assert_eq!(f.state.laboratory.get(f.job.id).unwrap().events.iter().filter(|event|event.kind=="compacted").count(),1);
    let again=serde_json::to_value(&interrupted).unwrap();f.state.agent.reconcile_lab_compaction(&f.state,&f.job,&f.request,&path(&f),&mut interrupted,&CancellationToken::new()).await.unwrap();assert_eq!(serde_json::to_value(interrupted).unwrap(),again);
}
