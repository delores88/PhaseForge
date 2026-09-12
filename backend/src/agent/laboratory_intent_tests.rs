use super::*;
use super::super::receipt_recovery_tests::fixture;

fn request(journal:&mut Journal,job:&LabJob,kind:OutputIntent,text:&str){journal.output_intent=Some(IntentLedger::new(kind,job.id,text,0));}
fn resolution(journal:&Journal,kind:OutputIntent,scope:&str,capability:Option<&str>)->Value{
    let current=journal.output_intent.as_ref().unwrap();json!({"request_id":current.request_id,"intent":kind,"scope":scope,"required_capability":capability,"user_instruction_quote":current.user_instruction,"update_effect":"replace_objective"})
}

#[tokio::test]
async fn resolved_contract_replay_preserves_gap_without_creating_another_user_instruction(){
    let f=fixture().await;let mut journal=Journal::default();
    request(&mut journal,&f.job,OutputIntent::Auto,"I want a playable physical black-hole merger, not a still");
    let pending=context(&journal).unwrap();assert_eq!(pending["role"],"developer");assert!(pending["content"].as_str().unwrap().contains("There has been no intervening user update"));
    let mut args=resolution(&journal,OutputIntent::Simulation,"Physical merger through ringdown",Some("numerical_relativity"));
    args["update_effect"]=json!("preserve_objective");args["capability_gap"]=json!("No Einstein evolution engine is integrated");
    set(&f.state,&f.job,&mut journal,&args).unwrap();
    let before=serde_json::to_value(journal.output_intent.as_ref().unwrap()).unwrap();
    let event_count=f.state.laboratory.get(f.job.id).unwrap().events.len();
    args["user_instruction_quote"]=json!("Preserve the previous_objective and continue its unfinished work after answering a status question.");
    args["scope"]=json!("A differently phrased scope must not overwrite the frozen request");
    for gap in [Value::Null,json!("Rephrased uncertainty about whether a solver is available")] {
        args["capability_gap"]=gap;
        let replay=set(&f.state,&f.job,&mut journal,&args).unwrap();assert_eq!(replay["already_resolved"],true);assert_eq!(replay["output_intent"],before);
    }
    assert_eq!(f.state.laboratory.get(f.job.id).unwrap().events.len(),event_count);
    let captured=context(&journal).unwrap();let text=captured["content"].as_str().unwrap();
    assert!(text.contains("already resolved and its capability gap is retained"));assert!(!text.contains("Call set_output_intent once"));assert!(!text.contains("after answering a status question"));
    assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"capability_gap");
    let mut changed=args.clone();changed["required_capability"]=json!("generated_temporal");assert!(set(&f.state,&f.job,&mut journal,&changed).is_err());
    changed=args.clone();changed["intent"]=json!("illustration");assert!(set(&f.state,&f.job,&mut journal,&changed).is_err());
    changed=args.clone();changed["request_id"]=json!(Uuid::new_v4());assert!(set(&f.state,&f.job,&mut journal,&changed).is_err());
    request(&mut journal,&f.job,OutputIntent::Auto,"Calculate the integral and retain an error estimate");
    let args=resolution(&journal,OutputIntent::Analysis,"Static quadrature",None);set(&f.state,&f.job,&mut journal,&args).unwrap();
    assert!(context(&journal).unwrap()["content"].as_str().unwrap().contains("Proceed with the appropriate execution"));assert!(guard(&journal,"run_generated_experiment",&json!({"purpose":"scientific"})).is_ok());
}

#[tokio::test]
async fn explicit_simulation_and_semantic_correction_cannot_downgrade_to_still(){
    let f=fixture().await;let mut journal=Journal::default();
    request(&mut journal,&f.job,OutputIntent::Simulation,"Run the collision simulation");
    let wrong=resolution(&journal,OutputIntent::Illustration,"Still scene",None);
    assert!(set(&f.state,&f.job,&mut journal,&wrong).is_err());
    let args=resolution(&journal,OutputIntent::Simulation,"Requested finite-speed merger",Some("numerical_relativity"));
    set(&f.state,&f.job,&mut journal,&args).unwrap();
    let update=Uuid::new_v4();steer(&mut journal,update,&json!({"content":"those are images I want a playable simulation"})).unwrap();
    assert_eq!(journal.output_intent.as_ref().unwrap().resolved,None);
    let args=resolution(&journal,OutputIntent::Simulation,"Requested playable merger",Some("numerical_relativity"));
    set(&f.state,&f.job,&mut journal,&args).unwrap();
    let mut substitute=args.clone();substitute["intent"]=json!("illustration");assert!(set(&f.state,&f.job,&mut journal,&substitute).is_err());
    substitute=args.clone();substitute["required_capability"]=json!("generated_temporal");assert!(set(&f.state,&f.job,&mut journal,&substitute).is_err());
    assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"capability_gap");
    assert!(guard(&journal,"launch_experiment",&json!({"engine":"newtonian_nbody"})).is_err());
    let still=Uuid::new_v4();steer(&mut journal,still,&json!({"content":"I accept a conceptual still instead","output_intent":"illustration"})).unwrap();
    let args=resolution(&journal,OutputIntent::Illustration,"Explicitly accepted conceptual still",None);set(&f.state,&f.job,&mut journal,&args).unwrap();
    assert!(guard(&journal,"render_illustration",&json!({})).is_ok());
    let again=Uuid::new_v4();steer(&mut journal,again,&json!({"content":"Now run the physical model","output_intent":"simulation"})).unwrap();
    assert_eq!(journal.output_intent.as_ref().unwrap().resolved,Some(OutputIntent::Simulation));
    assert_eq!(journal.output_intent.as_ref().unwrap().capability_gap,None);
}

