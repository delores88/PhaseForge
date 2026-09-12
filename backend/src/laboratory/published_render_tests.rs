//! Synthetic retained-data contract fixtures, not scientific validation studies.
use super::*;
use crate::{config::AppConfig,persistence::Database,domain::{CreateProjectRequest,ResearchProject}};
use tokio_util::sync::CancellationToken;

fn document(field:bool)->Value {
    let model=json!({"id":if field{"published-thermal-fixture"}else{"published-isolated-fixture"},"description":"Explicit retained numerical fixture for renderer compatibility","scope":"Synthetic test states; no dynamics or material validity claimed","limitations":["Fixture does not validate any physical solver"]});
    if field {json!({"schema":"phaseforge.simulation.v1","representation":"scalar_field","model":model,"time_unit":"s","length_unit":"um","field_unit":"K","field_name":"fixture_temperature","shape":[2,2],"lengths_um":[4,8],"boundary":"isolated","frames":[{"time":0,"values":[[280,282],[284,286]]},{"time":1,"values":[[282,284],[284,284]]},{"time":2,"values":[[283,283],[283,283]]}]})}
    else {json!({"schema":"phaseforge.simulation.v1","representation":"particle_trajectory","model":model,"time_unit":"days","position_unit":"AU","topology":{"boundary":"isolated","entities":[{"id":"a","display_radius":0.12,"color":"#66ccee"},{"id":"b","display_radius":0.08,"color":"#ffbb55"}],"bonds":[]},"frames":[{"time":0,"entities":[{"id":"a","position":[-2,0,0]},{"id":"b","position":[0,1,0]}]},{"time":1,"entities":[{"id":"a","position":[-1,0.3,0]},{"id":"b","position":[0,0,0]}]},{"time":2,"entities":[{"id":"a","position":[0,0.6,0]},{"id":"b","position":[0,-1,0]}]}]})}
}

fn service(root:&std::path::Path)->LaboratoryService {
    std::fs::create_dir_all(root).unwrap();
    let config=AppConfig{data_directory:root.canonicalize().unwrap(),..Default::default()};
    let db=Database::open(&config.database_path()).unwrap();LaboratoryService::new(db,config).unwrap()
}

fn fixture(service:&LaboratoryService,field:bool)->(LabJob,LabJob,LabJob,Value) {
    let project=ResearchProject::new(CreateProjectRequest{name:Some("Published renderer contract".into()),question:"Synthetic retained numeric fixtures, no model claims".into()});
    service.database.put_project(&project).unwrap();
    let parent=service.create(Uuid::new_v4(),project.id,None,"session","Fixture parent",json!({}),None).unwrap();
    let source=service.create(Uuid::new_v4(),project.id,Some(parent.id),"generated","Retained synthetic fixture",json!({"execution_purpose":"scientific","code":"# Fixed data-contract fixture; not an actual scientific engine execution"}),None).unwrap();
    let folder=service.directory(source.id);std::fs::create_dir_all(folder.join("work")).unwrap();
    let bytes=serde_json::to_vec(&document(field)).unwrap();let hash=format!("{:x}",Sha256::digest(&bytes));
    std::fs::write(folder.join("source.py"),source.input["code"].as_str().unwrap()).unwrap();
    write_json(&folder.join("manifest.json"),&json!({"scope":"Synthetic contract fixture; no numerical execution claim"})).unwrap();
    std::fs::write(folder.join("work/states.json"),&bytes).unwrap();
    write_json(&folder.join("generated-artifacts.json"),&json!({"artifacts":[{"path":"work/states.json","bytes":bytes.len(),"sha256":hash}]})).unwrap();
    service.update(source.id,|job|job.state="completed".into()).unwrap();
    let args=json!({"source_job_id":source.id,"path":"work/states.json","sha256":hash});
    let id=Uuid::new_v4();crate::laboratory::publication::publish(service,&parent,id,&args,&CancellationToken::new()).unwrap();
    (parent,service.get(id).unwrap(),source,args)
}

#[test]
fn published_render_admission_binds_units_model_and_immutable_source() {
    let temporary=tempfile::tempdir().unwrap();let service=service(temporary.path());
    for field in [false,true] {
        let (parent,source,_,_)=fixture(&service,field);
        let metadata=source_metadata(&service,&source).unwrap();
        assert_eq!(metadata["kind"],"published_simulation");assert_eq!(metadata["scientific_validation"],"not_established_by_publication");
        let index=source_index(&service,source.id).unwrap();assert_eq!(index["start_time"],0.0);assert_eq!(index["end_time"],2.0);
        let args=json!({"source_job_id":source.id,"time":1.0,"width":640,"height":360});
        let observation=service.create_observation(parent.id,Uuid::new_v4(),&args).unwrap();
        assert_eq!(observation.input["settings"]["source_metadata"],metadata);
        assert_eq!(observation.field_output(),field);assert_eq!(observation.input["settings"]["presentation"]["interpolate"],false);
        let foreign_project=ResearchProject::new(CreateProjectRequest{name:None,question:"Other project".into()});service.database.put_project(&foreign_project).unwrap();
        let foreign=service.create(Uuid::new_v4(),foreign_project.id,None,"session","Foreign",json!({}),None).unwrap();
        assert!(service.create_observation(foreign.id,Uuid::new_v4(),&args).is_err());
        let path=if field{"fields/field-00000000.npy"}else{"topology.json"};
        let original=std::fs::read(service.directory(source.id).join(path)).unwrap();
        std::fs::write(service.directory(source.id).join(path),b"changed retained data").unwrap();
        assert!(source_metadata(&service,&source).is_err());
        std::fs::write(service.directory(source.id).join(path),original).unwrap();
        let mut incomplete=source.clone();incomplete.state="paused".into();assert!(source_metadata(&service,&incomplete).is_err());
        let mut false_model=source.clone();false_model.result["model"]["scope"]=json!("validated replacement claim");assert!(source_metadata(&service,&false_model).is_err());
        let permit=service.acquire(source.id).unwrap();assert!(source_metadata(&service,&source).is_err());drop(permit);service.release(source.id);
    }
}

