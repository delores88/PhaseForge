//! Durable user updates are applied at action boundaries, without rewriting earlier inputs.
use super::*;
use crate::laboratory::data as files;
use axum::{Router,Json,extract::{State,Path},routing::post,http::StatusCode};

#[derive(Clone,Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
pub struct SteeringRequest{pub request_id:Uuid,#[serde(default)]pub content:String,#[serde(default)]pub attachments:Vec<files::Attachment>}
#[derive(Debug,thiserror::Error)]
#[error("The session already finished; this message can start a new session.")]
struct SessionFinished;

pub(super) fn has_pending(job:&LabJob,journal:&Journal)->bool{
    job.events.iter().any(|event|event.kind=="steering_received"&&event.data["request_id"].as_str().and_then(|value|Uuid::parse_str(value).ok()).is_some_and(|id|!journal.steering_ids.contains(&id)))
}
pub(super) fn receive(state:&AppState,id:Uuid,request:SteeringRequest)->anyhow::Result<Value>{
    anyhow::ensure!(request.request_id!=id,"User update needs its own request identity");
    anyhow::ensure!((!request.content.trim().is_empty()||!request.attachments.is_empty())&&request.content.len()<=16000,"An update needs text or files, with at most 16,000 text bytes");
    let initial=state.laboratory.get(id)?;anyhow::ensure!(initial.kind=="session","Send user updates to the parent conversation session");
    let (references,prepared)=files::prepare(&state.laboratory,initial.project_id,request.request_id,Some(id),&request.attachments)?;
    let update=json!({"request_id":request.request_id,"content":request.content,"attachments":references});
    if let Some(event)=initial.events.iter().find(|event|event.kind=="steering_received"&&event.data["request_id"]==json!(request.request_id)){
        anyhow::ensure!(event.data==update,"Update identity is bound to different instructions");return Ok(json!({"session_id":id,"request_id":request.request_id,"state":"saved","duplicate":true,"files":references}));
    }
    if matches!(initial.state.as_str(),"completed"|"cancelled"){return Err(SessionFinished.into());}
    files::persist(&state.laboratory,initial.project_id,prepared)?;
    let job=state.laboratory.update_with_message(id,|job|{
        if let Some(event)=job.events.iter().find(|event|event.kind=="steering_received"&&event.data["request_id"]==json!(request.request_id)){
            anyhow::ensure!(event.data==update,"Update identity is bound to different instructions");return Ok(None);
        }
        if matches!(job.state.as_str(),"completed"|"cancelled"){return Err(SessionFinished.into());}
        anyhow::ensure!(job.events.iter().filter(|event|event.kind=="steering_received").count()<64,"This session has reached its 64-update boundary; start a new conversation request with the retained evidence");
        let text=if request.content.trim().is_empty(){"Inspect these additional source files within the current research objective."}else{&request.content};
        let mut message=ConversationMessage::new(job.project_id,ConversationRole::User,MessageKind::Chat,text);message.id=request.request_id;
        message.metadata=json!({"laboratory_session_id":id,"request_id":request.request_id,"phase":"steering_queued","attachments":references,"provider":job.input["provider"],"model":job.input["model"],"reasoning_effort":job.input["reasoning_effort"]});
        job.event("steering_received","A user update is saved for the next action boundary. Earlier numerical inputs remain unchanged.",update.clone());Ok(Some(message))
    });
    match job{Ok(job)=>Ok(json!({"session_id":id,"request_id":request.request_id,"state":"saved","model":job.input["model"],"deadline_at":job.deadline_at,"files":references})),Err(error)=>Err(error)}
}

pub(super) fn apply(state:&AppState,job:&LabJob,journal_path:&std::path::Path,journal:&mut Journal)->anyhow::Result<bool>{
    let latest=state.laboratory.get(job.id)?;
    let updates=latest.events.iter().filter(|event|event.kind=="steering_received").filter_map(|event|{
        let id=Uuid::parse_str(event.data["request_id"].as_str()?).ok()?;(!journal.steering_ids.contains(&id)).then(||(id,event.data.clone()))
    }).collect::<Vec<_>>();
    if updates.is_empty(){deliver_applied(state,job.id,journal)?;return Ok(false);}
    let mut user_items=vec![];
    for (id,update) in &updates{
        user_items.push(json!({"role":"user","content":format!("Explicit user update to this ongoing task. Preserve the original research objective unless this update explicitly changes it. Apply new constraints to future actions and distinguish earlier results under their original assumptions. Do not mutate a running solver's physics. The session's captured model and deadline remain unchanged. USER UPDATE: {}\nADDITIONAL RETAINED SOURCE FILES: {}",update["content"].as_str().unwrap_or("Inspect the additional source files."),update["attachments"])}));
        journal.steering_ids.push(*id);
    }
    if !journal.memory.is_object(){journal.memory=json!({"earlier_memory":journal.memory});}
    journal.memory["user_update_ids"]=json!(journal.steering_ids);journal.memory["latest_user_update"]=updates.last().unwrap().1.clone();
    // A model response can arrive after the user changes its assumptions. Do not
    // blindly execute its proposed actions. Preserve any already-created target
    // as evidence; an uncertain prepared receipt is not declared unexecuted.
    let calls=journal.items.iter().filter(|item|item["type"]=="function_call"&&!journal.tools.get(item["call_id"].as_str().unwrap_or("")).is_some_and(|receipt|receipt["state"]=="completed")).cloned().collect::<Vec<_>>();
    for call in calls{
        let call_id=call["call_id"].as_str().context("Tool call ID missing")?.to_string();let name=call["name"].as_str().context("Tool name missing")?;
        let arguments:Value=serde_json::from_str(call["arguments"].as_str().unwrap_or("{}"))?;
        let prior=journal.tools.get(&call_id);let target=prior.and_then(|receipt|receipt["target_id"].as_str()).and_then(|id|Uuid::parse_str(id).ok());
        let recorded=target.and_then(|id|state.laboratory.get(id).ok()).filter(|target|target.project_id==job.project_id).map(|target|target.summary());
        let output=json!({"new_action_taken":false,"superseded_by_user_update":true,"prior_execution":if prior.is_none(){"not started"}else if recorded.is_some(){"target already recorded; inspect it before further action"}else{"unconfirmed prepared receipt; inspect saved artifacts before retrying"},"recorded_target":recorded,"target_id":target,"user_update_ids":updates.iter().map(|(id,_)|id).collect::<Vec<_>>()});
        journal.tools.insert(call_id.clone(),json!({"state":"completed","name":name,"arguments":arguments,"target_id":target,"output":output,"superseded":true}));
        journal.items.push(json!({"type":"function_call_output","call_id":call_id,"output":serde_json::to_string(&output)?}));
    }
    journal.items.extend(user_items);
    native::retain_order(journal)?;
    write_json(journal_path,journal)?;
    deliver_applied(state,job.id,journal)?;
    Ok(true)
}
fn deliver_applied(state:&AppState,id:Uuid,journal:&Journal)->anyhow::Result<()>{
    let job=state.laboratory.get(id)?;
    if !journal.steering_ids.iter().any(|id|!job.events.iter().any(|event|event.kind=="steering_applied"&&event.data["request_id"]==json!(id))){return Ok(());}
    state.laboratory.update(id,|job|{for id in &journal.steering_ids{if !job.events.iter().any(|event|event.kind=="steering_applied"&&event.data["request_id"]==json!(id)){job.event("steering_applied","The saved user update is in the agent's continuation context. Earlier artifacts remain preserved.",json!({"request_id":id}));}}})?;Ok(())
}

pub fn routes()->Router<Arc<AppState>>{Router::new().route("/api/laboratory/jobs/:id/steer",post(steer_route))}
async fn steer_route(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<SteeringRequest>)->Result<Json<Value>,(StatusCode,Json<Value>)>{
    // Prepare references separately for the finish/steer race: if admission saved
    // files just before completion, a new session can reuse those exact sources.
    let references=state.laboratory.get(id).ok().and_then(|job|files::prepare(&state.laboratory,job.project_id,request.request_id,Some(id),&request.attachments).ok()).map(|(references,_)|references);
    receive(&state,id,request).map(Json).map_err(|error|{
        let closed=error.downcast_ref::<SessionFinished>().is_some();
        let retained=references.map(|refs|refs.into_iter().filter(|file|files::owned(&state.laboratory,state.laboratory.get(id).map(|job|job.project_id).unwrap_or_default(),file.job_id).is_ok()).collect::<Vec<_>>()).unwrap_or_default();
        (if closed{StatusCode::CONFLICT}else{StatusCode::UNPROCESSABLE_ENTITY},Json(json!({"error":{"code":if closed{"session_finished"}else{"invalid_update"},"message":format!("{error:#}"),"retained_files":retained}})))
    })
}

#[cfg(test)]
#[path="laboratory_steering_tests.rs"]
mod tests;
