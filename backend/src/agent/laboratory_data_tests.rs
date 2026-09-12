use super::*;
use axum::{routing::post,Json,Router};
use parking_lot::Mutex;
use crate::{config::AppConfig,compute::{HardwareManager,Scheduler},domain::{CreateProjectRequest,ProviderStatus,ResearchProject},persistence::Database};

struct Fixture{state:Arc<AppState>,project:Uuid,calls:Arc<Mutex<Vec<Value>>>,server:tokio::task::JoinHandle<()>,_temp:Option<tempfile::TempDir>}
impl Drop for Fixture{fn drop(&mut self){
    self.server.abort();
    if std::thread::panicking(){if let Some(temp)=self._temp.take(){eprintln!("Attachment fixture retained after failure: {}",temp.keep().display());}}
}}
async fn fixture()->Fixture{
    let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().into(),gpu_enabled:false,..Default::default()};let database=Database::open(&config.database_path()).unwrap();
    let scheduler=Scheduler::new(database.clone(),HardwareManager::discover(&config).await.unwrap()).unwrap();let mut agent=AgentService::new(database.clone(),scheduler.clone()).unwrap();agent.test_key=Some("isolated-attachment-fixture".into());
    let calls=Arc::new(Mutex::new(vec![]));let captured=calls.clone();
    let app=Router::new().route("/v1/responses",post(move |Json(body):Json<Value>|{let captured=captured.clone();async move{
        let number={let mut calls=captured.lock();calls.push(body.clone());calls.len()};
        let output=if number==1{
            let files=body["input"].as_array().unwrap().iter().filter_map(|item|item["content"].as_str()).find_map(|text|text.split_once("FILES: ").map(|(_,json)|serde_json::from_str::<Value>(json).unwrap())).expect("Provider did not receive a file index");
            let id=files[0]["job_id"].as_str().unwrap();
            let mut output=vec![];
            if let Some(contract)=body["instructions"].as_str().and_then(|text|text.split_once("Application output-contract state (not a new user message): ").map(|(_,tail)|serde_json::Deserializer::from_str(tail).into_iter::<Value>().next().unwrap().unwrap())) {
                output.push(json!({"type":"function_call","call_id":"resolve-output","name":"set_output_intent","arguments":json!({"request_id":contract["request_id"],"intent":"explanation","update_effect":"replace_objective","user_instruction_quote":contract["user_instruction"],"scope":"Inspect attached originals and descriptive profiles without starting new science"}).to_string()}));
            }
            output.extend([json!({"type":"function_call","call_id":"read-original","name":"read_project_file","arguments":json!({"job_id":id,"max_bytes":18000}).to_string()}),
                json!({"type":"function_call","call_id":"profile-original","name":"profile_dataset","arguments":json!({"job_id":id}).to_string()})]);
            json!(output)
        }else{json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"The original file and its descriptive statistics were inspected. Units still need confirmation."}]}])};
        Json(json!({"id":format!("attachment-response-{number}"),"status":"completed","output":output,"usage":{"input_tokens":25,"output_tokens":12}}))
    }}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base_url=format!("http://{}/v1",listener.local_addr().unwrap());let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    database.put_provider_status(&ProviderStatus{provider:ProviderKind::OpenAi,configured:true,key_configured:true,model_configured:true,model:"gpt-4.1".into(),base_url}).unwrap();
    let project=ResearchProject::new(CreateProjectRequest{name:None,question:"Inspect retained files".into()});database.put_project(&project).unwrap();
    let state=Arc::new(AppState{config:config.clone(),discovery:crate::discovery::DiscoveryService::new(database.clone(),scheduler.clone(),agent.clone()).unwrap(),assurance:crate::assurance::AssuranceService::new(database.clone()).unwrap(),laboratory:crate::laboratory::LaboratoryService::new(database.clone(),config).unwrap(),database,scheduler,agent,started_at:std::time::Instant::now(),telemetry:crate::compute::telemetry::Telemetry::start()});
    Fixture{state,project:project.id,calls,server,_temp:Some(temp)}
}
fn request()->SessionRequest{SessionRequest{request_id:Some(Uuid::new_v4()),content:String::new(),provider:Some(ProviderKind::OpenAi),model:Some("gpt-6-astra".into()),reasoning_effort:Some("low".into()),time_limit_seconds:None,attachments:vec![files::Attachment::Upload(files::Upload{name:"observations.csv".into(),mime_type:"text/csv".into(),size_bytes:0,content:"time,value\r\n0,1\r\n1,3\r\n".into(),units:Default::default()})],..Default::default()}}
// These are durable local-HTTP integration tests, not six-second performance
// benchmarks. A resumed upload crosses several fsync/rename boundaries and two
// provider turns. Bound both lack of progress and total elapsed time, and expose
// the retained stage if either expires instead of an uninformative Elapsed(()).
// These Tokio watchdogs remain cooperative; CI's job timeout also bounds a
// synchronous OS call that blocks the runtime itself.
fn wait_diagnostics(f:&Fixture,id:Uuid,reason:&str,elapsed:Duration,idle:Duration)->Value{
    let job=f.state.laboratory.get(id).unwrap();
    let journal=std::fs::read(f.state.laboratory.directory(id).join("journal.json"))
        .ok().and_then(|raw|serde_json::from_slice::<Value>(&raw).ok()).unwrap_or(Value::Null);
    let tools=journal["tools"].as_object().map(|rows|rows.iter().map(|(id,row)|json!({"call_id":id,"name":row["name"],"state":row["state"]})).collect::<Vec<_>>()).unwrap_or_default();
    let usage=f.state.database.list_usage_records().unwrap().into_iter().filter(|row|row.request_id==id)
        .map(|row|json!({"id":row.id,"attempt":row.attempt,"status":row.status,"purpose":row.purpose,"provider_response_id":row.provider_response_id})).collect::<Vec<_>>();
    json!({"reason":reason,"elapsed_ms":elapsed.as_millis(),"idle_ms":idle.as_millis(),"job_id":id,
        "state":job.state,"executing":f.state.laboratory.executing(id),"error":job.error,
        "events":job.events.iter().map(|event|json!({"sequence":event.sequence,"kind":event.kind,"at":event.at})).collect::<Vec<_>>(),
        "provider_calls":f.calls.lock().len(),"mock_server_finished":f.server.is_finished(),"usage":usage,
        "journal":{"exists":!journal.is_null(),"round":journal["round"],"pending":journal["pending"],
            "delivery_pending":!journal["delivery"].is_null(),"tool_stages":tools,"attachment_context":journal["attachment_context"]}})
}
async fn wait_finished(f:&Fixture,id:Uuid,idle_limit:Duration,total_limit:Duration)->Result<LabJob,Value>{
    let started=tokio::time::Instant::now();let mut progressed=started;let mut previous=None;
    loop{
        let job=f.state.laboratory.get(id).unwrap();let executing=f.state.laboratory.executing(id);
        if !job.active()&&!executing{return Ok(job);}
        let signature=(job.state.clone(),job.events.len(),executing,f.calls.lock().len());
        let now=tokio::time::Instant::now();
        if previous.as_ref()!=Some(&signature){progressed=now;previous=Some(signature);}
        let elapsed=now.duration_since(started);let idle=now.duration_since(progressed);
        if elapsed>=total_limit||idle>=idle_limit{
            let reason=if elapsed>=total_limit{"absolute_completion_deadline"}else{"no_durable_progress"};
            return Err(wait_diagnostics(f,id,reason,elapsed,idle));
        }
        tokio::time::sleep(Duration::from_millis(100).min(total_limit-elapsed).min(idle_limit-idle)).await;
    }
}
async fn finished(f:&Fixture,id:Uuid)->LabJob{
    match wait_finished(f,id,Duration::from_secs(30),Duration::from_secs(90)).await{
        Ok(job)=>job,
        Err(diagnostics)=>{
            let path=f.state.laboratory.directory(id).join("test-wait-diagnostics.json");
            let _=std::fs::write(&path,serde_json::to_vec_pretty(&diagnostics).unwrap());
            panic!("Attachment completion wait failed; retained state: {diagnostics}; receipt: {}",path.display());
        }
    }
}