#[tokio::test]
async fn explanation_never_launches_science_and_presentation_generation_keeps_explicit_purpose(){
    let f=fixture().await;let mut journal=Journal::default();
    request(&mut journal,&f.job,OutputIntent::Explanation,"Explain this result. Do not rerun it.");
    let args=resolution(&journal,OutputIntent::Explanation,"Explain saved evidence without new experiments",None);set(&f.state,&f.job,&mut journal,&args).unwrap();
    let before=f.state.laboratory.list(Some(f.job.project_id)).unwrap().len();
    for name in ["launch_experiment","run_generated_experiment","launch_solver_sweep","launch_ml_study"] {
        assert!(f.state.agent.execute_lab_tool(f.state.clone(),&f.job,&f.request,name,&json!({}),Uuid::new_v4(),&mut journal,&CancellationToken::new()).await.is_err());
    }
    assert_eq!(before,f.state.laboratory.list(Some(f.job.project_id)).unwrap().len());
    assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"fulfilled");
    steer(&mut journal,Uuid::new_v4(),&json!({"content":"Generate labels on this saved view","output_intent":"presentation_edit"})).unwrap();
    let args=resolution(&journal,OutputIntent::PresentationEdit,"Presentation-only labels from retained source",None);set(&f.state,&f.job,&mut journal,&args).unwrap();
    assert!(guard(&journal,"run_generated_experiment",&json!({"purpose":"scientific","sources":[]})).is_err());
    assert!(guard(&journal,"run_generated_experiment",&json!({"purpose":"presentation","sources":[]})).is_err());
    assert!(guard(&journal,"run_generated_experiment",&json!({"purpose":"presentation","sources":[{"job_id":f.job.id,"path":"source.json"}]})).is_ok());
}

#[tokio::test]
async fn summaries_and_serialization_preserve_current_correction_and_capability(){
    let f=fixture().await;let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::Illustration,"Make a still");
    let update=Uuid::new_v4();steer(&mut journal,update,&json!({"content":"I want actual time evolution","output_intent":"simulation"})).unwrap();
    let args=resolution(&journal,OutputIntent::Simulation,"Temporal heat conduction",Some("heat_conduction_2d"));set(&f.state,&f.job,&mut journal,&args).unwrap();
    journal.memory=json!({"summary":"A stale model summary said to make a still"});journal.context_start=journal.items.len();
    let restored:Journal=serde_json::from_slice(&serde_json::to_vec(&journal).unwrap()).unwrap();
    assert_eq!(restored.output_intent.as_ref().unwrap().request_id,update);
    assert_eq!(restored.output_intent.as_ref().unwrap().resolved,Some(OutputIntent::Simulation));
    assert!(context(&restored).unwrap()["content"].as_str().unwrap().contains("heat_conduction_2d"));
    assert!(guard(&restored,"launch_experiment",&json!({"engine":"openmm_argon"})).is_err());
    assert!(guard(&restored,"launch_solver_sweep",&json!({"cases":[{"engine":"heat_conduction_2d"},{"engine":"openmm_argon"}]})).is_err());
    let mut continued=restored;steer(&mut continued,Uuid::new_v4(),&json!({"content":"How far along is it?"})).unwrap();
    let mut args=resolution(&continued,OutputIntent::Simulation,"Continue the requested heat calculation",Some("heat_conduction_2d"));args["update_effect"]=json!("preserve_objective");
    set(&f.state,&f.job,&mut continued,&args).unwrap();
    assert_eq!(continued.output_intent.as_ref().unwrap().resolved,Some(OutputIntent::Simulation));
}

