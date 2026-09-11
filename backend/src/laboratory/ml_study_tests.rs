use super::*;
use crate::{config::AppConfig,domain::{CreateProjectRequest,ResearchProject},persistence::Database};
fn fixture(path:&std::path::Path)->(LaboratoryService,Uuid,LabJob){
    fs::create_dir_all(path).unwrap();let config=AppConfig{data_directory:path.into(),gpu_enabled:false,..Default::default()};
    let database=Database::open(&config.database_path()).unwrap();let project=ResearchProject::new(CreateProjectRequest{name:Some("ML protocol acceptance".into()),question:"Frozen data roles and actual numerical execution".into()});database.put_project(&project).unwrap();
    let service=LaboratoryService::new(database,config).unwrap();let parent=service.create(Uuid::new_v4(),project.id,None,"session","ML study tests",json!({}),None).unwrap();
    let study=service.create_ml_study(Uuid::new_v4(),parent.id,"Fit and independently evaluate the preregistered argon endpoint",1024).unwrap();
    service.save_ml_ledger(study.id,&Ledger{schema_version:1,input_sha256:value_sha(&study.input).unwrap(),..Default::default()}).unwrap();
    (service,project.id,study)
}
#[test]
fn protected_labels_cannot_be_retagged_copied_or_observed_through_monitor_lineage(){
    let temp=tempfile::tempdir().unwrap();let (service,project,study)=fixture(temp.path());
    let source=service.create_active_child(Uuid::new_v4(),study.id,"solver","Held-out fixture only",json!({}),).unwrap();
    service.update(source.id,|job|job.state="completed".into()).unwrap();
    let forged=service.create(Uuid::new_v4(),project,None,"generated","Forged role",json!({"code":WORKER,"role":"train","study_id":study.id,"sources":[{"job_id":source.id,"path":"renamed.json"}]}),None).unwrap();
    assert!(service.ensure_study_import_access(&forged,source.id).is_err());
    assert!(service.ensure_model_study_access(forged.id).is_err());
    let monitor=service.create(Uuid::new_v4(),project,None,"monitor","Derived numerical event",json!({"job_id":source.id}),None).unwrap();
    assert!(service.ensure_model_study_access(monitor.id).is_err());
    service.update(study.id,|job|job.state="completed".into()).unwrap();
    assert!(service.ensure_model_study_access(source.id).is_ok());assert!(service.ensure_model_study_access(monitor.id).is_ok());
}
#[test]
fn only_exact_registered_stage_source_and_imports_can_open_protected_data(){
    let temp=tempfile::tempdir().unwrap();let (service,_,study)=fixture(temp.path());
    let source=service.create_active_child(Uuid::new_v4(),study.id,"study_data","Label fixture only",json!({})).unwrap();
    let target=Uuid::new_v4();let input=json!({"code":WORKER,"sources":[{"job_id":source.id,"path":"bundle.json","destination":"train.json"}],"inputs":{"phase":"fit"}});
    let mut ledger=service.ml_ledger(study.id).unwrap();ledger.stages.insert("fit".into(),Stage{kind:"generated".into(),input_sha256:value_sha(&input).unwrap(),attempts:vec![target],completion:None});service.save_ml_ledger(study.id,&ledger).unwrap();
    let admitted=service.create_active_child(target,study.id,"generated","Registered fit fixture",input).unwrap();assert!(service.ensure_study_import_access(&admitted,source.id).is_ok());
    let mut altered=admitted.clone();altered.input["sources"][0]["path"]=json!("test.json");assert!(service.ensure_study_import_access(&altered,source.id).is_err());
    altered=admitted.clone();altered.input["code"]=json!("print('forged')");assert!(service.ensure_study_import_access(&altered,source.id).is_err());
    altered=admitted;altered.id=Uuid::new_v4();assert!(service.ensure_study_import_access(&altered,source.id).is_err());
}
#[test]
fn storage_traversal_ignores_unrelated_invalid_requests_and_counts_reserved_failures(){
    let temp=tempfile::tempdir().unwrap();let (service,project,study)=fixture(temp.path());
    service.create(Uuid::new_v4(),project,None,"generated","Unresolved unrelated request",json!({"sources":[{"job_id":Uuid::new_v4()}]}),None).unwrap();
    let baseline=service.ml_retained_bytes(study.id).unwrap();let reserved=Uuid::new_v4();let mut ledger=service.ml_ledger(study.id).unwrap();ledger.stages.insert("reserved".into(),Stage{attempts:vec![reserved],..Default::default()});service.save_ml_ledger(study.id,&ledger).unwrap();
    fs::create_dir_all(service.directory(reserved)).unwrap();fs::write(service.directory(reserved).join("partial.tmp"),vec![0u8;4096]).unwrap();
    assert!(service.ml_retained_bytes(study.id).unwrap()>=baseline+4096);
    service.update(study.id,|job|job.input["storage_mb"]=json!(0)).unwrap();assert!(service.check_ml_storage(study.id).is_err());
}
#[tokio::test]
async fn immutable_metadata_stage_replays_once_and_rejects_changed_completed_bytes(){
    let temp=tempfile::tempdir().unwrap();let (service,_,study)=fixture(temp.path());let token=CancellationToken::new();
    let first=service.ml_metadata(study.id,"split-fixture",json!({"membership":"synthetic access fixture"}),&token).await.unwrap();
    let replay=service.ml_metadata(study.id,"split-fixture",json!({"membership":"synthetic access fixture"}),&token).await.unwrap();assert_eq!(first,replay);
    fs::write(service.directory(first).join("bundle.json"),b"{\"changed\":true}").unwrap();
    assert!(service.ml_metadata(study.id,"split-fixture",json!({"membership":"synthetic access fixture"}),&token).await.is_err());
    service.stop(study.id,"paused").unwrap();assert!(service.ml_metadata(study.id,"late",json!({}),&token).await.is_err());
}
#[test]
fn model_freeze_and_source_identity_are_part_of_the_retained_study_input(){
    let temp=tempfile::tempdir().unwrap();let (service,_,study)=fixture(temp.path());
    assert_eq!(study.input["worker_sha256"],sha(WORKER.as_bytes()));assert_eq!(study.input["solver_worker_sha256"],sha(SOLVER.as_bytes()));
    let mut changed=study.input.clone();changed["worker_sha256"]=json!("different");assert_ne!(value_sha(&study.input).unwrap(),value_sha(&changed).unwrap());
    assert_eq!(service.ml_ledger(study.id).unwrap().input_sha256,value_sha(&study.input).unwrap());
}
#[tokio::test]
async fn cost_snapshot_survives_reserved_publication_without_recomputing_later_stages(){
    let temp=tempfile::tempdir().unwrap();let (service,_,study)=fixture(temp.path());
    let snapshot=service.ml_cost_snapshot(study.id,&BTreeMap::new()).unwrap();
    let reserved=Uuid::new_v4();let input=json!({"metadata":snapshot});
    let mut ledger=service.ml_ledger(study.id).unwrap();
    ledger.stages.insert("measured-costs".into(),Stage{kind:"study_data".into(),input_sha256:value_sha(&input).unwrap(),attempts:vec![reserved],completion:None});
    // A later stage reservation has no DB row, so an eager cost recomputation
    // would fail. Neither missing row changes the already committed snapshot.
    ledger.stages.insert("finalize-costs".into(),Stage{kind:"generated".into(),attempts:vec![Uuid::new_v4()],..Default::default()});
    service.save_ml_ledger(study.id,&ledger).unwrap();
    assert_eq!(service.ml_cost_snapshot(study.id,&BTreeMap::new()).unwrap(),snapshot);
    let result=service.ml_metadata(study.id,"measured-costs",snapshot.clone(),&CancellationToken::new()).await.unwrap();
    assert_eq!(result,reserved);assert_eq!(service.read_json(result,"bundle.json").unwrap(),snapshot);
}
#[test]
fn cost_charge_covers_elapsed_coordinator_time_and_terminal_receipts_ignore_later_updates(){
    let temp=tempfile::tempdir().unwrap();let (service,_,study)=fixture(temp.path());
    let mut original=study.clone();original.created_at=Utc::now()-chrono::Duration::seconds(60);service.database.put_lab_record(&original).unwrap();
    let costs=service.ml_cost_snapshot(study.id,&BTreeMap::new()).unwrap();
    assert!(costs["environment_startup_seconds"].as_f64().unwrap()>=60.0);
    assert_eq!(costs["child_cost_subtotal_seconds"],0.0);
    original.event("failed","Synthetic terminal fixture",json!({}));let expected=retained_attempt_seconds(&original).unwrap();
    original.updated_at=Utc::now()+chrono::Duration::days(1);original.seen_at=Some(original.updated_at);
    assert_eq!(retained_attempt_seconds(&original).unwrap(),expected);
}
#[tokio::test(flavor="multi_thread",worker_threads=4)]
#[ignore="Requires explicit local output and a ready source freeze; runs one actual scientific seed in LPAC/managed OpenMM"]
async fn actual_ml_protocol_freeze_pilot_and_reduction(){
    let root=std::path::PathBuf::from(std::env::var_os("PHASEFORGE_ML_ACCEPTANCE_ROOT").expect("Set an explicit acceptance root"));assert!(root.is_absolute());
    let run=root.join(Uuid::new_v4().to_string());let (service,_,study)=fixture(&run);let token=service.acquire(study.id).unwrap();
    let freeze=service.ml_compute(study.id,"freeze",json!({"schema_version":1,"phase":"freeze","study_id":study.id,"proposal_sha256":PROPOSAL,"solver_worker_sha256":sha(SOLVER.as_bytes()),"solver_engine_version":"8.5.2.dev-36a30cb"}),vec![],&token).await.unwrap();
    let frozen=service.read_json(freeze,"work/runs.json").unwrap();let runs=frozen["runs"].as_array().unwrap();assert_eq!(runs.len(),99);assert_eq!(runs[0]["role"],"train");
    let pilot=service.ml_sweep(study.id,"pilot",&runs[..1],&frozen["protocol"],&token).await.unwrap();
    let rows=service.ml_solver_rows(&[pilot]).unwrap();let shards=service.ml_reduce_role(study.id,freeze,"train",&runs[..1],&rows,&token).await.unwrap();
    assert_eq!(shards.len(),1);let shard=service.read_json(shards[0],"work/shard.json").unwrap();assert_eq!(shard["seeds"].as_array().unwrap().len(),1);
    assert!(shard["seeds"][0]["pressure_bar"].as_f64().unwrap().is_finite());
    assert!(service.ensure_model_study_access(shards[0]).is_err());
    write_json(&root.join("latest-report.json"),&json!({"passed":true,"scope":"Actual protocol freeze, one real training pilot and isolated reduction; no scientific model fitting or complete study claim","data_directory":run,"study_id":study.id,"freeze_job_id":freeze,"pilot_sweep_job_id":pilot,"shard_job_id":shards[0],"seed":shard["seeds"][0],"worker_sha256":sha(WORKER.as_bytes())})).unwrap();
    service.stop(study.id,"paused").unwrap();service.release(study.id);
}

