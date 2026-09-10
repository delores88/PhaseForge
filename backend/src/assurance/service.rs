//! One approved, bounded independent worker at a time. No dynamic agent-written code.
use std::{collections::HashMap,path::PathBuf,sync::Arc,time::{Duration,Instant},process::Stdio};
use anyhow::{bail,Context};
use chrono::Utc;
use parking_lot::Mutex;
use serde_json::{json,Value};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use crate::{domain::{ModelSpec,RunStatus},persistence::Database,discovery::math};
use super::{comparison as cmp,types::*};
pub const WORKER:&str=include_str!("../../../tools/verification_worker.py");
pub const REFERENCE:&str=include_str!("../../../tools/reference_ode.py");
const LEGACY_WORKER:&str=include_str!("../../../tools/legacy/0.5.0/verification_worker.py");
const LEGACY_REFERENCE:&str=include_str!("../../../tools/legacy/0.5.0/reference_ode.py");
/// Exact hash matching, never silently replace a frozen dossier's executable source.
pub fn frozen_sources(d:&Dossier)->anyhow::Result<(&'static str,&'static str)> {
    for (worker,reference) in [(WORKER,REFERENCE),(LEGACY_WORKER,LEGACY_REFERENCE)] {
        if cmp::text_hash(worker)==d.worker_source_hash && cmp::text_hash(reference)==d.reference_source_hash {return Ok((worker,reference));}
    }
    bail!("Unknown frozen verifier source version; preserve the matching release or create a new dossier without replacing old results")
}

const MAX_PAYLOAD:usize=64*1024*1024;

