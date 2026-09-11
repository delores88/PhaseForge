//! Canonical native tool turns keep every result before user text or image blocks.
use super::*;
use std::collections::BTreeSet;
use sha2::{Digest,Sha256};

#[derive(Clone,Serialize,Deserialize)]
pub(super) struct ImageInput{pub(super) call_id:String,item:Value,evidence:Value}

pub(super) fn record_image(journal:&mut Journal,call_id:&str,output:&Value){
    let Some(image)=output["image_data_url"].as_str()else{return};
    let item=json!({"role":"user","content":[{"type":"input_text","text":format!("Actual recorded image from tool call {call_id}. Source evidence: {}",output["evidence"])},{"type":"input_image","image_url":image,"detail":"high"}]});
    if !journal.image_outbox.iter().any(|entry|entry.call_id==call_id){journal.image_outbox.push(ImageInput{call_id:call_id.into(),item:item.clone(),evidence:output["evidence"].clone()});}
    journal.items.push(item);
}
pub(super) fn context(journal:&Journal)->anyhow::Result<Vec<Value>>{
    let mut context=ordered(journal.items.get(journal.context_start..).context("Saved context boundary is invalid")?,true)?;
    // Summaries never acknowledge pixels. Carry unsent observations after the
    // summary as ordinary native image inputs, without orphaned tool results.
    for image in &journal.image_outbox{if !context.contains(&image.item){context.push(image.item.clone());}}
    Ok(context)
}
pub(super) fn save_dispatch(state:&AppState,session:Uuid,usage:Uuid,provider:ProviderKind,model:&str,effort:Option<&str>,journal:&Journal,context:&[Value])->anyhow::Result<Option<Value>>{
    let mut images=vec![];let mut input_images=vec![];
    for item in context{
        for part in item["content"].as_array().into_iter().flatten().filter(|part|part["type"]=="input_image"){
            let url=part["image_url"].as_str().context("Native image input has no bytes")?;
            let bytes=base64::engine::general_purpose::STANDARD.decode(url.strip_prefix("data:image/png;base64,").context("Native observation must be a PNG")?)?;
            let hash=format!("{:x}",Sha256::digest(bytes));
            let pending=journal.image_outbox.iter().find(|entry|entry.item==*item);
            let evidence=pending.map(|entry|entry.evidence.clone()).or_else(||item["content"].as_array()?.iter().find_map(|part|part["text"].as_str()?.split_once("Source evidence: ").and_then(|(_,json)|serde_json::from_str::<Value>(json).ok()))).unwrap_or(Value::Null);
            anyhow::ensure!(evidence["sha256"].as_str().is_none_or(|saved|saved==hash),"Native image bytes disagree with their retained source hash");
            images.push(json!({"call_id":pending.map(|entry|entry.call_id.as_str()),"sha256":hash,"source_job_id":evidence["job_id"],"path":evidence["path"],"evidence":evidence}));input_images.push(item.clone());
        }
    }
    if images.is_empty(){return Ok(None);}
    let filename=format!("native-request-{usage}.json");
    let receipt=json!({"usage_id":usage,"session_id":session,"provider":provider,"model":model,"reasoning_effort":effort,"image_count":images.len(),"images":images,"request_path":filename});
    write_json(&state.laboratory.directory(session).join(&filename),&json!({"receipt":receipt,"input_images":input_images}))?;Ok(Some(receipt))
}
pub(super) fn acknowledge(state:&AppState,session:Uuid,usage:Uuid,response:&crate::agent::providers::ProviderResponse,journal:&mut Journal)->anyhow::Result<Option<Value>>{
    let pending=journal.pending.as_ref();
    let ids=pending.and_then(|value|value["image_call_ids"].as_array()).into_iter().flatten().filter_map(Value::as_str).collect::<BTreeSet<_>>();
    let vision=pending.and_then(|value|value.get("vision")).filter(|value|!value.is_null()).cloned();
    let ack=if let Some(mut receipt)=vision{
        anyhow::ensure!(receipt["usage_id"]==json!(usage)&&receipt["session_id"]==json!(session),"Native image receipt identity mismatch");
        receipt["provider_response_id"]=response.body["id"].clone();receipt["status"]=json!("completed_response_received");
        let filename=format!("native-vision-ack-{usage}.json");receipt["acknowledgement_path"]=json!(filename);
        write_json(&state.laboratory.directory(session).join(filename),&receipt)?;Some(receipt)
    }else{None};
    journal.image_outbox.retain(|image|!ids.contains(image.call_id.as_str()));Ok(ack)
}

pub(super) fn ordered(items:&[Value],complete:bool)->anyhow::Result<Vec<Value>>{
    let mut result=Vec::with_capacity(items.len());let mut deferred=vec![];
    let mut pending=BTreeSet::new();let mut known=BTreeSet::new();let mut returning=false;
    for item in items{
        match item["type"].as_str(){
            Some("function_call")=>{
                anyhow::ensure!(!returning,"A new tool proposal interrupted unfinished native tool results");
                let id=item["call_id"].as_str().context("Native tool proposal has no call ID")?;
                anyhow::ensure!(known.insert(id),"Native tool call identity was repeated");pending.insert(id);result.push(item.clone());
            }
            Some("function_call_output")=>{
                let id=item["call_id"].as_str().context("Native tool result has no call ID")?;
                anyhow::ensure!(pending.remove(id),"Native tool result has no outstanding proposal: {id}");
                returning=true;result.push(item.clone());
                if pending.is_empty(){result.append(&mut deferred);returning=false;}
            }
            _ if item["role"]=="user"&&!pending.is_empty()=>deferred.push(item.clone()),
            _=>{
                anyhow::ensure!(!returning,"An assistant turn interrupted unfinished native tool results");
                result.push(item.clone());
            }
        }
    }
    anyhow::ensure!(!complete||pending.is_empty(),"Native tool proposals still require results before another model turn");
    result.append(&mut deferred);Ok(result)
}

pub(super) fn retain_order(journal:&mut Journal)->anyhow::Result<()>{
    let tail=journal.items.get(journal.context_start..).context("Saved context boundary is invalid")?;
    let ordered=ordered(tail,false)?;
    journal.items.truncate(journal.context_start);journal.items.extend(ordered);Ok(())
}

#[cfg(test)]
#[path="laboratory_native_tests.rs"]
mod tests;
