//! Requested scientific illustrations reuse retained data-only scenes, independently of solvers.
use std::sync::Arc;
use anyhow::Context;
use serde_json::{json,Value};
use uuid::Uuid;
use tokio_util::sync::CancellationToken;
use sha2::{Digest,Sha256};
use crate::{app::AppState,studio::render::{RenderRequest,resolved_input}};
use super::{LabJob,LaboratoryService,write_json,process};

const MAX_SCENE_SOURCE_BYTES: usize = 4 * 1024 * 1024;

pub fn admission_limits() -> Value {
    let mut limits=crate::sandbox::scene_admission_limits();
    limits["scene_source_json_bytes"]=json!(MAX_SCENE_SOURCE_BYTES);
    limits["authoring_guidance"]=json!("Before writing a generated scene, count vertices, points and inline atoms across all nodes and check both compact UTF-8 JSON bytes and final file bytes. Use supported procedural primitive parameters instead of hand-tessellating large surfaces. Renderer's internal tessellation does not increase authored-input budgets. Preserve registered PDB coordinates in structure bindings; do not independently recenter chains or fabricate/decimate source coordinates to fit these authoring limits.");
    limits
}

fn validate_scene_source_bytes(bytes:&[u8])->anyhow::Result<()> {
    anyhow::ensure!(bytes.len()<=MAX_SCENE_SOURCE_BYTES,"Scene source JSON exceeds 4 MiB");
    Ok(())
}

pub fn create(state:&Arc<AppState>,session:&LabJob,id:Uuid,args:&Value)->anyhow::Result<LabJob>{
    let job=state.laboratory.admit_illustration(session,id,args,||resolve_request(state,session,args))?;
    if job.state=="queued"&&!state.laboratory.executing(id){state.laboratory.start_illustration(id)?;}
    Ok(job)
}

fn resolve_request(state:&AppState,session:&LabJob,args:&Value)->anyhow::Result<Value>{
    // A retained tool target captures its original scene and presentation once.
    // Recovery must not resolve a later camera revision into the same identity.
    let source=args.get("source_illustration_id").and_then(Value::as_str).map(Uuid::parse_str).transpose()?;
    let original=source.map(|id|state.laboratory.get(id)).transpose()?;
    if let Some(original)=&original{anyhow::ensure!(original.project_id==session.project_id&&original.kind=="illustration","Illustration source belongs to another project or kind");}
    let mut request_value=json!({
        "project_id":session.project_id,"scene":args["scene"],"structure_id":args["structure_id"],
        "bindings":args.get("bindings").cloned().unwrap_or_else(||json!([])),
        "style":args.get("style").cloned().unwrap_or(json!("studio")),
        "width":args.get("width").cloned().unwrap_or(json!(1536)),"height":args.get("height").cloned().unwrap_or(json!(1536)),
        "samples":args.get("samples").cloned().unwrap_or(json!(96)),"camera":args["camera"]
    });
    if let Some(original)=&original{
        request_value=original.input["resolved"]["request"].clone();
        for key in ["scene","structure_id","bindings","style","width","height","samples","camera"]{if let Some(value)=args.get(key){request_value[key]=value.clone();}}
    }
    let scene_source=resolve_scene_source(&state.laboratory,session.project_id,args)?;
    if let Some((scene,_))=&scene_source{request_value["scene"]=scene.clone();request_value["structure_id"]=Value::Null;}
    let presentation=if let Some(source)=source{super::api::get_presentation(state,source)?["settings"].clone()}else{json!({})};
    if args.get("camera").is_none()&&presentation["camera"]["position"].is_array()&&presentation["camera"]["target"].is_array(){
        let fov=presentation["camera"]["fov"].as_f64().unwrap_or(38.0).clamp(17.1,84.0);
        let lens=presentation["camera"]["focal_length_mm"].as_f64().unwrap_or_else(||36.0/(2.0*(fov.to_radians()/2.0).tan()));
        request_value["camera"]=json!({"position":presentation["camera"]["position"],"target":presentation["camera"]["target"],"up":presentation["camera"]["up"],"focal_length_mm":lens.clamp(20.0,120.0)});
    }
    let request:RenderRequest=serde_json::from_value(request_value)?;
    let mut resolved=resolved_input(state,&request)?;resolved["presentation"]=presentation;
    Ok(json!({"engine":"blender_cycles","request":args,"resolved":resolved,"source_illustration_id":source,"scene_source":scene_source.map(|(_,receipt)|receipt),"scope":"Scientific illustration; no numerical experiment was requested"}))
}