#[derive(Clone)]pub struct AssuranceService{inner:Arc<Inner>}
struct Inner{db:Database,gate:Mutex<()>,active:Mutex<HashMap<Uuid,CancellationToken>>}
impl AssuranceService{
    pub fn new(db:Database)->anyhow::Result<Self>{
        for mut d in db.list_dossiers()? {
            if matches!(d.state.as_str(),"running"|"stopping") {
                d.state="interrupted".into();d.stage="Backend restarted; no work automatically resumed".into();d.updated_at=Utc::now();db.put_dossier(&d)?;
            }
            for mut search in db.list_literature_searches(d.id)? {
                if search.status=="running" { search.status="interrupted".into();search.error=Some("Backend restarted before the public search completed; not an empty search result".into());db.put_literature_search(&search)?; }
            }
        }
        Ok(Self{inner:Arc::new(Inner{db,gate:Mutex::new(()),active:Mutex::new(HashMap::new())})})
    }
    pub fn get(&self,id:Uuid)->anyhow::Result<Dossier>{self.inner.db.get_dossier(id)?.context("verification dossier not found")}
    pub fn list(&self,project:Option<Uuid>,study:Option<Uuid>)->anyhow::Result<Vec<Dossier>>{
        let mut rows=self.inner.db.list_dossiers()?;
        rows.retain(|d|project.map(|p|d.project_id==p).unwrap_or(true)&&study.map(|s|d.protocol.study_id==s).unwrap_or(true));
        rows.sort_by(|a,b|b.created_at.cmp(&a.created_at));Ok(rows)
    }
    pub fn delete_project(&self,discovery:&crate::discovery::DiscoveryService,project:Uuid)->anyhow::Result<()> {
        let _guard=self.inner.gate.lock();self.ensure_idle_project(project)?;discovery.delete_project(project)
    }
    pub fn ensure_idle_project(&self,project:Uuid)->anyhow::Result<()>{
        for id in self.inner.active.lock().keys(){if self.get(*id)?.project_id==project{bail!("stop independent verification before deleting the research world");}}Ok(())
    }
    pub fn create(&self,p:VerificationRequest)->anyhow::Result<Dossier>{
        if self.inner.db.list_dossiers()?.len()>=10000{bail!("verification dossier capacity reached; archive the current research database before creating more");}
        cmp::validate_rules(&p.metrics)?;
        if p.question.trim().len()<8||p.question.len()>6000{bail!("state a bounded verification question (8-6000 bytes)");}
        if !(2..=16).contains(&p.holdout_replicates)||!p.perturb_fraction.is_finite()||p.perturb_fraction<=0.0||p.perturb_fraction>0.25{bail!("2-16 holdouts and perturb fraction in (0,0.25] required");}
        if !(5..=3600).contains(&p.wall_seconds)||!(100..=20_000_000).contains(&p.max_rhs_evaluations){bail!("wall budget 5-3600s and RHS cap 100-20,000,000 required");}
        if ![p.solver_absolute_tolerance,p.solver_relative_tolerance].iter().all(|v|v.is_finite()&&*v>0.0&&*v<=0.01){bail!("solver tolerances must be positive, finite and <=0.01");}
        if !p.duplicate_distance.is_finite()||p.duplicate_distance<0.0||p.duplicate_distance>1.0{bail!("feature-match threshold must be in [0,1]");}
        if p.controls.len()>4||p.reference_ids.len()>100{bail!("at most 4 controls and 100 frozen comparison records");}
        let study=self.inner.db.get_study(p.study_id)?.context("study not found")?;
        self.inner.db.get_project(study.project_id)?.context("research world no longer exists")?;
        if matches!(study.state.as_str(),"running"|"pausing"){bail!("pause or finish the campaign before freezing a verification snapshot");}
        if p.holdout_seed==study.recipe.seed{bail!("use a new holdout seed, different from exploration; this is input perturbation, not population inference");}
        let candidate=study.trials.iter().find(|t|t.id==p.trial_id).context("candidate not found in study")?;
        if candidate.phase!="explore"||!candidate.eligible{bail!("choose a completed feasible exploration candidate; failures remain in the study ledger");}
        let run=self.inner.db.get_run(candidate.run_id.context("candidate has no run")?)?.context("candidate run missing")?;
        let manifest=self.inner.db.get_manifest(run.manifest_id)?.context("exact candidate manifest missing")?;
        if !matches!(&manifest.model,ModelSpec::StateVectorOde{..}){bail!("the independent worker supports generic state-vector ODEs; particle/QM/MD verification requires a different engine adapter");}
        if manifest.search.enabled{bail!("select a frozen single candidate, not an unresolved nested search");}
        cmp::signature(&manifest,&run,&p.metrics)?;
        let mut controls=Vec::new();let mut unique=std::collections::HashSet::new();
        for spec in &p.controls{
            if !unique.insert(spec.run_id)||spec.run_id==run.id||!spec.minimum.is_finite()||!spec.maximum.is_finite()||spec.minimum>spec.maximum||spec.rationale.trim().len()<8{bail!("controls must be distinct from the candidate, have finite declared intervals and an explicit rationale");}
            let cr=self.inner.db.get_run(spec.run_id)?.context("control run missing")?;
            let cm=self.inner.db.get_manifest(cr.manifest_id)?.context("control manifest missing")?;
            if cr.project_id!=study.project_id||cr.status!=RunStatus::Completed||cm.search.enabled||!matches!(&cm.model,ModelSpec::StateVectorOde{..}){bail!("controls must be completed single-run ODE evidence in this research world");}
            let control_rule=MetricRule{metric:spec.metric.clone(),scale:1.,absolute_tolerance:0.,relative_tolerance:0.,robustness_absolute:0.};
            cmp::signature(&cm,&cr,&[control_rule])?;
            controls.push(ControlSnapshot{spec:spec.clone(),manifest:cm,run:cr});
        }
        let comparison_space=cmp::space(&study,&p.metrics)?;let mut references=Vec::new();let mut reference_ids=std::collections::HashSet::new();
        for id in &p.reference_ids{
            if !reference_ids.insert(*id){bail!("duplicate reference ID");}
            let r=self.inner.db.get_catalog_entry(*id)?.context("reference no longer exists")?;
            if r.project_id!=study.project_id{bail!("reference belongs to another research world");}references.push(r);
        }
        let now=Utc::now();
        let protocol_hash=cmp::hash(&json!({"protocol":p,"manifest":manifest,"run":run,"controls":controls,"references":references,"recipe_hash":study.recipe_hash}))?;
        let d=Dossier{id:Uuid::new_v4(),project_id:study.project_id,protocol:p.clone(),protocol_hash,recipe_hash:study.recipe_hash.clone(),comparison_space,
            source_run_hash:cmp::hash(&run)?,manifest,source_run:run,parameters:study.recipe.parameters.clone(),references,controls,
            frozen_refinement:math::validation(&study,p.trial_id),within_study:cmp::within_study(&study,p.trial_id,&p.metrics),
            state:"draft".into(),stage:"Protocol frozen; awaiting separate compute approval".into(),completed_tasks:0,total_tasks:2+p.holdout_replicates+p.controls.len(),
            worker_source_hash:cmp::text_hash(WORKER),reference_source_hash:cmp::text_hash(REFERENCE),result:None,error:None,created_at:now,updated_at:now};
        if serde_json::to_vec(&d)?.len()>MAX_PAYLOAD{bail!("verification snapshot exceeds 64 MiB; select a smaller retained run");}
        self.inner.db.put_dossier(&d)?;Ok(d)
    }
    pub fn import_reference(&self,input:CatalogImport)->anyhow::Result<CatalogEntry>{
        if self.inner.db.list_catalog()?.len()>=10000{bail!("comparison catalog capacity reached");}
        cmp::validate_rules(&input.metric_rules)?;
        if input.title.trim().is_empty()||input.source.trim().len()<3||input.license.trim().is_empty()||input.title.len()>240||input.source.len()>3000||input.license.len()>500{bail!("bounded reference title, provenance source and license/permission note required");}
        let study=self.inner.db.get_study(input.study_id)?.context("study not found")?;
        let measurements=if let Some(id)=input.run_id{
            let r=self.inner.db.get_run(id)?.context("reference run not found")?;
            if r.project_id!=study.project_id{bail!("reference run belongs to a different research world");}
            let m=self.inner.db.get_manifest(r.manifest_id)?.context("reference manifest missing")?;
            // Compare the exact normalized model context, not only labels.
            let mut comparison_study=study.clone();comparison_study.base_manifest=m.clone();
            if cmp::space(&comparison_study,&input.metric_rules)?!=cmp::space(&study,&input.metric_rules)?{bail!("reference run is outside this exact comparison space");}
            cmp::signature(&m,&r,&input.metric_rules)?
        }else{
            if input.measurements.len()!=input.metric_rules.len()||input.metric_rules.iter().any(|rule|!input.measurements.get(&rule.metric).map(|v|v.is_finite()).unwrap_or(false)){bail!("external reference needs one finite value per selected metric, no extras");}
            input.measurements.clone()
        };
        let mut r=CatalogEntry{id:Uuid::new_v4(),project_id:study.project_id,comparison_space:cmp::space(&study,&input.metric_rules)?,title:input.title,source:input.source,license:input.license,
            provenance_kind:if input.run_id.is_some(){"local_completed_run"}else{"external_author_supplied"}.into(),source_run_id:input.run_id,measurements,content_hash:String::new(),created_at:Utc::now()};
        r.content_hash=cmp::hash(&r)?;self.inner.db.put_catalog_entry(&r)?;Ok(r)
    }
    fn update<F>(&self,id:Uuid,f:F)->anyhow::Result<Dossier>where F:FnOnce(&mut Dossier){
        let _guard=self.inner.gate.lock();let mut d=self.get(id)?;
        let before=(d.state.clone(),d.stage.clone(),d.completed_tasks,d.error.clone(),d.result.is_some());
        f(&mut d);
        let after=(d.state.clone(),d.stage.clone(),d.completed_tasks,d.error.clone(),d.result.is_some());
        if before!=after{d.updated_at=Utc::now();self.inner.db.put_dossier(&d)?;}Ok(d)
    }
    pub fn start(&self,id:Uuid)->anyhow::Result<Dossier>{
        let token=CancellationToken::new();let dossier={
            let _guard=self.inner.gate.lock();let mut d=self.get(id)?;
            if d.state!="draft"{bail!("verification attempts are immutable; freeze a new dossier to run again");}
            if !self.inner.active.lock().is_empty(){bail!("one independent worker at a time; stop or wait for the active verification");}
            d.state="running".into();d.stage="Probing trusted Python interpreter".into();d.updated_at=Utc::now();
            self.inner.db.put_dossier(&d)?;self.inner.active.lock().insert(id,token.clone());d
        };
        let service=self.clone();tokio::spawn(async move{
            if let Err(e)=service.execute(id,token).await{
                let _=service.update(id,|d|{if d.state!="cancelled"{d.state="failed".into();}d.stage="Verification stopped; inspect error".into();d.error=Some(format!("{e:#}"));});
            }
            service.inner.active.lock().remove(&id);
        });Ok(dossier)
    }
    pub fn cancel(&self,id:Uuid)->anyhow::Result<Dossier>{
        let _guard=self.inner.gate.lock();let mut d=self.get(id)?;
        if d.state!="running"{bail!("verification is not running");}
        let token=self.inner.active.lock().get(&id).cloned().context("no active verification for this dossier")?;
        d.state="stopping".into();d.stage="Stopping independent process".into();d.updated_at=Utc::now();self.inner.db.put_dossier(&d)?;token.cancel();Ok(d)
    }
    async fn execute(&self,id:Uuid,token:CancellationToken)->anyhow::Result<()> {
        let dossier=self.get(id)?;let started=Instant::now();
        let python=tokio::select!{_ = token.cancelled()=>{self.update(id,|d|{d.state="cancelled".into();d.stage="Stopped before worker launch".into();})?;return Ok(());},p=probe_python()=>p?};
        let folder=std::env::temp_dir().join(format!("phaseforge-verification-{}",Uuid::new_v4()));
        std::fs::create_dir(&folder)?;let cleanup=Cleanup(folder.clone());
        let (worker,reference)=frozen_sources(&dossier)?;
        std::fs::write(folder.join("verification_worker.py"),worker)?;std::fs::write(folder.join("reference_ode.py"),reference)?;
        let input=json!({"protocol":dossier.protocol,"manifest":dossier.manifest,"source_run":dossier.source_run,
            "controls":dossier.controls,"parameters":dossier.parameters,"protocol_hash":dossier.protocol_hash,
            "worker_source_hash":dossier.worker_source_hash,"reference_source_hash":dossier.reference_source_hash});
        let bytes=serde_json::to_vec(&input)?;if bytes.len()>MAX_PAYLOAD{bail!("worker input exceeds 64 MiB");}
        let input_hash=cmp::text_hash(std::str::from_utf8(&bytes)?);std::fs::write(folder.join("input.json"),bytes)?;
        let stderr=std::fs::File::create(folder.join("stderr.txt"))?;
        let mut child=Command::new(&python.0).args(&python.1).args(["-I","-u"]).arg(folder.join("verification_worker.py")).arg(&folder)
            .current_dir(&folder).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::from(stderr)).kill_on_drop(true)
            .env("OMP_NUM_THREADS","1").env("OPENBLAS_NUM_THREADS","1").spawn().context("cannot start the trusted independent worker")?;
        loop{
            if token.is_cancelled()||started.elapsed().as_secs()>dossier.protocol.wall_seconds+3{
                let _=child.kill().await;let _=child.wait().await;
                let cancelled=token.is_cancelled();
                let partial=bounded_read(&folder.join("checkpoint.json"),16*1024*1024).ok().and_then(|raw|serde_json::from_slice::<Value>(&raw).ok()).filter(|v|v["protocol_hash"]==dossier.protocol_hash&&v["input_sha256"]==input_hash&&v["worker_source_hash"]==dossier.worker_source_hash&&v["reference_source_hash"]==dossier.reference_source_hash);
                self.update(id,|d|{d.state=if cancelled{"cancelled"}else{"budget_exhausted"}.into();d.stage="Worker terminated; completed checkpoints retained, unfinished checks not verified".into();
                    if let Some(mut value)=partial{value["state"]=json!(d.state);value["partial"]=json!(true);value["checkpoint_notice"]=json!("Counters cover only the last completed checkpoint, not work interrupted inside a calculation.");d.result=Some(value);}
                })?;
                break;
            }
            if let Some(status)=child.try_wait()?{
                if !status.success(){
                    let err=bounded_read(&folder.join("stderr.txt"),8192).unwrap_or_default();
                    bail!("independent worker exited {status}: {}",String::from_utf8_lossy(&err));
                }
                let out:Value=serde_json::from_slice(&bounded_read(&folder.join("output.json"),16*1024*1024)?)?;
                if out["protocol_hash"]!=dossier.protocol_hash||out["input_sha256"]!=input_hash||out["worker_source_hash"]!=dossier.worker_source_hash||out["reference_source_hash"]!=dossier.reference_source_hash{bail!("worker provenance envelope does not match frozen inputs/source");}
                let state=out["state"].as_str().unwrap_or("failed").to_owned();
                if !["completed","failed","budget_exhausted"].contains(&state.as_str()){bail!("worker returned unsupported terminal state");}
                self.update(id,|d|{d.state=if d.state=="stopping"{"cancelled".into()}else{state};d.error=out["error"].as_str().map(str::to_owned);if d.state=="completed"{d.completed_tasks=d.total_tasks;}d.stage="Independent evidence ready for review".into();d.result=Some(out);})?;break;
            }
            if let Ok(raw)=bounded_read(&folder.join("progress.json"),8192){if let Ok(p)=serde_json::from_slice::<Value>(&raw){
                self.update(id,|d|{if d.state=="running"{d.stage=p["stage"].as_str().unwrap_or("Independent computation").chars().take(180).collect();d.completed_tasks=p["completed"].as_u64().unwrap_or(0).min(d.total_tasks as u64) as usize;}})?;
            }}
            tokio::select!{_=token.cancelled()=>{},_=tokio::time::sleep(Duration::from_millis(300))=>{}};
        }
        drop(child);drop(cleanup);Ok(())
    }
    pub fn assessment(&self,d:&Dossier)->anyhow::Result<Value>{assessment(&self.inner.db,d)}
    pub fn save_review(&self,id:Uuid,review:ReviewRequest)->anyhow::Result<ReviewRecord>{
        let _guard=self.inner.gate.lock();let d=self.get(id)?;
        if self.inner.db.list_verification_reviews(id)?.len()>=100{bail!("review history capacity reached; preserve this record and start a follow-up dossier");}
        if matches!(d.state.as_str(),"running"|"stopping"){bail!("wait for a stable verification record before review");}
        if !["artifact_or_failed","rediscovery","potentially_distinct","inconclusive"].contains(&review.disposition.as_str())||review.reviewer.trim().is_empty()||review.reviewer.len()>240||review.comparison_notes.trim().len()<20||review.comparison_notes.len()>16000||review.limitations.trim().len()<10||review.limitations.len()>16000||review.examined_sources.len()>100{bail!("named reviewer, allowed disposition, substantive comparison and limitations are required");}
        if review.examined_sources.iter().any(|v|v.trim().is_empty()||v.len()>2000){bail!("source reference too long");}
        let evidence=assessment(&self.inner.db,&d)?;
        let r=ReviewRecord{id:Uuid::new_v4(),dossier_id:id,review,evidence_hash:evidence["evidence_hash"].as_str().context("assessment digest missing")?.to_owned(),recorded_at:Utc::now()};
        self.inner.db.put_verification_review(&r)?;Ok(r)
    }
}

