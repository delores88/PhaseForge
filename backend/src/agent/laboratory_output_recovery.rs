//! Consume a billed incomplete receipt once; bounded retries never execute partial calls.
use super::*;

#[derive(Clone,Serialize,Deserialize)]
pub(super) struct Recovery {
    usage_id:Uuid, failed_round:u32, failures:u32, retry_limit:u32,
    output_cap:u32, paused:bool, proposed_tools:Vec<String>,
}

pub(super) fn is_output_limit(provider:ProviderKind,body:&Value)->bool {
    match provider {
        ProviderKind::OpenAi=>body["status"]=="incomplete"&&body["incomplete_details"]["reason"]=="max_output_tokens",
        ProviderKind::Anthropic=>body["stop_reason"]=="max_tokens",
    }
}

pub(super) fn retain(_session:Uuid,usage_id:Uuid,body:&Value,retry_limit:u32,output_cap:u32,path:&std::path::Path,journal:&mut Journal)->anyhow::Result<()> {
    let failures=journal.output_recovery.as_ref().map_or(1,|r|r.failures.saturating_add(1));
    let proposed_tools=body["output"].as_array().into_iter().flatten().chain(body["content"].as_array().into_iter().flatten())
        .filter(|item|item["type"]=="function_call"||item["type"]=="tool_use")
        .filter_map(|item|item["name"].as_str()).map(|name|name.chars().take(100).collect()).take(20).collect::<Vec<String>>();
    let recovery=Recovery{usage_id,failed_round:journal.round,failures,retry_limit,output_cap,paused:failures>retry_limit,proposed_tools};
    // No output item from an incomplete response enters the executable native
    // transcript, including apparently complete calls preceding its truncated tail.
    // Original response bytes remain in provider-NNNNN.json for audit/retrieval.
    journal.items.push(json!({"role":"user","content":format!("Runtime recovery notice: the previous generation reached its {output_cap}-token output cap (reasoning and visible output share that cap). This is not input-context overflow. None of that incomplete response's tool proposals were executed. Its original receipt is provider-{:05}.json; proposed tools: {}. Continue the existing authorized request from completed tool receipts. Produce a smaller complete tool call, split work into bounded stages, or use generated_experiment_contract to construct a complex data-only scene as a retained JSON artifact and render_illustration.scene_source to render it. Preserve the user's requested scientific detail; do not substitute a promise or decorative simulation. The selected model, reasoning effort, output cap and remaining time/spending limits are unchanged.",journal.round,serde_json::to_string(&recovery.proposed_tools)?)}));
    journal.output_recovery=Some(recovery);journal.round+=1;journal.pending=None;
    write_json(path,journal)
}

pub(super) fn reconcile(state:&AppState,id:Uuid,journal:&Journal)->anyhow::Result<()> {
    let Some(recovery)=&journal.output_recovery else{return Ok(())};
    state.laboratory.update(id,|job|{
        if !job.events.iter().any(|event|event.kind=="output_limit_recovery"&&event.data["usage_id"]==json!(recovery.usage_id)) {
            job.event("output_limit_recovery",if recovery.paused{"The response hit its output cap and used the configured repair allowance. The incomplete proposal is saved and was not executed. Resume explicitly to try again."}else{"The response hit its output cap. The incomplete proposal is saved and was not executed; a bounded repair will use the same selected model and usage limits."},serde_json::to_value(recovery).expect("Serializable output recovery"));
        }
    })?;
    anyhow::ensure!(!recovery.paused,"The model exhausted its output cap and configured repair allowance. Saved tools and results are retained. Resume explicitly to request another attempt; adjust the output cap in Usage & cost if needed. No partial tool call was executed.");
    Ok(())
}

pub(super) fn authorize_resume(state:&AppState,id:Uuid)->anyhow::Result<()> {
    let path=state.laboratory.directory(id).join("journal.json");
    if !path.is_file(){return Ok(())}
    let mut journal:Journal=serde_json::from_slice(&std::fs::read(&path)?)?;
    if let Some(recovery)=journal.output_recovery.as_mut().filter(|r|r.paused) {
        recovery.paused=false;recovery.failures=0;
        journal.items.push(json!({"role":"user","content":"The user explicitly resumed after the output-cap pause. Continue from the retained evidence with the same selected model and reasoning effort, within the currently configured usage and time limits. Do not repeat completed tools."}));
        write_json(&path,&journal)?;
    }
    Ok(())
}

pub(super) fn automatic_resume_allowed(state:&AppState,id:Uuid)->anyhow::Result<bool>{
    let path=state.laboratory.directory(id).join("journal.json");
    if !path.is_file(){return Ok(true)}
    let journal:Journal=serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(!journal.output_recovery.is_some_and(|recovery|recovery.paused))
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn only_actual_output_limits_are_automatically_repairable() {
        for reason in ["content_filter","unknown",""]{assert!(!is_output_limit(ProviderKind::OpenAi,&json!({"status":"incomplete","incomplete_details":{"reason":reason}})));}
        assert!(!is_output_limit(ProviderKind::OpenAi,&json!({"status":"cancelled"})));
        assert!(is_output_limit(ProviderKind::OpenAi,&json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}})));
        assert!(is_output_limit(ProviderKind::Anthropic,&json!({"stop_reason":"max_tokens"})));
    }
    #[test]fn contradictory_completed_envelopes_cannot_execute_incomplete_tool_items(){
        for status in ["incomplete","in_progress"]{
            assert!(canonical_output(ProviderKind::OpenAi,&json!({"status":"completed","output":[{"type":"function_call","call_id":"untrusted","name":"render_illustration","status":status,"arguments":"{}"}]})).is_err());
        }
        assert!(canonical_output(ProviderKind::OpenAi,&json!({"status":"completed","output":[{"type":"function_call","status":"completed","arguments":"{\"scene\":"}]})).is_err());
    }
}