#[tokio::test]async fn completion_wait_reports_stall_and_requires_execution_release(){
    let f=fixture().await;let id=Uuid::new_v4();
    f.state.laboratory.create(id,f.project,None,"session","Stalled fixture",json!({}),None).unwrap();
    let stalled=wait_finished(&f,id,Duration::ZERO,Duration::from_secs(1)).await.unwrap_err();
    assert_eq!(stalled["reason"],"no_durable_progress");assert_eq!(stalled["state"],"queued");
    assert_eq!(stalled["provider_calls"],0);assert_eq!(stalled["journal"]["exists"],false);
    let _token=f.state.laboratory.acquire(id).unwrap();
    f.state.laboratory.update(id,|job|job.state="completed".into()).unwrap();
    let held=wait_finished(&f,id,Duration::from_secs(1),Duration::ZERO).await.unwrap_err();
    assert_eq!(held["reason"],"absolute_completion_deadline");assert_eq!(held["executing"],true);
    f.state.laboratory.release(id);
    assert_eq!(wait_finished(&f,id,Duration::ZERO,Duration::ZERO).await.unwrap().state,"completed");
}

#[tokio::test]async fn file_only_chat_reads_actual_originals_and_profiles_with_selected_model(){
    let f=fixture().await;let request=request();let original=request.clone();
    let job=f.state.agent.start_lab_session(f.state.clone(),f.project,request,None).unwrap();let completed=finished(&f,job.id).await;assert_eq!(completed.state,"completed");
    let calls=f.calls.lock();assert_eq!(calls.len(),2);assert_eq!(calls[0]["model"],"gpt-6-astra");assert_eq!(calls[0]["reasoning"]["effort"],"low");
    assert!(!calls[0]["input"].to_string().contains("Application output-contract state"));assert!(calls[1]["instructions"].as_str().unwrap().contains("The current request is already resolved."));
    let results=calls[1]["input"].as_array().unwrap().iter().filter(|item|item["type"]=="function_call_output").map(|item|(item["call_id"].as_str().unwrap(),serde_json::from_str::<Value>(item["output"].as_str().unwrap()).unwrap())).collect::<std::collections::BTreeMap<_,_>>();
    assert_eq!(results["resolve-output"]["output_intent"]["resolved"],"explanation");
    assert_eq!(results["read-original"]["text"],"time,value\r\n0,1\r\n1,3\r\n");assert_eq!(results["profile-original"]["profile"]["columns"][1]["mean"],2.0);assert_eq!(results["read-original"]["file"]["sha256"],results["profile-original"]["file"]["sha256"]);drop(calls);
    assert_eq!(completed.result["deliverable"]["status"],"fulfilled");assert_eq!(completed.result["deliverable"]["output_intent"],"explanation");
    assert!(f.state.laboratory.list(Some(f.project)).unwrap().iter().all(|job|!matches!(job.kind.as_str(),"solver"|"generated"|"ml_study"|"sweep")));
    assert!(completed.input["attachments"][0].get("content").is_none());assert!(completed.input["attachments"][0]["job_id"].is_string());
    let messages=f.state.database.list_messages(f.project,20).unwrap();let user=messages.iter().find(|message|message.role==ConversationRole::User).unwrap();assert_eq!(user.metadata["attachments"][0]["name"],"observations.csv");assert!(user.content.contains("Do not start an experiment"));
    f.state.agent.start_lab_session(f.state.clone(),f.project,original,None).unwrap();assert_eq!(f.calls.lock().len(),2);assert_eq!(f.state.database.list_messages(f.project,20).unwrap().iter().filter(|message|message.role==ConversationRole::User).count(),1);
}
#[tokio::test]async fn invalid_batch_starts_no_session_or_provider_and_leaves_no_partial_file(){
    let f=fixture().await;let mut request=request();request.attachments.push(files::Attachment::Upload(files::Upload{name:"malformed.json".into(),mime_type:String::new(),size_bytes:1,content:"{".into(),units:Default::default()}));
    assert!(f.state.agent.start_lab_session(f.state.clone(),f.project,request,None).is_err());assert!(f.calls.lock().is_empty());assert!(f.state.laboratory.list(Some(f.project)).unwrap().is_empty());assert!(f.state.database.list_messages(f.project,20).unwrap().is_empty());
}
#[tokio::test]async fn legacy_raw_attachments_are_admitted_on_resume_and_retrievable(){
    let f=fixture().await;let mut request=request();request.content="Inspect my previously attached table.".into();let id=request.request_id.unwrap();
    let job=f.state.laboratory.create(id,f.project,None,"session","Legacy file admission",serde_json::to_value(&request).unwrap(),None).unwrap();f.state.laboratory.update(id,|job|job.state="paused".into()).unwrap();
    f.state.agent.resume_lab_session(f.state.clone(),job.id).unwrap();let job=finished(&f,id).await;assert_eq!(job.state,"completed");assert!(job.input["attachments"][0].get("content").is_none());assert!(job.events.iter().any(|event|event.kind=="attachments_admitted"));assert_eq!(f.calls.lock().len(),2);
}