#[tokio::test]
async fn status_before_first_resolution_preserves_the_unresolved_original_instruction(){
    let f=fixture().await;let mut journal=Journal::default();
    request(&mut journal,&f.job,OutputIntent::Auto,"Run a playable heat-conduction simulation and retain the temperatures");
    let status_id=Uuid::new_v4();steer(&mut journal,status_id,&json!({"content":"How far along?"})).unwrap();
    let previous=journal.output_intent.as_ref().unwrap().previous_objective.as_ref().unwrap();
    assert_eq!(previous["request_id"],json!(f.job.id));assert_eq!(previous["requested"],"auto");assert!(previous["resolved"].is_null());
    let original=previous["user_instruction"].clone();
    let mut args=resolution(&journal,OutputIntent::Simulation,"Continue the requested heat simulation",Some("heat_conduction_2d"));args["update_effect"]=json!("preserve_objective");
    assert!(set(&f.state,&f.job,&mut journal,&args).is_err(),"A status quote alone cannot establish what the original unresolved request asked for");
    args["preserved_instruction_quote"]=json!("How far along?");assert!(set(&f.state,&f.job,&mut journal,&args).is_err());
    args["preserved_instruction_quote"]=original.clone();set(&f.state,&f.job,&mut journal,&args).unwrap();
    assert_eq!(journal.output_intent.as_ref().unwrap().request_id,status_id);assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"not_fulfilled");
    assert!(guard(&journal,"launch_experiment",&json!({"engine":"heat_conduction_2d"})).is_ok());
    steer(&mut journal,Uuid::new_v4(),&json!({"content":"Any progress?"})).unwrap();
    assert_eq!(journal.output_intent.as_ref().unwrap().previous_objective.as_ref().unwrap()["user_instruction"],original);
    let mut changed=resolution(&journal,OutputIntent::Explanation,"Status only",None);changed["update_effect"]=json!("preserve_objective");assert!(set(&f.state,&f.job,&mut journal,&changed).is_err());
    steer(&mut journal,Uuid::new_v4(),&json!({"content":"Cancel the simulation objective; explain the existing result only","output_intent":"explanation"})).unwrap();
    let corrected=resolution(&journal,OutputIntent::Explanation,"Explicit replacement with explanation only",None);set(&f.state,&f.job,&mut journal,&corrected).unwrap();
    assert!(guard(&journal,"launch_experiment",&json!({"engine":"heat_conduction_2d"})).is_err());
    request(&mut journal,&f.job,OutputIntent::Simulation,"Run the requested heat-conduction simulation");
    steer(&mut journal,Uuid::new_v4(),&json!({"content":"What is the status?"})).unwrap();
    let mut explicit=resolution(&journal,OutputIntent::Simulation,"Resolve the originally requested heat model",Some("heat_conduction_2d"));explicit["update_effect"]=json!("preserve_objective");explicit["preserved_instruction_quote"]=json!("Run the requested heat-conduction simulation");
    set(&f.state,&f.job,&mut journal,&explicit).unwrap();
    assert!(guard(&journal,"launch_experiment",&json!({"engine":"heat_conduction_2d"})).is_ok());
}

#[tokio::test]
async fn static_analysis_can_execute_and_complete_without_temporal_states(){
    let f=fixture().await;let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::Auto,"Compute an integral numerically and retain the estimate");
    assert!(guard(&journal,"run_generated_experiment",&json!({"purpose":"scientific"})).is_err());
    let args=resolution(&journal,OutputIntent::Analysis,"Static numerical quadrature with convergence checks",None);set(&f.state,&f.job,&mut journal,&args).unwrap();
    assert!(guard(&journal,"run_generated_experiment",&json!({"purpose":"scientific"})).is_ok());
    assert!(guard(&journal,"launch_ml_study",&json!({})).is_ok());
    let source=f.state.laboratory.create(Uuid::new_v4(),f.job.project_id,Some(f.job.id),"generated","Synthetic static numerical receipt",json!({"execution_purpose":"scientific"}),None).unwrap();
    let root=f.state.laboratory.directory(source.id);std::fs::create_dir_all(root.join("work")).unwrap();
    let raw=serde_json::to_vec(&json!({"estimate":0.3333333333,"absolute_error":3.3e-11})).unwrap();std::fs::write(root.join("work/result.json"),&raw).unwrap();
    write_json(&root.join("generated-artifacts.json"),&json!({"artifacts":[{"path":"work/result.json","bytes":raw.len(),"sha256":hash(&raw)}]})).unwrap();
    f.state.laboratory.update(source.id,|job|job.state="completed".into()).unwrap();
    let result=check(&f.state,&f.job,&mut journal,&json!({"request_id":f.job.id,"evidence_job_ids":[source.id]})).unwrap();
    assert_eq!(result["status"],"fulfilled");assert_eq!(result["evidence"][0]["temporal_sampling_required"],false);
    assert!(!root.join("trajectory").exists());
    f.state.laboratory.update(source.id,|job|job.input["execution_purpose"]=json!("presentation")).unwrap();
    assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"not_fulfilled");
}