fn resolve_scene_source(service:&LaboratoryService,project:Uuid,args:&Value)->anyhow::Result<Option<(Value,Value)>> {
    let Some(reference)=args.get("scene_source").filter(|v|!v.is_null()) else{return Ok(None)};
    anyhow::ensure!(args.get("scene").is_none_or(Value::is_null)&&args.get("structure_id").is_none_or(Value::is_null),"Choose one authored scene, scene_source or molecular structure");
    let id=Uuid::parse_str(reference["job_id"].as_str().context("Scene source job_id is required")?)?;
    let source=service.get(id)?;
    anyhow::ensure!(source.project_id==project&&source.kind=="generated"&&source.state=="completed","Scene source must be a completed isolated generation in this project");
    service.ensure_model_study_access(id)?;
    let path=reference["path"].as_str().context("Scene source JSON path is required")?;
    anyhow::ensure!(path.ends_with(".json"),"Scene source must be a data-only JSON artifact");
    let expected=reference["sha256"].as_str().context("Pin the retained scene source SHA256")?;
    let bytes=service.read_generated_artifact(id,path)?;
    validate_scene_source_bytes(&bytes)?;
    let hash=format!("{:x}",Sha256::digest(&bytes));
    anyhow::ensure!(hash==expected,"Scene source bytes differ from the requested SHA256");
    let scene:Value=serde_json::from_slice(&bytes)?;
    crate::sandbox::validate_scene(&scene)?;
    Ok(Some((scene,json!({"job_id":id,"path":path,"sha256":hash,"bytes":bytes.len(),"kind":"generated_data_only_scene"}))))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn fixture()->(tempfile::TempDir,LaboratoryService,LabJob){
        let dir=tempfile::tempdir().unwrap();let config=crate::config::AppConfig{data_directory:dir.path().into(),..Default::default()};
        let database=crate::persistence::Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Render lifecycle".into()),question:"test".into()});database.put_project(&project).unwrap();
        let service=LaboratoryService::new(database,config).unwrap();
        let parent=service.create(Uuid::new_v4(),project.id,None,"session","parent",json!({}),Some(chrono::Utc::now()+chrono::Duration::minutes(10))).unwrap();
        (dir,service,parent)
    }
    fn saved_input(args:&Value)->Value{json!({"engine":"blender_cycles","request":args,"resolved":{"request":{"scene":{"nodes":[{"color":"blue"}]}},"presentation":{"camera":{"position":[1,2,3]},"contrast":1.25}}})}
    #[test]fn advertised_admission_limits_enforce_retained_source_bytes_before_json_parsing(){
        let (_dir,service,parent)=fixture();
        let source=service.create(Uuid::new_v4(),parent.project_id,Some(parent.id),"generated","Scene size boundary",json!({}),None).unwrap();
        service.update(source.id,|job|job.state="completed".into()).unwrap();
        let scene=json!({"schema_version":"1.0","provenance":{"kind":"conceptual","description":"Bounded fixture"},"nodes":[{"id":"cell","type":"sphere","parameters":{"radius":1.0}}]});
        let compact=serde_json::to_vec(&scene).unwrap();let cap=admission_limits()["scene_source_json_bytes"].as_u64().unwrap() as usize;
        let root=service.directory(source.id);std::fs::create_dir_all(root.join("work")).unwrap();
        for (length,accepted) in [(cap,true),(cap+1,false)] {
            let mut bytes=compact.clone();bytes.resize(length,b' ');let hash=format!("{:x}",Sha256::digest(&bytes));
            std::fs::write(root.join("work/scene.json"),&bytes).unwrap();write_json(&root.join("generated-artifacts.json"),&json!({"artifacts":[{"path":"work/scene.json","bytes":bytes.len(),"sha256":hash}]})).unwrap();
            let result=resolve_scene_source(&service,parent.project_id,&json!({"scene_source":{"job_id":source.id,"path":"work/scene.json","sha256":hash}}));
            if accepted{assert_eq!(result.unwrap().unwrap().0,scene);}else{assert!(result.unwrap_err().to_string().contains("4 MiB"));}
        }
    }
    #[test]fn retained_scene_source_requires_registered_exact_bytes_project_and_valid_data_schema(){
        let (_dir,service,parent)=fixture();
        let source=service.create(Uuid::new_v4(),parent.project_id,Some(parent.id),"generated","Scene authoring fixture",json!({}),None).unwrap();
        let scene=json!({"schema_version":"1.0","title":"Authored fixture","units":"illustrative","provenance":{"kind":"conceptual","description":"Data-only test illustration"},"nodes":[{"id":"cell","type":"atom","label":"T cell","description":"Conceptual cell","entity_id":null,"position":[0,0,0],"rotation":[0,0,0],"scale":[1,1,1],"color":"#258aff","parameters":{"radius":1.0}}],"bonds":[],"camera":null});
        let bytes=serde_json::to_vec(&scene).unwrap();let hash=format!("{:x}",Sha256::digest(&bytes));let root=service.directory(source.id);
        std::fs::create_dir_all(root.join("work")).unwrap();std::fs::write(root.join("work/scene.json"),&bytes).unwrap();
        write_json(&root.join("generated-artifacts.json"),&json!({"artifacts":[{"path":"work/scene.json","bytes":bytes.len(),"sha256":hash}]})).unwrap();
        let args=json!({"scene_source":{"job_id":source.id,"path":"work/scene.json","sha256":hash}});
        assert!(resolve_scene_source(&service,parent.project_id,&args).is_err());
        service.update(source.id,|job|job.state="completed".into()).unwrap();
        let (retained,pin)=resolve_scene_source(&service,parent.project_id,&args).unwrap().unwrap();assert_eq!(retained,scene);assert_eq!(pin["sha256"],hash);
        assert!(resolve_scene_source(&service,Uuid::new_v4(),&args).is_err());
        let mut conflicting=args.clone();conflicting["scene"]=scene;assert!(resolve_scene_source(&service,parent.project_id,&conflicting).is_err());
        let mut wrong=args.clone();wrong["scene_source"]["sha256"]=json!("0".repeat(64));assert!(resolve_scene_source(&service,parent.project_id,&wrong).is_err());
        std::fs::write(root.join("work/scene.json"),b"{}").unwrap();assert!(resolve_scene_source(&service,parent.project_id,&args).is_err());
    }
    #[test]fn lifecycle_admission_freezes_input_and_replay_never_resolves_again(){
        let (_dir,service,parent)=fixture();let args=json!({"title":"Retained scene"});let id=Uuid::new_v4();
        let child=service.admit_illustration(&parent,id,&args,||{assert!(service.gate.try_lock().is_none());Ok(saved_input(&args))}).unwrap();
        assert_eq!(child.deadline_at,parent.deadline_at);
        service.update(parent.id,|job|job.state="completed".into()).unwrap();
        let replay=service.admit_illustration(&parent,id,&args,||panic!("Recovery re-resolved a later scene")).unwrap();
        assert_eq!(replay.input,child.input);assert_eq!(replay.id,child.id);
        assert!(service.start_illustration(id).is_err());assert!(!service.executing(id));
        assert!(service.admit_illustration(&parent,id,&json!({"title":"Changed request"}),||unreachable!()).is_err());
        assert!(service.admit_illustration(&parent,Uuid::new_v4(),&args,||unreachable!()).is_err());
        service.update(parent.id,|job|{job.state="running".into();job.deadline_at=Some(chrono::Utc::now()-chrono::Duration::seconds(1));}).unwrap();
        assert!(service.admit_illustration(&parent,Uuid::new_v4(),&args,||unreachable!()).is_err());
        let mut foreign=parent.clone();foreign.project_id=Uuid::new_v4();
        assert!(service.admit_illustration(&foreign,Uuid::new_v4(),&args,||unreachable!()).is_err());
    }
    #[test]fn lifecycle_stop_and_admission_share_one_atomic_gate(){
        let (_dir,service,parent)=fixture();let id=Uuid::new_v4();let child_service=service.clone();let stop_service=service.clone();let parent_id=parent.id;
        let (entered_tx,entered_rx)=std::sync::mpsc::channel();let (continue_tx,continue_rx)=std::sync::mpsc::channel();
        let admission=std::thread::spawn(move||{let args=json!({"title":"Race"});child_service.admit_illustration(&parent,id,&args,||{entered_tx.send(()).unwrap();continue_rx.recv().unwrap();Ok(saved_input(&args))})});
        entered_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        let (stopped_tx,stopped_rx)=std::sync::mpsc::channel();
        let stop=std::thread::spawn(move||{let result=stop_service.stop(parent_id,"paused");stopped_tx.send(result).unwrap();});
        assert!(matches!(stopped_rx.recv_timeout(std::time::Duration::from_millis(25)),Err(std::sync::mpsc::RecvTimeoutError::Timeout)));
        continue_tx.send(()).unwrap();admission.join().unwrap().unwrap();stopped_rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap().unwrap();stop.join().unwrap();
        assert_eq!(service.get(id).unwrap().state,"paused");assert!(service.start_illustration(id).is_err());
    }
    #[test]fn lifecycle_retry_detaches_inactive_parent_and_preserves_scene_and_lineage(){
        for state in ["paused","failed","timed_out","completed","cancelled","running"]{
            let (_dir,service,parent)=fixture();let args=json!({"title":"Frozen"});
            let original=service.admit_illustration(&parent,Uuid::new_v4(),&args,||Ok(saved_input(&args))).unwrap();
            service.update(original.id,|job|job.state="failed".into()).unwrap();
            service.update(parent.id,|job|{job.state=state.into();if state=="running"{job.deadline_at=Some(chrono::Utc::now()-chrono::Duration::seconds(1));}}).unwrap();
            let next=service.prepare_illustration_restart(original.id,None).unwrap();
            assert_eq!(next.parent_id,None,"{state}");assert_eq!(next.deadline_at,None);assert_ne!(next.id,original.id);
            assert_eq!(next.input["resolved"],original.input["resolved"]);assert_eq!(next.input["request"],args);
            assert_eq!(next.input["restarted_from_illustration_id"],json!(original.id));assert_eq!(next.input["restart_parent_job_id"],json!(parent.id));
            let retained=service.get(original.id).unwrap();assert_eq!(retained.input,original.input);assert_eq!(retained.deadline_at,original.deadline_at);assert_eq!(retained.state,"failed");
            assert_eq!(service.prepare_illustration_restart(original.id,Some(chrono::Utc::now()+chrono::Duration::seconds(40))).unwrap().id,next.id);
            assert_eq!(service.get(original.id).unwrap().events.iter().filter(|event|event.kind=="illustration_restart").count(),1);
        }
    }
    #[test]fn lifecycle_retry_inherits_exact_live_parent_budget_including_off(){
        for deadline in [None,Some(chrono::Utc::now()+chrono::Duration::seconds(90))]{
            let (_dir,service,parent)=fixture();let parent=service.update(parent.id,|job|job.deadline_at=deadline).unwrap();let args=json!({});
            let original=service.admit_illustration(&parent,Uuid::new_v4(),&args,||Ok(saved_input(&args))).unwrap();
            service.update(original.id,|job|job.state="paused".into()).unwrap();
            let next=service.prepare_illustration_restart(original.id,Some(chrono::Utc::now()+chrono::Duration::hours(1))).unwrap();
            assert_eq!(next.parent_id,Some(parent.id));assert_eq!(next.deadline_at,deadline);
        }
        let (_dir,service,parent)=fixture();let args=json!({});let original=service.admit_illustration(&parent,Uuid::new_v4(),&args,||Ok(saved_input(&args))).unwrap();
        service.update(original.id,|job|job.state="timed_out".into()).unwrap();service.update(parent.id,|job|job.state="completed".into()).unwrap();
        assert!(service.prepare_illustration_restart(original.id,Some(chrono::Utc::now()-chrono::Duration::seconds(1))).is_err());
        assert_eq!(service.list(None).unwrap().len(),2);
    }
    #[test]fn lifecycle_restart_recovers_committed_attempt_without_backlink_and_is_concurrent_idempotent(){
        let (_dir,service,parent)=fixture();let args=json!({});let original=service.admit_illustration(&parent,Uuid::new_v4(),&args,||Ok(saved_input(&args))).unwrap();
        service.update(original.id,|job|job.state="failed".into()).unwrap();
        let mut input=original.input.clone();input["restarted_from_illustration_id"]=json!(original.id);input["restart_parent_job_id"]=json!(parent.id);
        let orphan=service.create(Uuid::new_v4(),original.project_id,Some(parent.id),"illustration","retained retry",input,parent.deadline_at).unwrap();
        let barrier=std::sync::Arc::new(std::sync::Barrier::new(8));
        let threads:Vec<_>=(0..8).map(|_|{let service=service.clone();let barrier=barrier.clone();std::thread::spawn(move||{barrier.wait();service.prepare_illustration_restart(original.id,None).unwrap().id})}).collect();
        for thread in threads{assert_eq!(thread.join().unwrap(),orphan.id);}
        assert_eq!(service.list(None).unwrap().len(),3);assert_eq!(service.get(original.id).unwrap().events.iter().filter(|event|event.kind=="illustration_restart").count(),1);
    }
    #[tokio::test] async fn actual_blender_illustration_retains_scene_and_color_without_a_solver(){
        let Some(root)=std::env::var_os("PHASEFORGE_ILLUSTRATION_TEST_ROOT")else{eprintln!("SKIP actual Blender: set a dedicated PHASEFORGE_ILLUSTRATION_TEST_ROOT");return;};
        let path=std::path::PathBuf::from(root).join(Uuid::new_v4().to_string());std::fs::create_dir_all(&path).unwrap();
        let config=crate::config::AppConfig{data_directory:path.clone(),gpu_enabled:false,..Default::default()};
        let db=crate::persistence::Database::open(&config.database_path()).unwrap();let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Illustration acceptance".into()),question:"Colored conceptual atom illustration".into()});db.put_project(&project).unwrap();
        let scene=json!({"schema_version":"1.0","title":"Blue conceptual atom","units":"illustrative","provenance":{"kind":"conceptual","description":"Requested static scientific illustration, no calculation"},"nodes":[{"id":"atom-a","type":"atom","label":"A","description":"Colored conceptual atom","entity_id":null,"position":[0,0,0],"rotation":[0,0,0],"scale":[1,1,1],"color":"#258aff","parameters":{"radius":1.0}}],"bonds":[],"camera":null});
        crate::sandbox::validate_scene(&scene).unwrap();
        let request:RenderRequest=serde_json::from_value(json!({"project_id":project.id,"scene":scene,"style":"studio","width":320,"height":320,"samples":16})).unwrap();
        let service=LaboratoryService::new(db,config).unwrap();let id=Uuid::new_v4();
        service.create(id,project.id,None,"illustration","Blue conceptual atom",json!({"engine":"blender_cycles","resolved":{"request":request,"structure":null,"bound_structures":{},"gpu_enabled":false}}),Some(chrono::Utc::now()+chrono::Duration::seconds(120))).unwrap();
        service.start_illustration(id).unwrap();
        let job=tokio::time::timeout(std::time::Duration::from_secs(125),async{loop{let job=service.get(id).unwrap();if !job.active(){break job;}tokio::time::sleep(std::time::Duration::from_millis(100)).await;}}).await.unwrap();
        assert_eq!(job.state,"completed","{:?}",job.error);assert_eq!(job.result["renderer"]["width"],320);assert_eq!(job.result["renderer"]["height"],320);
        assert_eq!(service.list(None).unwrap().len(),1);assert_eq!(job.input["resolved"]["request"]["scene"]["nodes"][0]["color"],"#258aff");
        let png=std::fs::read(service.directory(id).join("render.png")).unwrap();assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));assert!(service.directory(id).join("scene.blend").is_file());assert!(service.directory(id).join("scene.glb").is_file());
        write_json(&path.join("acceptance.json"),&json!({"passed":true,"job_id":id,"path":service.directory(id),"result":job.result,"solver_count":0})).unwrap();println!("Illustration evidence {}",path.display());
    }
}