#[tokio::test]async fn resumed_file_admission_preserves_native_tool_result_order(){
    let f=fixture().await;let mut request=request();request.content="Inspect the attached table after the saved lookup.".into();let id=request.request_id.unwrap();
    f.state.laboratory.create(id,f.project,None,"session","Legacy pending lookup",serde_json::to_value(&request).unwrap(),None).unwrap();
    let journal=Journal{items:vec![json!({"role":"user","content":"Inspect my files."}),json!({"type":"function_call","call_id":"saved-file-lookup","name":"list_project_files","arguments":"{}"})],..Default::default()};
    write_json(&f.state.laboratory.directory(id).join("journal.json"),&journal).unwrap();
    f.state.laboratory.update(id,|job|job.state="paused".into()).unwrap();
    f.state.agent.resume_lab_session(f.state.clone(),id).unwrap();assert_eq!(finished(&f,id).await.state,"completed");
    let calls=f.calls.lock();let first=calls[0]["input"].as_array().unwrap();
    let proposal=first.iter().position(|item|item["type"]=="function_call"&&item["call_id"]=="saved-file-lookup").unwrap();
    let output=first.iter().position(|item|item["type"]=="function_call_output"&&item["call_id"]=="saved-file-lookup").unwrap();
    let file_index=first.iter().position(|item|item["content"].as_str().is_some_and(|text|text.contains("FILES: "))).unwrap();
    assert_eq!(output,proposal+1,"The saved tool proposal must receive its result before any newly admitted file context");assert!(file_index>output);
    assert_eq!(first.iter().filter(|item|item["content"].as_str().is_some_and(|text|text.contains("FILES: "))).count(),1);
}