#[tokio::test]
async fn temporal_completion_checks_real_times_hashes_and_kind_not_claimed_counts(){
    let f=fixture().await;let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::Simulation,"Run an isolated trajectory");
    let args=resolution(&journal,OutputIntent::Simulation,"Isolated Newtonian model",Some("newtonian_nbody"));set(&f.state,&f.job,&mut journal,&args).unwrap();
    let source=f.state.laboratory.create(Uuid::new_v4(),f.job.project_id,Some(f.job.id),"solver","Synthetic retained contract fixture",json!({"engine":"newtonian_nbody"}),None).unwrap();
    let root=f.state.laboratory.directory(source.id);std::fs::create_dir_all(root.join("trajectory")).unwrap();
    write_json(&root.join("manifest.json"),&json!({"engine":"newtonian_nbody","scientific_scope":"Synthetic isolated fixture"})).unwrap();
    let chunk=json!({"frames":[{"time":0,"entities":[{"id":"a","position":[0,0,0]}]},{"time":1,"entities":[{"id":"a","position":[1,0,0]}]}]});
    let raw=serde_json::to_vec(&chunk).unwrap();std::fs::write(root.join("trajectory/chunk.json"),&raw).unwrap();
    write_json(&root.join("trajectory/index.json"),&json!({"time_unit":"T0","frame_count":2,"chunks":[{"path":"trajectory/chunk.json","sha256":hash(&raw)}]})).unwrap();
    f.state.laboratory.update(source.id,|job|job.state="completed".into()).unwrap();
    let ids=json!({"request_id":f.job.id,"evidence_job_ids":[source.id]});
    assert_eq!(check(&f.state,&f.job,&mut journal,&ids).unwrap()["status"],"fulfilled");
    std::fs::write(root.join("trajectory/chunk.json"),b"{}").unwrap();
    assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"not_fulfilled");
    std::fs::write(root.join("trajectory/chunk.json"),raw).unwrap();
    f.state.laboratory.update(source.id,|job|{job.kind="illustration".into();job.result=json!({"frame_count":999999,"answer":"A real simulation finished"});}).unwrap();
    assert_eq!(assess(&f.state,&f.job,&journal).unwrap()["status"],"not_fulfilled");
}

#[tokio::test]
async fn final_prose_cannot_complete_a_simulation_or_duplicate_received_usage(){
    let f=fixture().await;let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::Simulation,"Run a collision");
    let args=resolution(&journal,OutputIntent::Simulation,"Requested Newtonian collision",Some("newtonian_nbody"));set(&f.state,&f.job,&mut journal,&args).unwrap();
    let row=f.state.agent.usage.begin(f.job.id,Some(f.job.project_id),ProviderKind::OpenAi,"gpt-6-astra","laboratory_tool_turn",0,10,128).unwrap();
    let result=json!({"answer":"The simulation is complete. Here is a beautiful still.","rounds":1,"tool_count":0});
    let message=ConversationMessage::new(f.job.project_id,ConversationRole::Assistant,MessageKind::Chat,result["answer"].as_str().unwrap());
    journal.delivery=Some(ResponseDelivery{usage_id:row.id,message:Some(message),result:Some(result),vision:None});
    let path=f.state.laboratory.directory(f.job.id).join("journal.json");write_json(&path,&journal).unwrap();
    assert!(f.state.agent.deliver_lab_response(&f.state,f.job.id,&path,&mut journal).await.unwrap());
    let saved=f.state.laboratory.get(f.job.id).unwrap();assert_eq!(saved.state,"paused");assert_eq!(saved.result["deliverable"]["status"],"not_fulfilled");assert!(saved.completed_at.is_none());
    assert_eq!(f.state.database.get_usage_record(row.id).unwrap().unwrap().status,"completed");
    assert!(!f.state.agent.deliver_lab_response(&f.state,f.job.id,&path,&mut journal).await.unwrap());
    assert_eq!(f.state.laboratory.get(f.job.id).unwrap().events.iter().filter(|e|e.kind=="deliverable_attention").count(),1);
}

