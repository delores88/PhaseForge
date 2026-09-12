//! Durable action progress, independent of model prose and compacted context.
use super::*;
use sha2::{Digest,Sha256};

const STALLED_ROUND_LIMIT:u32=3;
const RECENT_EVIDENCE_LIMIT:usize=128;

#[derive(Default,Clone,Serialize,Deserialize)]
pub(super) struct Progress {
    request_id:Option<Uuid>,
    round:Option<u32>,
    finished_round:Option<u32>,
    calls:Vec<String>,
    new_evidence:bool,
    #[serde(default)] failed_keys:Vec<String>,
    #[serde(default)] unresolved_failures:Vec<(u32,Vec<String>)>,
    #[serde(default)] recent_evidence:Vec<String>,
    consecutive_stalled_rounds:u32,
    paused:Option<Value>,
}

fn action(id:&str,label:&str,description:&str)->Value{json!({"id":id,"label":label,"description":description})}

pub(super) fn attention(kind:&str,reason:&str,requires_user_change:bool,evidence:Vec<Uuid>)->Value{
    let mut actions=vec![action("revise_request","Revise the request","Clarify the objective, change supported inputs, or explicitly choose a different model."),
        action("inspect_capabilities","Inspect capabilities","Check the installed engine's scope and current resource requirements."),
        action("inspect_evidence","Inspect saved evidence","Review original tool failures, partial numerical output and source identities before choosing another attempt.")];
    if !requires_user_change{actions.push(action("resume","Try a bounded continuation","Explicitly allow another limited repair attempt using saved receipts. This does not reset the timer or rerun completed tools."));}
    json!({"schema":"phaseforge.agent-attention.v1","status":"needs_direction","kind":kind,"reason":reason,
        "requires_user_change":requires_user_change,"actions":actions,"evidence_job_ids":evidence,
        "tool_call_ids":[],"consecutive_stalled_rounds":0})
}

// These fields describe polling/bookkeeping, not a changed measurement. Physical
// time, numerical progress, artifact hashes and job state are deliberately kept.
fn substantive(value:&Value)->Value{
    match value {
        Value::Object(map)=>Value::Object(map.iter().filter(|(key,_)|!matches!(key.as_str(),
            "created_at"|"updated_at"|"seen_at"|"started_at"|"finished_at"|"completed_at"|
            "elapsed_seconds"|"elapsed_ms"|"remaining_seconds"|"event_count"|"events"))
            .map(|(key,value)|(key.clone(),substantive(value))).collect()),
        Value::Array(items)=>Value::Array(items.iter().map(substantive).collect()),
        _=>value.clone(),
    }
}

fn failed_output(output:&Value)->bool{
    output.get("error").is_some_and(|value|!value.is_null())
        || failed_execution(output)
}
fn failed_execution(output:&Value)->bool{[&output["job"]["state"],&output["state"]].iter().any(|state|matches!(state.as_str(),Some("failed"|"timed_out"|"cancelled")))}

fn execution_key(output:&Value)->String{
    if output["execution_identity"].is_object(){return format!("execution:{}",output["execution_identity"]);}
    let job=if output["job"].is_object(){&output["job"]}else{output};
    // Ignore attempt UUIDs, but do not mistake a changed physical protocol for
    // successful recovery of the original failed calculation.
    format!("execution:{}",json!({"kind":job["kind"],"engine":job["input"]["engine"],
        "parameters":job["input"]["parameters"],"code_sha256":job["input"]["code_sha256"],
        "sources":job["input"]["sources"]}))
}

