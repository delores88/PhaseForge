//! Parent-bound checkpoint continuation and explicitly detached solver resumes.
use super::*;

impl LaboratoryService{
    pub fn resume_solver_with_mode(&self,id:Uuid,deadline:Option<DateTime<Utc>>,independent:bool)->anyhow::Result<()>{
        let token=self.prepare_solver_resume(id,deadline,independent)?;
        self.spawn_solver(id,token)
    }
    fn prepare_solver_resume(&self,id:Uuid,deadline:Option<DateTime<Utc>>,independent:bool)->anyhow::Result<CancellationToken>{
        let _guard=self.gate.lock();let mut job=self.get(id)?;
        anyhow::ensure!(job.kind=="solver"&&matches!(job.state.as_str(),"paused"|"failed"|"timed_out"),"Solver is not resumable");
        anyhow::ensure!(!super::nr_engines::supports(job.input["engine"].as_str().unwrap_or("")),"This NR adapter retains stopped attempts but does not yet support checkpoint continuation. Create a new immutable diagnostic request; the prior attempt will not be overwritten.");
        self.ensure_science_attempt_identity(id)?;
        let original_parent=job.parent_id;let original_deadline=job.deadline_at;
        if let Some(parent_id)=job.parent_id{
            let parent=self.get(parent_id)?;
            anyhow::ensure!(parent.project_id==job.project_id,"Solver parent belongs to another project");
            if parent.active()&&parent.deadline_at.is_none_or(|at|at>Utc::now()){
                anyhow::ensure!(!independent&&deadline==parent.deadline_at,"An active parent's solver must retain its exact deadline");
            }else{
                anyhow::ensure!(independent,"Resume the parent or explicitly continue this solver independently");
                let jobs=self.list(Some(job.project_id))?;
                let mut tree=vec![parent.id];let mut cursor=0;
                while cursor<tree.len(){let current=tree[cursor];cursor+=1;for child in &jobs{if child.parent_id==Some(current)&&!tree.contains(&child.id){tree.push(child.id);}}}
                let active=self.active.lock();
                anyhow::ensure!(tree.iter().all(|id|!active.contains_key(id)),"The former parent execution tree is still stopping; wait for all workers to exit before independent continuation");
                drop(active);job.parent_id=None;
            }
        }
        anyhow::ensure!(deadline.is_none_or(|at|at>Utc::now()),"Choose an explicit unexpired solver budget or Off");
        let token=self.acquire(id).context("The solver worker is still stopping; wait for its checkpoint before resuming")?;
        let saved=(||->anyhow::Result<()>{
            let signal=self.directory(id).join("cancel.request");if signal.is_file(){std::fs::remove_file(signal)?;}
            if original_parent!=job.parent_id{
                job.event("independent_continuation","Explicit independent continuation keeps the original numerical input and checkpoint lineage.",json!({"kind":"solver","original_parent_id":original_parent,"original_deadline_at":original_deadline,"deadline_at":deadline}));
            }
            job.state="queued".into();job.deadline_at=deadline;job.error=None;
            job.event("resume_requested","The trusted worker will verify its checkpoint against the original input and engine version before continuing.",json!({"strategy":"checkpoint_when_compatible","independent":independent,"parent_id":job.parent_id,"deadline_at":deadline}));
            self.database.put_lab_record(&job)?;Ok(())
        })();
        if let Err(error)=saved{self.release(id);return Err(error);}Ok(token)
    }
}