impl LaboratoryService{
    // Snapshot source scene/presentation and save the child under the same gate
    // used by parent stop and presentation writes. Recovery never re-resolves.
    fn admit_illustration(&self,session:&LabJob,id:Uuid,args:&Value,resolve:impl FnOnce()->anyhow::Result<Value>)->anyhow::Result<LabJob>{
        let _guard=self.gate.lock();
        let parent=self.get(session.id)?;
        anyhow::ensure!(parent.project_id==session.project_id&&matches!(parent.kind.as_str(),"session"|"specialist"),"Illustration parent is not this project's agent session");
        if let Some(existing)=self.database.lab_record(id)?{
            anyhow::ensure!(existing.project_id==parent.project_id&&existing.parent_id==Some(parent.id)&&existing.kind=="illustration"&&existing.input["request"]==*args,"Illustration request ID is bound to another request");
            return Ok(existing);
        }
        anyhow::ensure!(parent.active()&&parent.deadline_at.is_none_or(|at|at>chrono::Utc::now()),"The parent session is paused, stopped or out of time");
        let input=resolve()?;
        anyhow::ensure!(parent.deadline_at.is_none_or(|at|at>chrono::Utc::now()),"The parent deadline expired while resolving the illustration");
        self.create_locked(id,parent.project_id,Some(parent.id),"illustration",args["title"].as_str().unwrap_or("Scientific illustration"),input,parent.deadline_at)
    }
    pub fn restart_illustration(&self,id:Uuid,deadline:Option<chrono::DateTime<chrono::Utc>>)->anyhow::Result<LabJob>{
        let _reservation=self.acquire(id).context("The renderer is still stopping; retry after it exits")?;
        let result=self.prepare_illustration_restart(id,deadline);self.release(id);
        let replacement=result?;
        if replacement.state=="queued"&&!self.executing(replacement.id){self.start_illustration(replacement.id)?;}
        Ok(replacement)
    }
    fn prepare_illustration_restart(&self,id:Uuid,deadline:Option<chrono::DateTime<chrono::Utc>>)->anyhow::Result<LabJob>{
        let _guard=self.gate.lock();let mut original=self.get(id)?;
        anyhow::ensure!(original.kind=="illustration"&&matches!(original.state.as_str(),"paused"|"failed"|"timed_out"),"Illustration is not resumable");
        if let Some(event)=original.events.iter().rev().find(|event|event.kind=="illustration_restart"){
            let replacement=self.get(Uuid::parse_str(event.data["new_job_id"].as_str().context("Restart receipt missing")?)?)?;
            anyhow::ensure!(replacement.project_id==original.project_id&&replacement.kind=="illustration"&&replacement.input["restarted_from_illustration_id"]==json!(id),"Invalid illustration restart lineage");
            return Ok(replacement);
        }
        // Recover a replacement committed before its backlink event was saved.
        let retained=self.list(Some(original.project_id))?.into_iter().find(|job|job.kind=="illustration"&&job.input["restarted_from_illustration_id"]==json!(id));
        let replacement=if let Some(retained)=retained{retained}else{
            let parent=original.parent_id.map(|parent|self.get(parent)).transpose()?;
            if let Some(parent)=&parent{anyhow::ensure!(parent.project_id==original.project_id&&matches!(parent.kind.as_str(),"session"|"specialist"),"Invalid illustration parent");}
            let parent=parent.filter(|parent|parent.active()&&parent.deadline_at.is_none_or(|at|at>chrono::Utc::now()));
            let deadline=parent.as_ref().map(|parent|parent.deadline_at).unwrap_or(deadline);
            anyhow::ensure!(deadline.is_none_or(|at|at>chrono::Utc::now()),"The render retry requires an explicit unexpired budget or Off");
            let mut input=original.input.clone();input["restarted_from_illustration_id"]=json!(id);
            input["restart_parent_job_id"]=json!(original.parent_id);
            self.create_locked(Uuid::new_v4(),original.project_id,parent.map(|parent|parent.id),"illustration",&original.title,input,deadline)?
        };
        original.event("illustration_restart","A new render attempt reuses the immutable scene; no solver runs.",json!({"new_job_id":replacement.id,"parent_id":replacement.parent_id,"deadline_at":replacement.deadline_at}));
        original.updated_at=chrono::Utc::now();self.database.put_lab_record(&original)?;
        Ok(replacement)
    }
    pub fn start_illustration(&self,id:Uuid)->anyhow::Result<()>{
        let token={
            let _guard=self.gate.lock();let mut job=self.get(id)?;
            anyhow::ensure!(job.kind=="illustration"&&job.state=="queued","Illustration is stopped or already started");
            let mut blocked=job.deadline_at.is_some_and(|at|at<=chrono::Utc::now());
            if let Some(parent)=job.parent_id{
                let parent=self.get(parent)?;
                blocked|=parent.project_id!=job.project_id||!matches!(parent.kind.as_str(),"session"|"specialist")||!parent.active()||parent.deadline_at.is_some_and(|at|at<=chrono::Utc::now())||parent.deadline_at!=job.deadline_at;
            }
            if blocked{
                job.state=if job.deadline_at.is_some_and(|at|at<=chrono::Utc::now()){"timed_out"}else{"paused"}.into();
                job.event("render_launch_blocked","The render or its parent stopped, expired or changed budget before launch. Retry as a separate immutable attempt.",json!({}));
                job.updated_at=chrono::Utc::now();self.database.put_lab_record(&job)?;
                anyhow::bail!("The render or parent stopped, expired or changed budget before launch");
            }
            self.acquire(id)?
        };let service=self.clone();
        tokio::spawn(async move{
            let job=match service.get(id){Ok(job)=>job,Err(_)=>{service.release(id);return;}};
            let expiry_service=service.clone();let expiry=job.deadline_at.map(|deadline|tokio::spawn(async move{tokio::time::sleep((deadline-chrono::Utc::now()).to_std().unwrap_or_default()).await;let _=expiry_service.stop(id,"timed_out");}));
            let result=service.render_illustration(id,&token).await;
            if let Some(expiry)=expiry{expiry.abort();}
            if let Err(error)=result{let _=service.update(id,|job|{if job.active(){job.state="failed".into();}job.error=Some(format!("{error:#}"));job.event("render_stopped",format!("{error:#}"),json!({}));});}
            service.release(id);
        });Ok(())
    }
    async fn render_illustration(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<()>{
        let _slot=tokio::select!{_=token.cancelled()=>anyhow::bail!("Illustration stopped in queue"),slot=self.render_slots.acquire()=>slot?};
        let job=self.get(id)?;anyhow::ensure!(job.kind=="illustration"&&job.state=="queued"&&!token.is_cancelled(),"Illustration stopped before launch");
        let folder=self.directory(id);let mut input=job.input["resolved"].clone();
        let request:RenderRequest=serde_json::from_value(input["request"].clone())?;
        input["parent_pid"]=json!(std::process::id());
        write_json(&folder.join("render-input.json"),&input)?;
        std::fs::write(folder.join("blender_render.py"),include_str!("../../../tools/blender_render.py"))?;
        let blender=crate::studio::render::find_blender().context("Blender is unavailable; no substitute image was generated")?;
        let remaining=job.deadline_at.map(|at|(at-chrono::Utc::now()).num_seconds().max(1)).unwrap_or(0);
        let mut command=process::clean_command(&blender,&folder);
        command.args(["--background","--factory-startup","--disable-autoexec","--python-exit-code","1","--python"]).arg(folder.join("blender_render.py")).arg("--").arg(folder.join("render-input.json")).arg(&folder).arg(remaining.to_string()).arg((request.max_memory_mb*1024*1024).to_string())
            .stdout(std::fs::File::create(folder.join("stdout.log"))?).stderr(std::fs::File::create(folder.join("stderr.log"))?);
        let started=self.update(id,|job|{if job.state=="queued"&&!token.is_cancelled(){job.state="running".into();job.event("render_started","Blender Cycles is rendering the retained illustration scene.",json!({"width":request.width,"height":request.height,"samples":request.samples}));}})?;
        anyhow::ensure!(started.state=="running"&&!token.is_cancelled(),"Illustration stopped before process launch");
        let status=process::OwnedProcess::spawn(&mut command,request.max_memory_mb as usize)?.wait(token).await?;
        anyhow::ensure!(status.success(),"Blender failed ({status}); inspect stderr.log");
        let metadata=self.read_json(id,"renderer.json")?;
        anyhow::ensure!(metadata["width"].as_u64()==Some(request.width as u64)&&metadata["height"].as_u64()==Some(request.height as u64),"Renderer did not honor the requested dimensions");
        let mut artifacts=vec![];
        for name in ["render.png","scene.blend","scene.glb","renderer.json"]{
            let path=folder.join(name);let file=std::fs::File::open(&path)?;
            let length=file.metadata()?.len();anyhow::ensure!(length>0,"Renderer left an empty {name}");
            let mut digest=Sha256::new();std::io::copy(&mut std::io::BufReader::new(file),&mut digest)?;
            artifacts.push(json!({"path":name,"bytes":length,"sha256":format!("{:x}",digest.finalize())}));
        }
        let result=json!({"engine":"blender_cycles","source_illustration_id":job.input["source_illustration_id"],"renderer":metadata,"artifacts":artifacts,"scope":"Scientific illustration; this render is not simulated experimental evidence"});
        write_json(&folder.join("result.json"),&result)?;
        self.update(id,|job|{if job.state=="running"&&!token.is_cancelled(){job.state="completed".into();job.result=result;job.progress=json!({"fraction":1.0});job.event("completed","The illustration, editable scene and renderer provenance are ready.",json!({}));}})?;Ok(())
    }
}
