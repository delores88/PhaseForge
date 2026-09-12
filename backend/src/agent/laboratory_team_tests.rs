use super::*;
use axum::{routing::post,Json,Router};
use parking_lot::Mutex;
use crate::{config::AppConfig,compute::{HardwareManager,Scheduler},domain::{CreateProjectRequest,ProviderStatus,ResearchProject},persistence::Database};

struct Fixture{state:Arc<AppState>,parent:LabJob,token:CancellationToken,calls:Arc<Mutex<Vec<Value>>>,server:tokio::task::JoinHandle<()>,_temp:tempfile::TempDir}
impl Drop for Fixture{fn drop(&mut self){self.token.cancel();self.server.abort();}}
async fn fixture(mode:&'static str)->Fixture{
    let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().into(),gpu_enabled:false,..Default::default()};
    let database=Database::open(&config.database_path()).unwrap();let hardware=HardwareManager::discover(&config).await.unwrap();
    let scheduler=Scheduler::new(database.clone(),hardware).unwrap();let mut agent=AgentService::new(database.clone(),scheduler.clone()).unwrap();agent.test_key=Some("isolated-specialist-test-key".into());
    let calls=Arc::new(Mutex::new(vec![]));let captured=calls.clone();
    let app=Router::new().route("/v1/responses",post(move |Json(body):Json<Value>|{
        let captured=captured.clone();async move{
            let count={let mut rows=captured.lock();rows.push(body.clone());rows.len()};
            if mode=="slow"{tokio::time::sleep(Duration::from_secs(10)).await;}
            let output=if mode=="loop"{json!([{"type":"function_call","call_id":format!("remember-{count}"),"name":"remember","arguments":"{\"goal\":\"Check evidence\",\"constraints\":[],\"evidence\":[],\"next_actions\":[]}"}])}
            else{json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Scoped specialist analysis completed. This is advisory, not an empirical result."}]}])};
            Json(json!({"id":format!("specialist-{count}"),"status":"completed","output":output,"usage":{"input_tokens":11,"output_tokens":7}}))
        }
    }));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base_url=format!("http://{}/v1",listener.local_addr().unwrap());
    let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    database.put_provider_status(&ProviderStatus{provider:ProviderKind::OpenAi,configured:true,key_configured:true,model_configured:true,model:"gpt-4.1".into(),base_url}).unwrap();
    let project=ResearchProject::new(CreateProjectRequest{name:Some("Specialist fixture".into()),question:"Test scoped specialist orchestration".into()});database.put_project(&project).unwrap();
    let discovery=crate::discovery::DiscoveryService::new(database.clone(),scheduler.clone(),agent.clone()).unwrap();let assurance=crate::assurance::AssuranceService::new(database.clone()).unwrap();let laboratory=crate::laboratory::LaboratoryService::new(database.clone(),config.clone()).unwrap();
    let state=Arc::new(AppState{config,database,scheduler,agent,discovery,assurance,laboratory,started_at:std::time::Instant::now(),telemetry:crate::compute::telemetry::Telemetry::start()});
    let request=SessionRequest{content:"Analyze the assigned numerical evidence.".into(),provider:Some(ProviderKind::OpenAi),model:Some("gpt-6-astra".into()),reasoning_effort:Some("low".into()),time_limit_seconds:Some(300),..Default::default()};
    let parent=state.laboratory.create(Uuid::new_v4(),project.id,None,"session","Parent fixture",serde_json::to_value(request).unwrap(),Some(Utc::now()+chrono::Duration::seconds(30))).unwrap();
    let token=state.laboratory.acquire(parent.id).unwrap();
    Fixture{state,parent,token,calls,server,_temp:temp}
}
fn args(task:&str,evidence:&[Uuid])->Value{json!({"task":task,"evidence_job_ids":evidence})}
fn evidence(f:&Fixture)->LabJob{f.state.laboratory.create(Uuid::new_v4(),f.parent.project_id,Some(f.parent.id),"solver","Synthetic software-test evidence",json!({}),f.parent.deadline_at).unwrap()}
async fn stopped(f:&Fixture,id:Uuid)->LabJob{
    stopped_within(f,id,Duration::from_secs(6)).await
}
async fn stopped_within(f:&Fixture,id:Uuid,wait:Duration)->LabJob{
    tokio::time::timeout(wait,async{loop{let job=f.state.laboratory.get(id).unwrap();if !job.active()&&!f.state.laboratory.executing(id){return job;}tokio::time::sleep(Duration::from_millis(20)).await;}}).await.unwrap_or_else(|_|{
        let child=f.state.laboratory.get(id);
        let retained=match child{
            Ok(job)=>json!({"state":job.state,"deadline_at":job.deadline_at,"error":job.error,"recent_event_kinds":job.events.iter().rev().take(12).map(|event|event.kind.as_str()).collect::<Vec<_>>()}),
            Err(error)=>json!({"read_error":error.to_string()}),
        };
        panic!("Specialist did not stop within {wait:?}: {}",json!({"child_id":id,"child":retained,"executing":f.state.laboratory.executing(id),"http_call_count":f.calls.lock().len(),"parent_deadline_at":f.parent.deadline_at}));
    })
}
fn child_id(result:&Value)->Uuid{Uuid::parse_str(result["child_job_id"].as_str().unwrap()).unwrap()}