#[cfg(test)]mod tests{
    use super::*;
    fn fixture()->(tempfile::TempDir,LaboratoryService,LabJob,LabJob){
        let dir=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:dir.path().into(),..Default::default()};let db=Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Solver lifecycle".into()),question:"test".into()});db.put_project(&project).unwrap();
        let service=LaboratoryService::new(db,config).unwrap();let parent=service.create(Uuid::new_v4(),project.id,None,"session","parent",json!({}),Some(Utc::now()+chrono::Duration::seconds(60))).unwrap();
        let child=service.create(Uuid::new_v4(),project.id,Some(parent.id),"solver","solver",json!({"engine":"openmm_argon","source_session_id":parent.id,"parameters":{"seed":1234}}),parent.deadline_at).unwrap();service.update(child.id,|job|job.state="paused".into()).unwrap();
        (dir,service,parent,child)
    }
    #[test]fn science_v5_refuses_old_runtime_resume_before_any_mutation_and_keeps_artifacts_readable(){
        for prior in ["science-v2","science-v3","science-v4"] {
        let (_dir,service,parent,child)=fixture();
        service.event(child.id,"runtime_verified","Prior runtime",json!({"kind":prior,"manifest_sha256":"original prior manifest"})).unwrap();
        let root=service.directory(child.id);write_json(&root.join("manifest.json"),&json!({"runtime":prior,"steps":42})).unwrap();std::fs::write(root.join("cancel.request"),b"original pause").unwrap();
        let before=serde_json::to_vec(&service.get(child.id).unwrap()).unwrap();let artifact=std::fs::read(root.join("manifest.json")).unwrap();
        let error=service.prepare_solver_resume(child.id,parent.deadline_at,false).unwrap_err();assert!(error.to_string().contains("Original scientific runtime differs"));
        assert_eq!(serde_json::to_vec(&service.get(child.id).unwrap()).unwrap(),before);assert_eq!(std::fs::read(root.join("manifest.json")).unwrap(),artifact);assert_eq!(std::fs::read(root.join("cancel.request")).unwrap(),b"original pause");
        assert!(!service.executing(child.id));assert!(!service.config.data_directory.join("environments/science-v5").exists());
        assert_eq!(service.read_json(child.id,"manifest.json").unwrap()["steps"],42);
        assert!(service.list(Some(child.project_id)).unwrap().iter().any(|job|job.id==child.id));
        }
    }
    #[test]fn science_v5_matching_receipt_resumes_and_generated_runtime_path_is_v4(){
        let (_dir,service,parent,child)=fixture();
        service.event(child.id,"runtime_verified","Current runtime",json!({"kind":runtime::RuntimeKind::Science.name(),"manifest_sha256":runtime::RuntimeKind::Science.manifest_sha256()})).unwrap();
        assert_eq!(service.environment_python(),service.config.data_directory.join("environments/science-v5/python.exe"));
        assert_eq!(runtime::RuntimeKind::Generated.directory(&service.config.data_directory),service.config.data_directory.join("environments/python-numpy-v4/runtime"));
        let token=service.prepare_solver_resume(child.id,parent.deadline_at,false).unwrap();assert!(!token.is_cancelled());service.release(child.id);
    }
    #[test]fn solver_parent_deadline_cannot_be_overridden_or_detached_while_active(){
        let (_dir,service,parent,child)=fixture();let before=service.get(child.id).unwrap();
        assert!(service.prepare_solver_resume(child.id,None,false).is_err());
        assert!(service.prepare_solver_resume(child.id,parent.deadline_at,true).is_err());
        assert_eq!(service.get(child.id).unwrap().deadline_at,before.deadline_at);assert!(!service.executing(child.id));
        let token=service.prepare_solver_resume(child.id,parent.deadline_at,false).unwrap();let resumed=service.get(child.id).unwrap();
        assert_eq!(resumed.parent_id,Some(parent.id));assert_eq!(resumed.deadline_at,parent.deadline_at);assert_eq!(resumed.input,child.input);
        service.stop(parent.id,"paused").unwrap();assert!(token.is_cancelled());assert_eq!(service.get(child.id).unwrap().state,"paused");service.release(child.id);
    }
    #[test]fn solver_independent_resume_requires_explicit_choice_and_drained_former_tree(){
        let (_dir,service,parent,child)=fixture();let parent_token=service.acquire(parent.id).unwrap();
        let sibling=service.create(Uuid::new_v4(),parent.project_id,Some(parent.id),"solver","sibling",json!({}),parent.deadline_at).unwrap();let sibling_token=service.acquire(sibling.id).unwrap();
        service.stop(parent.id,"paused").unwrap();assert!(parent_token.is_cancelled()&&sibling_token.is_cancelled());
        assert!(service.prepare_solver_resume(child.id,None,false).is_err());
        assert!(service.prepare_solver_resume(child.id,None,true).is_err());service.release(parent.id);
        assert!(service.prepare_solver_resume(child.id,None,true).is_err());service.release(sibling.id);
        let input=std::fs::read(service.directory(child.id).join("input.json")).unwrap();
        service.prepare_solver_resume(child.id,None,true).unwrap();let resumed=service.get(child.id).unwrap();
        assert_eq!(resumed.parent_id,None);assert_eq!(resumed.deadline_at,None);assert_eq!(resumed.input,child.input);
        assert_eq!(std::fs::read(service.directory(child.id).join("input.json")).unwrap(),input);
        assert_eq!(resumed.events.iter().find(|event|event.kind=="independent_continuation").unwrap().data["original_parent_id"],json!(parent.id));service.release(child.id);
    }
    #[test]fn solver_off_and_expired_parent_resume_policies_survive_recovery(){
        let (_dir,service,parent,child)=fixture();service.update(parent.id,|job|job.deadline_at=None).unwrap();
        assert!(service.prepare_solver_resume(child.id,child.deadline_at,false).is_err());
        service.prepare_solver_resume(child.id,None,false).unwrap();assert_eq!(service.get(child.id).unwrap().deadline_at,None);service.release(child.id);
        service.update(child.id,|job|job.state="paused".into()).unwrap();service.update(parent.id,|job|job.deadline_at=Some(Utc::now()-chrono::Duration::seconds(1))).unwrap();
        assert!(service.prepare_solver_resume(child.id,None,false).is_err());
        assert!(service.prepare_solver_resume(child.id,Some(Utc::now()-chrono::Duration::seconds(1)),true).is_err());
        service.prepare_solver_resume(child.id,None,true).unwrap();assert_eq!(service.get(child.id).unwrap().parent_id,None);service.release(child.id);
    }
    #[test]fn solver_checkpoint_resume_reserves_execution_atomically(){
        let (_dir,service,parent,child)=fixture();let barrier=Arc::new(std::sync::Barrier::new(8));
        let threads:Vec<_>=(0..8).map(|_|{let service=service.clone();let barrier=barrier.clone();std::thread::spawn(move||{barrier.wait();service.prepare_solver_resume(child.id,parent.deadline_at,false).is_ok()})}).collect();
        assert_eq!(threads.into_iter().map(|thread|usize::from(thread.join().unwrap())).sum::<usize>(),1);
        assert_eq!(service.get(child.id).unwrap().events.iter().filter(|event|event.kind=="resume_requested").count(),1);service.release(child.id);
    }
    #[test]fn detached_solver_preserves_protected_study_lineage(){
        let (_dir,service,parent,child)=fixture();service.update(parent.id,|job|{job.kind="ml_study".into();job.state="paused".into();}).unwrap();
        service.prepare_solver_resume(child.id,None,true).unwrap();assert_eq!(service.get(child.id).unwrap().parent_id,None);
        assert!(service.ensure_model_study_access(child.id).is_err());service.release(child.id);
    }
}