pub fn assessment(db:&Database,d:&Dossier)->anyhow::Result<Value>{
    let mut gaps=Vec::new();let result=d.result.as_ref().unwrap_or(&Value::Null).clone();
    if d.state!="completed"{gaps.push("Independent verification has not completed; approve the frozen budget or inspect the failure.");}
    if result["independent_agreement"]!=true{gaps.push("Agreement with both independent implementations is missing or failed.");}
    if d.frozen_refinement["status"]!="agreement"{gaps.push("The selected candidate lacks successful h/2 and h/4 checks. Freeze and validate finalists in a campaign first.");}
    if d.protocol.controls.is_empty(){gaps.push("No known calibration/control interval was declared. Add a completed control run and freeze a new dossier.");}
    else if result["controls_passed"]!=true{gaps.push("Declared controls are incomplete or failed; interpret candidate results only after diagnosing them.");}
    if result["robustness"]!="within_declared_limits"{gaps.push("New perturbations have not stayed within declared limits. Sensitivity may itself be interesting; it is not evidence of robust behavior.");}
    let catalog=cmp::catalog_report(d);
    if catalog["status"]=="no_comparable_reference"{gaps.push("No comparable documented numerical reference was frozen. An empty catalog is not novelty evidence.");}
    if catalog["status"]=="near_match"{gaps.push("A near match exists in the selected features; review possible rediscovery and feature inadequacy.");}
    let searches=db.list_literature_searches(d.id)?;
    let succeeded=searches.iter().any(|s|s.status=="completed");
    if !succeeded{gaps.push("No successful, recorded prior-work search. Failed/not-run searches are not evidence of absence.");}
    let evidence_hash=cmp::hash(&json!({"dossier":d,"searches":searches}))?;
    let reviews=db.list_verification_reviews(d.id)?;
    let current_review=reviews.iter().rev().find(|r|r.evidence_hash==evidence_hash);
    let reviewed=current_review.map(|r|r.review.full_text_reviewed&&!r.review.examined_sources.is_empty()).unwrap_or(false);
    if !reviewed{gaps.push("A named scientist still needs to read relevant source texts and record substantive comparisons.");}
    if let Some(r)=current_review {
        if r.review.disposition!="potentially_distinct" { gaps.push("The current human review does not identify a potentially distinct result. Preserve that disposition rather than promoting a novelty claim."); }
    }
    Ok(json!({"dossier_id":d.id,"evidence_hash":evidence_hash,"protocol_hash":d.protocol_hash,"catalog":catalog,"within_study":d.within_study,
        "independent_agreement":result["independent_agreement"],"robustness":result["robustness"],"controls_passed":result["controls_passed"],
        "searches":searches,"reviews":reviews,"gaps":gaps,"next_step":gaps.first().copied().unwrap_or("Assemble the reviewed evidence package for external scientific review."),
        "status":if gaps.is_empty(){"ready_for_external_review"}else{"evidence_gaps_remain"},"novelty":"not_certified",
        "scope":"Frozen post-selection verification; not external preregistration. Numeric feature distance is not equivalence classification. No statistical significance is inferred from an adaptively chosen winner."}))
}
fn bounded_read(path:&std::path::Path,limit:usize)->anyhow::Result<Vec<u8>>{
    use std::io::Read;let f=std::fs::File::open(path)?;let mut bytes=Vec::new();f.take(limit as u64+1).read_to_end(&mut bytes)?;
    if bytes.len()>limit{bail!("worker output exceeded its byte limit");}Ok(bytes)
}
struct Cleanup(PathBuf);impl Drop for Cleanup{fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
pub async fn probe_python()->anyhow::Result<(PathBuf,Vec<String>)>{
    let root=PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().context("project root unavailable")?.to_path_buf();
    let mut candidates=Vec::<(PathBuf,Vec<String>)>::new();
    if let Ok(value)=std::env::var("PHASEFORGE_VERIFIER_PYTHON") {let p=PathBuf::from(value);if p.is_absolute()&&p.is_file(){candidates.push((p,vec![]));}}
    candidates.push((root.join(".phaseforge-verifier").join(if cfg!(windows){"Scripts/python.exe"}else{"bin/python"}),vec![]));
    for name in if cfg!(windows){vec!["python.exe","py.exe"]}else{vec!["python3","python"]}{candidates.push((PathBuf::from(name),if name=="py.exe"{vec!["-3".into()]}else{vec![]}));}
    for (program,prefix) in candidates{
        let mut command=Command::new(&program);command.args(&prefix).args(["-I","-c","import sys; assert sys.version_info >= (3,10); print(sys.version)"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
        if let Ok(Ok(status))=tokio::time::timeout(Duration::from_secs(3),command.status()).await {if status.success(){return Ok((program,prefix));}}
    }
    bail!("Python 3.10+ verifier unavailable. Paste INSTALL_VERIFIER.txt from the project root, then restart the backend. No scientific packages or API keys are required.")
}