#[tokio::test(flavor="current_thread")]
async fn stopped_or_steered_during_async_evidence_read_never_completes_the_old_answer(){
    for action in ["cancel","steer"] {
        let f=fixture().await;f.state.laboratory.update(f.job.id,|job|job.state="running".into()).unwrap();
        let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::Explanation,"Explain the retained diagnostic scope");
        let row=f.state.agent.usage.begin(f.job.id,Some(f.job.project_id),ProviderKind::OpenAi,"gpt-6-astra","laboratory_tool_turn",0,10,128).unwrap();
        let message=ConversationMessage::new(f.job.project_id,ConversationRole::Assistant,MessageKind::Chat,"Retained explanation under the earlier request.");
        journal.delivery=Some(ResponseDelivery{usage_id:row.id,message:Some(message),result:Some(json!({"answer":"Retained explanation"})),vision:None});
        let path=f.state.laboratory.directory(f.job.id).join("journal.json");write_json(&path,&journal).unwrap();
        let (started,ready)=tokio::sync::oneshot::channel();let (release,held)=std::sync::mpsc::channel();
        let io=tokio::spawn(crate::laboratory::LaboratoryService::retained_nr_io(move||{let _=started.send(());held.recv_timeout(Duration::from_secs(3))?;Ok(())}));
        ready.await.unwrap();
        let delivery=f.state.agent.deliver_lab_response(&f.state,f.job.id,&path,&mut journal);tokio::pin!(delivery);
        tokio::select!{result=&mut delivery=>panic!("Evidence queue was not awaited: {result:?}"),_=tokio::time::sleep(Duration::from_millis(20))=>{}}
        if action=="cancel"{f.state.laboratory.stop(f.job.id,"cancelled").unwrap();}
        else {f.state.laboratory.event(f.job.id,"steering_received","Synthetic delayed user correction",json!({"request_id":Uuid::new_v4(),"content":"Use the corrected question"})).unwrap();}
        release.send(()).unwrap();io.await.unwrap().unwrap();assert!(!delivery.await.unwrap());
        let saved=f.state.laboratory.get(f.job.id).unwrap();assert_eq!(saved.state,if action=="cancel"{"cancelled"}else{"running"});
        assert!(saved.result.is_null());assert!(saved.events.iter().all(|event|event.kind!="completed"));assert_eq!(f.provider_post_count(),0);
    }
}

#[tokio::test]
async fn completed_presentation_receipt_survives_async_check_and_final_delivery(){
    let f=fixture().await;let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::PresentationEdit,"Change the saved view color");
    journal.tools.insert("retained-edit".into(),json!({"name":"edit_presentation","state":"completed","intent_request_id":f.job.id,
        "output":{"revision":2,"settings":{"color":"#3388ff"}}}));
    let request:SessionRequest=serde_json::from_value(f.job.input.clone()).unwrap();
    let checked=f.state.agent.execute_lab_tool(f.state.clone(),&f.job,&request,"check_deliverable",&json!({"request_id":f.job.id,"evidence_job_ids":[]}),
        Uuid::new_v4(),&mut journal,&CancellationToken::new()).await.unwrap();
    assert_eq!(checked["fulfilled"],true,"{checked:#}");assert_eq!(journal.tools.len(),1);
    let row=f.state.agent.usage.begin(f.job.id,Some(f.job.project_id),ProviderKind::OpenAi,"gpt-6-astra","laboratory_tool_turn",0,10,128).unwrap();
    journal.delivery=Some(ResponseDelivery{usage_id:row.id,message:None,result:Some(json!({"answer":"Saved view color changed"})),vision:None});
    let path=f.state.laboratory.directory(f.job.id).join("journal.json");write_json(&path,&journal).unwrap();
    assert!(f.state.agent.deliver_lab_response(&f.state,f.job.id,&path,&mut journal).await.unwrap());
    assert_eq!(f.state.laboratory.get(f.job.id).unwrap().result["deliverable"]["fulfilled"],true);assert_eq!(f.provider_post_count(),0);
}

