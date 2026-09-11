//! Immutable, resumable batches of real solver jobs. No model call per case.
use super::{generated::SourceImport, write_json, LabJob, LaboratoryService};
use anyhow::{bail, ensure, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Read, path::Path, time::{Duration, Instant}};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MIB:u64=1024*1024;
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepCase {
    pub case_id:String, pub engine:String, pub parameters:Value,
    #[serde(default)] pub sources:Vec<SourceImport>,
    #[serde(default)] pub metadata:Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AppConfig,domain::{CreateProjectRequest,ResearchProject},persistence::Database};
    fn fixture(path:&Path)->(LaboratoryService,Uuid){
        fs::create_dir_all(path).unwrap();
        let config=AppConfig{data_directory:path.into(),gpu_enabled:false,..Default::default()};
        let db=Database::open(&config.database_path()).unwrap();let project=ResearchProject::new(CreateProjectRequest{name:Some("Sweep acceptance".into()),question:"Fixed-case execution and recovery".into()});db.put_project(&project).unwrap();
        (LaboratoryService::new(db,config).unwrap(),project.id)
    }
    fn input()->Value{json!({"question":"Compare fixed diffusion cases","storage_mb":256,"source_session_id":Uuid::nil(),"cases":[{"case_id":"control","engine":"diffusion_2d","parameters":{"nx":16,"ny":16,"steps":8,"record_interval":8}}]})}
    #[test]fn sweep_rejects_duplicate_cases_unknown_engines_and_invalid_budget(){
        assert!(validate(&input()).is_ok());
        let mut duplicate=input();let first=duplicate["cases"][0].clone();duplicate["cases"].as_array_mut().unwrap().push(first);assert!(validate(&duplicate).is_err());
        let mut invalid=input();invalid["cases"][0]["engine"]=json!("invented_solver");assert!(validate(&invalid).is_err());
        invalid=input();invalid["storage_mb"]=json!(1);assert!(validate(&invalid).is_err());
        invalid=input();invalid["cases"][0]["parameters"]["dt_s"]=json!(100000);assert!(validate(&invalid).is_err());
    }
    #[test]fn stop_and_child_admission_share_a_gate_and_exact_deadline(){
        let temp=tempfile::tempdir().unwrap();let (service,project)=fixture(temp.path());let parent=service.create(Uuid::new_v4(),project,None,"session","parent",json!({}),Some(Utc::now()+chrono::Duration::seconds(45))).unwrap();
        let child=service.create_active_child(Uuid::new_v4(),parent.id,"sweep","batch",input()).unwrap();assert_eq!(child.deadline_at,parent.deadline_at);
        service.stop(parent.id,"paused").unwrap();assert_eq!(service.get(child.id).unwrap().state,"paused");
        assert!(service.create_active_child(Uuid::new_v4(),parent.id,"solver","late",json!({})).is_err());
    }
    #[test]fn completed_case_reuse_rejects_altered_science_but_allows_presentation(){
        let temp=tempfile::tempdir().unwrap();let (service,project)=fixture(temp.path());let sweep=service.create(Uuid::new_v4(),project,None,"sweep","batch",input(),None).unwrap();
        let case=validate(&sweep.input).unwrap().cases.remove(0);
        let job=service.create(Uuid::new_v4(),project,Some(sweep.id),"solver","fixture receipts only",solver_input(&sweep,&case),None).unwrap();
        for name in ["result.json","manifest.json","measurements.json"]{fs::write(service.directory(job.id).join(name),b"{}").unwrap();}
        service.update(job.id,|job|job.state="completed".into()).unwrap();let job=service.get(job.id).unwrap();let hash=digest(&job.input).unwrap();let receipt=service.sweep_completion(&job,&hash).unwrap();
        let record=CaseRecord{case_id:case.case_id,input_sha256:hash.clone(),attempts:vec![job.id],completion:Some(receipt.clone())};
        write_json(&service.directory(sweep.id).join("sweep-ledger.json"),&Ledger{schema_version:1,input_sha256:digest(&sweep.input).unwrap(),cases:vec![record.clone()]}).unwrap();
        service.update(sweep.id,|job|job.state="completed".into()).unwrap();service.verify_retained_sweep(sweep.id).unwrap();
        fs::write(service.directory(job.id).join("presentation.json"),b"{\"color\":\"blue\"}").unwrap();assert!(service.verify_sweep_completion(&receipt,&hash,&sweep,&record).is_ok());
        service.update(job.id,|job|job.project_id=Uuid::new_v4()).unwrap();assert!(service.verify_sweep_completion(&receipt,&hash,&sweep,&record).is_err());
        assert!(service.verify_retained_sweep(sweep.id).is_err());
        service.update(job.id,|job|{job.project_id=project;job.parent_id=Some(Uuid::new_v4());}).unwrap();assert!(service.verify_sweep_completion(&receipt,&hash,&sweep,&record).is_err());
        service.update(job.id,|job|job.parent_id=Some(sweep.id)).unwrap();
        let mut unreserved=record.clone();unreserved.attempts.clear();assert!(service.verify_sweep_completion(&receipt,&hash,&sweep,&unreserved).is_err());
        fs::write(service.directory(job.id).join("measurements.json"),b"{\"modified\":true}").unwrap();assert!(service.verify_sweep_completion(&receipt,&hash,&sweep,&record).is_err());
        assert!(service.verify_sweep_completion(&receipt,"changed_input",&sweep,&record).is_err());
    }
    #[test]fn detached_continuation_keeps_original_case_input_identity(){
        let temp=tempfile::tempdir().unwrap();let (service,project)=fixture(temp.path());let mut job=service.create(Uuid::new_v4(),project,Some(Uuid::new_v4()),"sweep","batch",input(),None).unwrap();let case=validate(&job.input).unwrap().cases.remove(0);let before=digest(&solver_input(&job,&case)).unwrap();job.parent_id=None;assert_eq!(before,digest(&solver_input(&job,&case)).unwrap());
    }
    #[test]fn storage_counts_every_failed_paused_completed_and_reserved_attempt(){
        let temp=tempfile::tempdir().unwrap();let (service,project)=fixture(temp.path());let sweep=service.create(Uuid::new_v4(),project,None,"sweep","batch",input(),None).unwrap();
        let case=validate(&sweep.input).unwrap().cases.remove(0);let child_input=solver_input(&sweep,&case);
        let mut record=CaseRecord{case_id:case.case_id,input_sha256:digest(&child_input).unwrap(),..Default::default()};let mut expected=0u64;
        for (state,size) in [("failed",17usize),("paused",29),("completed",41)]{
            let child=service.create(Uuid::new_v4(),project,Some(sweep.id),"solver",state,child_input.clone(),None).unwrap();
            service.update(child.id,|job|job.state=state.into()).unwrap();
            fs::write(service.directory(child.id).join("output.tmp"),vec![7u8;size]).unwrap();
            fs::write(service.directory(child.id).join("presentation.json"),b"{}").unwrap();
            expected+=service.artifact_inventory(child.id).unwrap().as_array().unwrap().iter().map(|item|item["bytes"].as_u64().unwrap()).sum::<u64>();record.attempts.push(child.id);
        }
        let pending=Uuid::new_v4();fs::create_dir_all(service.directory(pending)).unwrap();fs::write(service.directory(pending).join("partial.tmp"),b"reserved bytes").unwrap();record.attempts.push(pending);expected+=14;
        let mut ledger=Ledger{schema_version:1,input_sha256:digest(&sweep.input).unwrap(),cases:vec![record]};
        assert_eq!(service.sweep_retained_bytes(&sweep,&ledger).unwrap(),expected);
        let duplicate=ledger.cases[0].attempts[0];ledger.cases[0].attempts.push(duplicate);assert!(service.sweep_retained_bytes(&sweep,&ledger).is_err());
    }
    #[tokio::test]async fn retained_failed_attempt_over_budget_prevents_new_solver_admission(){
        let temp=tempfile::tempdir().unwrap();let (service,project)=fixture(temp.path());let sweep=service.create(Uuid::new_v4(),project,None,"sweep","batch",input(),None).unwrap();
        let case=validate(&sweep.input).unwrap().cases.remove(0);let child_input=solver_input(&sweep,&case);
        let child=service.create(Uuid::new_v4(),project,Some(sweep.id),"solver","retained failed attempt",child_input.clone(),None).unwrap();service.update(child.id,|job|job.state="failed".into()).unwrap();
        fs::File::create(service.directory(child.id).join("retained-output.bin")).unwrap().set_len(256*MIB+1).unwrap();
        let ledger=Ledger{schema_version:1,input_sha256:digest(&sweep.input).unwrap(),cases:vec![CaseRecord{case_id:case.case_id,input_sha256:digest(&child_input).unwrap(),attempts:vec![child.id],completion:None}]};
        write_json(&service.directory(sweep.id).join("sweep-ledger.json"),&ledger).unwrap();
        let error=service.execute_sweep(sweep.id,&CancellationToken::new()).await.unwrap_err();assert!(error.to_string().contains("All retained sweep attempts exceed"),"{error:#}");
        assert_eq!(service.list(Some(project)).unwrap().len(),2,"No new attempt should be created or scheduled");
        assert!(!service.executing(child.id));assert_eq!(service.get(child.id).unwrap().state,"failed");
    }
    #[tokio::test]
    #[ignore = "Actual isolated acceptance directory and a local bootstrap Python are required"]
    async fn actual_sweep_pauses_between_cases_preserves_completed_bytes_and_resumes(){
        let root=std::path::PathBuf::from(std::env::var_os("PHASEFORGE_SWEEP_ACCEPTANCE_ROOT").expect("Set an explicit acceptance output directory"));
        assert!(root.is_absolute());fs::create_dir_all(&root).unwrap();let run=root.join(Uuid::new_v4().to_string());let (service,project)=fixture(&run);
        let mut request=input();request["storage_mb"]=json!(512);
        request["cases"].as_array_mut().unwrap().extend([
            json!({"case_id":"long","engine":"diffusion_2d","parameters":{"nx":32,"ny":32,"steps":20000,"record_interval":100,"dt_s":0.001}}),
            json!({"case_id":"last","engine":"diffusion_2d","parameters":{"nx":16,"ny":16,"steps":16,"record_interval":8}})]);
        let job=service.create(Uuid::new_v4(),project,None,"sweep","Actual sequential diffusion recovery",request,Some(Utc::now()+chrono::Duration::minutes(6))).unwrap();service.start_sweep(job.id).unwrap();
        let limit=Instant::now()+Duration::from_secs(300);let paused_child=loop{
            assert!(Instant::now()<limit,"Actual batch did not expose an intermediate case");let current=service.get(job.id).unwrap();assert!(current.active(),"Batch stopped before pause: {:?}",current.error);
            let children=service.list(Some(project)).unwrap();
            if let Some(child)=children.iter().find(|j|j.parent_id==Some(job.id)&&j.input["sweep"]["case_id"]=="long"&&j.state=="running"){
                if child.progress["step"].as_u64().unwrap_or(0)>0{let child=child.id;service.stop(job.id,"paused").unwrap();break child;}
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        while service.executing(job.id)||service.executing(paused_child){assert!(Instant::now()<limit);tokio::time::sleep(Duration::from_millis(50)).await;}
        let ledger_path=service.directory(job.id).join("sweep-ledger.json");let before:Ledger=serde_json::from_slice(&fs::read(&ledger_path).unwrap()).unwrap();let first=before.cases[0].completion.clone().expect("First real case must complete before pause");
        assert!(before.cases[2].attempts.is_empty(),"Stop must prevent the final case from being scheduled");assert_eq!(service.get(paused_child).unwrap().state,"paused");
        service.resume_sweep(job.id,job.deadline_at).unwrap();
        loop{let current=service.get(job.id).unwrap();if !current.active(){assert_eq!(current.state,"completed","{:?}",current.error);break;}assert!(Instant::now()<limit);tokio::time::sleep(Duration::from_millis(50)).await;}
        let after:Ledger=serde_json::from_slice(&fs::read(&ledger_path).unwrap()).unwrap();assert_eq!(after.cases[0].completion,Some(first.clone()));assert_eq!(after.cases[0].attempts.len(),1);assert_eq!(after.cases[1].attempts.len(),2);assert_eq!(after.cases[2].attempts.len(),1);
        service.verify_sweep_completion(&first,&after.cases[0].input_sha256,&job,&after.cases[0]).unwrap();
        for case in &after.cases{let done=case.completion.as_ref().unwrap();service.verify_sweep_completion(done,&case.input_sha256,&job,case).unwrap();}
        let report=json!({"passed":true,"scope":"Actual service batch and managed diffusion worker lifecycle; not installed UI acceptance or new scientific reference validation","job_id":job.id,"project_id":project,"data_directory":run,"paused_child_id":paused_child,"cases":after.cases,"checks":{"three_measured_cases":true,"paused_after_real_progress":true,"completed_case_hashes_unchanged":true,"unscheduled_case_preserved_on_pause":true,"new_attempt_lineage":true,"all_final_hashes_verified":true}});
        write_json(&root.join("latest-report.json"),&report).unwrap();println!("{}",json!({"report":root.join("latest-report.json"),"job_id":job.id,"passed":true}));
    }
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SweepInput {
    pub question:String, pub cases:Vec<SweepCase>, pub storage_mb:u64,
    #[serde(default)] pub protocol:Value,
    #[serde(default)] pub source_session_id:Option<Uuid>,
}
#[derive(Default,Clone,Debug,Serialize,Deserialize)]
struct CaseRecord { case_id:String, input_sha256:String, attempts:Vec<Uuid>, completion:Option<Value> }
#[derive(Default,Clone,Debug,Serialize,Deserialize)]
struct Ledger { schema_version:u32, input_sha256:String, cases:Vec<CaseRecord> }

pub fn capability()->Value {
    json!({"tool":"launch_solver_sweep","scope":"A fixed list of 1–128 real supported solver inputs, executed sequentially as durable child jobs. No model requests between cases.",
        "input":{"question":"scientific question","cases":[{"case_id":"unique stable label","engine":"catalog engine","parameters":{},"sources":[],"metadata":{"condition_id":"optional","role":"optional","replicate":"optional"}}],"storage_mb":"256–16384; monitored logical bytes of every retained attempt, including failed/interrupted output and temporary files; soft limit with possible polling overshoot","protocol":"optional immutable scientific protocol"},
        "recovery":"Completed cases are reused only after checking their exact input and retained file hashes. Interrupted cases retain lineage and receive a new immutable attempt on explicit resume.",
        "concurrency":1,"time":"Inherits the parent deadline; Off has no hidden total timer. Storage and per-engine bounds still apply.",
        "failure":"A failed case stops scheduling. No missing labels are imputed or dropped. Final result contains every case and exact solver identity."})
}
fn digest(value:&impl Serialize)->anyhow::Result<String>{Ok(format!("{:x}",Sha256::digest(serde_json::to_vec(value)?)))}
pub fn validate(value:&Value)->anyhow::Result<SweepInput>{
    let input:SweepInput=serde_json::from_value(value.clone()).context("Invalid solver sweep input")?;
    ensure!(!input.question.trim().is_empty()&&input.question.len()<=8000,"State the bounded scientific question");
    ensure!((1..=128).contains(&input.cases.len()),"A sweep needs 1–128 immutable cases");
    ensure!((256..=16384).contains(&input.storage_mb),"Sweep storage must be 256–16384 MiB");
    ensure!(serde_json::to_vec(value)?.len()<=2*MIB as usize,"Sweep input exceeds 2 MiB");
    let mut names=BTreeSet::new();
    for case in &input.cases{
        ensure!(!case.case_id.is_empty()&&case.case_id.len()<=100&&case.case_id.bytes().all(|c|c.is_ascii_alphanumeric()||b"-_.".contains(&c)),"Case IDs use 1–100 ASCII letters, numbers, dashes, dots or underscores");
        ensure!(names.insert(&case.case_id),"Duplicate sweep case ID");
        ensure!(["openmm_argon","diffusion_2d","newtonian_nbody"].contains(&case.engine.as_str())&&case.parameters.is_object(),"Each case needs an executable solver and parameter object");
        ensure!(case.sources.len()<=32&&serde_json::to_vec(&case.metadata)?.len()<=8192,"Case source or metadata limit exceeded");
        for source in &case.sources{super::safe_relative(&source.path)?;super::safe_relative(&source.destination)?;}
        if case.engine=="diffusion_2d"{super::field::validate(&case.parameters)?;}
    }
    Ok(input)
}
fn solver_input(sweep:&LabJob,case:&SweepCase)->Value{
    json!({"engine":case.engine,"parameters":case.parameters,"sources":case.sources,"question":sweep.input["question"],
        "hypothesis":"Fixed batch case; compare the declared condition groups using measured outputs.","source_session_id":sweep.input["source_session_id"],
        "sweep":{"job_id":sweep.id,"case_id":case.case_id,"metadata":case.metadata,"protocol":sweep.input["protocol"]}})
}
fn hash_file(path:&Path)->anyhow::Result<(u64,String)>{
    let mut file=fs::File::open(path)?;let length=file.metadata()?.len();let mut hash=Sha256::new();let mut buffer=[0u8;65536];let mut total=0u64;
    loop{let n=file.read(&mut buffer)?;if n==0{break;}total+=n as u64;hash.update(&buffer[..n]);}
    ensure!(total==length,"Retained artifact changed during hashing");Ok((total,format!("{:x}",hash.finalize())))
}
fn retained_science(name:&str)->bool{
    !name.ends_with(".tmp")&&!name.starts_with("presentation")&&!name.starts_with("cancel.")
}
impl LaboratoryService {
    /// New child admission is serialized with Stop. Historical matching receipts
    /// may be read again, but cannot grant a fresh deadline or launch under Stop.
    pub fn create_active_child(&self,id:Uuid,parent_id:Uuid,kind:&str,title:&str,input:Value)->anyhow::Result<LabJob>{
        let _guard=self.gate.lock();let parent=self.get(parent_id)?;
        ensure!(parent.active()&&parent.deadline_at.is_none_or(|at|at>Utc::now()),"The parent is stopped, paused, complete, or out of time");
        self.create_locked(id,parent.project_id,Some(parent_id),kind,title,input,parent.deadline_at)
    }
    pub fn start_sweep(&self,id:Uuid)->anyhow::Result<()>{
        let job=self.get(id)?;ensure!(job.kind=="sweep"&&job.state=="queued","Only a queued sweep can start");validate(&job.input)?;
        let token=if let Some(parent)=job.parent_id{
            let parent=self.get(parent)?;ensure!(parent.active()&&parent.project_id==job.project_id,"The parent must be active when starting the sweep");
            self.acquire_child(id,&self.execution_token(parent.id).context("Parent execution is unavailable")?)?
        }else{self.acquire(id)?};
        let service=self.clone();tokio::spawn(async move{
            let deadline=service.get(id).ok().and_then(|j|j.deadline_at);
            let limit_token=token.clone();let timer=deadline.map(|at|tokio::spawn(async move{tokio::time::sleep((at-Utc::now()).to_std().unwrap_or_default()).await;limit_token.cancel();}));
            let result=service.execute_sweep(id,&token).await;
            if let Some(timer)=timer{timer.abort();}
            if let Err(error)=result{
                let current=service.get(id).ok();let state=if current.as_ref().is_some_and(|j|!j.active()){current.as_ref().unwrap().state.as_str()}
                    else if deadline.is_some_and(|at|at<=Utc::now()){"timed_out"}else if token.is_cancelled(){"cancelled"}else{"failed"};
                let _=service.stop(id,state);
                let _=service.update(id,|j|{j.error=Some(format!("{error:#}"));j.event("sweep_stopped","Batch scheduling stopped; completed cases and failed attempts remain saved.",json!({"error":format!("{error:#}")}));});
            }
            service.release(id);
        });Ok(())
    }
    pub fn resume_sweep(&self,id:Uuid,deadline:Option<DateTime<Utc>>)->anyhow::Result<()>{
        {
            let _guard=self.gate.lock();let mut job=self.get(id)?;
            ensure!(job.kind=="sweep"&&matches!(job.state.as_str(),"paused"|"failed"|"timed_out"),"Sweep is not resumable");
            ensure!(!self.executing(id),"The previous batch is still stopping");
            for child in self.list(Some(job.project_id))?.into_iter().filter(|j|j.parent_id==Some(id)){
                ensure!(!self.executing(child.id),"A previous case is still stopping; wait for its retained receipt");
            }
            if let Some(parent_id)=job.parent_id{
                let parent=self.get(parent_id)?;
                if parent.active()&&parent.deadline_at.is_none_or(|at|at>Utc::now()){
                    ensure!(deadline==parent.deadline_at,"Resume the parent to change a live child budget");
                }else{job.parent_id=None;job.event("independent_continuation","Explicit continuation detached from the inactive parent; original lineage remains in the event and solver inputs.",json!({"original_parent_id":parent_id}));}
            }
            ensure!(deadline.is_none_or(|at|at>Utc::now()),"Choose a remaining explicit continuation budget or Off");
            job.state="queued".into();job.deadline_at=deadline;job.error=None;
            job.event("sweep_resume","Completed cases will be verified; incomplete cases receive retained new attempts.",json!({"deadline_at":deadline}));self.database.put_lab_record(&job)?;
        }
        self.start_sweep(id)
    }
    fn sweep_completion(&self,job:&LabJob,input_hash:&str)->anyhow::Result<Value>{
        ensure!(job.kind=="solver"&&job.state=="completed"&&digest(&job.input)?==input_hash,"Completed case identity changed");
        let inventory=self.artifact_inventory(job.id)?;let mut artifacts=Vec::new();let mut bytes=0u64;
        for item in inventory.as_array().context("Invalid solver inventory")?{
            let name=item["path"].as_str().context("Missing artifact path")?;if !retained_science(name){continue;}
            let (length,hash)=hash_file(&self.path(job.id,name)?)?;bytes=bytes.checked_add(length).context("Output byte count overflow")?;
            artifacts.push(json!({"path":name,"bytes":length,"sha256":hash}));
        }
        ensure!(["result.json","manifest.json","measurements.json"].iter().all(|name|artifacts.iter().any(|a|a["path"]==*name)),"Completed case lacks required scientific receipts");
        Ok(json!({"job_id":job.id,"input_sha256":input_hash,"artifacts":artifacts,"bytes":bytes,"completed_at":job.completed_at,"solver_wall_seconds":(job.completed_at.unwrap_or(job.updated_at)-job.created_at).num_milliseconds() as f64/1000.0}))
    }
    fn verify_sweep_completion(&self,value:&Value,input_hash:&str,sweep:&LabJob,record:&CaseRecord)->anyhow::Result<()>{
        ensure!(value["input_sha256"]==input_hash,"Retained sweep input pin changed");let id=Uuid::parse_str(value["job_id"].as_str().context("Missing completed job ID")?)?;
        ensure!(record.attempts.contains(&id),"Completed job is not a reserved attempt of this case");
        let job=self.get(id)?;self.verify_sweep_lineage(sweep,record,&job)?;
        ensure!(job.state=="completed"&&digest(&job.input)?==input_hash,"Retained completed case changed");
        for item in value["artifacts"].as_array().context("Completed case has no artifact pins")?{
            let (bytes,hash)=hash_file(&self.path(id,item["path"].as_str().context("Artifact path missing")?)?)?;
            ensure!(item["sha256"]==hash&&item["bytes"]==bytes,"A completed case artifact no longer matches its frozen bytes");
        }Ok(())
    }
    /// Reverify descendants as well as the sweep journal before downstream data
    /// reduction. Hashing only the sweep directory does not pin solver labels.
    pub(crate) fn verify_retained_sweep(&self,id:Uuid)->anyhow::Result<()>{
        let sweep=self.get(id)?;ensure!(sweep.kind=="sweep"&&sweep.state=="completed","Only a completed sweep may admit retained labels");
        let input=validate(&sweep.input)?;let ledger:Ledger=serde_json::from_value(self.read_json(id,"sweep-ledger.json")?)?;
        ensure!(ledger.schema_version==1&&ledger.input_sha256==digest(&sweep.input)?&&ledger.cases.len()==input.cases.len(),"Retained sweep journal identity changed");
        for (record,case) in ledger.cases.iter().zip(&input.cases){
            let expected=digest(&solver_input(&sweep,case))?;
            ensure!(record.case_id==case.case_id&&record.input_sha256==expected,"Retained sweep case ordering or input changed");
            self.verify_sweep_completion(record.completion.as_ref().context("A sweep case lacks completion")?,&expected,&sweep,record)?;
        }
        ensure!(self.sweep_retained_bytes(&sweep,&ledger)?<=input.storage_mb*MIB,"Retained sweep attempts exceed their declared storage budget");
        Ok(())
    }
    fn verify_sweep_lineage(&self,sweep:&LabJob,record:&CaseRecord,job:&LabJob)->anyhow::Result<()>{
        ensure!(job.kind=="solver"&&job.project_id==sweep.project_id&&job.parent_id==Some(sweep.id)
            &&job.input["sweep"]["job_id"]==json!(sweep.id)&&job.input["sweep"]["case_id"]==record.case_id
            &&digest(&job.input)?==record.input_sha256,"Retained case does not belong to its expected sweep, project or fixed input");
        Ok(())
    }
    fn sweep_retained_bytes(&self,sweep:&LabJob,ledger:&Ledger)->anyhow::Result<u64>{
        let mut total=0u64;let mut seen=BTreeSet::new();
        for record in &ledger.cases{for id in &record.attempts{
            ensure!(seen.insert(*id),"A solver attempt is reserved by more than one sweep case");
            // A reservation may exist before job creation. Its directory, if any,
            // still consumes the declared budget even without a final DB row.
            if let Some(attempt)=self.database.lab_record(*id)?{self.verify_sweep_lineage(sweep,record,&attempt)?;}
            if !self.directory(*id).exists(){continue;}
            for item in self.artifact_inventory(*id)?.as_array().context("Invalid retained attempt inventory")?{
                total=total.checked_add(item["bytes"].as_u64().context("Missing retained attempt byte count")?).context("Sweep output byte count overflow")?;
            }
        }}
        Ok(total)
    }
    async fn execute_sweep(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<()>{
        let job=self.get(id)?;let input=validate(&job.input)?;let path=self.directory(id).join("sweep-ledger.json");let input_hash=digest(&job.input)?;
        let mut ledger:Ledger=if path.is_file(){serde_json::from_slice(&fs::read(&path)?)?}else{
            Ledger{schema_version:1,input_sha256:input_hash.clone(),cases:input.cases.iter().map(|case|Ok(CaseRecord{case_id:case.case_id.clone(),input_sha256:digest(&solver_input(&job,case))?,..Default::default()})).collect::<anyhow::Result<_>>()?}
        };
        ensure!(ledger.input_sha256==input_hash&&ledger.cases.len()==input.cases.len(),"Sweep input or ledger identity changed");
        write_json(&path,&ledger)?;
        let budget=input.storage_mb*MIB;
        ensure!(super::generated::free_bytes(&self.directory(id))?>=budget+256*MIB,"Insufficient free space for the declared sweep budget plus reserve");
        self.update(id,|j|{if j.active(){j.state="running".into();j.event("sweep_started","Executing fixed cases sequentially; model tokens are not used between cases.",json!({"case_count":input.cases.len(),"concurrency":1,"storage_mb":input.storage_mb}));}})?;
        let started=Instant::now();let mut completed_bytes=0u64;
        for (index,case) in input.cases.iter().enumerate(){
            ensure!(!token.is_cancelled()&&self.get(id)?.active(),"Sweep stopped before scheduling the next case");
            ensure!(self.sweep_retained_bytes(&job,&ledger)?<=budget,"All retained sweep attempts exceed the monitored storage budget; no new case was scheduled");
            let record=&mut ledger.cases[index];let case_input=solver_input(&job,case);let case_hash=digest(&case_input)?;
            ensure!(record.case_id==case.case_id&&record.input_sha256==case_hash,"Case ordering/input changed");
            if let Some(completion)=&record.completion{
                self.verify_sweep_completion(completion,&case_hash,&job,record)?;completed_bytes+=completion["bytes"].as_u64().context("Missing byte count")?;
                continue;
            }
            let reserved=record.attempts.last().copied();
            let existing=reserved.and_then(|id|self.get(id).ok());
            let target=match existing{
                Some(previous) if previous.state=="completed"||previous.state=="queued"=>previous.id,
                Some(previous) if previous.active()=>bail!("The preceding attempt is still active; reconcile it before resuming"),
                None if reserved.is_some()=>reserved.unwrap(),
                _=>{let target=Uuid::new_v4();record.attempts.push(target);target}
            };
            // Persist reservation before creation. Crash recovery reuses this ID.
            write_json(&path,&ledger)?;
            let child=self.create_active_child(target,id,"solver",&format!("{} · {}",input.question,case.case_id),case_input)?;
            self.event(id,"sweep_case",format!("Computing case {} of {}: {}",index+1,input.cases.len(),case.case_id),json!({"case_id":case.case_id,"job_id":target,"index":index,"metadata":case.metadata}))?;
            if child.state=="queued"&&!self.executing(target){self.start_solver(target)?;}
            let completed=loop{
                ensure!(!token.is_cancelled(),"Sweep stopped while a case was running");let current=self.get(target)?;
                if !current.active(){break current;}
                let bytes=self.artifact_inventory(target)?.as_array().into_iter().flatten().map(|a|a["bytes"].as_u64().unwrap_or(0)).sum::<u64>();
                let retained_bytes=self.sweep_retained_bytes(&job,&ledger)?;
                ensure!(retained_bytes<=budget&&super::generated::free_bytes(&self.directory(id))?>=64*MIB,"Sweep exceeded monitored storage budget or reserve including every retained failed/interrupted attempt (soft limit, overshoot possible)");
                self.update(id,|j|{if j.active(){j.progress=json!({"fraction":index as f64/input.cases.len() as f64,"case_index":index,"case_count":input.cases.len(),"case_id":case.case_id,"child_job_id":target,"case_progress":current.progress,"completed_bytes":completed_bytes,"active_bytes":bytes,"retained_bytes":retained_bytes,"elapsed_seconds":started.elapsed().as_secs_f64()});}})?;
                tokio::select!{_=token.cancelled()=>bail!("Sweep stopped"),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
            };
            ensure!(completed.state=="completed","Case {} ended {}: {}. No later cases were scheduled.",case.case_id,completed.state,completed.error.as_deref().unwrap_or_default());
            self.verify_sweep_lineage(&job,&ledger.cases[index],&completed)?;
            let completion=self.sweep_completion(&completed,&case_hash)?;completed_bytes+=completion["bytes"].as_u64().unwrap();
            let total=self.sweep_retained_bytes(&job,&ledger)?;
            ensure!(total<=budget,"Completed case exceeded the monitored sweep storage budget");
            ledger.cases[index].completion=Some(completion);write_json(&path,&ledger)?;
            self.event(id,"sweep_case_completed",format!("Saved measured result for {}.",case.case_id),json!({"case_id":case.case_id,"job_id":target,"completed_cases":index+1,"total_cases":input.cases.len(),"retained_bytes":total}))?;
        }
        let total=self.sweep_retained_bytes(&job,&ledger)?;
        ensure!(total<=budget,"All retained sweep attempts exceed the monitored storage budget");
        let result=json!({"status":"completed","case_count":ledger.cases.len(),"completed_cases":ledger.cases.len(),"cases":ledger.cases,"input_sha256":input_hash,"retained_bytes":total,"concurrency":1,"elapsed_this_continuation_seconds":started.elapsed().as_secs_f64(),"scientific_validation":"Each solver needs its own reference and downstream evaluation; batch completion does not validate predictions."});
        write_json(&self.directory(id).join("result.json"),&result)?;
        self.update(id,|j|{if j.active()&&!token.is_cancelled(){j.state="completed".into();j.result=result;j.progress=json!({"fraction":1.0,"completed_cases":input.cases.len(),"case_count":input.cases.len()});j.event("completed","All fixed solver cases and their numerical artifact hashes are retained.",json!({"retained_bytes":total}));}})?;Ok(())
    }
}