#[test]
fn published_old_support_receipt_replays_without_rewriting_history() {
    let temporary=tempfile::tempdir().unwrap();let service=service(temporary.path());
    let (parent,source,_,args)=fixture(&service,false);
    let mut old=source.result.clone();old["presentation"]=json!({"native_playback":true,"exports":false,"capture":false,"scope":"Historical publication-time support"});
    std::fs::write(service.directory(source.id).join("result.json"),serde_json::to_vec(&old).unwrap()).unwrap();service.update(source.id,|job|job.result=old.clone()).unwrap();
    let before=std::fs::read(service.directory(source.id).join("result.json")).unwrap();
    crate::laboratory::publication::publish(&service,&parent,source.id,&args,&CancellationToken::new()).unwrap();
    assert_eq!(std::fs::read(service.directory(source.id).join("result.json")).unwrap(),before);
    assert!(source_metadata(&service,&service.get(source.id).unwrap()).is_ok());
}

#[tokio::test]
#[ignore="Requires existing Blender and explicit PHASEFORGE_PUBLISHED_RENDER_EVIDENCE; runs trusted rendering only"]
async fn actual_published_field_and_isolated_particle_capture_and_video() {
    let root=std::path::PathBuf::from(std::env::var_os("PHASEFORGE_PUBLISHED_RENDER_EVIDENCE").expect("New persistent evidence directory required"));
    assert!(!root.exists(),"Preserve prior evidence");let service=service(&root);
    let mut reports=vec![];
    for field in [false,true] {
        let (parent,source,_,_)=fixture(&service,field);
        let before=std::fs::read(service.directory(source.id).join(source.result["index_path"].as_str().unwrap())).unwrap();
        let observation=service.create_observation(parent.id,Uuid::new_v4(),&json!({"source_job_id":source.id,"time":1,"width":640,"height":360})).unwrap();
        service.start_observation(observation.id).unwrap();
        let completed=tokio::time::timeout(Duration::from_secs(90),async{loop{let job=service.get(observation.id).unwrap();if !job.active()&&!service.executing(job.id){break job;}tokio::time::sleep(Duration::from_millis(100)).await;}}).await.unwrap();
        assert_eq!(completed.state,"completed","{:?}",completed.error);assert_eq!(completed.result["scientific_rerun"],false);
        assert_eq!(completed.result["renderer"]["source_metadata"]["model"],source.result["model"]);
        let request=ExportRequest{width:640,height:360,fps:2,playback_duration_seconds:1.0,start_time:0.0,end_time:2.0,presentation:json!({}),labels:true,renderer:"eevee".into(),samples:8,time_limit_seconds:Some(90)};
        let metadata=source_metadata(&service,&source).unwrap();let settings=worker_settings(&request,2).unwrap();
        let export=service.create(Uuid::new_v4(),source.project_id,Some(source.id),"export","Published fixture video",json!({"source_job_id":source.id,"settings":settings,"source_metadata":metadata}),None).unwrap();
        service.run_export(export.id,&CancellationToken::new()).await.unwrap();
        let exported=service.get(export.id).unwrap();assert_eq!(exported.state,"completed");assert_eq!(exported.result["scientific_rerun"],false);
        assert_eq!(exported.result["source_metadata"],metadata);assert_eq!(exported.result["decode_check"]["frame_count"],2);
        if !field {assert_eq!(exported.result["display_domain"]["periodic"],false);assert_eq!(exported.result["position_unit"],"AU");assert_eq!(exported.result["time_unit"],"days");}
        assert_eq!(std::fs::read(service.directory(source.id).join(source.result["index_path"].as_str().unwrap())).unwrap(),before);
        reports.push(json!({"source_job_id":source.id,"representation":source.result["representation"],"source_index_sha256":source.result["index_sha256"],"observation_job_id":completed.id,"observation":completed.result,"export_job_id":exported.id,"export":exported.result,"source_index_unchanged":true}));
    }
    write_json(&root.join("report.json"),&json!({"passed":true,"scope":"Actual supervised trusted Blender rendering of synthetic publication fixtures; no scientific solver or validity claim","cases":reports})).unwrap();
}