// Same descriptor/units/execution layout as nr_gauge_result.py. Opaque container
// bytes are deliberate: this intent check hashes native data, not a second decoder.
fn gauge_fixture(state: &AppState, session: &LabJob, mut alter: impl FnMut(&str, &mut Value)) -> LabJob {
    let job = state.laboratory.create(Uuid::new_v4(), session.project_id, Some(session.id), "solver",
        "Synthetic gauge intent evidence", json!({"engine":"athenak_gauge_wave","parameters":{"nx":2}}), None).unwrap();
    let root = state.laboratory.directory(job.id);
    let mut artifacts = vec![];
    fn raw(root: &std::path::Path, name: &str, content: &[u8]) -> Value {
        let path = root.join(name); std::fs::create_dir_all(path.parent().unwrap()).unwrap(); std::fs::write(path, content).unwrap();
        json!({"path":name,"bytes":content.len(),"sha256":hash(content)})
    }
    fn document(root: &std::path::Path, name: &str, mut value: Value, alter: &mut impl FnMut(&str, &mut Value), inventory: &mut Vec<Value>) -> (Value, Value) {
        alter(name, &mut value); let entry = raw(&root.join("diagnostics"), name, &serde_json::to_vec(&value).unwrap());
        inventory.push(entry.clone()); (entry, value)
    }
    let units = json!({"length":"L","time":"L/c","length_scale":1.0,"speed_of_light":1.0,
        "system":"dimensionless geometrized code coordinates; L=1 and c=1","si_mapping":null});
    let scope = "Flat-spacetime harmonic coordinate gauge; not a black-hole collision or physical radiation";
    let engine = json!({"schema":"phaseforge.nr-engine.v1","engine_id":"athenak_gauge_wave",
        "source":{"synthetic_fixture":true},"build":{"problem":"z4c/z4c_gauge_wave","backend":"Serial","precision":"double"},
        "executable":{"path":"/trusted/synthetic-not-executed","sha256":"f".repeat(64)}});
    let engine_pin = raw(&root, "nr-engine.json", &serde_json::to_vec(&engine).unwrap());
    let input = raw(&root, "input.athinput", b"Synthetic input; never executed");
    let mut frames = vec![]; let mut native = vec![]; let mut checks = vec![]; let mut series = vec![];
    for n in 0..3 {
        let time = n as f64 * 0.05;
        let mut source = vec![];
        for channel in ["z4c", "con"] {
            let name = format!("bin/gauge.{channel}.{n:05}.bin");
            let content = format!("Synthetic native bytes: {channel} frame {n}");
            let pin = raw(&root.join("native"), &name, content.as_bytes()); native.push(pin);
            let pin = raw(&root.join("diagnostics"), &format!("native/{name}"), content.as_bytes());
            artifacts.push(pin.clone()); source.push(pin);
        }
        let mut arrays = vec![];
        for name in [format!("slices/frame-{n:05}.npz"),format!("blocks/metric-{n:05}.npz"),format!("blocks/constraints-{n:05}.npz")] {
            let pin = raw(&root.join("diagnostics"), &name, format!("Synthetic pinned NPZ stand-in: {name}").as_bytes());
            artifacts.push(pin.clone()); arrays.push(pin);
        }
        let (data, _) = document(&root, &format!("slices/frame-{n:05}.json"), json!({
            "schema":"phaseforge.nr-diagnostic-slice-data.v1","x":[0.25,0.75],"y":[0.25,0.75],"z":0.375,
            "shape":[2,2],"axis_order":["y","x"],"channels":{
                "alpha":[[1.0+time,1.0],[1.0,1.0]],"gxx":[[1.0,1.0],[1.0,1.0]],
                "Kxx":[[0.0,0.0],[0.0,0.0]],"hamiltonian":[[0.0,0.0],[0.0,0.0]]}}), &mut alter, &mut artifacts);
        frames.push(json!({"index":n,"time":time,"scientific_time":time,"cycle":n,"time_unit":"L/c","shape":[2,2],
            "plane":{"axes":["x","y"],"normal_axis":"z","coordinate":0.375,"coordinate_unit":"L","index":1},
            "data":data,"arrays":arrays[0],"source_frame":source[0],"source_constraint_frame":source[1],
            "source_block":{"metric":arrays[1],"constraints":arrays[2]},"analytic_passed":true}));
        checks.push(json!({"time":time,"cycle":n,"passed":true}));
        series.push(json!({"time":time,"cycle":n,"analytic_passed":true,"channels_3d":{
            "alpha":{"min":1.0,"max":1.0+time,"mean":1.0+time/4.0,"unit":"1"},
            "gxx":{"min":1.0,"max":1.0,"mean":1.0,"unit":"1"},
            "Kxx":{"min":0.0,"max":0.0,"mean":0.0,"unit":"1/L"},
            "hamiltonian":{"min":0.0,"max":0.0,"mean":0.0,"unit":"1/L^2"}}}));
    }
    let receipt = json!({"schema":"phaseforge.nr-process-receipt.v1","job_id":job.id,"engine_id":"athenak_gauge_wave",
        "engine_manifest_sha256":engine_pin["sha256"],"source":engine["source"],"build":engine["build"],
        "executable":engine["executable"],"input":input,"files":native,"exit_code":0,
        "termination_reason":"completed","process_group_drained":true,"deadline_at":null});
    let (receipt_pin, receipt) = document(&root, "sources/execution-receipt.json", receipt, &mut alter, &mut artifacts);
    raw(&root, "nr-process-receipt.json", &serde_json::to_vec(&receipt).unwrap());
    let (analytic_pin, _) = document(&root, "analytic.json", json!({"schema":"phaseforge.nr-gauge-analytic.v1",
        "passed":true,"status":"passed","check":{"passed":true,"frames":checks}}), &mut alter, &mut artifacts);
    let (measurements, _) = document(&root, "measurements.json", json!({"schema":"phaseforge.nr-gauge-measurements.v1","units":units,"series":series}), &mut alter, &mut artifacts);
    let (index_pin, index) = document(&root, "slices/index.json", json!({"schema":"phaseforge.nr-diagnostic-slices.v1",
        "representation":"nr_diagnostic_slice","units":units,"axis_order":["y","x"],"time_unit":"L/c",
        "grid_location":"cell_center","interpolation":"none","primary_channel":"alpha",
        "channels":{"alpha":{"unit":"1"},"gxx":{"unit":"1"},"Kxx":{"unit":"1/L"},"hamiltonian":{"unit":"1/L^2"}},
        "frame_count":3,"initial_time":0.0,"final_time":0.1,"frames":frames,"black_hole_collision":false,"physical_radiation":false}), &mut alter, &mut artifacts);
    let mut diagnostic = json!({"schema":"phaseforge.nr-gauge-result.v1","engine":"athenak_gauge_wave","scientific_scope":scope,"units":units,
        "execution":{"receipt_type":"phaseforge.nr-process-receipt.v1","job_id":job.id,"success":true,
            "exit_code":0,"termination_reason":"completed","process_group_drained":true},
        "analytic":{"passed":true,"status":"passed","report":analytic_pin},
        "fulfillment":{"gauge_reference_validated":true,"black_hole_collision_validated":false,"physical_radiation":false},
        "slice_index":index_pin,"measurements":measurements,"frame_count":3,"initial_time":0.0,"final_time":0.1,
        "timeframes":index["frames"],"sources":{"execution_receipt":receipt_pin},"artifacts":artifacts});
    alter("result.json", &mut diagnostic);
    let diagnostic_pin = raw(&root, "diagnostics/result.json", &serde_json::to_vec(&diagnostic).unwrap());
    let saved = json!({"path":"diagnostics/result.json","sha256":diagnostic_pin["sha256"],
        "gauge_reference_validated":diagnostic["fulfillment"]["gauge_reference_validated"],"analytic":diagnostic["analytic"],
        "slice_index":diagnostic["slice_index"],"frame_count":diagnostic["frame_count"],"initial_time":diagnostic["initial_time"],
        "final_time":diagnostic["final_time"],"scope":diagnostic["scientific_scope"]});
    state.laboratory.update(job.id, |job| { job.state="completed".into(); job.result=json!({"engine":"athenak_gauge_wave",
        "execution_completed":true,"exit_code":0,"termination_reason":"completed","process_receipt":"nr-process-receipt.json",
        "process_receipt_sha256":receipt_pin["sha256"],"diagnostics":saved}); }).unwrap()
}