#[tokio::test]
async fn inspecting_output_exhausted_specialist_never_authorizes_another_paid_attempt(){
    let f=fixture("answer").await;let child=f.state.laboratory.create_specialist(Uuid::new_v4(),f.parent.id,"Review existing evidence",vec![]).unwrap();
    let mut journal=Journal{items:vec![initial_context(&child)],..Default::default()};let path=f.state.laboratory.directory(child.id).join("journal.json");
    super::super::output_recovery::retain(child.id,Uuid::new_v4(),&json!({"output":[]}),0,12000,&path,&mut journal).unwrap();
    f.state.laboratory.update(child.id,|job|{job.state="paused".into();job.error=Some("Output cap exhausted".into());}).unwrap();let original=std::fs::read(&path).unwrap();
    for _ in 0..3{let result=inspect(&f.state.agent,f.state.clone(),&f.parent,&json!({"child_job_id":child.id}),&f.token).await.unwrap();assert_eq!(result["state"],"paused");}
    assert_eq!(std::fs::read(&path).unwrap(),original);assert!(f.calls.lock().is_empty());
    f.state.agent.resume_lab_session(f.state.clone(),child.id).unwrap();let completed=stopped(&f,child.id).await;assert_eq!(completed.state,"completed");assert_eq!(f.calls.lock().len(),1);
}

