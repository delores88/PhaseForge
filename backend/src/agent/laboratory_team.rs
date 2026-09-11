//! Real specialist jobs, scoped evidence handoffs and shared provider admission.
use super::*;

const READ_TOOLS:&[&str]=&["lab_catalog","inspect_result","list_artifacts","read_artifact","read_project_file","profile_dataset","observe_frame","remember","delegate_specialist","inspect_specialist"];

pub(super) fn initial_context(job:&LabJob)->Value {
    json!({"role":"user","content":format!("You are a delegated scientific specialist. Complete only this concrete assignment, then return the answer and its supporting evidence IDs, limitations and unresolved questions. Your output is advisory analysis, never new empirical evidence. Stop when this assignment is answered. Use only the listed evidence jobs and your own saved artifacts; do not launch experiments, change presentation, or retrieve unrelated project conversation. Further delegation is limited and must answer a separate subquestion; inspect each child's actual result.\nYOUR JOB ID: {}\nASSIGNMENT: {}\nPARENT QUESTION: {}\nSCOPED EVIDENCE JOB IDS: {}\nLIMITS: at most eight model turns, two nesting levels, three direct children and eight specialists in the entire root session. Inherited deadline: {:?}",job.id,job.input["delegation"]["objective"],job.input["delegation"]["parent_objective"],job.input["delegation"]["evidence_job_ids"],job.deadline_at)})
}
pub(super) fn tools_for(job:&LabJob,tools:Value)->Value {
    if job.kind!="specialist"{return tools;}
    Value::Array(tools.as_array().into_iter().flatten().filter(|tool|READ_TOOLS.contains(&tool["name"].as_str().unwrap_or(""))).cloned().collect())
}
pub(super) fn ensure_scope(job:&LabJob,name:&str,args:&Value)->anyhow::Result<()> {
    if job.kind!="specialist"{return Ok(());}
    anyhow::ensure!(READ_TOOLS.contains(&name),"This specialist may only inspect its assigned evidence; it cannot execute {name}");
    // Compacted specialists can retrieve their own retained transcript and receipts.
    if name=="read_artifact"&&args["job_id"]==json!(job.id){return Ok(());}
    if ["inspect_result","list_artifacts","read_artifact","read_project_file","profile_dataset","observe_frame"].contains(&name){
        anyhow::ensure!(job.input["delegation"]["evidence_job_ids"].as_array().is_some_and(|ids|ids.contains(&args["job_id"])),"This evidence was not assigned to the specialist");
    }
    Ok(())
}
pub(super) fn ensure_turn_budget(job:&LabJob,round:u32)->anyhow::Result<()> {
    if job.kind=="specialist"{anyhow::ensure!(round<8,"Specialist reached its eight-turn limit. Completed evidence is retained; inspect its partial result before assigning more work.");}
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn reserve_call(agent:&AgentService,state:&AppState,job_id:Uuid,provider:ProviderKind,model:&str,purpose:&str,attempt:u32,input_bytes:usize,output_limit:u32,token:&CancellationToken)->anyhow::Result<crate::usage::UsageRecord>{
    let mut waiting=false;
    loop {
        anyhow::ensure!(!token.is_cancelled(),"Stopped while waiting for provider admission");
        match agent.usage.begin(job_id,Some(state.laboratory.get(job_id)?.project_id),provider,model,purpose,attempt,input_bytes,output_limit){
            Ok(row)=>{
                if waiting{state.laboratory.update(job_id,|job|{if job.active(){job.state="running".into();job.event("model_slot_acquired","The configured provider-call slot is available.",json!({"usage_id":row.id}));}})?;}
                return Ok(row);
            },
            Err(error) if error.downcast_ref::<crate::usage::ConcurrentCallLimit>().is_some()=>{
                if !waiting{state.laboratory.update(job_id,|job|{if job.active(){job.state="waiting".into();job.event("waiting_for_model_slot","Waiting locally for the configured model-call limit; no provider request has been sent.",json!({"purpose":purpose}));}})?;waiting=true;}
                tokio::select!{_=token.cancelled()=>bail!("Stopped before a provider slot became available"),_=tokio::time::sleep(Duration::from_millis(100))=>{}}
            },
            Err(error)=>return Err(error),
        }
    }
}

pub(super) fn delegate(agent:&AgentService,state:Arc<AppState>,parent:&LabJob,args:&Value,target:Uuid,token:&CancellationToken)->anyhow::Result<Value>{
    anyhow::ensure!(!token.is_cancelled(),"Parent stopped before delegation");
    let task=args["task"].as_str().context("A concrete specialist task is required")?;
    let evidence=args["evidence_job_ids"].as_array().context("List the exact evidence job IDs, or an empty list for a theoretical subquestion")?.iter().map(|id|Uuid::parse_str(id.as_str().context("Evidence job ID must be a UUID")?).map_err(Into::into)).collect::<anyhow::Result<Vec<_>>>()?;
    let child=state.laboratory.create_specialist(target,parent.id,task,evidence)?;
    state.laboratory.update(parent.id,|job|{
        if !job.events.iter().any(|event|event.kind=="specialist_delegated"&&event.data["child_job_id"]==json!(child.id)){
            job.event("specialist_delegated",format!("Assigned specialist {}: {}",child.id,task),json!({"child_job_id":child.id,"objective":task,"evidence_job_ids":child.input["delegation"]["evidence_job_ids"],"provider":child.input["provider"],"model":child.input["model"],"reasoning_effort":child.input["reasoning_effort"],"deadline_at":child.deadline_at}));
        }
    })?;
    state.laboratory.update(child.id,|job|{
        if !job.events.iter().any(|event|event.kind=="assignment_received"){
            job.event("assignment_received",format!("Received the scoped question from parent {}.",parent.id),json!({"parent_job_id":parent.id,"objective":task,"evidence_job_ids":job.input["delegation"]["evidence_job_ids"]}));
        }
    })?;
    if child.state=="queued"&&!state.laboratory.executing(child.id){
        if let Err(error)=agent.spawn_lab_session_linked(state.clone(),child.id,Some(token),false){
            if token.is_cancelled(){let _=state.laboratory.stop(child.id,"cancelled");}
            return Err(error);
        }
    }
    Ok(json!({"child_job_id":child.id,"parent_job_id":parent.id,"objective":task,"state":state.laboratory.get(child.id)?.state,"model":child.input["model"],"reasoning_effort":child.input["reasoning_effort"],"deadline_at":child.deadline_at,"next":"Use inspect_specialist to receive the child's actual output. Delegation alone is not evidence."}))
}

pub(super) async fn inspect(agent:&AgentService,state:Arc<AppState>,parent:&LabJob,args:&Value,token:&CancellationToken)->anyhow::Result<Value>{
    let id=Uuid::parse_str(args["child_job_id"].as_str().context("child_job_id is required")?)?;
    let mut child=state.laboratory.get(id)?;
    anyhow::ensure!(child.kind=="specialist"&&child.parent_id==Some(parent.id)&&child.project_id==parent.project_id,"Inspect only a specialist assigned by this parent");
    let current_parent=state.laboratory.get(parent.id)?;
    anyhow::ensure!(current_parent.active()&&!token.is_cancelled(),"Parent is stopped");
    if matches!(child.state.as_str(),"paused"|"failed"|"timed_out")&&!state.laboratory.executing(id)&&super::output_recovery::automatic_resume_allowed(&state,id)?{
        agent.resume_lab_session(state.clone(),id)?;child=state.laboratory.get(id)?;
    }
    if child.active(){
        let updates_before=current_parent.events.iter().filter(|event|event.kind=="steering_received").count();
        state.laboratory.update(parent.id,|job|{if job.active(){job.state="waiting".into();job.event("waiting_for_specialist",format!("Waiting for specialist {id}; no parent model tokens are used while waiting."),json!({"child_job_id":id,"objective":child.input["delegation"]["objective"]}));}})?;
        while child.active(){
            tokio::select!{_=token.cancelled()=>bail!("Parent stopped while waiting for specialist output"),_=tokio::time::sleep(Duration::from_millis(100))=>{}}
            child=state.laboratory.get(id)?;
            if state.laboratory.get(parent.id)?.events.iter().filter(|event|event.kind=="steering_received").count()>updates_before{break;}
        }
        state.laboratory.update(parent.id,|job|{if job.active(){job.state="running".into();}})?;
    }
    let output=json!({"child_job_id":id,"parent_job_id":parent.id,"objective":child.input["delegation"]["objective"],"state":child.state,"result":child.result,"error":child.error,"evidence_job_ids":child.input["delegation"]["evidence_job_ids"],"provenance":{"provider":child.input["provider"],"model":child.input["model"],"reasoning_effort":child.input["reasoning_effort"],"completed_at":child.completed_at},"evidence_kind":"advisory specialist analysis; check claims against the recorded numerical evidence"});
    state.laboratory.update(parent.id,|job|{
        if !job.events.iter().any(|event|event.kind=="specialist_result"&&event.data["child_job_id"]==json!(id)&&event.data["state"]==json!(child.state)){
            job.event("specialist_result",format!("Specialist {id} returned {}. Its output is now available to the parent.",child.state),output.clone());
        }
    })?;
    Ok(output)
}

pub(super) fn finish_children(state:&AppState,parent_id:Uuid)->anyhow::Result<()> {
    let pending=state.laboratory.list(None)?.into_iter().filter(|job|job.parent_id==Some(parent_id)&&job.kind=="specialist"&&job.active()).collect::<Vec<_>>();
    for child in pending{
        state.laboratory.stop(child.id,"cancelled")?;
        state.laboratory.update(parent_id,|job|{
            job.event("specialist_uncollected",format!("Parent finished before specialist {} returned; unfinished child work was stopped.",child.id),json!({"child_job_id":child.id,"completed":false}));
        })?;
    }
    Ok(())
}

#[cfg(test)]
#[path="laboratory_team_tests.rs"]
mod tests;
