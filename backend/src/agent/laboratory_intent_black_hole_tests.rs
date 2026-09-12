use super::*;
use super::super::hash;
use std::collections::HashMap;

// Tiny schema fixtures exercise the evidence reader only. They are deliberately
// synthetic and are never registered as an engine, application job or result.
struct Fixture { job: LabJob, diagnostic: Value, documents: HashMap<String, Value> }
impl Fixture {
    fn new(engine: &str) -> Self {
        let id = uuid::Uuid::new_v4();
        let mut job: LabJob = serde_json::from_value(json!({"id":id,"project_id":uuid::Uuid::new_v4(),"parent_id":null,
            "kind":"solver","state":"completed","title":"Synthetic evidence-reader fixture; no engine execution",
            "created_at":"2026-09-12T00:00:00Z","updated_at":"2026-09-12T00:00:00Z","deadline_at":null,"seen_at":null,
            "input":{"engine":engine},"result":{"engine":engine},"progress":{},"error":null,"events":[]})).unwrap();
        let units=json!({"length":"L","time":"L/c","mass":"c^2 L/G","si_mapping":null});
        let unknown=json!({"status":"unknown","gamma":null,"speed_over_c":null,"calibration":null});
        let execution=json!({"complete":true,"process_completed":true,"retained_time_target_met":true,
            "termination_reason":"completed","exit_code":0,"process_group_drained":true,"time_target":1.0,"last_paired_native_time":1.0});
        let mut documents=HashMap::new();let mut artifacts=vec![];let mut frames=vec![];let mut reductions=vec![];
        fn add(documents:&mut HashMap<String,Value>,artifacts:&mut Vec<Value>,name:&str,value:Value)->Value {
            let raw=serde_json::to_vec(&value).unwrap();let pin=json!({"path":name,"sha256":hash(&raw),"bytes":raw.len()});
            documents.insert(name.into(),value);artifacts.push(pin.clone());pin
        }
        for (n,t) in [0.0,1.0].into_iter().enumerate() {
            let metric=add(&mut documents,&mut artifacts,&format!("native/metric-{n}.bin"),json!({"fixture_only":true,"time":t}));
            let constraint=add(&mut documents,&mut artifacts,&format!("native/constraints-{n}.bin"),json!({"fixture_only":true,"time":t}));
            let plane=json!({"axes":["x","y"],"normal_axis":"z","requested_coordinate":0.0});
            let channels=json!({"chi":[[1.0,0.9],[0.8,0.7]],"lapse":[[1.0,0.9],[0.9,1.0]],"hamiltonian":[[0.0,0.01],[0.02,0.01]]});
            let view=json!({"schema":"phaseforge.nr-amr-diagnostic-slice.v1","fixture_only":true,"time":t,"scientific_time":t,
                "cycle":n,"time_unit":"L/c","coordinate_unit":"L","axis_order":["y","x"],"interpolation":"none","grid_location":"cell_center",
                "plane":plane,"source_metric":metric,"source_constraints":constraint,"cell_count":4,
                "blocks":[{"logical":[0,0,0,0],"geometry":[-1.0,1.0,-1.0,1.0,-1.0,1.0],"shape":[2,2],"x":[-0.5,0.5],"y":[-0.5,0.5],"z":-0.5,"channels":channels}]});
            let pin=add(&mut documents,&mut artifacts,&format!("slices/frame-{n:05}.json"),view);
            frames.push(json!({"index":n,"time":t,"scientific_time":t,"cycle":n,"time_unit":"L/c","plane":plane,
                "source_metric":metric,"source_constraints":constraint,"data":pin,"block_count":1,"cell_count":4}));
            reductions.push(json!({"time":t,"cycle":n,"units":units,"accuracy_accepted":null,"H_rms":0.01,"M_rms":0.02,"proper_volume":1.0,"included_cells":8}));
        }
        let measurements=add(&mut documents,&mut artifacts,"measurements.json",json!({"schema":"phaseforge.nr-black-hole-measurements.v1",
            "units":units,"physical_boost":unknown,"constraints":reductions,"fixture_only":true}));
        let index=add(&mut documents,&mut artifacts,"slices/index.json",json!({"schema":"phaseforge.nr-amr-diagnostic-slices.v1",
            "representation":"nr_amr_diagnostic_slice","engine_id":engine,"units":units,"time_unit":"L/c","interpolation":"none","grid_location":"cell_center",
            "axis_order":["y","x"],"execution":execution,"physical_boost":unknown,"primary_channel":"chi","horizons":[],
            "fulfillment":{"requested_0_999c_collision":false,"merger_validated":false,"horizon_properties_validated":false},
            "frame_count":2,"frames":frames,"initial_time":0.0,"final_time":1.0,
            "channels":{"chi":{"unit":"1"},"lapse":{"unit":"1"},"hamiltonian":{"unit":"1/L^2"}}}));
        let diagnostic=json!({"schema":"phaseforge.nr-black-hole-result.v1","job_id":id,"engine_identity":{"engine_id":engine},
            "scope":"Synthetic parser fixture, no physical validation or engine execution","units":units,"execution":execution,"physical_boost":unknown,
            "fulfillment":{"requested_0_999c_collision":false,"merger_validated":false,"horizon_properties_validated":false,"convergence_validated":false},
            "paired_times":[0.0,1.0],"paired_state_count":2,"diagnostic_slices":index,"measurements":measurements,"artifacts":artifacts});
        job.result["diagnostics"]=json!({"path":"diagnostics/result.json","sha256":hash(&serde_json::to_vec(&diagnostic).unwrap())});
        Self{job,diagnostic,documents}
    }
    fn read(&self,descriptor:&Value)->anyhow::Result<Value>{
        let value=self.documents.get(descriptor["path"].as_str().context("Missing fixture path")?).context("Missing fixture data")?;
        let raw=serde_json::to_vec(value)?;
        ensure!(descriptor["bytes"]==json!(raw.len())&&descriptor["sha256"]==hash(&raw),"Fixture retained bytes changed");Ok(value.clone())
    }
    fn inspect(&self)->anyhow::Result<Value>{inspect(&self.job,&self.diagnostic,|pin|self.read(pin))}
    fn replace(&mut self,name:&str,value:Value){
        let old=self.diagnostic["artifacts"].as_array().unwrap().iter().find(|v|v["path"]==name).unwrap().clone();
        let raw=serde_json::to_vec(&value).unwrap();let new=json!({"path":name,"sha256":hash(&raw),"bytes":raw.len()});
        fn walk(value:&mut Value,old:&Value,new:&Value){if value==old{*value=new.clone();return;}match value{Value::Array(a)=>for v in a{walk(v,old,new)},Value::Object(o)=>for v in o.values_mut(){walk(v,old,new)},_=>{}}}
        walk(&mut self.diagnostic,&old,&new);
        for value in self.documents.values_mut(){walk(value,&old,&new);}
        self.documents.insert(name.into(),value);
        // A slice's changed descriptor also changes its parent index bytes.
        if name!="slices/index.json" {let index=self.documents["slices/index.json"].clone();self.replace("slices/index.json",index);}
    }
}