#[tokio::test]
async fn delegate_runs_a_real_child_with_exact_model_budget_and_durable_handoff(){
    let f=fixture("answer").await;let source=evidence(&f);let target=Uuid::new_v4();let assignment=args("Check the dimensions in this saved evidence",&[source.id]);
    let created=delegate(&f.state.agent,f.state.clone(),&f.parent,&assignment,target,&f.token).unwrap();let id=child_id(&created);
    let result=inspect(&f.state.agent,f.state.clone(),&f.parent,&json!({"child_job_id":id}),&f.token).await.unwrap();
    assert_eq!(result["state"],"completed");assert!(result["result"]["answer"].as_str().unwrap().contains("Scoped specialist"));
    let child=stopped(&f,id).await;assert_eq!(child.kind,"specialist");assert_eq!(child.parent_id,Some(f.parent.id));assert_eq!(child.deadline_at,f.parent.deadline_at);
    assert_eq!(child.input["model"],"gpt-6-astra");assert_eq!(child.input["reasoning_effort"],"low");assert_eq!(child.input["delegation"]["evidence_job_ids"],json!([source.id]));
    let parent=f.state.laboratory.get(f.parent.id).unwrap();assert!(parent.events.iter().any(|e|e.kind=="specialist_delegated"&&e.data["child_job_id"]==json!(id)));assert!(parent.events.iter().any(|e|e.kind=="specialist_result"&&e.data["child_job_id"]==json!(id)));
    let calls=f.calls.lock();assert_eq!(calls.len(),1,"A answered specialist task stops early");assert_eq!(calls[0]["model"],"gpt-6-astra");assert_eq!(calls[0]["reasoning"]["effort"],"low");
    assert!(calls[0]["input"].to_string().contains(&source.id.to_string()));assert!(!calls[0]["tools"].as_array().unwrap().iter().any(|tool|tool["name"]=="launch_experiment"));
}
#[tokio::test]
async fn repeated_assignments_and_recovered_target_ids_do_not_generate_again(){
    let f=fixture("answer").await;let target=Uuid::new_v4();let assignment=args("Inspect the declared modeling assumptions",&[]);
    let first=delegate(&f.state.agent,f.state.clone(),&f.parent,&assignment,target,&f.token).unwrap();stopped(&f,child_id(&first)).await;
    let exact=delegate(&f.state.agent,f.state.clone(),&f.parent,&assignment,target,&f.token).unwrap();
    let repeated=delegate(&f.state.agent,f.state.clone(),&f.parent,&assignment,Uuid::new_v4(),&f.token).unwrap();
    assert_eq!(first["child_job_id"],exact["child_job_id"]);assert_eq!(first["child_job_id"],repeated["child_job_id"]);assert_eq!(f.calls.lock().len(),1);
    assert!(delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Changed assignment",&[]),target,&f.token).is_err());
}
#[tokio::test]
async fn evidence_ownership_and_read_only_scope_are_enforced_before_work(){
    let f=fixture("answer").await;let other=ResearchProject::new(CreateProjectRequest{name:None,question:"Other project".into()});f.state.database.put_project(&other).unwrap();
    let foreign=f.state.laboratory.create(Uuid::new_v4(),other.id,None,"solver","Foreign",json!({}),None).unwrap();
    assert!(delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Read foreign data",&[foreign.id]),Uuid::new_v4(),&f.token).is_err());assert!(f.calls.lock().is_empty());
    let source=evidence(&f);let child=f.state.laboratory.create_specialist(Uuid::new_v4(),f.parent.id,"Only this evidence",vec![source.id]).unwrap();
    ensure_scope(&child,"read_artifact",&json!({"job_id":source.id})).unwrap();
    ensure_scope(&child,"read_artifact",&json!({"job_id":child.id,"path":"journal.json"})).unwrap();
    assert!(ensure_scope(&child,"read_artifact",&json!({"job_id":foreign.id})).is_err());assert!(ensure_scope(&child,"launch_experiment",&json!({})).is_err());assert!(ensure_scope(&child,"recall",&json!({})).is_err());
    assert!(inspect(&f.state.agent,f.state.clone(),&f.parent,&json!({"child_job_id":foreign.id}),&f.token).await.is_err());
}
#[tokio::test]
async fn atomic_tree_limits_bound_depth_fanout_and_total_children(){
    let f=fixture("answer").await;let service=&f.state.laboratory;
    let children=(0..3).map(|i|service.create_specialist(Uuid::new_v4(),f.parent.id,&format!("Root task {i}"),vec![]).unwrap()).collect::<Vec<_>>();
    assert!(service.create_specialist(Uuid::new_v4(),f.parent.id,"Fourth root child",vec![]).is_err());
    let grand=service.create_specialist(Uuid::new_v4(),children[0].id,"Grandchild task",vec![]).unwrap();
    assert!(service.create_specialist(Uuid::new_v4(),grand.id,"Third nesting level",vec![]).is_err());
    for i in 0..3{service.create_specialist(Uuid::new_v4(),children[1].id,&format!("Other branch {i}"),vec![]).unwrap();}
    let left=service.clone();let right=service.clone();let id=children[2].id;
    let first=std::thread::spawn(move||left.create_specialist(Uuid::new_v4(),id,"Concurrent A",vec![]));let second=std::thread::spawn(move||right.create_specialist(Uuid::new_v4(),id,"Concurrent B",vec![]));
    assert_eq!([first.join().unwrap(),second.join().unwrap()].iter().filter(|r|r.is_ok()).count(),1);
    assert_eq!(service.list(Some(f.parent.project_id)).unwrap().iter().filter(|j|j.kind=="specialist").count(),8);
}
#[tokio::test]
async fn cancellation_is_inherited_and_cannot_create_an_orphan_after_parent_stop(){
    let f=fixture("slow").await;let created=delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Check cancellation",&[]),Uuid::new_v4(),&f.token).unwrap();let id=child_id(&created);
    tokio::time::timeout(Duration::from_secs(3),async{while f.calls.lock().is_empty(){tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();
    f.state.laboratory.stop(f.parent.id,"cancelled").unwrap();assert!(f.token.is_cancelled());
    assert_eq!(stopped(&f,id).await.state,"cancelled");
    let count=f.state.laboratory.list(Some(f.parent.project_id)).unwrap().len();assert!(delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Late child",&[]),Uuid::new_v4(),&f.token).is_err());assert_eq!(f.state.laboratory.list(Some(f.parent.project_id)).unwrap().len(),count);
}
#[tokio::test]
async fn contention_waits_locally_and_off_and_absolute_deadlines_are_preserved(){
    let f=fixture("answer").await;
    f.state.laboratory.update(f.parent.id,|job|job.deadline_at=None).unwrap();
    let reserved=f.state.agent.usage.begin(f.parent.id,Some(f.parent.project_id),ProviderKind::OpenAi,"gpt-6-astra","occupied-test-slot",0,10,128).unwrap();
    let created=delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Wait for the configured model slot",&[]),Uuid::new_v4(),&f.token).unwrap();let id=child_id(&created);
    tokio::time::sleep(Duration::from_millis(200)).await;assert!(f.calls.lock().is_empty());assert_eq!(f.state.laboratory.get(id).unwrap().state,"waiting");assert_eq!(f.state.laboratory.get(id).unwrap().deadline_at,None);
    f.state.agent.usage.finish(reserved.id,"completed",None,None).unwrap();assert_eq!(stopped(&f,id).await.state,"completed");assert_eq!(f.calls.lock().len(),1);
    f.state.laboratory.update(f.parent.id,|job|job.deadline_at=Some(Utc::now()-chrono::Duration::seconds(1))).unwrap();
    assert!(f.state.laboratory.create_specialist(Uuid::new_v4(),f.parent.id,"Expired assignment",vec![]).is_err());
}
#[tokio::test]
async fn a_specialist_cannot_loop_past_its_model_turn_budget(){
    let f=fixture("loop").await;let created=delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Bound an unfinished analysis",&[]),Uuid::new_v4(),&f.token).unwrap();
    // Eight localhost HTTP rounds also persist requests, results and journal
    // replacements. Use this fixture's existing absolute work deadline, rather
    // than the six-second stop-latency bound retained by cancellation tests.
    let remaining=(f.parent.deadline_at.expect("Loop fixture needs its existing parent deadline")-Utc::now()).to_std().unwrap_or_default();
    let child=stopped_within(&f,child_id(&created),remaining).await;
    assert_eq!(child.deadline_at,f.parent.deadline_at);
    assert_eq!(child.state,"paused");assert!(child.error.unwrap().contains("eight-turn limit"));assert_eq!(f.calls.lock().len(),8);assert!(child.result.is_null());
}

#[tokio::test]
async fn resumed_specialist_requires_live_parent_and_recovers_outbox_with_inherited_budget(){
    let f=fixture("answer").await;
    let child=f.state.laboratory.create_specialist(Uuid::new_v4(),f.parent.id,"Recover the saved specialist answer",vec![]).unwrap();
    f.state.laboratory.update(child.id,|job|{job.state="paused".into();job.deadline_at=Some(Utc::now()-chrono::Duration::seconds(1));}).unwrap();
    f.state.laboratory.update(f.parent.id,|job|job.state="paused".into()).unwrap();
    assert!(f.state.agent.resume_lab_session(f.state.clone(),child.id).unwrap_err().to_string().contains("parent"));
    assert!(f.calls.lock().is_empty());
    // The parent was explicitly continued with Off. Its child's old elapsed
    // deadline must not override that snapshot or become a fresh numeric timer.
    f.state.laboratory.update(f.parent.id,|job|{job.state="running".into();job.deadline_at=None;}).unwrap();
    let row=f.state.agent.usage.begin(child.id,Some(child.project_id),ProviderKind::OpenAi,"gpt-6-astra","laboratory_tool_turn",0,10,128).unwrap();
    let mut message=ConversationMessage::new(child.project_id,ConversationRole::Assistant,MessageKind::Chat,"Recovered specialist evidence");message.id=row.id;
    let journal=Journal{delivery:Some(ResponseDelivery{usage_id:row.id,message:Some(message),result:Some(json!({"answer":"Recovered specialist evidence"})),vision:None}),..Default::default()};
    write_json(&f.state.laboratory.directory(child.id).join("journal.json"),&journal).unwrap();
    f.state.agent.resume_lab_session(f.state.clone(),child.id).unwrap();
    let result=stopped(&f,child.id).await;assert_eq!(result.state,"completed");assert_eq!(result.deadline_at,None);assert_eq!(result.result["answer"],"Recovered specialist evidence");assert!(f.calls.lock().is_empty());
    f.state.agent.resume_lab_session(f.state.clone(),child.id).unwrap();
    assert_eq!(f.state.laboratory.get(child.id).unwrap().events.iter().filter(|event|event.kind=="completed").count(),1);
    assert_eq!(f.state.database.list_messages(child.project_id,50).unwrap().iter().filter(|message|message.id==row.id).count(),1);
}

#[tokio::test]
async fn resumed_specialist_still_stops_with_its_parent(){
    let f=fixture("slow").await;
    let child=f.state.laboratory.create_specialist(Uuid::new_v4(),f.parent.id,"Continue under the parent token",vec![]).unwrap();
    f.state.laboratory.update(child.id,|job|job.state="paused".into()).unwrap();
    f.state.agent.resume_lab_session(f.state.clone(),child.id).unwrap();
    tokio::time::timeout(Duration::from_secs(3),async{while f.calls.lock().is_empty(){tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();
    f.state.laboratory.stop(f.parent.id,"paused").unwrap();
    let result=stopped(&f,child.id).await;
    assert_eq!(result.state,"paused");assert_eq!(result.deadline_at,f.parent.deadline_at);assert_eq!(f.calls.lock().len(),1);
}

#[tokio::test]
async fn specialist_admission_keeps_existing_account_usage_limits(){
    let f=fixture("answer").await;
    let mut settings=f.state.agent.usage.settings().unwrap();settings.token_limit=Some(1);f.state.agent.usage.save_settings(settings).unwrap();
    let created=delegate(&f.state.agent,f.state.clone(),&f.parent,&args("Respect the account token budget",&[]),Uuid::new_v4(),&f.token).unwrap();
    let child=stopped(&f,child_id(&created)).await;
    assert_eq!(child.state,"paused");assert!(child.error.unwrap().contains("Token admission limit"));assert!(f.calls.lock().is_empty());assert!(f.state.database.list_usage_records().unwrap().is_empty());
}
