//! Durable checkpoint-selected image acquisition inside the parent's tool loop.
//! The next model turn receives actual pixels and contemporaneous numerical data.
use super::*;
use sha2::{Digest,Sha256};

pub(super) fn image_payload(state:&AppState,job:Uuid,path:&str,question:Value)->anyhow::Result<Value>{
    state.laboratory.ensure_model_study_access(job)?;
    retained_image_payload(&state.laboratory,job,path,question)
}
fn retained_image_payload(service:&crate::laboratory::LaboratoryService,job:Uuid,path:&str,question:Value)->anyhow::Result<Value>{
    let record=service.get(job)?;
    anyhow::ensure!(!matches!(record.kind.as_str(),"observation"|"illustration")||record.state=="completed","An observation or illustration must complete validation before its pixels can be supplied");
    let bytes=if record.kind=="generated"{service.read_generated_artifact(job,path)?}else{
        let path=crate::laboratory::api::published_artifact_path(service,job,path)?;anyhow::ensure!(std::fs::metadata(&path)?.len()<=9*1024*1024,"Image exceeds the 9 MiB observation budget");std::fs::read(path)?
    };
    anyhow::ensure!(bytes.len()<=9*1024*1024&&bytes.starts_with(b"\x89PNG\r\n\x1a\n"),"Observation requires an actual bounded PNG");
    let hash=format!("{:x}",Sha256::digest(&bytes));
    Ok(json!({"image_data_url":format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)),"evidence":{"job_id":job,"path":path,"sha256":hash,"request":question,"numeric_cross_check":"Compare the exact frame with recorded numerical instruments. Projection, lighting and occlusion can change appearance."}}))
}

