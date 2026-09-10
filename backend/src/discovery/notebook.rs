use anyhow::{bail,Context};
use chrono::Utc;
use parking_lot::Mutex;
use serde_json::{json,Value};
use uuid::Uuid;
use crate::{domain::RunStatus,persistence::Database};
use super::types::*;
static NOTEBOOK_GATE:Mutex<()>=Mutex::new(());

pub fn load(db:&Database,project_id:Uuid)->anyhow::Result<Notebook>{
    if let Some(value)=db.get_notebook(project_id)?{return Ok(value);}
    let project=db.get_project(project_id)?.context("research world not found")?;
    Ok(Notebook{project_id,revision:0,title:project.name,authors:vec![],hypothesis:project.question,
        protocol:String::new(),notes:String::new(),claims:vec![],references:vec![],data_availability:String::new(),
        code_availability:String::new(),ai_disclosure:"PhaseForge and configured language models assisted experiment design and/or interpretation. Authors must specify actual use, model identifiers, and responsibility before submission.".into(),
        data_license:"Not yet specified by the authors".into(),independent_validation_notes:String::new(),updated_at:Utc::now()})
}
pub fn save(db:&Database,project_id:Uuid,mut record:Notebook)->anyhow::Result<Notebook>{
    let _guard=NOTEBOOK_GATE.lock();let existing=load(db,project_id)?;
    if record.project_id!=project_id{bail!("notebook project mismatch");}
    if record.revision!=existing.revision{bail!("notebook changed in another window; reload before saving");}
    if serde_json::to_vec(&record)?.len()>512_000 || record.claims.len()>100 || record.references.len()>200 || record.authors.len()>100{bail!("research record exceeds bounded storage limits");}
    if record.title.trim().is_empty(){bail!("research title is required");}
    let mut ids=std::collections::HashSet::new();
    for claim in &record.claims {
        if !ids.insert(claim.id){bail!("claim IDs must be unique");}
        if !["observation","interpretation","hypothesis"].contains(&claim.kind.as_str()){bail!("claim kind must be observation, interpretation or hypothesis");}
        if claim.statement.trim().is_empty(){bail!("claim statement cannot be empty");}
        for id in claim.supporting_runs.iter().chain(&claim.contradicting_runs){
            let run=db.get_run(*id)?.context("claim references a missing run")?;
            if run.project_id!=project_id{bail!("claim evidence belongs to another research world");}
            if run.status!=RunStatus::Completed{bail!("only completed runs may support a scientific claim; record failed attempts in notes/campaign ledger");}
        }
    }
    for reference in &mut record.references {
        if reference.doi.len()>400 || reference.title.trim().is_empty() || (!reference.url.is_empty() && !(reference.url.starts_with("https://")||reference.url.starts_with("http://"))){bail!("reference title and safe URL required");}
        // Persisted metadata is user-editable: never present it as tamper-proof verification.
        if reference.source=="crossref" {reference.source="Crossref metadata; author-editable record".into();}
    }
    record.revision=existing.revision.checked_add(1).context("revision overflow")?;record.updated_at=Utc::now();
    db.put_notebook_version(&record)?;Ok(record)
}
pub fn checklist(db:&Database,record:&Notebook,study:&Study)->Value {
    let summary=super::math::summary(study);
    let validations=summary["validation"].as_array().cloned().unwrap_or_default();
    let mut gaps=Vec::<String>::new();
    if record.authors.is_empty()||record.authors.iter().any(|a|a.name.trim().is_empty()){gaps.push("Identify responsible human authors and contributions.".into());}
    if record.protocol.trim().is_empty(){gaps.push("Describe the research protocol, control conditions, and selection rules.".into());}
    if record.claims.is_empty(){gaps.push("Write explicit claims and link each observation to its evidence runs.".into());}
    for claim in &record.claims {
        if claim.kind!="hypothesis"&&claim.supporting_runs.is_empty(){gaps.push(format!("Claim {} lacks supporting run IDs.",claim.id));}
        if claim.reviewed_by.trim().is_empty(){gaps.push(format!("Claim {} lacks named human review.",claim.id));}
        if claim.literature_comparison.trim().is_empty(){gaps.push(format!("Claim {} has no recorded comparison to prior work.",claim.id));}
        for id in &claim.supporting_runs {if db.get_run(*id).ok().flatten().and_then(|r|r.result).is_none(){gaps.push(format!("Claim evidence run {id} is unavailable."));}}
    }
    if record.references.is_empty(){gaps.push("No references recorded; metadata search alone cannot establish novelty.".into());}
    if record.data_availability.trim().is_empty(){gaps.push("Supply an author-approved data-availability statement.".into());}
    if record.code_availability.trim().is_empty(){gaps.push("Supply an author-approved code-availability statement and code archive/version.".into());}
    if record.independent_validation_notes.trim().is_empty(){gaps.push("Independent verification has not been documented. Use the bundled reference replay where applicable.".into());}
    if study.state!="completed"{gaps.push("The campaign did not complete every approved phase.".into());}
    if validations.is_empty()||validations.iter().any(|v|v["status"]!="agreement"){gaps.push("Finalist numerical agreement is missing, inconclusive, or fails the predeclared tolerances.".into());}
    let dossiers=db.list_dossiers().unwrap_or_default().into_iter().filter(|d|d.protocol.study_id==study.id).collect::<Vec<_>>();
    if dossiers.is_empty(){gaps.push("No independent verification / prior-work dossier linked to this campaign.".into());}
    for d in &dossiers {match crate::assurance::service::assessment(db,d){
        Ok(a)=>{if a["status"]!="ready_for_external_review"{gaps.push(format!("Verification dossier {} still has evidence or literature-review gaps.",d.id));}},
        Err(_)=>gaps.push(format!("Verification dossier {} could not be assessed.",d.id)),
    }}
    gaps.push("A responsible scientist must evaluate novelty, method validity, multiple-testing/selection bias, ethics, permissions, and journal-specific requirements. No automated acceptance certification.".into());
    json!({"status":"author_review_required","gaps":gaps,"novelty":"not_certified","notebook_revision":record.revision})
}
/// Explicit public-metadata query. No account tokens, private credentials, arbitrary hosts or auto search.
static CROSSREF_GATE:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(1);
pub async fn literature(query:&str)->anyhow::Result<Vec<Reference>>{
    let _permit=CROSSREF_GATE.acquire().await.context("literature search gate closed")?;
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    if !(3..=300).contains(&query.trim().len()){bail!("literature query must be 3-300 characters");}
    let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(18)).redirect(reqwest::redirect::Policy::none()).user_agent("PhaseForge/0.5 research metadata client").build()?;
    let mut response=client.get("https://api.crossref.org/works").query(&[("query.bibliographic",query.trim()),("rows","12")]).send().await?.error_for_status()?;
    let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await?{if bytes.len()+chunk.len()>2_000_000{bail!("metadata response too large");}bytes.extend_from_slice(&chunk);}
    let body:Value=serde_json::from_slice(&bytes)?;
    Ok(body.pointer("/message/items").and_then(Value::as_array).into_iter().flatten().filter_map(|item|{
        let doi=item["DOI"].as_str()?.to_owned();let title=item["title"].as_array()?.first()?.as_str()?.to_owned();
        Some(Reference{url:format!("https://doi.org/{doi}"),doi,title,
            authors:item["author"].as_array().into_iter().flatten().map(|a|format!("{} {}",a["given"].as_str().unwrap_or(""),a["family"].as_str().unwrap_or(""))).collect(),
            year:item.pointer("/issued/date-parts/0/0").and_then(Value::as_i64),publisher:item["publisher"].as_str().unwrap_or("").into(),source:"crossref".into(),retrieved_at:Utc::now(),reading_notes:String::new()})
    }).collect())
}
