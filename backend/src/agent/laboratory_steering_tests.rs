use super::*;
use parking_lot::Mutex;
use crate::{config::AppConfig,compute::{HardwareManager,Scheduler},domain::{CreateProjectRequest,ProviderStatus,ResearchProject},persistence::Database};
struct Fixture{state:Arc<AppState>,project:Uuid,source:Uuid,request:SessionRequest,calls:Arc<Mutex<Vec<Value>>>,arrived:Arc<tokio::sync::Notify>,release:Arc<tokio::sync::Notify>,server:tokio::task::JoinHandle<()>,_temp:tempfile::TempDir}
impl Drop for Fixture{fn drop(&mut self){self.release.notify_one();self.server.abort();}}
async fn fixture(mode:&'static str)->Fixture{
    let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().into(),gpu_enabled:false,..Default::default()};let database=Database::open(&config.database_path()).unwrap();
    let scheduler=Scheduler::new(database.clone(),HardwareManager::discover(&config).await.unwrap()).unwrap();let mut agent=AgentService::new(database.clone(),scheduler.clone()).unwrap();agent.test_key=Some("isolated-steering-fixture".into());
    let source=Uuid::new_v4();let calls=Arc::new(Mutex::new(vec![]));let captured=calls.clone();let arrived=Arc::new(tokio::sync::Notify::new());let arrived_signal=arrived.clone();let release=Arc::new(tokio::sync::Notify::new());let release_signal=release.clone();
    let app=Router::new().route("/v1/responses",post(move |Json(body):Json<Value>|{let captured=captured.clone();let arrived=arrived_signal.clone();let release=release_signal.clone();async move{
        let number={let mut calls=captured.lock();calls.push(body.clone());calls.len()};
        if number==1{arrived.notify_one();release.notified().await;}
        let output=if number==1&&mode=="tool"{json!([{"type":"function_call","call_id":"old-color","name":"edit_presentation","arguments":json!({"job_id":source,"patch":{"color":"red"}}).to_string()}])}
        else if number==1&&mode=="wait"{json!([{"type":"function_call","call_id":"old-wait","name":"inspect_result","arguments":json!({"job_id":source}).to_string()}])}
        else if number==2{
            let (_,tail)=body["instructions"].as_str().unwrap().split_once("Application output-contract state (not a new user message): ").expect("Current steering contract missing");
            let contract=serde_json::Deserializer::from_str(tail).into_iter::<Value>().next().unwrap().unwrap();
            json!([{"type":"function_call","call_id":"resolve-current-update","name":"set_output_intent","arguments":json!({"request_id":contract["request_id"],"intent":"explanation","update_effect":"preserve_objective","user_instruction_quote":contract["user_instruction"],"preserved_instruction_quote":contract["previous_objective"]["user_instruction"],"scope":"Explain the dimensional-analysis assumptions and incorporate the saved update without changing numerical inputs"}).to_string()}])
        }
        else{json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":if number==1{"Response under the earlier assumptions."}else{"The update was considered within the original objective; earlier numerical inputs remain unchanged."}}]}])};
        Json(json!({"id":format!("steering-response-{number}"),"status":"completed","output":output,"usage":{"input_tokens":31,"output_tokens":17}}))
    }}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base_url=format!("http://{}/v1",listener.local_addr().unwrap());let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    database.put_provider_status(&ProviderStatus{provider:ProviderKind::OpenAi,configured:true,key_configured:true,model_configured:true,model:"gpt-4.1".into(),base_url}).unwrap();
    let project=ResearchProject::new(CreateProjectRequest{name:None,question:"Original dimensional-analysis objective".into()});database.put_project(&project).unwrap();
    let state=Arc::new(AppState{config:config.clone(),discovery:crate::discovery::DiscoveryService::new(database.clone(),scheduler.clone(),agent.clone()).unwrap(),assurance:crate::assurance::AssuranceService::new(database.clone()).unwrap(),laboratory:crate::laboratory::LaboratoryService::new(database.clone(),config).unwrap(),database,scheduler,agent,started_at:std::time::Instant::now(),telemetry:crate::compute::telemetry::Telemetry::start()});
    state.laboratory.create(source,project.id,None,"solver","Unstarted software-test source",json!({"parameters":{"original":true}}),None).unwrap();
    let request=SessionRequest{request_id:Some(Uuid::new_v4()),content:"Keep the original dimensional-analysis objective and inspect its prior assumptions.".into(),provider:Some(ProviderKind::OpenAi),model:Some("gpt-6-astra".into()),reasoning_effort:Some("low".into()),time_limit_seconds:None,context_job_id:Some(source),..Default::default()};
    Fixture{state,project:project.id,source,request,calls,arrived,release,server,_temp:temp}
}
fn parent(f:&Fixture)->LabJob{f.state.laboratory.create(f.request.request_id.unwrap(),f.project,None,"session","Parent",serde_json::to_value(&f.request).unwrap(),None).unwrap()}
fn update(text:&str)->SteeringRequest{SteeringRequest{request_id:Uuid::new_v4(),content:text.into(),attachments:vec![],output_intent:None}}
async fn finished(f:&Fixture,id:Uuid)->LabJob{tokio::time::timeout(Duration::from_secs(8),async{loop{let job=f.state.laboratory.get(id).unwrap();if !job.active()&&!f.state.laboratory.executing(id){return job;}tokio::time::sleep(Duration::from_millis(15)).await;}}).await.unwrap()}
async fn arrived(f:&Fixture){tokio::time::timeout(Duration::from_secs(5),f.arrived.notified()).await.unwrap();}

#[tokio::test]async fn updates_are_idempotent_and_do_not_mutate_input_model_deadline_or_existing_evidence(){
    let f=fixture("final").await;let parent=parent(&f);let request=update("Treat time as milliseconds, and retain the original objective.");
    receive(&f.state,parent.id,request.clone()).unwrap();receive(&f.state,parent.id,request.clone()).unwrap();
    let stored=f.state.laboratory.get(parent.id).unwrap();assert_eq!(stored.input,parent.input);assert_eq!(stored.deadline_at,None);assert_eq!(stored.events.iter().filter(|event|event.kind=="steering_received").count(),1);
    assert_eq!(f.state.database.list_messages(f.project,20).unwrap().len(),1);assert_eq!(f.state.laboratory.get(f.source).unwrap().input["parameters"]["original"],true);assert!(f.calls.lock().is_empty());
    assert!(receive(&f.state,parent.id,SteeringRequest{content:"Different update".into(),..request}).is_err());
    assert!(serde_json::from_value::<SteeringRequest>(json!({"request_id":Uuid::new_v4(),"content":"Override","model":"another-model","time_limit_seconds":null})).is_err());
}
#[tokio::test]async fn superseded_actions_keep_uncertain_targets_and_completed_receipts_with_valid_tool_order(){
    let f=fixture("final").await;let parent=parent(&f);let request=update("Do not launch another run or recolor the old evidence.");receive(&f.state,parent.id,request.clone()).unwrap();
    let old_target=f.source;let mut journal=Journal{items:vec![json!({"type":"function_call","call_id":"planned","name":"launch_experiment","arguments":"{}"}),json!({"type":"function_call","call_id":"prepared","name":"launch_experiment","arguments":"{}"})],tools:BTreeMap::from([("done".into(),json!({"state":"completed","output":{"measured":42}})),("prepared".into(),json!({"state":"prepared","target_id":old_target}))]),..Default::default()};
    let path=f.state.laboratory.directory(parent.id).join("journal.json");assert!(apply(&f.state,&parent,&path,&mut journal).unwrap());
    assert_eq!(journal.tools["planned"]["output"]["prior_execution"],"not started");assert!(journal.tools["prepared"]["output"]["prior_execution"].as_str().unwrap().starts_with("target already recorded"));assert_eq!(journal.tools["done"]["output"]["measured"],42);
    let last_result=journal.items.iter().rposition(|item|item["type"]=="function_call_output").unwrap();let user=journal.items.iter().position(|item|item["content"].as_str().is_some_and(|text|text.contains("Explicit user update"))).unwrap();assert!(last_result<user,"Tool outputs must precede the new user text for both provider protocols");
    let original_len=journal.items.len();f.state.laboratory.update(parent.id,|job|job.events.retain(|event|event.kind!="steering_applied")).unwrap();
    let mut recovered:Journal=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();assert!(!apply(&f.state,&parent,&path,&mut recovered).unwrap());assert_eq!(recovered.items.len(),original_len);assert_eq!(f.state.laboratory.get(parent.id).unwrap().events.iter().filter(|event|event.kind=="steering_applied").count(),1);
}
#[tokio::test]async fn in_flight_proposal_is_superseded_before_its_visual_side_effect(){
    let f=fixture("tool").await;let job=f.state.agent.start_lab_session(f.state.clone(),f.project,f.request.clone(),None).unwrap();arrived(&f).await;
    receive(&f.state,job.id,update("Do not recolor anything; explain the dimensions using the original objective.")).unwrap();f.release.notify_one();
    let job=finished(&f,job.id).await;assert_eq!(job.state,"completed");assert!(!f.state.laboratory.directory(f.source).join("presentation.json").exists());
    let journal:Journal=serde_json::from_slice(&std::fs::read(f.state.laboratory.directory(job.id).join("journal.json")).unwrap()).unwrap();assert_eq!(journal.tools["old-color"]["superseded"],true);
    assert_eq!(journal.tools["resolve-current-update"]["output"]["output_intent"]["resolved"],"explanation");
    let calls=f.calls.lock();assert_eq!(calls.len(),3);assert!(calls[1]["input"].to_string().contains("Do not recolor"));assert_eq!(calls[1]["model"],"gpt-6-astra");assert_eq!(calls[1]["reasoning"]["effort"],"low");assert_eq!(job.deadline_at,None);
}
#[tokio::test]async fn final_response_cannot_complete_over_a_saved_unanswered_update(){
    let f=fixture("final").await;let job=f.state.agent.start_lab_session(f.state.clone(),f.project,f.request.clone(),None).unwrap();arrived(&f).await;
    let request=update("Also explain the uncertainty, while preserving the original objective.");receive(&f.state,job.id,request.clone()).unwrap();f.release.notify_one();
    let completed=finished(&f,job.id).await;assert_eq!(completed.state,"completed");assert_eq!(f.calls.lock().len(),3);assert!(completed.events.iter().any(|event|event.kind=="continuing_for_update"));assert_eq!(completed.events.iter().filter(|event|event.kind=="completed").count(),1);
    assert_eq!(completed.result["deliverable"]["request_id"],json!(request.request_id));assert_eq!(completed.result["deliverable"]["output_intent"],"explanation");
    let messages=f.state.database.list_messages(f.project,20).unwrap();let old=messages.iter().find(|message|message.content=="Response under the earlier assumptions.").unwrap();assert_eq!(old.metadata["phase"],"progress");assert_eq!(messages.iter().filter(|message|message.id==request.request_id).count(),1);
    receive(&f.state,job.id,request).unwrap();assert!(receive(&f.state,job.id,update("A later question")).unwrap_err().downcast_ref::<SessionFinished>().is_some());assert_eq!(f.calls.lock().len(),3);
}
#[tokio::test]async fn solver_wait_yields_to_an_update_without_rewriting_or_stopping_the_solver(){
    let f=fixture("wait").await;let job=f.state.agent.start_lab_session(f.state.clone(),f.project,f.request.clone(),None).unwrap();arrived(&f).await;f.release.notify_one();
    tokio::time::timeout(Duration::from_secs(4),async{while !f.state.laboratory.get(job.id).unwrap().events.iter().any(|event|event.kind=="waiting_for_solver"){tokio::time::sleep(Duration::from_millis(15)).await;}}).await.unwrap();
    receive(&f.state,job.id,update("While it runs, explain the dimensional-analysis assumptions.")).unwrap();let completed=finished(&f,job.id).await;assert_eq!(completed.state,"completed");
    let source=f.state.laboratory.get(f.source).unwrap();assert_eq!(source.state,"queued");assert_eq!(source.input["parameters"]["original"],true);assert_eq!(f.calls.lock().len(),3);
}
#[tokio::test]async fn message_identity_conflicts_roll_back_the_update_receipt(){
    let f=fixture("final").await;let parent=parent(&f);let original=ConversationMessage::new(f.project,ConversationRole::Assistant,MessageKind::Chat,"Retained earlier answer");f.state.database.put_message(&original).unwrap();
    assert!(receive(&f.state,parent.id,SteeringRequest{request_id:original.id,content:"Replace the old answer".into(),attachments:vec![],output_intent:None}).is_err());
    assert!(!f.state.laboratory.get(parent.id).unwrap().events.iter().any(|event|event.kind=="steering_received"));assert_eq!(f.state.database.list_messages(f.project,20).unwrap()[0].content,original.content);
}
