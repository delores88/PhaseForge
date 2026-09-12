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
    assert!(f.state.agent.deliver_lab_response(&f.state,f.job.id,&path,&mut journal).unwrap());
    let saved=f.state.laboratory.get(f.job.id).unwrap();assert_eq!(saved.state,"paused");assert_eq!(saved.result["deliverable"]["status"],"not_fulfilled");assert!(saved.completed_at.is_none());
    assert_eq!(f.state.database.get_usage_record(row.id).unwrap().unwrap().status,"completed");
    assert!(!f.state.agent.deliver_lab_response(&f.state,f.job.id,&path,&mut journal).unwrap());
    assert_eq!(f.state.laboratory.get(f.job.id).unwrap().events.iter().filter(|e|e.kind=="deliverable_attention").count(),1);
}