#[test]
fn explicit_cpu_and_cuda_diagnostics_have_numeric_evidence_without_physical_claims(){
    for engine in ["athenak_two_punctures_serial","athenak_two_punctures_cuda"]{
        let f=Fixture::new(engine);require_capability(&f.job,Some(engine)).unwrap();let result=f.inspect().unwrap();
        assert_eq!(result["frame_count"],2);assert_eq!(result["verified_endpoint_samples"][1]["numeric_values"],12);
        assert_eq!(result["measurements"]["sha256"],f.diagnostic["measurements"]["sha256"]);
        for key in ["accuracy_validated","merger_validated","horizon_properties_validated","convergence_validated","requested_0_999c_collision"]{assert_eq!(result[key],false);}
        assert!(result["physical_boost"]["speed_over_c"].is_null());
    }
}

#[test]
fn fixed_diagnostic_never_satisfies_a_different_or_missing_requested_capability(){
    let f=Fixture::new("athenak_two_punctures_cuda");
    for requested in [None,Some("numerical_relativity"),Some("0.999c"),Some("athenak_two_punctures_serial"),Some("athenak_gauge_wave")]{
        assert!(require_capability(&f.job,requested).is_err(),"{requested:?}");
    }
}

#[test]
fn partial_failed_or_single_time_data_do_not_count_as_completed_evidence(){
    for state in ["running","failed","paused","timed_out"]{let mut f=Fixture::new("athenak_two_punctures_serial");f.job.state=state.into();assert!(f.inspect().is_err());}
    let mut f=Fixture::new("athenak_two_punctures_serial");f.diagnostic["execution"]["complete"]=json!(false);assert!(f.inspect().is_err());
    let mut f=Fixture::new("athenak_two_punctures_serial");f.diagnostic["paired_times"]=json!([0.0]);assert!(f.inspect().is_err());
}

#[test]
fn tampered_slice_measurements_or_native_descriptor_are_rejected(){
    for name in ["slices/index.json","slices/frame-00000.json","slices/frame-00001.json","measurements.json"]{
        let mut f=Fixture::new("athenak_two_punctures_cuda");f.documents.get_mut(name).unwrap()["tampered"]=json!(true);assert!(f.inspect().is_err(),"{name}");
    }
    let mut f=Fixture::new("athenak_two_punctures_cuda");let mut index=f.documents["slices/index.json"].clone();
    index["frames"][1]["source_metric"]["sha256"]=json!("0".repeat(64));f.replace("slices/index.json",index);assert!(f.inspect().is_err());
}

