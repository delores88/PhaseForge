use super::*;
use crate::{config::AppConfig, domain::{CreateProjectRequest, ResearchProject}, persistence::Database};
use std::path::Path;

fn protocol() -> Value {
    json!({"engine":"openmm_argon","engine_version":"8.5.2.dev-36a30cb","worker_sha256":sha(SOLVER.as_bytes()),
        "platform":"CPU","platform_properties":{"DeterministicForces":"true","Threads":"1"},
        "potential":{"scope":"fixed classical argon fixture identity"},
        "fixed_parameters":{"atom_count":108,"thermostat":"langevin","friction_per_ps":5.0,"timestep_fs":1.0,
            "steps":20000,"sample_interval":100,"chunk_frames":50,"platform":"CPU","cpu_threads":1},
        "target":{"name":"mean_pressure_bar","step_exclusive":10000,"step_inclusive":20000,"replicates":3,"observations_per_seed":100}})
}
fn query() -> Value {
    json!({"features":{"temperature_kelvin":199.0,"density_g_cm3":0.55},
        "units":{"temperature_kelvin":"K","density_g_cm3":"g/cm^3"},"intent":"scientific","protocol":protocol()})
}
fn service(path: &Path) -> (LaboratoryService, LabJob) {
    fs::create_dir_all(path).unwrap();
    let config = AppConfig{data_directory:path.into(),gpu_enabled:false,..Default::default()};
    let database = Database::open(&config.database_path()).unwrap();
    let project = ResearchProject::new(CreateProjectRequest{name:Some("Surrogate query receipts".into()),question:"Deterministic fixtures, no scientific execution".into()});
    database.put_project(&project).unwrap();
    let service = LaboratoryService::new(database,config).unwrap();
    let parent = service.create(Uuid::new_v4(),project.id,None,"session","Fixture parent",json!({}),None).unwrap();
    (service,parent)
}
fn completed_stage(service: &LaboratoryService, parent: Uuid, kind: &str, input: Value, files: Vec<(&str,Vec<u8>)>) -> (LabJob,Value) {
    let job = service.create_active_child(Uuid::new_v4(),parent,kind,"Synthetic receipt fixture only",input).unwrap();
    let mut artifacts = vec![];
    for (path,bytes) in files {
        let full = service.directory(job.id).join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap(); fs::write(full,&bytes).unwrap();
        artifacts.push(json!({"path":path,"bytes":bytes.len(),"sha256":sha(&bytes)}));
    }
    if kind == "generated" { write_json(&service.directory(job.id).join("generated-artifacts.json"),&json!({"artifacts":artifacts})).unwrap(); }
    write_json(&service.directory(job.id).join("result.json"),&json!({"scope":"synthetic receipt fixture"})).unwrap();
    service.update(job.id,|job|job.state="completed".into()).unwrap();
    let job=service.get(job.id).unwrap();
    let pins=service.artifact_inventory(job.id).unwrap().as_array().unwrap().iter().map(|row| {
        let path=row["path"].as_str().unwrap();let bytes=fs::read(service.path(job.id,path).unwrap()).unwrap();
        json!({"path":path,"bytes":bytes.len(),"sha256":sha(&bytes)})
    }).collect::<Vec<_>>();
    let hash=sha(&serde_json::to_vec(&job.input).unwrap());
    let stage=json!({"kind":kind,"input_sha256":hash,"attempts":[job.id],
        "completion":{"job_id":job.id,"input_sha256":hash,"artifacts":pins,"wall_seconds":0.001}});
    (job,stage)
}
fn completed_source(service: &LaboratoryService, parent: &LabJob) -> (LabJob,Uuid) {
    let study=service.create_ml_study(Uuid::new_v4(),parent.id,"Synthetic admission fixture; no scientific fitting",1024).unwrap();
    let split=serde_json::to_vec(&json!({"protocol":protocol(),"protocol_sha256":"fixture-protocol"})).unwrap();
    let split_hash=sha(&split);let model=b"synthetic fixture bytes; not a trained model".to_vec();let model_hash=sha(&model);
    let card=serde_json::to_vec(&json!({"model_sha256":model_hash,"protocol":protocol()})).unwrap();let card_hash=sha(&card);
    let calibration=serde_json::to_vec(&json!({"model_sha256":model_hash,"q_Z":0.2})).unwrap();let calibration_hash=sha(&calibration);
    let (freeze,freeze_stage)=completed_stage(service,study.id,"generated",json!({"phase":"freeze"}),vec![("work/split.json",split)]);
    let (fit,fit_stage)=completed_stage(service,study.id,"generated",json!({"phase":"fit"}),vec![("work/model.npz",model),("work/model-card.json",card)]);
    let (calibrated,calibration_stage)=completed_stage(service,study.id,"generated",json!({"phase":"calibrate"}),vec![("work/calibration.json",calibration)]);
    let (evaluated,evaluation_stage)=completed_stage(service,study.id,"generated",json!({"phase":"evaluate"}),vec![("work/evaluation.json",serde_json::to_vec(&json!({"model_sha256":model_hash,"calibration_sha256":calibration_hash,"useful_acceleration":false})).unwrap())]);
    let model_freeze=json!({"fit_job_id":fit.id,"model_sha256":model_hash,"model_card_sha256":card_hash,"split_sha256":split_hash,"protocol_sha256":"fixture-protocol","worker_sha256":sha(WORKER.as_bytes())});
    write_json(&service.directory(study.id).join("model-freeze.json"),&model_freeze).unwrap();
    write_json(&service.directory(study.id).join("study-ledger.json"),&json!({"schema_version":1,"input_sha256":sha(&serde_json::to_vec(&study.input).unwrap()),
        "stages":{"freeze":freeze_stage,"fit":fit_stage,"calibrate":calibration_stage,"evaluate":evaluation_stage},"model_freeze":model_freeze})).unwrap();
    service.update(study.id,|job| {job.state="completed".into();job.result=json!({"model_freeze":model_freeze,"freeze_job_id":freeze.id,"fit_job_id":fit.id,"calibration_job_id":calibrated.id,"evaluation_job_id":evaluated.id});}).unwrap();
    (service.get(study.id).unwrap(),calibrated.id)
}
fn sample_evidence(parameters: &Value) -> (Value,Value) {
    let volume=parameters["atom_count"].as_u64().unwrap() as f64*39.948/6.02214076e23/parameters["density_g_cm3"].as_f64().unwrap()*1e21;
    let side=volume.cbrt();let cutoff=(2.5_f64*0.3405).min(0.49*side);
    let p=protocol();let manifest=json!({"engine":p["engine"],"engine_version":p["engine_version"],"worker_sha256":p["worker_sha256"],
        "platform":p["platform"],"platform_properties":p["platform_properties"],"parameters":parameters,
        "model":{"box_nm":[side,side,side],"mass_dalton":39.948,"sigma_nm":0.3405,"epsilon_kj_mol":0.997,"charges":0,
            "dispersion_correction":false,"boundary":"cubic periodic","cutoff_nm":cutoff,"switch_nm":cutoff*0.8}});
    let steps=parameters["steps"].as_u64().unwrap();let interval=parameters["sample_interval"].as_u64().unwrap();let dt=parameters["timestep_fs"].as_f64().unwrap();
    let series=(0..=steps.div_ceil(interval)).map(|index| {let step=(index*interval).min(steps);
        json!({"step":step,"time_ps":step as f64*dt/1000.0,"pressure_bar":index as f64*2.0-300.0,"kinetic_energy_kj_mol":1.0})}).collect::<Vec<_>>();
    (manifest,json!({"units":{"time":"ps","position":"nm","velocity":"nm/ps","energy":"kJ/mol","force":"kJ/(mol nm)","temperature":"K","pressure":"bar","density":"g/cm^3","msd":"nm^2"},"series":series}))
}