fn row_for_image(state:&AppState,source:Uuid,image:&Value)->anyhow::Result<Value>{
    if image["measurements"].is_object(){return Ok(image["measurements"].clone());}
    let measurements=state.laboratory.read_json(source,"measurements.json")?;
    let step=image["step"].as_u64().context("Image step missing")?;
    measurements["series"].as_array().context("This engine does not expose checkpoint instruments")?.iter().find(|row|row["step"].as_u64()==Some(step)).cloned().context("The corresponding numerical sample is not committed yet")
}
fn detects(policy:&str,metric:&str,threshold:f64,row:&Value)->bool{
    match policy{"next_checkpoint"=>true,"metric_above"=>row[metric].as_f64().is_some_and(|value|value>threshold),"metric_below"=>row[metric].as_f64().is_some_and(|value|value<threshold),_=>false}
}
fn live_parent(state:&AppState,id:Uuid,token:&CancellationToken)->anyhow::Result<LabJob>{
    let parent=state.laboratory.get(id)?;
    anyhow::ensure!(parent.active()&&!token.is_cancelled()&&parent.deadline_at.is_none_or(|at|at>Utc::now()),"The observation session is paused, stopped or out of time");Ok(parent)
}
fn publish(state:&AppState,parent:Uuid,id:Uuid,result:Value,message:&str,details:Value,token:&CancellationToken)->anyhow::Result<LabJob>{
    state.laboratory.update_with_message(id,|job|{
        live_parent(state,parent,token)?;
        anyhow::ensure!(job.active(),"Observation paused or stopped before acquisition was published");
        job.state="completed".into();job.result=result;job.error=None;job.event("completed",message,details);Ok(None)
    })
}
pub(super) async fn watch(state:Arc<AppState>,session:&LabJob,id:Uuid,args:&Value,token:&CancellationToken,journal:&Journal)->anyhow::Result<Value>{
    live_parent(&state,session.id,token)?;
    let source_id=Uuid::parse_str(args["job_id"].as_str().context("A numerical source job is required")?)?;
    state.laboratory.ensure_model_study_access(source_id)?;
    let source=state.laboratory.get(source_id)?;
    anyhow::ensure!(source.project_id==session.project_id&&source.kind=="solver","Monitor a numerical solver in this project");
    let policy=args["policy"].as_str().unwrap_or("next_checkpoint");
    anyhow::ensure!(["next_checkpoint","metric_above","metric_below"].contains(&policy),"Select next_checkpoint, metric_above or metric_below");
    let metric=args["metric"].as_str().unwrap_or("");let threshold=args["threshold"].as_f64().unwrap_or(0.0);
    anyhow::ensure!(policy=="next_checkpoint"||(!metric.is_empty()&&args["threshold"].as_f64().is_some()),"Metric policy requires a numerical field name and finite threshold");
    if let Ok(existing)=state.laboratory.get(id){
        anyhow::ensure!(existing.kind=="monitor"&&existing.project_id==session.project_id&&existing.parent_id==Some(session.id)&&existing.input["request"]==*args,"Observation request ID belongs to different input");
        if existing.state=="completed"{return recovered_payload(&state,&existing);}
        anyhow::ensure!(existing.active(),"The saved observation is paused or stopped. Its artifacts remain available; recovery does not resume it automatically");
        if let Ok(result)=state.laboratory.read_json(id,"observation.json"){
            let mut acquisition=existing.clone();acquisition.result=result.clone();let payload=recovered_payload(&state,&acquisition)?;
            publish(&state,session.id,id,result,"Recovered the already acquired checkpoint image without observing a different frame.",json!({}),token)?;
            return Ok(payload);
        }
    }
    let previous=state.laboratory.list(Some(session.project_id))?.iter().filter(|job|job.kind=="monitor"&&job.parent_id==Some(session.id)&&job.input["job_id"]==json!(source_id)).filter_map(|job|job.result["source_frame"]["step"].as_i64()).max().unwrap_or(-1);
    let after=state.laboratory.get(id).ok().and_then(|job|job.input["after_step"].as_i64()).unwrap_or_else(||args["after_step"].as_i64().unwrap_or(previous).max(previous));
    let input=json!({"job_id":source_id,"policy":policy,"metric":metric,"threshold":threshold,"after_step":after,"request":args});
    let job=state.laboratory.create_monitor(id,session.id,&format!("Observe {} at a scientific checkpoint",source.title.chars().take(70).collect::<String>()),input,token)?;
    if job.state=="completed"{return recovered_payload(&state,&job);}
    anyhow::ensure!(job.active()&&!token.is_cancelled(),"The observation was interrupted; preserved acquisition can be inspected before requesting another");
    state.laboratory.update_with_message(id,|job|{live_parent(&state,session.id,token)?;anyhow::ensure!(job.active(),"Observation paused or stopped from Activity");job.state="running".into();job.event("monitor_enabled","Waiting for a committed numerical checkpoint matching this observation policy.",job.input.clone());Ok(None)})?;
    state.laboratory.event(session.id,"visual_monitoring","Visual monitoring is enabled for this tool step. Only matching saved checkpoints are inspected, within this session's deadline and usage limits.",json!({"monitor_job_id":id,"source_job_id":source_id,"policy":policy,"after_step":after,"metric":metric,"threshold":threshold}))?;
    let result=async{
        loop{
            let parent=live_parent(&state,session.id,token)?;
            anyhow::ensure!(state.laboratory.get(id)?.active(),"Observation paused or stopped from Activity");
            let current=state.laboratory.get(source_id)?;
            if steering::has_pending(&parent,journal){
                let result=json!({"detected":Value::Null,"yielded_for_user_update":true,"source_job_id":source_id,"source_state":current.state,"policy":policy,"after_step":after,"message":"The monitor wait yielded to the saved user update before acquiring another frame. The solver continues with its original inputs. This is not evidence that the monitored event did or did not occur; re-arm monitoring if the updated objective requires it."});
                let saved=publish(&state,session.id,id,result,"Observation wait yielded to a saved user update; the numerical source remains unchanged.",json!({"yielded_for_user_update":true,"source_job_id":source_id}),token)?;
                return Ok(saved.result);
            }
            if let Ok(index)=state.laboratory.read_json(source_id,"observations/index.json"){
                for image in index["images"].as_array().into_iter().flatten().filter(|image|image["step"].as_i64().is_some_and(|step|step>after)){
                    let Ok(row)=row_for_image(&state,source_id,image)else{continue;};
                    anyhow::ensure!(policy=="next_checkpoint"||row[metric].is_number(),"The selected metric is absent from this engine's checkpoint instruments: {metric}");
                    if !detects(policy,metric,threshold,&row){continue;}
                    let source_path=image["path"].as_str().context("Observation file path missing")?;
                    let payload=image_payload(&state,source_id,source_path,json!("Scheduled checkpoint observation"))?;
                    anyhow::ensure!(payload["evidence"]["sha256"]==image["sha256"],"Saved observation hash does not match its scientific index");
                    let bytes=base64::engine::general_purpose::STANDARD.decode(payload["image_data_url"].as_str().unwrap().split_once(',').unwrap().1)?;
                    std::fs::write(state.laboratory.directory(id).join("frame.png"),bytes)?;
                    let result=json!({"detected":true,"source_job_id":source_id,"source_frame":image,"numerical_instruments":row,"source_was_running":current.active(),"policy":policy,"metric":metric,"threshold":threshold,"observed_at":Utc::now(),"next":"Inspect the supplied pixels against numerical_instruments. If the view is ambiguous, request another camera or quantitative instrument; use evidence to choose a follow-up, not visual confidence alone."});
                    write_json(&state.laboratory.directory(id).join("observation.json"),&result)?;
                    let saved=publish(&state,session.id,id,result,"Checkpoint image and matching numerical measurements were acquired for the observer.",json!({"source_job_id":source_id,"step":image["step"],"source_was_running":current.active()}),token)?;
                    return recovered_payload(&state,&saved);
                }
            }
            if !current.active(){
                let result=json!({"detected":false,"source_job_id":source_id,"source_state":current.state,"policy":policy,"after_step":after,"message":"The run ended without another committed image satisfying this policy. This is not continuous observation and does not prove no unsampled event occurred."});
                let saved=publish(&state,session.id,id,result,"No further matching observation was available.",json!({"source_job_id":source_id}),token)?;return Ok(saved.result);
            }
            tokio::select!{_=token.cancelled()=>bail!("Observation cancelled"),_=tokio::time::sleep(Duration::from_millis(250))=>{}}
        }
    }.await;
    if let Err(error)=&result{let _=state.laboratory.update(id,|job|{if job.active(){job.state="failed".into();}job.error=Some(format!("{error:#}"));});}
    result
}
fn recovered_payload(state:&AppState,job:&LabJob)->anyhow::Result<Value>{
    if job.result["detected"]!=true{return Ok(job.result.clone());}
    let mut payload=image_payload(state,job.id,"frame.png",json!("Inspect the scheduled frame and cross-check its retained instruments"))?;
    anyhow::ensure!(job.result["source_frame"]["sha256"]==payload["evidence"]["sha256"],"Recovered observation pixels no longer match their recorded numerical frame");
    payload["observation"]=job.result.clone();Ok(payload)
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn native_pixels_require_completed_illustration_or_observation_for_every_case_alias(){
        let dir=tempfile::tempdir().unwrap();let config=crate::config::AppConfig{data_directory:dir.path().into(),..Default::default()};
        let database=crate::persistence::Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Native image publication".into()),question:"test".into()});database.put_project(&project).unwrap();
        let service=crate::laboratory::LaboratoryService::new(database,config).unwrap();
        let bytes=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jS1sAAAAASUVORK5CYII=").unwrap();
        for (kind,path) in [("illustration","RENDER.PNG"),("observation","FIRST-FRAME.PNG")]{
            let job=service.create(Uuid::new_v4(),project.id,None,kind,"publication",json!({}),None).unwrap();
            std::fs::write(service.directory(job.id).join(path),&bytes).unwrap();
            // Even an unlisted image name cannot expose unvalidated render pixels.
            std::fs::write(service.directory(job.id).join("other-view.png"),&bytes).unwrap();
            for state in ["queued","running","paused","failed","timed_out","cancelled"]{
                service.update(job.id,|job|job.state=state.into()).unwrap();
                for alias in [path,"other-view.png"]{assert!(retained_image_payload(&service,job.id,alias,json!({})).unwrap_err().to_string().contains("complete validation"));}
            }
            service.update(job.id,|job|job.state="completed".into()).unwrap();
            let payload=retained_image_payload(&service,job.id,path,json!({"question":"Inspect retained bytes"})).unwrap();
            assert_eq!(payload["evidence"]["sha256"],format!("{:x}",Sha256::digest(&bytes)));
            assert_eq!(payload["image_data_url"],format!("data:image/png;base64,{}",base64::engine::general_purpose::STANDARD.encode(&bytes)));
        }
    }
    #[test]fn detector_uses_numeric_thresholds_and_missing_values_are_not_events(){
        let row=json!({"temperature_kelvin":120.0,"msd_nm2":0.25});
        assert!(detects("next_checkpoint","",0.0,&row));assert!(detects("metric_above","msd_nm2",0.2,&row));assert!(!detects("metric_above","msd_nm2",0.3,&row));assert!(!detects("metric_below","missing",100.0,&row));assert!(detects("metric_below","temperature_kelvin",125.0,&row));
    }
}