#[test]
fn rehashed_numeric_shape_time_and_accuracy_claims_are_rejected(){
    for (path,value) in [("/blocks/0/channels/chi/0/0",json!("not a number")),("/blocks/0/shape",json!([2,3])),
        ("/blocks/0/x",json!([0.5,-0.5])),("/time",json!(0.25)),("/source_metric/sha256",json!("0".repeat(64)))]{
        let mut f=Fixture::new("athenak_two_punctures_serial");let mut view=f.documents["slices/frame-00001.json"].clone();
        *view.pointer_mut(path).unwrap()=value;f.replace("slices/frame-00001.json",view);assert!(f.inspect().is_err(),"{path}");
    }
    let mut f=Fixture::new("athenak_two_punctures_serial");let mut measurements=f.documents["measurements.json"].clone();
    measurements["constraints"][0]["accuracy_accepted"]=json!(true);f.replace("measurements.json",measurements);assert!(f.inspect().is_err());
    let mut f=Fixture::new("athenak_two_punctures_serial");f.diagnostic["physical_boost"]["speed_over_c"]=json!(0.999);assert!(f.inspect().is_err());
}

#[tokio::test]
async fn ordinary_null_capability_analysis_reads_verified_puncture_measurements_but_collision_simulation_cannot(){
    use super::super::{IntentLedger,Journal,OutputIntent,assess};
    use super::super::super::receipt_recovery_tests::fixture;
    let f=fixture().await;
    let job=f.state.laboratory.synthetic_puncture_evidence_fixture(f.job.project_id);
    let directory=f.state.laboratory.directory(job.id).join("diagnostics");
    let mut diagnostic:Value=serde_json::from_slice(&std::fs::read(directory.join("result.json")).unwrap()).unwrap();
    let native:Vec<Value>=diagnostic["artifacts"].as_array().unwrap().iter().filter(|row|row["path"].as_str().unwrap().starts_with("native/")).cloned().collect();
    let mut numeric=Fixture::new("athenak_two_punctures_serial");
    for n in 0..2 {
        let name=format!("slices/frame-{n:05}.json");let mut view=numeric.documents[&name].clone();
        view["source_metric"]=native[0].clone();view["source_constraints"]=native[1].clone();numeric.replace(&name,view);
    }
    let mut index=numeric.documents["slices/index.json"].clone();index["execution"]=diagnostic["execution"].clone();
    for frame in index["frames"].as_array_mut().unwrap(){frame["source_metric"]=native[0].clone();frame["source_constraints"]=native[1].clone();}
    numeric.replace("slices/index.json",index);
    diagnostic["units"]=numeric.diagnostic["units"].clone();diagnostic["diagnostic_slices"]=numeric.diagnostic["diagnostic_slices"].clone();
    diagnostic["measurements"]=numeric.diagnostic["measurements"].clone();
    diagnostic["artifacts"].as_array_mut().unwrap().retain(|row|row["path"]!="measurements.json");
    for pin in numeric.diagnostic["artifacts"].as_array().unwrap().iter().filter(|p|!p["path"].as_str().unwrap().starts_with("native/")) {
        let name=pin["path"].as_str().unwrap();let path=directory.join(name);std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path,serde_json::to_vec(&numeric.documents[name]).unwrap()).unwrap();diagnostic["artifacts"].as_array_mut().unwrap().push(pin.clone());
    }
    let raw=serde_json::to_vec(&diagnostic).unwrap();std::fs::write(directory.join("result.json"),&raw).unwrap();
    let job=f.state.laboratory.update(job.id,|job|{
        let saved=&mut job.result["diagnostics"];saved["sha256"]=json!(hash(&raw));saved["measurements"]=diagnostic["measurements"].clone();
    }).unwrap();
    let before=serde_json::to_value(&job).unwrap();
    let mut intent=IntentLedger::new(OutputIntent::Analysis,f.job.id,"Analyze these saved numerical results",0);
    intent.scope="Read retained diagnostics only".into();intent.evidence_job_ids=vec![job.id];assert!(intent.required_capability.is_none());
    let analysis=assess(&f.state,&f.job,&Journal{output_intent:Some(intent.clone()),..Default::default()}).unwrap();
    assert_eq!(analysis["status"],"fulfilled","{analysis:#}");assert_eq!(analysis["evidence"][0]["path"],"diagnostics/measurements.json");
    assert_eq!(analysis["evidence"][0]["sha256"],diagnostic["measurements"]["sha256"]);
    assert_eq!(serde_json::to_value(f.state.laboratory.get(job.id).unwrap()).unwrap(),before);
    intent.resolved=Some(OutputIntent::Simulation);intent.requested=OutputIntent::Simulation;intent.required_capability=Some("numerical_relativity".into());
    let collision=assess(&f.state,&f.job,&Journal{output_intent:Some(intent),..Default::default()}).unwrap();
    assert_eq!(collision["fulfilled"],false);assert!(collision["evidence"].as_array().unwrap().is_empty());
}