#[test]
fn arbitrary_valid_query_has_three_fresh_replay_stable_solver_seeds() {
    let parameters=fallback_parameters(&query(),&protocol()).unwrap();let id=Uuid::new_v4();
    let first=query_runs(id,&parameters);assert_eq!(first,query_runs(id,&parameters));
    assert_ne!(first,query_runs(Uuid::new_v4(),&parameters));
    let seeds=first.iter().map(|run|run["seed"].as_u64().unwrap()).collect::<BTreeSet<_>>();
    assert_eq!(seeds.len(),3);assert!(seeds.iter().all(|seed|*seed>=1_000_000&&*seed<=i32::MAX as u64));
    assert!(first.iter().all(|run|run["parameters"]["temperature_kelvin"]==199.0&&run["parameters"]["density_g_cm3"]==0.55));
}
#[test]
fn incompatible_identity_invalid_units_and_invalid_work_never_form_solver_inputs() {
    for (key,value) in [("engine",json!("invented")),("potential",json!({"epsilon":900.0})),("target",json!({"name":"equilibrium_pressure"})),("worker_sha256",json!("changed"))] {
        let mut request=query();request["protocol"][key]=value;assert!(fallback_parameters(&request,&protocol()).is_err());
    }
    let mut request=query();request["units"]["temperature_kelvin"]=json!("C");assert!(fallback_parameters(&request,&protocol()).is_err());
    request=query();request["features"]["density_g_cm3"]=json!(-1.0);assert!(fallback_parameters(&request,&protocol()).is_err());
    request=query();request["protocol"]["fixed_parameters"]["steps"]=json!(5_000_000);assert!(fallback_parameters(&request,&protocol()).is_err());
    request=query();request["protocol"]["fixed_parameters"]["sample_interval"]=json!(200);assert!(fallback_parameters(&request,&protocol()).is_err());
    request=query();request["protocol"]["fixed_parameters"]["platform"]=json!("CUDA");assert!(fallback_parameters(&request,&protocol()).is_err());
    request=query();request["protocol"]["fixed_parameters"]["extra"]=json!(1);assert!(fallback_parameters(&request,&protocol()).is_err());
}
#[test]
fn changed_supported_protocol_is_executed_as_requested_and_window_times_are_measured() {
    let mut request=query();request["protocol"]["fixed_parameters"]["atom_count"]=json!(32);
    request["protocol"]["fixed_parameters"]["timestep_fs"]=json!(2.0);
    let parameters=query_runs(Uuid::new_v4(),&fallback_parameters(&request,&protocol()).unwrap())[0]["parameters"].clone();
    let (manifest,measurements)=sample_evidence(&parameters);
    let endpoint=pressure_window(&manifest,&measurements,&parameters,&request["protocol"]).unwrap();
    // Arithmetic progression 101..200 gives mean 150.5, hence 2*150.5-300=1.
    assert_eq!(endpoint["pressure_bar"],1.0);assert_eq!(endpoint["observations"],100);
    assert_eq!(endpoint["window"]["time_exclusive_ps"],20.0);assert_eq!(endpoint["window"]["time_inclusive_ps"],40.0);
    assert_eq!(endpoint["block_means_bar"],json!([-79.0,-39.0,1.0,41.0,81.0]));
}
#[test]
fn numerical_endpoint_rejects_reordered_missing_or_mislabelled_evidence() {
    let parameters=query_runs(Uuid::new_v4(),&fallback_parameters(&query(),&protocol()).unwrap())[0]["parameters"].clone();
    let (manifest,measurements)=sample_evidence(&parameters);
    for mutation in 0..5 {
        let mut changed=measurements.clone();let mut changed_manifest=manifest.clone();
        match mutation {
            0=>changed["series"].as_array_mut().unwrap().swap(101,102),
            1=>{changed["series"].as_array_mut().unwrap().pop();},
            2=>changed["units"]["pressure"]=json!("Pa"),
            3=>changed["series"][200]["time_ps"]=json!(99.0),
            _=>changed_manifest["model"]["epsilon_kj_mol"]=json!(2.0),
        }
        assert!(pressure_window(&changed_manifest,&changed,&parameters,&protocol()).is_err());
    }
}
#[test]
fn query_admission_pins_source_defaults_only_protocol_and_inherits_off() {
    let temp=tempfile::tempdir().unwrap();let (service,parent)=service(temp.path());let (source,calibration)=completed_source(&service,&parent);
    let mut requested=query();requested.as_object_mut().unwrap().remove("protocol");let id=Uuid::new_v4();
    let job=service.create_ml_query(id,parent.id,source.id,requested.clone(),512).unwrap();
    assert_eq!(job.deadline_at,None);assert_eq!(job.input["original_query"],requested);assert_eq!(job.input["query"]["protocol"],protocol());
    assert_eq!(job.input["source_pins"]["artifacts"]["calibration"]["job_id"],json!(calibration));
    service.stop(parent.id,"paused").unwrap();
    assert_eq!(service.create_ml_query(id,parent.id,source.id,requested.clone(),512).unwrap().id,id);
    assert!(service.create_ml_query(Uuid::new_v4(),parent.id,source.id,requested.clone(),512).is_err());
    requested["features"]["temperature_kelvin"]=json!(250.0);
    assert!(service.create_ml_query(id,parent.id,source.id,requested,512).is_err());
}
#[test]
fn altered_calibration_is_rejected_before_a_query_receipt_or_solver_is_created() {
    let temp=tempfile::tempdir().unwrap();let (service,parent)=service(temp.path());let (source,calibration)=completed_source(&service,&parent);
    let count=service.list(Some(parent.project_id)).unwrap().len();
    fs::write(service.directory(calibration).join("work/calibration.json"),b"{\"q_Z\":0.00001}").unwrap();
    assert!(service.create_ml_query(Uuid::new_v4(),parent.id,source.id,query(),512).is_err());
    assert_eq!(service.list(Some(parent.project_id)).unwrap().len(),count);
}

