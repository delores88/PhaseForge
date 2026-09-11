//! A durable, preregistered solver-to-surrogate study. The coordinator, rather
//! than generated code or a model-supplied role tag, owns data admission.
use super::{generated::SourceImport, write_json, LabJob, LaboratoryService};
use anyhow::{bail, ensure, Context};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, fs, time::Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) const PROPOSAL:&str="b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737";
pub(super) const WORKER:&str=include_str!("../../../tools/ml_surrogate_worker.py");
pub(super) const SOLVER:&str=include_str!("../../../tools/scientific_worker.py");
const DATASET_EXPORT:&str=include_str!("../../../tools/ml_dataset_export.py");
#[cfg(test)]
#[path="ml_study_tests.rs"] mod tests;
pub(super) fn sha(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn value_sha(value:&Value)->anyhow::Result<String>{Ok(sha(&serde_json::to_vec(value)?))}
fn retained_attempt_seconds(job:&LabJob)->anyhow::Result<f64>{
    let at=job.events.iter().rev().find(|event|matches!(event.kind.as_str(),"completed"|"failed"|"paused"|"cancelled"|"timed_out"|"interrupted"|"worker_stopped"|"generated_stopped"|"study_stopped"))
        .map(|event|event.at).or(job.completed_at).context("An attempted computation lacks a retained terminal timestamp")?;
    Ok((at-job.created_at).num_milliseconds().max(0) as f64/1000.0)
}

#[derive(Clone,Default,Serialize,Deserialize)]
struct Stage { input_sha256:String, kind:String, attempts:Vec<Uuid>, completion:Option<Value> }
#[derive(Clone,Default,Serialize,Deserialize)]
struct Ledger { schema_version:u32, input_sha256:String, stages:BTreeMap<String,Stage>, model_freeze:Option<Value>, #[serde(default)] cost_snapshot:Option<Value> }

pub fn capability()->Value{
    json!({"id":"argon_pressure_surrogate_v1","tool":"launch_ml_study","scope":"A real, bounded ML study of a finite-window classical argon pressure endpoint; not biology, equilibrium or empirical validation.",
        "protocol_sha256":PROPOSAL,"computation":"99 actual OpenMM runs: 32 fixed conditions × 3 seeds, plus three fresh out-of-domain fallback runs. A pilot is reused, never discarded.",
        "learning":"72 fixed polynomial/kernel ridge candidates. Whole-condition train/validation/calibration/test/regime splits frozen before any labels. Model freeze precedes calibration and held-out access.",
        "evaluation":"Independent baselines, per-seed uncertainty, calibration coverage, support/accuracy gates and measured solver/fit/inference cost. A rejected surrogate remains rejected; actual solver fallback is required.",
        "recovery":"Durable stage and case journals, source/data hashes, immutable failed attempts. Pause/Stop and the parent deadline apply to the complete tree; Off has no hidden total deadline.",
        "inputs":{"question":"Explicit request to build and evaluate this bounded surrogate","storage_mb":"1024–16384 MiB monitored retained study output budget"},
        "access":"During the study, model tools see progress. Only coordinator-admitted frozen stages can import protected labels. Final evidence is released after the workflow completes."})
}

impl LaboratoryService {
    pub fn create_ml_study(&self,id:Uuid,parent:Uuid,question:&str,storage_mb:u64)->anyhow::Result<LabJob>{
        ensure!(!question.trim().is_empty()&&question.len()<=8000,"State the requested bounded ML study");
        ensure!((1024..=16384).contains(&storage_mb),"ML study storage must be 1024–16384 MiB");
        self.create_active_child(id,parent,"ml_study",question,json!({"protocol_id":"argon_pressure_surrogate_v1","question":question,"storage_mb":storage_mb,
            "proposal_sha256":PROPOSAL,"worker_sha256":sha(WORKER.as_bytes()),"solver_worker_sha256":sha(SOLVER.as_bytes()),"source_session_id":parent}))
    }
    fn ml_ledger(&self,id:Uuid)->anyhow::Result<Ledger>{
        Ok(serde_json::from_slice(&fs::read(self.directory(id).join("study-ledger.json"))?)?)
    }
    fn save_ml_ledger(&self,id:Uuid,ledger:&Ledger)->anyhow::Result<()>{write_json(&self.directory(id).join("study-ledger.json"),ledger)}
    pub(super) fn initialize_ml_ledger(&self,id:Uuid)->anyhow::Result<()>{
        let hash=value_sha(&self.get(id)?.input)?;
        if self.directory(id).join("study-ledger.json").exists(){ensure!(self.ml_ledger(id)?.input_sha256==hash,"Frozen study input changed");}
        else{self.save_ml_ledger(id,&Ledger{schema_version:1,input_sha256:hash,..Default::default()})?;}Ok(())
    }
    pub fn start_ml_study(&self,id:Uuid)->anyhow::Result<()>{
        let job=self.get(id)?;
        ensure!(matches!(job.kind.as_str(),"ml_study"|"ml_query")&&job.state=="queued","Only a queued ML study or query can start");
        ensure!(job.input["proposal_sha256"]==PROPOSAL&&job.input["worker_sha256"]==sha(WORKER.as_bytes())&&job.input["solver_worker_sha256"]==sha(SOLVER.as_bytes()),"The installed study/solver source differs from this frozen study; it cannot silently resume with changed scientific code");
        let token=if let Some(parent)=job.parent_id{self.acquire_child(id,&self.execution_token(parent).context("Study parent execution is unavailable")?)?}else{self.acquire(id)?};
        let service=self.clone();tokio::spawn(async move{
            let deadline=job.deadline_at;let limit=token.clone();
            let timer=deadline.map(|at|tokio::spawn(async move{tokio::time::sleep((at-Utc::now()).to_std().unwrap_or_default()).await;limit.cancel();}));
            let outcome=if job.kind=="ml_query"{service.execute_ml_query(id,&token).await}else{service.execute_ml_study(id,&token).await};
            if let Some(timer)=timer{timer.abort();}
            if let Err(error)=outcome{
                let current=service.get(id).ok();let state=if current.as_ref().is_some_and(|j|!j.active()){current.as_ref().unwrap().state.as_str()}
                    else if deadline.is_some_and(|at|at<=Utc::now()){"timed_out"}else if token.is_cancelled(){"cancelled"}else{"failed"};
                let _=service.stop(id,state);
                let _=service.update(id,|j|{j.error=Some(format!("{error:#}"));j.event("study_stopped","Study stopped; source, completed stages, withheld data and failed attempts remain retained.",json!({"error":format!("{error:#}")}));});
            }
            service.release(id);
        });Ok(())
    }
    pub fn resume_ml_study(&self,id:Uuid,deadline:Option<DateTime<Utc>>)->anyhow::Result<()>{
        {
            let _guard=self.gate.lock();let mut job=self.get(id)?;
            ensure!(matches!(job.kind.as_str(),"ml_study"|"ml_query")&&matches!(job.state.as_str(),"paused"|"failed"|"timed_out"),"Study or query is not resumable");
            ensure!(!self.executing(id),"The previous study execution is still stopping");
            for child in self.ml_owned_ids(id)?{ensure!(!self.executing(child),"A study process is still stopping");}
            if let Some(parent_id)=job.parent_id{
                let parent=self.get(parent_id)?;
                if parent.active()&&parent.deadline_at.is_none_or(|at|at>Utc::now()){ensure!(deadline==parent.deadline_at,"Resume the active parent to change a study budget");}
                else{job.parent_id=None;job.event("independent_continuation","Explicit study continuation detached from its inactive parent; the original session remains in the immutable input.",json!({"original_parent_id":parent_id}));}
            }
            ensure!(deadline.is_none_or(|at|at>Utc::now()),"Choose a remaining continuation budget or Off");
            job.state="queued".into();job.error=None;job.deadline_at=deadline;
            job.event("study_resume","Verifying frozen completed stages and resuming the next incomplete stage.",json!({"deadline_at":deadline}));self.database.put_lab_record(&job)?;
        }self.start_ml_study(id)
    }
    /// Provenance follows real parent records and imported/copied artifacts. A
    /// caller-written role or study_id never creates or removes access rights.
    fn protected_ml_studies(&self,id:Uuid)->anyhow::Result<BTreeSet<Uuid>>{
        let mut pending=vec![id];let mut seen=BTreeSet::new();let mut studies=BTreeSet::new();
        while let Some(id)=pending.pop(){
            if !seen.insert(id){continue;}ensure!(seen.len()<=4096,"Artifact lineage exceeds the bounded study traversal");
            let job=self.get(id)?;
            if job.kind=="ml_study"{studies.insert(id);}
            if let Some(parent)=job.parent_id{pending.push(parent);}
            for source in job.input["sources"].as_array().into_iter().flatten(){if let Some(id)=source["job_id"].as_str(){pending.push(Uuid::parse_str(id)?);}}
            for key in ["restarted_from_job_id","source_job_id"]{if let Some(id)=job.input[key].as_str(){pending.push(Uuid::parse_str(id)?);}}
            if job.kind=="monitor"{if let Some(id)=job.input["job_id"].as_str(){pending.push(Uuid::parse_str(id)?);}}
            for event in &job.events{if event.kind=="independent_continuation"{if let Some(id)=event.data["original_parent_id"].as_str(){pending.push(Uuid::parse_str(id)?);}}}
        }Ok(studies)
    }
    pub(crate) fn ensure_model_study_access(&self,id:Uuid)->anyhow::Result<()>{
        for study in self.protected_ml_studies(id)?{ensure!(self.get(study)?.state=="completed","Study labels and derived evidence are protected until the frozen workflow completes. Inspect the ML study's progress instead; do not fit or review held-out labels early.");}Ok(())
    }
    pub(crate) fn ensure_study_import_access(&self,target:&LabJob,source:Uuid)->anyhow::Result<()>{
        for study in self.protected_ml_studies(source)?{
            if self.get(study)?.state=="completed"{continue;}
            let ledger=self.ml_ledger(study)?;
            let admitted=ledger.stages.values().any(|stage|stage.kind==target.kind&&stage.attempts.contains(&target.id)&&stage.input_sha256==value_sha(&target.input).unwrap_or_default());
            let trusted=match target.kind.as_str(){"generated"=>target.input["code"]==WORKER||target.input["code"]==DATASET_EXPORT,"study_plot"=>target.input["worker_sha256"]==super::ml_plot::source_hash(),_=>false};
            ensure!(target.parent_id==Some(study)&&admitted&&trusted,"Only a registered frozen coordinator stage can import this study's protected labels");
        }Ok(())
    }
    pub(super) fn ml_pin(&self,id:Uuid,path:&str,destination:&str)->anyhow::Result<(SourceImport,Value)>{
        let job=self.get(id)?;ensure!(job.state=="completed"&&!self.executing(id),"Study stage must stop before its outputs are admitted");
        let bytes=if job.kind=="generated"{self.read_generated_artifact(id,path)?}else{fs::read(self.path(id,path)?)?};
        let hash=sha(&bytes);
        Ok((SourceImport{job_id:id,path:path.into(),destination:destination.into(),sha256:Some(hash.clone())},json!({"path":destination,"sha256":hash})))
    }
    fn ml_complete_pin(&self,id:Uuid)->anyhow::Result<Value>{
        let job=self.get(id)?;ensure!(job.state=="completed"&&!self.executing(id),"Stage did not complete and drain");
        let mut files=vec![];
        for row in self.artifact_inventory(id)?.as_array().context("Stage inventory unavailable")?{
            let path=row["path"].as_str().context("Missing artifact path")?;
            if path.ends_with(".tmp")||path.starts_with("presentation")||path.starts_with("cancel."){continue;}
            let bytes=fs::read(self.path(id,path)?)?;files.push(json!({"path":path,"sha256":sha(&bytes),"bytes":bytes.len()}));
        }
        ensure!(files.iter().any(|row|row["path"]=="result.json"),"Completed stage lacks a result receipt");
        Ok(json!({"job_id":id,"input_sha256":value_sha(&job.input)?,"artifacts":files,"wall_seconds":(job.completed_at.unwrap_or(job.updated_at)-job.created_at).num_milliseconds() as f64/1000.0}))
    }
    fn verify_ml_completion(&self,study:Uuid,stage:&Stage,pin:&Value)->anyhow::Result<Uuid>{
        let id=Uuid::parse_str(pin["job_id"].as_str().context("Missing stage completion ID")?)?;let job=self.get(id)?;
        ensure!(job.parent_id==Some(study)&&job.project_id==self.get(study)?.project_id&&job.kind==stage.kind&&stage.attempts.contains(&id)&&job.state=="completed"&&value_sha(&job.input)?==stage.input_sha256&&pin["input_sha256"]==stage.input_sha256,"Completed study stage identity changed");
        for row in pin["artifacts"].as_array().context("Missing completed stage pins")?{let bytes=fs::read(self.path(id,row["path"].as_str().context("Missing pinned path")?)?)?;ensure!(row["sha256"]==sha(&bytes)&&row["bytes"]==bytes.len(),"A frozen study stage artifact changed");}Ok(id)
    }
    pub(super) fn verify_ml_stage_source(&self,study:Uuid,source:Uuid)->anyhow::Result<()>{
        let ledger=self.ml_ledger(study)?;
        let stage=ledger.stages.values().find(|stage|stage.completion.as_ref().is_some_and(|pin|pin["job_id"]==json!(source))).context("Source is not a completed registered stage of this study")?;
        ensure!(self.verify_ml_completion(study,stage,stage.completion.as_ref().unwrap())?==source,"Registered source identity changed");Ok(())
    }
    fn ml_owned_ids(&self,study:Uuid)->anyhow::Result<BTreeSet<Uuid>>{
        let jobs=self.list(Some(self.get(study)?.project_id))?;let mut ids=BTreeSet::from([study]);
        if self.directory(study).join("study-ledger.json").exists(){for stage in self.ml_ledger(study)?.stages.values(){ids.extend(&stage.attempts);}}
        loop{let before=ids.len();for job in &jobs{
            if job.parent_id.is_some_and(|parent|ids.contains(&parent))||job.events.iter().any(|event|event.kind=="independent_continuation"&&event.data["original_parent_id"].as_str().and_then(|s|Uuid::parse_str(s).ok()).is_some_and(|parent|ids.contains(&parent))){ids.insert(job.id);}
            if ids.contains(&job.id)&&job.kind=="sweep"&&self.directory(job.id).join("sweep-ledger.json").exists(){for case in self.read_json(job.id,"sweep-ledger.json")?["cases"].as_array().into_iter().flatten(){for id in case["attempts"].as_array().into_iter().flatten(){ids.insert(Uuid::parse_str(id.as_str().context("Invalid reserved solver ID")?)?);}}}
        }if before==ids.len(){break;}}Ok(ids)
    }
    fn ml_retained_bytes(&self,study:Uuid)->anyhow::Result<u64>{
        let mut total=0u64;for id in self.ml_owned_ids(study)?{if self.directory(id).exists(){
            for row in self.artifact_inventory(id)?.as_array().context("Study output inventory unavailable")?{total=total.checked_add(row["bytes"].as_u64().context("Missing artifact size")?).context("Study size overflow")?;}
        }}Ok(total)
    }
    pub(super) fn check_ml_storage(&self,study:Uuid)->anyhow::Result<u64>{
        let bytes=self.ml_retained_bytes(study)?;ensure!(bytes<=self.get(study)?.input["storage_mb"].as_u64().context("Missing study storage budget")?*1024*1024,"Study exceeded its monitored retained-output storage budget; all reserved attempts are counted");Ok(bytes)
    }
    pub(super) async fn ml_stage(&self,study:Uuid,name:&str,kind:&str,input:Value,token:&CancellationToken)->anyhow::Result<Uuid>{
        ensure!(!token.is_cancelled()&&self.get(study)?.active(),"Study stopped before stage admission");
        self.check_ml_storage(study)?;
        let hash=value_sha(&input)?;let mut ledger=self.ml_ledger(study)?;
        let stage=ledger.stages.entry(name.into()).or_insert_with(||Stage{input_sha256:hash.clone(),kind:kind.into(),..Default::default()});
        ensure!(stage.input_sha256==hash&&stage.kind==kind,"A frozen stage input or implementation changed");
        if let Some(pin)=&stage.completion{return self.verify_ml_completion(study,stage,pin);}
        ensure!(!(name=="fit"&&ledger.model_freeze.is_some()),"A frozen model cannot reopen selection after calibration or held-out access");
        let reserved=stage.attempts.last().copied();let prior=reserved.and_then(|id|self.get(id).ok());
        let target=match prior.as_ref(){
            Some(job) if job.state=="completed"||job.state=="queued"||kind=="sweep"=>job.id,
            Some(job) if job.active()||self.executing(job.id)=>bail!("The preceding study stage is still active"),
            None if reserved.is_some()=>reserved.unwrap(),
            _=>{let id=Uuid::new_v4();stage.attempts.push(id);id}
        };
        self.save_ml_ledger(study,&ledger)?;
        let job=self.create_active_child(target,study,kind,&format!("ML study · {name}"),input.clone())?;
        self.event(study,"study_stage",format!("Executing {name}; scientific roles and inputs are frozen."),json!({"stage":name,"job_id":target,"kind":kind}))?;
        if job.state=="queued"&&!self.executing(target){match kind{
            "generated"=>self.start_generated(target)?,"sweep"=>self.start_sweep(target)?,"study_plot"=>self.start_ml_plot(target)?,
            "study_data"=>{write_json(&self.directory(target).join("bundle.json"),&input["metadata"])?;write_json(&self.directory(target).join("result.json"),&json!({"status":"frozen_metadata","artifact":"bundle.json"}))?;self.update(target,|j|{j.state="completed".into();j.result=json!({"status":"frozen_metadata"});})?;},
            _=>bail!("Unsupported trusted study stage")
        }}else if kind=="sweep"&&matches!(job.state.as_str(),"paused"|"failed"|"timed_out")&&!self.executing(target){self.resume_sweep(target,self.get(study)?.deadline_at)?;}
        let mut ticks=0u64;
        loop{
            ensure!(!token.is_cancelled()&&self.get(study)?.active(),"Study stopped during a stage");
            let child=self.get(target)?;
            if !child.active()&&!self.executing(target){ensure!(child.state=="completed","Study stage {name} ended {}: {}",child.state,child.error.as_deref().unwrap_or_default());break;}
            let retained=if ticks%10==0{Some(self.ml_retained_bytes(study)?)}else{None};
            if let Some(bytes)=retained{ensure!(bytes<=self.get(study)?.input["storage_mb"].as_u64().context("Missing study storage budget")?*1024*1024,"Study exceeded its monitored retained-output storage budget (all attempts are counted)");}
            self.update(study,|j|{if j.active(){j.progress=json!({"stage":name,"child_job_id":target,"child_progress":child.progress,"retained_bytes":retained,"completed_stages":ledger.stages.values().filter(|s|s.completion.is_some()).count()});}})?;
            ticks+=1;tokio::select!{_=token.cancelled()=>bail!("Study stopped"),_=tokio::time::sleep(Duration::from_millis(500))=>{}}
        }
        self.check_ml_storage(study)?;
        let pin=self.ml_complete_pin(target)?;let mut ledger=self.ml_ledger(study)?;ledger.stages.get_mut(name).context("Lost study stage reservation")?.completion=Some(pin);self.save_ml_ledger(study,&ledger)?;Ok(target)
    }
    pub(super) async fn ml_compute(&self,study:Uuid,name:&str,inputs:Value,sources:Vec<SourceImport>,token:&CancellationToken)->anyhow::Result<Uuid>{
        self.ml_stage(study,name,"generated",json!({"engine":"python_numpy","code":WORKER,"inputs":inputs,"sources":sources,
            "limits":{"memory_mb":512,"process_limit":1,"wall_seconds":120,"storage_mb":128}}),token).await
    }
    pub(super) async fn ml_metadata(&self,study:Uuid,name:&str,data:Value,token:&CancellationToken)->anyhow::Result<Uuid>{
        self.ml_stage(study,name,"study_data",json!({"metadata":data}),token).await
    }
    pub(super) fn ml_request(&self,study:Uuid,phase:&str,freeze:Uuid)->anyhow::Result<(Value,Vec<SourceImport>)>{
        let (source,pin)=self.ml_pin(freeze,"work/split.json","split.json")?;
        Ok((json!({"schema_version":1,"phase":phase,"study_id":study,"proposal_sha256":PROPOSAL,"split":pin,"seal":{"stage":phase,"previous_job_id":freeze}}),vec![source]))
    }
    pub(super) fn ml_add(&self,request:&mut Value,sources:&mut Vec<SourceImport>,key:&str,job:Uuid,path:&str)->anyhow::Result<()>{
        let destination=format!("{key}-{}",path.rsplit('/').next().context("Invalid study artifact path")?);let (source,pin)=self.ml_pin(job,path,&destination)?;sources.push(source);request[key]=pin;Ok(())
    }
    pub(super) async fn ml_sweep(&self,study:Uuid,name:&str,runs:&[Value],protocol:&Value,token:&CancellationToken)->anyhow::Result<Uuid>{
        let cases=runs.iter().map(|run|json!({"case_id":run["case_id"],"engine":run["engine"],"parameters":run["parameters"],"metadata":{"condition_index":run["condition_index"],"replicate_index":run["replicate_index"],"role":run["role"],"seed":run["seed"]}})).collect::<Vec<_>>();
        self.ml_stage(study,name,"sweep",json!({"question":format!("Preregistered argon pressure study: {name}"),"cases":cases,"protocol":protocol,"storage_mb":self.get(study)?.input["storage_mb"],"source_session_id":self.get(study)?.input["source_session_id"]}),token).await
    }
    pub(super) fn ml_solver_rows(&self,sweeps:&[Uuid])->anyhow::Result<BTreeMap<String,Value>>{
        let mut rows=BTreeMap::new();for sweep in sweeps{self.verify_retained_sweep(*sweep)?;let ledger=self.read_json(*sweep,"sweep-ledger.json")?;for case in ledger["cases"].as_array().context("Missing solver cases")?{
            let name=case["case_id"].as_str().context("Missing case ID")?.to_owned();ensure!(case["completion"].is_object()&&rows.insert(name,case.clone()).is_none(),"Missing or duplicate measured study case");
        }}Ok(rows)
    }
    async fn ml_reduce_role(&self,study:Uuid,freeze:Uuid,role:&str,runs:&[Value],rows:&BTreeMap<String,Value>,token:&CancellationToken)->anyhow::Result<Vec<Uuid>>{
        let chosen=runs.iter().filter(|run|run["role"]==role).collect::<Vec<_>>();let mut outputs=vec![];
        for (index,chunk) in chosen.chunks(24).enumerate(){
            let (mut request,mut sources)=self.ml_request(study,"reduce",freeze)?;
            let split=self.read_json(freeze,"work/split.json")?;
            let mut bundle=json!({"schema_version":1,"study_id":study,"proposal_sha256":PROPOSAL,"protocol_sha256":split["protocol_sha256"],"split_sha256":request["split"]["sha256"],"role":role,"runs":[]});
            for run in chunk{
                let case=rows.get(run["case_id"].as_str().context("Invalid run identity")?).context("A preregistered seed has no completed solver")?;
                let solver_id=Uuid::parse_str(case["completion"]["job_id"].as_str().context("Missing solver receipt")?)?;let solver=self.get(solver_id)?;
                ensure!(solver.state=="completed"&&solver.input["engine"]==run["engine"]&&solver.input["parameters"]==run["parameters"],"Measured solver input differs from the frozen seed request");
                let raw=fs::read(self.path(solver_id,"manifest.json")?)?;
                let destination=format!("{}.json",run["case_id"].as_str().unwrap());let (source,pin)=self.ml_pin(solver_id,"measurements.json",&destination)?;sources.push(source);
                for (name,hash) in [("manifest.json",json!(sha(&raw))),("measurements.json",pin["sha256"].clone())]{ensure!(case["completion"]["artifacts"].as_array().context("Missing retained case artifact pins")?.iter().any(|row|row["path"]==name&&row["sha256"]==hash),"Solver labels changed after their immutable case completion");}
                let mut row=(*run).clone();row["solver_job_id"]=json!(solver_id);row["attempt_lineage"]=case["attempts"].clone();row["measurements_path"]=json!(destination);row["measurements_sha256"]=pin["sha256"].clone();row["manifest_sha256"]=json!(sha(&raw));row["manifest_text"]=json!(String::from_utf8(raw)?);
                bundle["runs"].as_array_mut().unwrap().push(row);
            }
            let bundle_id=self.ml_metadata(study,&format!("{role}-bundle-{index}"),bundle,token).await?;
            self.ml_add(&mut request,&mut sources,"manifest",bundle_id,"bundle.json")?;request["role"]=json!(role);
            outputs.push(self.ml_compute(study,&format!("{role}-reduce-{index}"),request,sources,token).await?);
        }Ok(outputs)
    }
    fn ml_add_shards(&self,request:&mut Value,sources:&mut Vec<SourceImport>,roles:&[(&str,&[Uuid])])->anyhow::Result<()>{
        let mut pins=vec![];for (role,jobs) in roles{for (index,job) in jobs.iter().enumerate(){let (source,mut pin)=self.ml_pin(*job,"work/shard.json",&format!("{role}-{index}.json"))?;sources.push(source);pin["role"]=json!(role);pins.push(pin);}}request["sources"]=json!(pins);Ok(())
    }
    async fn execute_ml_study(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<()>{
        let job=self.get(id)?;let hash=value_sha(&job.input)?;let ledger_path=self.directory(id).join("study-ledger.json");
        if ledger_path.exists(){ensure!(self.ml_ledger(id)?.input_sha256==hash,"Frozen study input changed");}
        else{self.save_ml_ledger(id,&Ledger{schema_version:1,input_sha256:hash,..Default::default()})?;}
        self.update(id,|j|{if j.active(){j.state="running".into();}})?;
        let freeze=self.ml_compute(id,"freeze",json!({"schema_version":1,"phase":"freeze","study_id":id,"proposal_sha256":PROPOSAL,"solver_worker_sha256":sha(SOLVER.as_bytes()),"solver_engine_version":"8.5.2.dev-36a30cb"}),vec![],token).await?;
        let frozen=self.read_json(freeze,"work/runs.json")?;let runs=frozen["runs"].as_array().context("Freeze did not produce solver inputs")?;
        ensure!(runs.len()==99&&runs[0]["role"]=="train","Preregistered pilot or run count changed");
        let pilot=self.ml_sweep(id,"pilot",&runs[..1],&frozen["protocol"],token).await?;
        let pilot_rows=self.ml_solver_rows(&[pilot])?;let pilot_wall=pilot_rows.values().next().unwrap()["completion"]["solver_wall_seconds"].as_f64().context("Pilot time missing")?;
        self.event(id,"pilot_estimate","The first real training seed completed. Continuing the fixed study; pilot cost and data are retained.",json!({"pilot_wall_seconds":pilot_wall,"remaining_run_count":98,"rough_remaining_solver_seconds":98.0*pilot_wall,"estimate":"First-run provisioning and workload contention can make this estimate conservative or inaccurate."}))?;
        let dataset_runs=runs[1..].iter().filter(|run|run["role"]!="ood").cloned().collect::<Vec<_>>();
        let dataset=self.ml_sweep(id,"dataset",&dataset_runs,&frozen["protocol"],token).await?;
        let rows=self.ml_solver_rows(&[pilot,dataset])?;ensure!(rows.len()==96,"The dataset requires all 96 original seeds");
        let train=self.ml_reduce_role(id,freeze,"train",runs,&rows,token).await?;
        let validation=self.ml_reduce_role(id,freeze,"validation",runs,&rows,token).await?;
        let (mut request,mut sources)=self.ml_request(id,"fit",freeze)?;self.ml_add_shards(&mut request,&mut sources,&[("train",&train),("validation",&validation)])?;
        let fit=self.ml_compute(id,"fit",request,sources,token).await?;
        let (_,model_pin)=self.ml_pin(fit,"work/model.npz","model.npz")?;let (_,card_pin)=self.ml_pin(fit,"work/model-card.json","model-card.json")?;
        let model_freeze=json!({"fit_job_id":fit,"model_sha256":model_pin["sha256"],"model_card_sha256":card_pin["sha256"],"split_sha256":frozen["split_sha256"],"protocol_sha256":frozen["protocol_sha256"],"worker_sha256":job.input["worker_sha256"]});
        let mut ledger=self.ml_ledger(id)?;if let Some(previous)=&ledger.model_freeze{ensure!(*previous==model_freeze,"The frozen model identity changed after fitting");}else{ledger.model_freeze=Some(model_freeze.clone());self.save_ml_ledger(id,&ledger)?;}
        let model_receipt=self.directory(id).join("model-freeze.json");
        if model_receipt.exists(){ensure!(self.read_json(id,"model-freeze.json")?==model_freeze,"The retained model freeze receipt changed");}else{write_json(&model_receipt,&model_freeze)?;}
        let calibration=self.ml_reduce_role(id,freeze,"calibration",runs,&rows,token).await?;
        let (mut request,mut sources)=self.ml_request(id,"calibrate",freeze)?;self.ml_add(&mut request,&mut sources,"model",fit,"work/model.npz")?;self.ml_add(&mut request,&mut sources,"model_card",fit,"work/model-card.json")?;self.ml_add_shards(&mut request,&mut sources,&[("calibration",&calibration)])?;
        let calibrated=self.ml_compute(id,"calibrate",request,sources,token).await?;
        let test=self.ml_reduce_role(id,freeze,"test",runs,&rows,token).await?;let regime=self.ml_reduce_role(id,freeze,"regime_test",runs,&rows,token).await?;
        let (mut evaluation_request,mut evaluation_sources)=self.ml_request(id,"evaluate",freeze)?;
        for (key,source,path) in [("model",fit,"work/model.npz"),("model_card",fit,"work/model-card.json"),("fit_data",fit,"work/fit-data.npz"),("calibration",calibrated,"work/calibration.json")]{self.ml_add(&mut evaluation_request,&mut evaluation_sources,key,source,path)?;}
        self.ml_add_shards(&mut evaluation_request,&mut evaluation_sources,&[("test",&test),("regime_test",&regime)])?;
        let evaluated=self.ml_compute(id,"evaluate-before-fallback",evaluation_request.clone(),evaluation_sources.clone(),token).await?;
        let (mut request,mut sources)=self.ml_request(id,"infer",freeze)?;
        for (key,source,path) in [("model",fit,"work/model.npz"),("model_card",fit,"work/model-card.json"),("calibration",calibrated,"work/calibration.json"),("evaluation",evaluated,"work/evaluation.json")]{self.ml_add(&mut request,&mut sources,key,source,path)?;}
        request["query"]=json!({"features":{"temperature_kelvin":180,"density_g_cm3":0.55},"units":{"temperature_kelvin":"K","density_g_cm3":"g/cm^3"},"protocol":frozen["protocol"],"intent":"scientific"});
        let inference=self.ml_compute(id,"ood-decision",request,sources,token).await?;
        let decision=self.read_json(inference,"work/inference.json")?;ensure!(decision["status"]=="requires_solver","The frozen OOD challenge must require a real solver fallback");
        let ood_runs=runs.iter().filter(|run|run["role"]=="ood").cloned().collect::<Vec<_>>();
        let fallback=self.ml_sweep(id,"ood-fallback",&ood_runs,&frozen["protocol"],token).await?;
        let all_rows=self.ml_solver_rows(&[pilot,dataset,fallback])?;ensure!(all_rows.len()==99,"A completed study requires all dataset and actual fallback seeds");
        let ood=self.ml_reduce_role(id,freeze,"ood",runs,&all_rows,token).await?;
        // This immutable accounting snapshot cannot change during recovery.
        // Cost finalization opens no labels and never repeats selection/prediction.
        let costs=self.ml_cost_snapshot(id,&all_rows)?;
        let costs_id=self.ml_metadata(id,"measured-costs",costs,token).await?;
        let (mut final_request,mut final_sources)=self.ml_request(id,"finalize_costs",freeze)?;
        for (key,source,path) in [("model",fit,"work/model.npz"),("model_card",fit,"work/model-card.json"),("calibration",calibrated,"work/calibration.json"),("evaluation",evaluated,"work/evaluation.json"),("costs",costs_id,"bundle.json")]{self.ml_add(&mut final_request,&mut final_sources,key,source,path)?;}
        let final_evaluation=self.ml_compute(id,"finalize-costs",final_request,final_sources,token).await?;
        let evaluation=self.read_json(final_evaluation,"work/evaluation.json")?;
        let (mut export_request,mut export_sources)=self.ml_request(id,"export_dataset",freeze)?;
        self.ml_add_shards(&mut export_request,&mut export_sources,&[("train",&train),("validation",&validation),("calibration",&calibration),("test",&test),("regime_test",&regime),("ood",&ood)])?;
        let dataset_export=self.ml_stage(id,"export-dataset","generated",json!({"engine":"python_numpy","code":DATASET_EXPORT,"inputs":export_request,"sources":export_sources,"limits":{"memory_mb":512,"process_limit":1,"wall_seconds":120,"storage_mb":128}}),token).await?;
        let mut plot_config=json!({"schema_version":1});let mut plot_sources=vec![];
        for (key,source,path) in [("evaluation",final_evaluation,"work/evaluation.json"),("split",freeze,"work/split.json"),("ood",ood[0],"work/shard.json")]{self.ml_add(&mut plot_config,&mut plot_sources,key,source,path)?;}
        let plot=self.ml_stage(id,"plot-evaluation","study_plot",json!({"worker_sha256":super::ml_plot::source_hash(),"config":plot_config,"sources":plot_sources}),token).await?;
        let result=json!({"status":"completed","study_id":id,"protocol_id":"argon_pressure_surrogate_v1","proposal_sha256":PROPOSAL,"model_freeze":model_freeze,
            "solver_runs":99,"dataset_seed_runs":96,"actual_ood_fallback_runs":3,"freeze_job_id":freeze,"fit_job_id":fit,"calibration_job_id":calibrated,"evaluation_job_id":final_evaluation,"dataset_export_job_id":dataset_export,"plot_job_id":plot,"plot_png":"plot/plot.png",
            "ood_decision_job_id":inference,"ood_solver_sweep_job_id":fallback,"ood_reduction_job_ids":ood,"evaluation":evaluation,"retained_bytes":self.check_ml_storage(id)?,
            "scope":"A real 10–20 ps classical argon pressure endpoint surrogate, with held-out evaluation. A completed workflow is not a claim that the surrogate passed usefulness gates or replaces a wet lab."});
        write_json(&self.directory(id).join("result.json"),&result)?;
        self.check_ml_storage(id)?;
        self.update(id,|j|{if j.active()&&!token.is_cancelled(){j.state="completed".into();j.result=result;j.progress=json!({"fraction":1.0,"solver_runs":99});j.event("completed","All fixed solver labels, frozen model, held-out scores, actual OOD fallback and measured costs are retained. Review the evaluation gates before using predictions.",json!({"evaluation_job_id":final_evaluation}));}})?;Ok(())
    }
    fn ml_cost_snapshot(&self,study:Uuid,rows:&BTreeMap<String,Value>)->anyhow::Result<Value>{
        let mut ledger=self.ml_ledger(study)?;
        if let Some(snapshot)=ledger.cost_snapshot{return Ok(snapshot);}
        // Commit the full value before reserving the publication job. Recovery
        // must not depend on a DB row that might not exist yet, or recompute an
        // earlier snapshot after additional finalization jobs have run.
        ensure!(!ledger.stages.contains_key("measured-costs"),"A legacy cost stage has no immutable snapshot; preserve it for explicit reconciliation");
        let snapshot=self.ml_costs(study,rows)?;
        ledger.cost_snapshot=Some(snapshot.clone());self.save_ml_ledger(study,&ledger)?;Ok(snapshot)
    }
    fn ml_costs(&self,study:Uuid,rows:&BTreeMap<String,Value>)->anyhow::Result<Value>{
        let ledger=self.ml_ledger(study)?;let mut solvers=vec![];let mut failed=0.0;
        for (case_id,row) in rows{
            solvers.push(json!({"case_id":case_id,"status":"completed","solver_job_id":row["completion"]["job_id"],"wall_seconds":row["completion"]["solver_wall_seconds"]}));
            for id in row["attempts"].as_array().into_iter().flatten(){if *id!=row["completion"]["job_id"]{if let Some(job)=self.database.lab_record(Uuid::parse_str(id.as_str().context("Invalid solver attempt")?)?)?{failed+=retained_attempt_seconds(&job)?;}}}
        }
        let mut startup=0.0;let mut fit=0.0;let mut calibration_evaluation=0.0;
        for (name,stage) in &ledger.stages{
            if stage.kind!="generated"{continue;}
            if let Some(pin)=&stage.completion{let seconds=pin["wall_seconds"].as_f64().context("Missing generated stage timing")?;
                if name=="fit"{fit+=seconds;}else if name=="calibrate"||name=="evaluate-before-fallback"{calibration_evaluation+=seconds;}else{startup+=seconds;}
            }
            for attempt in &stage.attempts{if stage.completion.as_ref().is_none_or(|pin|pin["job_id"]!=json!(attempt)){if let Some(job)=self.database.lab_record(*attempt)?{failed+=retained_attempt_seconds(&job)?;}}}
        }
        let snapshot_at=Utc::now();
        let elapsed=(snapshot_at-self.get(study)?.created_at).num_milliseconds().max(0) as f64/1000.0;
        let child_subtotal=solvers.iter().map(|row|row["wall_seconds"].as_f64().unwrap_or_default()).sum::<f64>()+startup+fit+calibration_evaluation+failed;
        let residual=(elapsed-child_subtotal).max(0.0);startup+=residual;
        Ok(json!({"solver_runs":solvers,"environment_startup_seconds":startup,"model_selection_seconds":fit,"calibration_evaluation_seconds":calibration_evaluation,
            "cold_model_load_seconds":0.0,"failed_attempt_seconds":failed,
            "snapshot_at":snapshot_at,"study_elapsed_seconds_including_pauses":elapsed,"child_cost_subtotal_seconds":child_subtotal,"coordinator_and_pause_charge_seconds":residual,
            "accounting":"Conservative pre-finalization setup charge: the larger of summed child job costs and study creation-to-snapshot elapsed time. Startup includes the nonnegative elapsed residual, covering coordinator work, queueing, pauses and recovery; concurrent child intervals can be overcharged. Each solver includes its provisioning/queue time; generated wall time includes cold process/model loading. Cold-load increment is zero because loading is already counted. Failed attempts use retained terminal events, not mutable seen/update timestamps. Final report, archival export and plot publication overhead are outside this snapshot and must be reported separately."}))
    }
}