#[tokio::test]
async fn retained_gauge_evidence_does_not_register_an_unavailable_engine() {
    let f=fixture().await; let job=gauge_fixture(&f.state,&f.job,|_,_|{});
    let mut journal=Journal::default();request(&mut journal,&f.job,OutputIntent::Simulation,"Evolve the harmonic gauge benchmark");
    let args=resolution(&journal,OutputIntent::Simulation,"Flat-spacetime gauge evolution",Some("athenak_gauge_wave"));
    set(&f.state,&f.job,&mut journal,&args).unwrap();
    let result=check(&f.state,&f.job,&mut journal,&json!({"request_id":f.job.id,"evidence_job_ids":[job.id]})).unwrap();
    assert_eq!(result["status"],"capability_gap","{result:#}");
    assert_eq!(result["fulfilled"],false);
    assert!(result["capability_gap"].as_str().unwrap().contains("No integrated executable capability named athenak_gauge_wave"));
    assert_eq!(result["evidence"].as_array().unwrap().len(),1);
    assert!(result["rejected_evidence"].as_array().unwrap().is_empty());
    assert!(guard(&journal,"launch_experiment",&json!({"engine":"athenak_gauge_wave"})).is_err());
}

#[tokio::test]
async fn gauge_completion_and_analysis_use_exact_diagnostics_without_a_generic_field_manifest() {
    let f=fixture().await; let job=gauge_fixture(&f.state,&f.job,|_,_|{});
    // This fixture represents a persisted contract resolved when its optional
    // runtime was admitted. Assessing retained evidence is independent of admitting
    // a new executable in this temporary profile; the separate test above covers
    // the real availability gate without registering a pretend WSL runtime.
    let mut admitted=IntentLedger::new(OutputIntent::Simulation,f.job.id,"Evolve the harmonic gauge benchmark",0);
    admitted.scope="Flat-spacetime gauge evolution".into();
    admitted.required_capability=Some("athenak_gauge_wave".into());
    let mut journal=Journal::default();journal.output_intent=Some(admitted);
    let result=check(&f.state,&f.job,&mut journal,&json!({"request_id":f.job.id,"evidence_job_ids":[job.id]})).unwrap();
    assert_eq!(result["status"],"fulfilled","{result:#}");
    let evidence=&result["evidence"][0]; assert_eq!(evidence["time_unit"],"L/c");assert_eq!(evidence["frame_count"],3);
    assert_eq!(evidence["verified_endpoint_samples"][1]["time"],0.1);assert_eq!(evidence["verified_endpoint_samples"][0]["numeric_values"],16);
    assert_eq!(evidence["black_hole_collision_validated"],false);assert_eq!(evidence["physical_radiation"],false);
    let restored:Journal=serde_json::from_slice(&serde_json::to_vec(&journal).unwrap()).unwrap();
    assert_eq!(assess(&f.state,&f.job,&restored).unwrap()["evidence"],result["evidence"]);
    let analysis=numerical_evidence(&f.state,&job).unwrap();assert_eq!(analysis["path"],"diagnostics/measurements.json");
    assert_eq!(analysis["sha256"],evidence["measurements"]["sha256"]);assert_eq!(analysis["temporal_sampling_required"],false);
    assert!(!f.state.laboratory.directory(job.id).join("manifest.json").exists());
}