async fn replay_fixed_decision(decision: Value, intent: &str) {
    let temp=tempfile::tempdir().unwrap();let (service,parent)=service(temp.path());let (source,_)=completed_source(&service,&parent);
    let mut requested=query();requested["intent"]=json!(intent);
    let job=service.create_ml_query(Uuid::new_v4(),parent.id,source.id,requested,512).unwrap();
    service.initialize_ml_ledger(job.id).unwrap();
    let pins=&job.input["source_pins"]["artifacts"];
    let (mut request,mut sources)=service.ml_request(source.id,"infer",id_at(&pins["split"],"job_id").unwrap()).unwrap();
    for key in ["model","model_card","calibration","evaluation"] { service.ml_add(&mut request,&mut sources,key,id_at(&pins[key],"job_id").unwrap(),pins[key]["path"].as_str().unwrap()).unwrap(); }
    request["query"]=job.input["query"].clone();
    let input=json!({"engine":"python_numpy","code":WORKER,"inputs":request,"sources":sources,
        "limits":{"memory_mb":512,"process_limit":1,"wall_seconds":120,"storage_mb":128}});
    let (inference,stage)=completed_stage(&service,job.id,"generated",input,vec![("work/inference.json",serde_json::to_vec(&decision).unwrap())]);
    let mut ledger=service.read_json(job.id,"study-ledger.json").unwrap();ledger["stages"]["query-inference"]=stage;
    write_json(&service.directory(job.id).join("study-ledger.json"),&ledger).unwrap();
    let count=service.list(Some(parent.project_id)).unwrap().len();
    service.execute_ml_query(job.id,&CancellationToken::new()).await.unwrap();
    let finished=service.get(job.id).unwrap();assert_eq!(finished.state,"completed");assert_eq!(finished.result["decision"],decision);
    assert_eq!(finished.result["inference_job_id"],json!(inference.id));assert_eq!(finished.result["solver_launched"],false);
    assert_eq!(service.list(Some(parent.project_id)).unwrap().len(),count,"No generation or solver was repeated during replay");
}
#[tokio::test]
async fn recovered_invalid_query_never_schedules_a_solver_even_if_advisory_flag_is_true() {
    replay_fixed_decision(json!({"status":"invalid_input","reason":"wrong unit fixture","schedule_solver":true}),"scientific").await;
}
#[tokio::test]
async fn recovered_explanation_refusal_reuses_inference_and_never_launches_a_solver() {
    replay_fixed_decision(json!({"status":"requires_solver","reason":"outside_surrogate_rectangle","schedule_solver":true}),"explanation").await;
}