pub(super) fn execution_identity(job:&LabJob)->Value{
    let protocol=json!({"engine":job.input["engine"],"parameters":job.input["parameters"],
        "code":job.input["code"],"inputs":job.input["inputs"],"sources":job.input["sources"],
        "cases":job.input["cases"],"protocol":job.input["protocol"],"study_job_id":job.input["study_job_id"]});
    json!({"kind":job.kind,"engine":job.input["engine"],"protocol_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&protocol).expect("Saved JSON input")))})
}

fn tool_key(name:&str,args:&Value)->String{
    // A successful read of another job/file is not recovery of this failure.
    // Invalid execution parameters may be repaired within the same engine;
    // completed scientific execution has the stricter protocol key above.
    format!("tool:{}",json!({"name":name,"job_id":args["job_id"],"source_job_id":args["source_job_id"],
        "study_job_id":args["study_job_id"],"path":args["path"],"engine":args["engine"]}))
}

impl Progress {
    pub(super) fn steer(&mut self,request_id:Option<Uuid>){
        if self.request_id!=request_id{*self=Self{request_id,..Default::default()};}
    }
    pub(super) fn resume(&mut self){
        self.paused=None;self.consecutive_stalled_rounds=0;self.unresolved_failures.clear();self.failed_keys.clear();
        // A completed round is never replayed as fresh work after Resume.
    }
    pub(super) fn record(&mut self,round:u32,call_id:&str,name:&str,args:&Value,output:&Value,request_id:Option<Uuid>){
        self.steer(request_id);
        if self.finished_round.is_some_and(|done|round<=done)||self.paused.is_some(){return;}
        if self.round!=Some(round){self.round=Some(round);self.calls.clear();self.new_evidence=false;self.failed_keys.clear();}
        if self.calls.iter().any(|id|id==call_id){return;}
        self.calls.push(call_id.into());
        let tool_key=tool_key(name,args);
        if failed_output(output){
            let key=if failed_execution(output){execution_key(output)}else{tool_key};
            if !self.failed_keys.contains(&key){self.failed_keys.push(key);}
            return;
        }
        let mut recovered=vec![tool_key];
        if name=="inspect_result"&&(output["job"]["state"]=="completed"||output["state"]=="completed"){
            recovered.push(execution_key(output));
        }
        self.failed_keys.retain(|key|!recovered.contains(key));
        for (_,keys) in &mut self.unresolved_failures{keys.retain(|key|!recovered.contains(key));}
        self.unresolved_failures.retain(|(_,keys)|!keys.is_empty());
        if name=="remember"||name=="set_output_intent"&&output["already_resolved"]==true{return;}
        let mut retained=substantive(output);
        // Re-reading an unchanged catalog cannot manufacture progress merely
        // because another application's live RAM usage moved between polls.
        if name=="lab_catalog"{if let Some(object)=retained.as_object_mut(){object.remove("resource_plan");}}
        let bytes=serde_json::to_vec(&json!({"name":name,"arguments":args,"output":retained})).expect("JSON tool receipt");
        let fingerprint=format!("{:x}",Sha256::digest(bytes));
        if !self.recent_evidence.contains(&fingerprint){
            self.new_evidence=true;self.recent_evidence.push(fingerprint);
            if self.recent_evidence.len()>RECENT_EVIDENCE_LIMIT{self.recent_evidence.remove(0);}
        }
    }
    pub(super) fn finish_round(&mut self,round:u32)->Option<Value>{
        if self.paused.is_some(){return self.paused.clone();}
        if self.round!=Some(round)||self.finished_round.is_some_and(|done|round<=done)||self.calls.is_empty(){return None;}
        self.finished_round=Some(round);
        if !self.failed_keys.is_empty(){self.unresolved_failures.push((round,self.failed_keys.clone()));}
        self.consecutive_stalled_rounds=if self.new_evidence{0}else{self.consecutive_stalled_rounds.saturating_add(1)};
        let errors=self.unresolved_failures.len()>=STALLED_ROUND_LIMIT as usize;
        if !errors&&self.consecutive_stalled_rounds<STALLED_ROUND_LIMIT{return None;}
        let mut value=attention(if errors{"repeated_tool_errors"}else{"no_progress"},
            if errors{"Three tool rounds reported failures without a matching successful recovery. The agent paused before another model call. Unrelated reads and queued attempts do not clear those failures. Inspect the saved errors, revise the request or explicitly authorize a bounded continuation."}
            else{"The last three tool rounds produced no new usable evidence. The agent paused before another model call. Inspect saved results, clarify the next step or explicitly authorize a bounded continuation."},false,vec![]);
        value["request_id"]=json!(self.request_id);value["round"]=json!(round);
        value["tool_call_ids"]=json!(self.calls.iter().take(32).collect::<Vec<_>>());
        value["consecutive_stalled_rounds"]=json!(self.consecutive_stalled_rounds);
        value["unresolved_failure_rounds"]=json!(self.unresolved_failures.len());
        value["failure_policy"]=json!("A successful retry of the same tool and target clears its input/tool error. Failed child execution requires completed evidence from the same engine and physical parameters; unrelated reads, changed protocols and queued attempts cannot clear it.");
        self.paused=Some(value.clone());Some(value)
    }
}

pub(super) fn reconcile(state:&AppState,id:Uuid,journal:&Journal)->anyhow::Result<()>{
    let Some(value)=journal.progress.paused.as_ref() else{return Ok(())};
    let job=state.laboratory.get(id)?;
    if steering::has_pending(&job,journal){return Ok(());}
    state.laboratory.update(id,|job|{
        if !job.result.is_object(){job.result=json!({});}
        job.result["attention"]=value.clone();
        if !job.events.iter().any(|event|event.kind=="agent_needs_direction"&&event.data==*value){
            job.event("agent_needs_direction",value["reason"].as_str().unwrap_or("The agent needs direction before another model call."),value.clone());
        }
    })?;
    bail!("{}",value["reason"].as_str().unwrap_or("The agent needs direction before another model call."));
}

pub(super) fn authorize_resume(state:&AppState,id:Uuid)->anyhow::Result<()>{
    let path=state.laboratory.directory(id).join("journal.json");
    if !path.is_file(){return Ok(());}
    let mut journal:Journal=serde_json::from_slice(&std::fs::read(&path)?)?;
    if journal.progress.paused.is_some(){
        journal.progress.resume();
        journal.items.push(json!({"role":"user","content":"The user explicitly resumed the paused task for a bounded continuation. Inspect the retained failures and change the approach when justified. Do not repeat completed tools or treat earlier uncertain execution as unexecuted. Original model, usage limits and deadline still apply."}));
        write_json(&path,&journal)?;
    }
    state.laboratory.update(id,|job|{if let Some(result)=job.result.as_object_mut(){result.remove("attention");}})?;
    Ok(())
}

pub(super) fn validate_resume(state:&AppState,id:Uuid)->anyhow::Result<()>{
    let job=state.laboratory.get(id)?;
    if !matches!(job.kind.as_str(),"session"|"specialist"){return Ok(());}
    let path=state.laboratory.directory(id).join("journal.json");
    if !path.is_file(){return Ok(());}
    let journal:Journal=serde_json::from_slice(&std::fs::read(&path)?)?;
    anyhow::ensure!(journal.output_intent.as_ref().is_none_or(|intent|intent.capability_gap.is_none())||steering::has_pending(&job,&journal),
        "This request has a saved capability gap. Revise the request in chat or inspect installed capabilities and saved evidence before continuing. Unchanged Resume cannot resolve the gap; no model call or new budget was started.");
    Ok(())
}

pub(super) fn resume_after_wait(state:&AppState,id:Uuid,token:&CancellationToken)->anyhow::Result<()>{
    state.laboratory.update(id,|job|{
        if job.active()&&!token.is_cancelled(){job.state="running".into();}
    })?;
    Ok(())
}

pub(super) fn recover_children(state:&AppState,parent:&LabJob,journal:&mut Journal,path:&std::path::Path,token:&CancellationToken)->anyhow::Result<()>{
    let children=state.laboratory.list(Some(parent.project_id))?.into_iter().filter(|child|child.parent_id==Some(parent.id)&&child.kind=="solver"&&child.state=="paused").collect::<Vec<_>>();
    for child in children {
        anyhow::ensure!(!token.is_cancelled(),"Parent stopped before child recovery");
        let outcome=if steering::has_pending(&state.laboratory.get(parent.id)?,journal){
            Err(anyhow::anyhow!("A saved user update must be interpreted before restarting earlier numerical work. The child remains paused with its original evidence."))
        }else if crate::laboratory::nr_engines::supports(child.input["engine"].as_str().unwrap_or("")){
            Err(anyhow::anyhow!("This engine does not support checkpoint continuation. Its partial native output remains available for recovery and inspection; a new numerical attempt requires an explicit new request."))
        }else{state.laboratory.resume_solver(child.id,state.laboratory.get(parent.id)?.deadline_at)};
        if let Err(error)=outcome{
            anyhow::ensure!(!token.is_cancelled(),"Parent stopped during child recovery");
            let reason=format!("Saved child {} was not resumed: {error:#}",child.id);
            let mut notice=attention("child_recovery",&reason,false,vec![child.id]);
            notice["child_state"]=json!(child.state);notice["engine"]=child.input["engine"].clone();
            notice["automatic_relaunch"]=json!(false);
            if !journal.child_recovery.iter().any(|prior|prior==&notice){
                journal.child_recovery.push(notice.clone());
                journal.items.push(json!({"role":"user","content":format!("Application recovery evidence, not a new instruction: {}. Do not relaunch this child automatically or silently substitute physics. Inspect its retained output and explain actionable options within the user's current request.",notice)}));
                write_json(path,journal)?;
            }
            state.laboratory.update(parent.id,|job|{
                if !job.result.is_object(){job.result=json!({});}job.result["child_recovery"]=json!(journal.child_recovery);
                if !job.events.iter().any(|event|event.kind=="child_recovery_attention"&&event.data==notice){job.event("child_recovery_attention",&reason,notice.clone());}
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path="laboratory_progress_tests.rs"]
mod tests;

#[cfg(test)]mod integration_tests{
    use super::*;
    use axum::{Router,Json,routing::post};
    use parking_lot::Mutex;
    use crate::domain::ProviderStatus;

    fn tool(call:usize,name:&str,args:Value)->Value{json!({"id":format!("response-{call}"),"status":"completed",
        "output":[{"type":"function_call","call_id":format!("tool-{call}"),"name":name,"arguments":args.to_string(),"status":"completed"}],
        "usage":{"input_tokens":10,"output_tokens":10}})}
    fn answer()->Value{json!({"id":"final","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"The saved child outcomes and next choices are retained; no numerical computation was repeated."}]}],"usage":{"input_tokens":10,"output_tokens":10}})}
    async fn provider(state:&AppState,responses:Vec<Value>)->(Arc<Mutex<Vec<Value>>>,tokio::task::JoinHandle<()>){
        let calls=Arc::new(Mutex::new(vec![]));let recorded=calls.clone();
        let app=Router::new().route("/v1/responses",post(move|Json(body):Json<Value>|{
            let mut calls=recorded.lock();let index=calls.len();calls.push(body);
            let response=responses.get(index).cloned().unwrap_or_else(answer);
            async{Json(response)}
        }));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url=format!("http://{}/v1",listener.local_addr().unwrap());
        state.database.put_provider_status(&ProviderStatus{provider:ProviderKind::OpenAi,configured:true,key_configured:true,model_configured:true,model:"gpt-6-astra".into(),base_url}).unwrap();
        let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});(calls,server)
    }

    #[tokio::test]async fn three_failed_rounds_with_off_stop_before_a_fourth_http_call_and_survive_restart(){
        let f=super::super::receipt_recovery_tests::fixture().await;
        assert!(f.job.deadline_at.is_none());
        let (calls,server)=provider(&f.state,(0..5).map(|i|tool(i,"unsupported_fixture_tool",json!({}))).collect()).await;
        let error=f.state.agent.run_lab_session(f.state.clone(),f.job.id,&CancellationToken::new()).await.unwrap_err();
        assert!(error.to_string().contains("Three tool rounds"));assert_eq!(calls.lock().len(),3);
        let path=f.state.laboratory.directory(f.job.id).join("journal.json");
        let before=std::fs::read(&path).unwrap();let journal:Journal=serde_json::from_slice(&before).unwrap();
        assert_eq!(journal.tools.len(),3);assert!(journal.pending.is_none());
        assert_eq!(f.state.laboratory.get(f.job.id).unwrap().result["attention"]["kind"],"repeated_tool_errors");
        assert!(f.state.agent.run_lab_session(f.state.clone(),f.job.id,&CancellationToken::new()).await.is_err());
        assert_eq!(calls.lock().len(),3);assert_eq!(std::fs::read(&path).unwrap(),before);
        assert_eq!(f.state.database.list_usage_records().unwrap().len(),3);server.abort();
    }

    #[tokio::test]async fn progressing_original_artifact_reads_with_off_pass_the_stall_limit_and_finish(){
        let f=super::super::receipt_recovery_tests::fixture().await;let root=f.state.laboratory.directory(f.job.id);
        let original=(0..4096).map(|i|b'a'+(i%26)as u8).collect::<Vec<_>>();std::fs::write(root.join("retained.txt"),&original).unwrap();
        let responses=(0..8).map(|i|tool(i,"read_artifact",json!({"job_id":f.job.id,"path":"retained.txt","offset":i*256,"max_bytes":256}))).chain(std::iter::once(answer())).collect();
        let(calls,server)=provider(&f.state,responses).await;
        f.state.agent.run_lab_session(f.state.clone(),f.job.id,&CancellationToken::new()).await.unwrap();
        assert_eq!(calls.lock().len(),9);assert_eq!(f.state.laboratory.get(f.job.id).unwrap().state,"completed");
        assert_eq!(std::fs::read(root.join("retained.txt")).unwrap(),original);server.abort();
    }

    #[tokio::test]async fn unrelated_fresh_reads_do_not_buy_a_fourth_failed_provider_round(){
        let f=super::super::receipt_recovery_tests::fixture().await;let root=f.state.laboratory.directory(f.job.id);
        let original=vec![b'x';4096];std::fs::write(root.join("retained.txt"),&original).unwrap();
        let responses=(0..5).map(|i|{
            let mut failed=tool(i*2,"unsupported_fixture_tool",json!({}));
            let read=tool(i*2+1,"read_artifact",json!({"job_id":f.job.id,"path":"retained.txt","offset":i*256,"max_bytes":256}));
            failed["output"].as_array_mut().unwrap().extend(read["output"].as_array().unwrap().clone());failed
        }).collect();
        let(calls,server)=provider(&f.state,responses).await;
        assert!(f.state.agent.run_lab_session(f.state.clone(),f.job.id,&CancellationToken::new()).await.is_err());
        assert_eq!(calls.lock().len(),3);
        let journal:Journal=serde_json::from_slice(&std::fs::read(root.join("journal.json")).unwrap()).unwrap();
        assert_eq!(journal.tools.len(),6);assert!(journal.pending.is_none());
        assert_eq!(f.state.laboratory.get(f.job.id).unwrap().result["attention"]["unresolved_failure_rounds"],3);
        assert_eq!(std::fs::read(root.join("retained.txt")).unwrap(),original);server.abort();
    }

    #[tokio::test]async fn parent_interprets_nonresumable_nr_and_changed_runtime_children_without_relaunch(){
        let f=super::super::receipt_recovery_tests::fixture().await;
        let mut children=vec![];
        for engine in ["athenak_two_punctures_cuda","openmm_argon"]{
            let child=f.state.laboratory.create(Uuid::new_v4(),f.job.project_id,Some(f.job.id),"solver","Retained paused child",json!({"engine":engine,"parameters":{}}),None).unwrap();
            if engine=="openmm_argon"{f.state.laboratory.event(child.id,"runtime_verified","Original runtime",json!({"kind":"science-v4","manifest_sha256":"original preserved runtime"})).unwrap();}
            f.state.laboratory.update(child.id,|job|job.state="paused".into()).unwrap();
            std::fs::write(f.state.laboratory.directory(child.id).join("partial.txt"),b"original numerical evidence").unwrap();
            children.push(f.state.laboratory.get(child.id).unwrap());
        }
        let(calls,server)=provider(&f.state,vec![tool(0,"inspect_result",json!({"job_id":children[0].id})),answer()]).await;
        f.state.agent.run_lab_session(f.state.clone(),f.job.id,&CancellationToken::new()).await.unwrap();
        assert_eq!(calls.lock().len(),2);let parent=f.state.laboratory.get(f.job.id).unwrap();assert_eq!(parent.state,"completed");
        assert_eq!(parent.result["child_recovery"].as_array().unwrap().len(),2);
        assert!(calls.lock()[0].to_string().contains("Application recovery evidence"));
        for child in children{
            assert_eq!(serde_json::to_value(f.state.laboratory.get(child.id).unwrap()).unwrap(),serde_json::to_value(&child).unwrap());
            assert!(!f.state.laboratory.executing(child.id));
            assert_eq!(std::fs::read(f.state.laboratory.directory(child.id).join("partial.txt")).unwrap(),b"original numerical evidence");
            assert!(!f.state.laboratory.directory(child.id).join("nr-request.json").exists());
        }server.abort();
    }

    #[tokio::test]async fn unchanged_gap_resume_is_read_only_but_saved_user_revision_is_allowed(){
        let f=super::super::receipt_recovery_tests::fixture().await;let mut journal=Journal::default();
        let mut request=f.request.clone();request.output_intent=Some(OutputIntent::Simulation);
        intent::initialize(&f.job,&request,&mut journal);
        let current=journal.output_intent.as_mut().unwrap();current.scope="Requested unsupported model".into();current.capability_gap=Some("This physical model is unavailable".into());current.required_capability=Some("unsupported_model".into());
        let path=f.state.laboratory.directory(f.job.id).join("journal.json");write_json(&path,&journal).unwrap();
        f.state.laboratory.update(f.job.id,|job|job.state="paused".into()).unwrap();
        let before=serde_json::to_value(f.state.laboratory.get(f.job.id).unwrap()).unwrap();let bytes=std::fs::read(&path).unwrap();
        assert!(f.state.agent.validate_lab_resume(&f.state,f.job.id).is_err());
        assert!(f.state.agent.resume_lab_session(f.state.clone(),f.job.id).is_err());
        assert_eq!(serde_json::to_value(f.state.laboratory.get(f.job.id).unwrap()).unwrap(),before);assert_eq!(std::fs::read(&path).unwrap(),bytes);
        assert_eq!(f.provider_post_count(),0);assert!(f.state.database.list_usage_records().unwrap().is_empty());
        steering::receive(&f.state,f.job.id,steering::SteeringRequest{request_id:Uuid::new_v4(),content:"Explain the saved capability gap; do not launch another experiment.".into(),attachments:vec![],output_intent:Some(OutputIntent::Explanation)}).unwrap();
        f.state.agent.validate_lab_resume(&f.state,f.job.id).unwrap();
        assert_eq!(f.provider_post_count(),0);assert!(f.state.database.list_usage_records().unwrap().is_empty());
    }

    #[tokio::test]async fn a_stop_between_wait_completion_and_state_update_is_never_reactivated(){
        let f=super::super::receipt_recovery_tests::fixture().await;
        for requested in ["paused","cancelled","timed_out"]{
            f.state.laboratory.update(f.job.id,|job|job.state="waiting".into()).unwrap();
            let token=f.state.laboratory.acquire(f.job.id).unwrap();
            // Deterministically order the user's stop before the delayed wait
            // continuation reaches its update closure.
            f.state.laboratory.stop(f.job.id,requested).unwrap();assert!(token.is_cancelled());
            resume_after_wait(&f.state,f.job.id,&token).unwrap();
            assert_eq!(f.state.laboratory.get(f.job.id).unwrap().state,requested);
            // The outer error handler's cancellation stop only affects active
            // jobs, so it must preserve the already recorded terminal choice.
            f.state.laboratory.stop(f.job.id,"cancelled").unwrap();
            assert_eq!(f.state.laboratory.get(f.job.id).unwrap().state,requested);
            f.state.laboratory.release(f.job.id);
        }
        assert_eq!(f.provider_post_count(),0);assert!(f.state.database.list_usage_records().unwrap().is_empty());
    }
}