#[tokio::test]
async fn gauge_cannot_fulfill_a_black_hole_capability_even_with_valid_gauge_artifacts() {
    let f=fixture().await;let job=gauge_fixture(&f.state,&f.job,|_,_|{});
    let mut intent=IntentLedger::new(OutputIntent::Simulation,f.job.id,"Run the actual black-hole collision",0);
    for capability in ["numerical_relativity","athenak_two_punctures_serial","athenak_two_punctures_cuda"] {
        intent.required_capability=Some(capability.into());
        let error=temporal_evidence(&f.state,&job,&intent).unwrap_err();assert!(error.to_string().contains("Requested capability does not match"));
    }
}

#[tokio::test]
async fn gauge_rejects_changed_pinned_result_index_measurements_endpoint_arrays_and_execution() {
    let f=fixture().await;let job=gauge_fixture(&f.state,&f.job,|_,_|{});let root=f.state.laboratory.directory(job.id);
    for name in ["diagnostics/result.json","diagnostics/slices/index.json","diagnostics/measurements.json","diagnostics/analytic.json",
        "diagnostics/slices/frame-00002.json","diagnostics/slices/frame-00000.npz","diagnostics/blocks/metric-00002.npz",
        "diagnostics/native/bin/gauge.z4c.00002.bin","native/bin/gauge.con.00000.bin","nr-process-receipt.json","nr-engine.json","input.athinput"] {
        let path=root.join(name);let original=std::fs::read(&path).unwrap();std::fs::write(&path,b"changed retained bytes").unwrap();
        assert!(gauge_evidence(&f.state,&job).is_err(),"Accepted corruption of {name}");
        assert!(numerical_evidence(&f.state,&job).is_err(),"Analysis accepted corruption of {name}");
        std::fs::write(path,original).unwrap();
    }
    assert!(gauge_evidence(&f.state,&job).is_ok());
}

#[tokio::test]
async fn gauge_rejects_false_analytic_success_and_execution_identity_even_when_new_hashes_match() {
    let f=fixture().await;
    for (path,pointer,value) in [
        ("analytic.json","/passed",json!(false)),("analytic.json","/check/frames/1/passed",json!(false)),
        ("sources/execution-receipt.json","/job_id",json!(Uuid::new_v4())),
        ("sources/execution-receipt.json","/termination_reason",json!("deadline")),
        ("sources/execution-receipt.json","/process_group_drained",json!(false)),
        ("result.json","/fulfillment/black_hole_collision_validated",json!(true)),
    ] {
        let job=gauge_fixture(&f.state,&f.job,|name,row|{if name==path{*row.pointer_mut(pointer).unwrap()=value.clone();}});
        assert!(gauge_evidence(&f.state,&job).is_err(),"Accepted invalid {path}{pointer}");
    }
}

#[tokio::test]
async fn gauge_rejects_off_contract_time_units_shapes_and_nonnumeric_channels() {
    let f=fixture().await;
    for (path,pointer,value) in [
        ("slices/index.json","/frames/1/time",json!(0.0)),
        ("slices/index.json","/frames/2/scientific_time",json!(100.0)),
        ("slices/index.json","/channels/Kxx/unit",json!("s^-1")),
        ("slices/index.json","/units/si_mapping",json!({"length_m":1.0})),
        ("slices/frame-00002.json","/channels/Kxx/0/0",json!("NaN")),
        ("slices/frame-00000.json","/channels/gxx/0",json!([1.0])),
        ("slices/frame-00000.json","/x",json!([0.75,0.25])),
        ("slices/frame-00002.json","/z",json!(0.625)),
    ] {
        let job=gauge_fixture(&f.state,&f.job,|name,row|{if name==path{*row.pointer_mut(pointer).unwrap()=value.clone();}});
        assert!(gauge_evidence(&f.state,&job).is_err(),"Accepted invalid {path}{pointer}");
    }
}