#[tokio::test(flavor="multi_thread",worker_threads=4)]
#[ignore="Requires an explicit synthetic plot fixture and output root; starts the supervised pinned Pillow worker"]
async fn actual_registered_ml_plot_uses_exact_copied_inputs(){
    let root=std::path::PathBuf::from(std::env::var_os("PHASEFORGE_ML_PLOT_ACCEPTANCE_ROOT").expect("Set a plot acceptance directory"));
    let fixture_root=std::path::PathBuf::from(std::env::var_os("PHASEFORGE_ML_PLOT_FIXTURE").expect("Set the synthetic plot fixture directory"));
    assert!(root.is_absolute()&&fixture_root.is_absolute());let run=root.join(Uuid::new_v4().to_string());let (service,_,study)=fixture(&run);let token=service.acquire(study.id).unwrap();
    let mut config=json!({"schema_version":1});let mut sources=vec![];
    for key in ["evaluation","split","ood"]{
        // Source hashes bind original bytes, including JSON serialization. This
        // fixture represents completed upstream data, not a newly parsed split.
        let raw=fs::read(fixture_root.join("imports").join(format!("{key}.json"))).unwrap();
        let source=service.create_active_child(Uuid::new_v4(),study.id,"study_data",&format!("Synthetic plot input: {key}"),json!({"fixture_only":true,"source_sha256":sha(&raw)})).unwrap();
        fs::write(service.directory(source.id).join("fixture.json"),&raw).unwrap();
        write_json(&service.directory(source.id).join("result.json"),&json!({"fixture_only":true,"sha256":sha(&raw)})).unwrap();
        service.update(source.id,|job|job.state="completed".into()).unwrap();
        service.ml_add(&mut config,&mut sources,key,source.id,"fixture.json").unwrap();
    }
    let plot=service.ml_stage(study.id,"actual-synthetic-plot","study_plot",json!({"worker_sha256":crate::laboratory::ml_plot::source_hash(),"config":config,"sources":sources}),&token).await.unwrap();
    let result=service.read_json(plot,"result.json").unwrap();let png=fs::read(service.path(plot,"plot/plot.png").unwrap()).unwrap();assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));assert_eq!(result["png_sha256"],sha(&png));
    assert_eq!(result["source_pins"].as_array().unwrap().len(),3);assert!(service.ensure_model_study_access(plot).is_err());
    write_json(&root.join("latest-report.json"),&json!({"passed":true,"scope":"Actual supervised plotting of explicitly synthetic test data; no scientific model or solver success claimed","data_directory":run,"study_id":study.id,"plot_job_id":plot,"result":result})).unwrap();
    service.stop(study.id,"paused").unwrap();service.release(study.id);
}
