//! Evidence-only result reviews. The UI action cannot grant scientific execution.
use super::{LabJob, OutputIntent, SessionRequest};
use crate::laboratory::LaboratoryService;
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResultReview {
    pub source_job_id: Uuid,
    pub action: ReviewAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
    #[serde(default)]
    pub evidence_files: BTreeMap<String, EvidenceFile>,
    #[serde(default)]
    pub omitted_file_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EvidenceFile { pub bytes: u64, pub sha256: String }

fn file_identity(service: &LaboratoryService, id: Uuid, relative: &str) -> anyhow::Result<EvidenceFile> {
    let path=service.path(id,relative)?;let mut file=std::fs::File::open(path)?;
    ensure!(file.metadata()?.len()<=16*1024*1024,"Review evidence member exceeds 16 MiB");
    let mut hash=Sha256::new();let mut bytes=0u64;let mut buffer=[0u8;65536];
    loop {let count=file.read(&mut buffer)?;if count==0{break}bytes+=count as u64;ensure!(bytes<=16*1024*1024,"Review evidence grew beyond its limit");hash.update(&buffer[..count]);}
    Ok(EvidenceFile{bytes,sha256:format!("{:x}",hash.finalize())})
}

fn evidence(service: &LaboratoryService, id: Uuid) -> anyhow::Result<(BTreeMap<String,EvidenceFile>,usize)> {
    let inventory=service.artifact_inventory(id)?;let rows=inventory.as_array().context("Missing retained artifact inventory")?;
    let mut candidates=rows.iter().filter_map(|row|{
        let path=row["path"].as_str()?;let extension=std::path::Path::new(path).extension()?.to_str()?;
        // Presentation may change independently while scientific evidence is read.
        if path=="presentation.json"||path.starts_with("observation-")||!["json","txt","csv","py","log","png"].contains(&extension){return None}
        Some((path.to_owned(),row["bytes"].as_u64()?))
    }).collect::<Vec<_>>();
    let priority=|path:&str|match path {
        "result.json"|"manifest.json"|"work/result.json"=>0,
        "measurements.json"|"topology.json"|"fields/index.json"|"trajectory/index.json"|"observations/index.json"|"parameters.json"|"checkpoint.json"=>1,
        _ if path.ends_with(".png")=>3,
        _=>2,
    };
    candidates.sort_by(|a,b|priority(&a.0).cmp(&priority(&b.0)).then_with(||a.0.cmp(&b.0)));
    let mut result=BTreeMap::new();let mut total=0u64;let mut omitted=0;
    for (path,size) in candidates {
        if size>16*1024*1024||total+size>64*1024*1024||result.len()>=512{omitted+=1;continue}
        let identity=file_identity(service,id,&path)?;
        ensure!(identity.bytes==size,"Retained evidence changed during review admission");
        total+=identity.bytes;result.insert(path,identity);
    }
    Ok((result,omitted))
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction { Explain, NextSteps, Validation }

fn source(service: &LaboratoryService, project: Uuid, id: Uuid) -> anyhow::Result<LabJob> {
    let job = service.get(id)?;
    ensure!(job.project_id == project, "The selected result belongs to another project");
    ensure!(job.state == "completed" && !service.executing(id), "Review a completed, inactive scientific result");
    ensure!(matches!(job.kind.as_str(), "solver" | "generated" | "published_simulation" | "ml_study" | "study_plot"), "This result type does not support numerical review");
    ensure!(!job.result.is_null(), "This job has no retained numerical result");
    service.ensure_model_study_access(id)?;
    Ok(job)
}

fn fingerprint(job: &LabJob, files: &BTreeMap<String,EvidenceFile>) -> anyhow::Result<String> {
    // Read markers, progress events and presentation preferences do not change
    // scientific identity. Inputs and registered result/artifact pins do.
    let bytes = serde_json::to_vec(&json!({"id":job.id,"project_id":job.project_id,
        "kind":job.kind,"input":job.input,"result":job.result,"evidence_files":files}))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(super) fn prepare(service: &LaboratoryService, project: Uuid, request: &mut SessionRequest) -> anyhow::Result<()> {
    let Some(review) = request.result_review.as_mut() else { return Ok(()) };
    let job = source(service, project, review.source_job_id)?;
    let (files,omitted)=evidence(service,job.id)?;
    for path in ["result.json","manifest.json"] {
        ensure!(!service.directory(job.id).join(path).exists()||files.contains_key(path),"The core result exceeds the bounded review evidence; a narrower result is required");
    }
    let hash = fingerprint(&job,&files)?;
    ensure!(review.source_sha256.as_ref().is_none_or(|expected|expected == &hash), "The selected scientific result changed; reopen it before requesting a review");
    review.source_sha256 = Some(hash);
    review.evidence_files=files;review.omitted_file_count=omitted;
    ensure!(request.context_job_id.is_none_or(|id|id == job.id), "Review context must match the selected result");
    ensure!(request.attachments.is_empty(), "A saved-result review reads its retained source; send new experiment files in chat");
    request.context_job_id = Some(job.id);
    request.output_intent = Some(OutputIntent::Explanation);
    let purpose = match review.action {
        ReviewAction::Explain => "Explain the measured results in plain language, including units, model assumptions, validation evidence and what remains unproven.",
        ReviewAction::NextSteps => "Suggest specific next steps grounded in this result. For each, state the question, proposed change, control, observable, needed solver and validation, and any missing information. These are proposals only; do not run or modify an experiment.",
        ReviewAction::Validation => "Review the retained numerical checks, constraints, convergence evidence and uncertainty. Distinguish tests of the code from validation of the physical model. State missing checks; do not perform new computations or call absent checks successful.",
    };
    request.content = format!("{purpose}\nUse the exact completed job {} in this project ({}). Read its retained result and source artifacts before answering. Cite its job ID and relevant artifact paths. A preview image alone is not numerical evidence.", job.id, job.title);
    Ok(())
}

pub(super) fn validate(service: &LaboratoryService, project: Uuid, request: &SessionRequest) -> anyhow::Result<()> {
    let Some(review) = &request.result_review else { return Ok(()) };
    let job = source(service, project, review.source_job_id)?;
    let expected = review.source_sha256.as_ref().context("Saved-result review lacks its source identity")?;
    ensure!(&fingerprint(&job,&review.evidence_files)? == expected, "The reviewed scientific input/result changed; this review cannot continue against different evidence");
    for (path,identity) in &review.evidence_files {
        ensure!(&file_identity(service,job.id,path)?==identity,"Retained review evidence changed: {path}");
    }
    Ok(())
}

fn allowed(name: &str) -> bool {
    matches!(name, "set_output_intent" | "check_deliverable" | "inspect_result" | "list_artifacts"
        | "read_artifact" | "observe_frame" | "remember" | "recall" | "lab_catalog")
}

pub(super) fn tools_for(request: &SessionRequest, tools: Value) -> Value {
    if request.result_review.is_none() { return tools }
    json!(tools.as_array().into_iter().flatten().filter(|tool| allowed(tool["name"].as_str().unwrap_or(""))).collect::<Vec<_>>())
}

pub(super) fn instructions(request: &SessionRequest, mut instructions: String) -> String {
    if let Some(review) = &request.result_review {
        instructions.push_str(&format!("\nThis is a restricted saved-result review of job {}, source SHA-256 {}. Read inspect_result and relevant retained artifacts before answering. Resolve only explanation intent. You may inspect that source and record review notes, but cannot compute, render, edit, install, delegate, launch, or resume any experiment. Suggest next steps as proposals, with uncertainty and needed capability explicit. A new experiment requires a separate explicit user request outside this review. If steering asks for execution, explain that boundary without changing review permissions. Summarize evidence and decisions; do not disclose private chain-of-thought.",review.source_job_id,review.source_sha256.as_deref().unwrap_or("missing")));
    }
    instructions
}

pub(super) fn guard(service: &LaboratoryService, session: &LabJob, request: &SessionRequest, name: &str, args: &Value) -> anyhow::Result<()> {
    let Some(review) = &request.result_review else { return Ok(()) };
    validate(service, session.project_id, request)?;
    ensure!(allowed(name), "Saved-result review permits evidence inspection only; new experiments and presentation changes require a separate user request");
    if name == "set_output_intent" {
        ensure!(args["intent"] == "explanation", "A saved-result review cannot change into scientific execution");
    }
    if matches!(name, "inspect_result" | "list_artifacts" | "read_artifact" | "observe_frame") {
        let id = Uuid::parse_str(args["job_id"].as_str().context("Choose the reviewed source job")?)?;
        ensure!(id == review.source_job_id, "Read the exact selected result; another job requires its own review");
        if matches!(name,"read_artifact"|"observe_frame") {
            let path=args["path"].as_str().context("Choose a pinned evidence path")?;
            ensure!(review.evidence_files.contains_key(path),"This artifact was not included in the bounded review evidence; report the missing evidence rather than reading a different snapshot");
        }
        if name=="inspect_result" {
            for path in ["result.json","manifest.json"] {
                ensure!(!service.directory(id).join(path).exists()||review.evidence_files.contains_key(path),"The core result exceeds the bounded review evidence; a narrower result is required");
            }
        }
    }
    Ok(())
}

pub(super) fn evidence_context(request: &SessionRequest) -> Value {
    request.result_review.as_ref().map(|review|json!({"source_job_id":review.source_job_id,"source_sha256":review.source_sha256,
        "files":review.evidence_files,"omitted_file_count":review.omitted_file_count,
        "scope":"Pinned text/JSON and PNG evidence, at most 512 files / 64 MiB total / 16 MiB each. omitted_file_count counts eligible files excluded by those limits. Binary formats are excluded and not decoded by this review. Omitted evidence and absent validation remain unknown; this review cannot compute replacements."})).unwrap_or(Value::Null)
}

fn verified_bytes(service: &LaboratoryService, request: &SessionRequest, path: &str) -> anyhow::Result<Vec<u8>> {
    let review=request.result_review.as_ref().context("Missing review context")?;
    let expected=review.evidence_files.get(path).context("Artifact is outside pinned review evidence")?;
    let file=std::fs::File::open(service.path(review.source_job_id,path)?)?;
    let mut bytes=Vec::new();file.take(16*1024*1024+1).read_to_end(&mut bytes)?;
    // Hash the same bytes passed to the model, not an earlier open of the path.
    ensure!(bytes.len() as u64==expected.bytes&&format!("{:x}",Sha256::digest(&bytes))==expected.sha256,"Retained review evidence changed while reading: {path}");
    Ok(bytes)
}

pub(super) fn optional_json(service: &LaboratoryService, request: &SessionRequest, source: Uuid, path: &str) -> anyhow::Result<Option<Value>> {
    if request.result_review.is_none(){return Ok(service.read_json(source,path).ok())}
    if !request.result_review.as_ref().unwrap().evidence_files.contains_key(path){return Ok(None)}
    Ok(Some(serde_json::from_slice(&verified_bytes(service,request,path)?)?))
}

pub(super) fn read_text(service: &LaboratoryService, request: &SessionRequest, args: &Value) -> anyhow::Result<Value> {
    let path=args["path"].as_str().context("Choose a retained text artifact")?;
    ensure!(["json","txt","csv","py","log"].contains(&std::path::Path::new(path).extension().and_then(|value|value.to_str()).unwrap_or("")),"Use observe_frame for images; this review cannot decode binary arrays");
    let bytes=verified_bytes(service,request,path)?;let size=bytes.len();
    let offset=args["offset"].as_u64().unwrap_or(0).min(size as u64) as usize;
    let end=(offset+args["max_bytes"].as_u64().unwrap_or(18000).clamp(256,60000) as usize).min(size);
    Ok(json!({"path":path,"offset":offset,"total_bytes":size,"next_offset":if end<size{Some(end)}else{None},"text":String::from_utf8_lossy(&bytes[offset..end]),"sha256":request.result_review.as_ref().unwrap().evidence_files[path].sha256}))
}

pub(super) fn verify_image(request: &SessionRequest, path: &str, payload: &Value) -> anyhow::Result<()> {
    if let Some(review)=&request.result_review {
        let expected=review.evidence_files.get(path).context("Image is outside pinned review evidence")?;
        ensure!(payload["evidence"]["sha256"]==expected.sha256,"Observed pixels changed from the pinned review evidence");
    }
    Ok(())
}

pub(super) fn assess(session: &LabJob, journal: &super::Journal, assessment: &mut Value) {
    let Some(review) = session.input.get("result_review").filter(|value| !value.is_null()) else { return };
    let read = journal.tools.values().any(|tool| tool["state"] == "completed"
        && tool["name"] == "inspect_result" && tool["arguments"]["job_id"] == review["source_job_id"]
        && tool["output"]["error"].is_null() && tool["output"]["job"]["id"] == review["source_job_id"]);
    if !assessment.is_object() { *assessment = json!({}); }
    assessment["result_review"] = json!({"source_job_id":review["source_job_id"],"source_sha256":review["source_sha256"],"action":review["action"],"source_read":read,"scientific_execution_allowed":false});
    if !read {
        assessment["fulfilled"] = json!(false);
        assessment["status"] = json!("missing_review_evidence");
        assessment["reason"] = json!("The response is retained, but the exact selected result was not successfully inspected. No new experiment was run.");
    }
}

#[cfg(test)]
#[path = "laboratory_review_tests.rs"]
mod tests;
