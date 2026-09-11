use super::*;
use axum::{routing::post,Json,Router};
use parking_lot::Mutex;
use crate::{config::AppConfig,compute::{HardwareManager,Scheduler},domain::{CreateProjectRequest,ProviderStatus,ResearchProject},persistence::Database};

struct Fixture{state:Arc<AppState>,project:Uuid,calls:Arc<Mutex<Vec<Value>>>,server:tokio::task::JoinHandle<()>,_temp:tempfile::TempDir}
impl Drop for Fixture{fn drop(&mut self){self.server.abort();}}
async fn fixture()->Fixture{
    let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().into(),gpu_enabled:false,..Default::default()};let database=Database::open(&config.database_path()).unwrap();
    let scheduler=Scheduler::new(database.clone(),HardwareManager::discover(&config).await.unwrap()).unwrap();let mut agent=AgentService::new(database.clone(),scheduler.clone()).unwrap();agent.test_key=Some("isolated-attachment-fixture".into());
    let calls=Arc::new(Mutex::new(vec![]));let captured=calls.clone();
    let app=Router::new().route("/v1/responses",post(move |Json(body):Json<Value>|{let captured=captured.clone();async move{
        let number={let mut calls=captured.lock();calls.push(body.clone());calls.len()};
        let output=if number==1{
            let files=body["input"].as_array().unwrap().iter().filter_map(|item|item["content"].as_str()).find_map(|text|text.split_once("FILES: ").map(|(_,json)|serde_json::from_str::<Value>(json).unwrap())).expect("Provider did not receive a file index");
            let id=files[0]["job_id"].as_str().unwrap();
            json!([{"type":"function_call","call_id":"read-original","name":"read_project_file","arguments":json!({"job_id":id,"max_bytes":18000}).to_string()},
                {"type":"function_call","call_id":"profile-original","name":"profile_dataset","arguments":json!({"job_id":id}).to_string()}])
        }else{json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"The original file and its descriptive statistics were inspected. Units still need confirmation."}]}])};
        Json(json!({"id":format!("attachment-response-{number}"),"status":"completed","output":output,"usage":{"input_tokens":25,"output_tokens":12}}))
    }}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base_url=format!("http://{}/v1",listener.local_addr().unwrap());let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    database.put_provider_status(&ProviderStatus{provider:ProviderKind::OpenAi,configured:true,key_configured:true,model_configured:true,model:"gpt-4.1".into(),base_url}).unwrap();
    let project=ResearchProject::new(CreateProjectRequest{name:None,question:"Inspect retained files".into()});database.put_project(&project).unwrap();
    let state=Arc::new(AppState{config:config.clone(),discovery:crate::discovery::DiscoveryService::new(database.clone(),scheduler.clone(),agent.clone()).unwrap(),assurance:crate::assurance::AssuranceService::new(database.clone()).unwrap(),laboratory:crate::laboratory::LaboratoryService::new(database.clone(),config).unwrap(),database,scheduler,agent,started_at:std::time::Instant::now(),telemetry:crate::compute::telemetry::Telemetry::start()});
    Fixture{state,project:project.id,calls,server,_temp:temp}
}
fn request()->SessionRequest{SessionRequest{request_id:Some(Uuid::new_v4()),content:String::new(),provider:Some(ProviderKind::OpenAi),model:Some("gpt-6-astra".into()),reasoning_effort:Some("low".into()),time_limit_seconds:None,attachments:vec![files::Attachment::Upload(files::Upload{name:"observations.csv".into(),mime_type:"text/csv".into(),size_bytes:0,content:"time,value\r\n0,1\r\n1,3\r\n".into(),units:Default::default()})],..Default::default()}}
async fn finished(f:&Fixture,id:Uuid)->LabJob{tokio::time::timeout(Duration::from_secs(6),async{loop{let job=f.state.laboratory.get(id).unwrap();if !job.active()&&!f.state.laboratory.executing(id){return job;}tokio::time::sleep(Duration::from_millis(15)).await;}}).await.unwrap()}

#[tokio::test]async fn file_only_chat_reads_actual_originals_and_profiles_with_selected_model(){
    let f=fixture().await;let request=request();let original=request.clone();
    let job=f.state.agent.start_lab_session(f.state.clone(),f.project,request,None).unwrap();let completed=finished(&f,job.id).await;assert_eq!(completed.state,"completed");
    let calls=f.calls.lock();assert_eq!(calls.len(),2);assert_eq!(calls[0]["model"],"gpt-6-astra");assert_eq!(calls[0]["reasoning"]["effort"],"low");
    let results=calls[1]["input"].as_array().unwrap().iter().filter(|item|item["type"]=="function_call_output").map(|item|(item["call_id"].as_str().unwrap(),serde_json::from_str::<Value>(item["output"].as_str().unwrap()).unwrap())).collect::<std::collections::BTreeMap<_,_>>();
    assert_eq!(results["read-original"]["text"],"time,value\r\n0,1\r\n1,3\r\n");assert_eq!(results["profile-original"]["profile"]["columns"][1]["mean"],2.0);assert_eq!(results["read-original"]["file"]["sha256"],results["profile-original"]["file"]["sha256"]);drop(calls);
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
