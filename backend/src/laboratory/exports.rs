use std::{sync::Arc,time::Duration};
use anyhow::Context;
use axum::{Router,Json,extract::{State,Path},routing::{get,post}};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use uuid::Uuid;
use crate::app::AppState;
use super::{LabJob,LaboratoryService,write_json,process,api::Error};

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ExportRequest {
    pub width:u32,pub height:u32,pub fps:u32,pub playback_duration_seconds:f64,
    pub start_time:f64,pub end_time:f64,
    #[serde(default)]pub presentation:Value,
    #[serde(default="yes")]pub labels:bool,
    #[serde(default="renderer")]pub renderer:String,
    #[serde(default="samples")]pub samples:u32,
    #[serde(default)]pub time_limit_seconds:Option<u64>,
}

#[cfg(test)]
mod tests{
    use super::*;
    fn request()->ExportRequest{ExportRequest{width:1280,height:720,fps:30,playback_duration_seconds:0.15,start_time:0.0,end_time:10.0,presentation:json!({"color":"#589fff"}),labels:true,renderer:"eevee".into(),samples:64,time_limit_seconds:None}}
    fn fixture()->(tempfile::TempDir,LaboratoryService,LabJob){
        let directory=tempfile::tempdir().unwrap();
        let config=crate::config::AppConfig{data_directory:directory.path().into(),..Default::default()};
        let database=crate::persistence::Database::open(&config.database_path()).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("Export contract".into()),question:"test".into()});database.put_project(&project).unwrap();
        let service=LaboratoryService::new(database,config).unwrap();
        let source=service.create(Uuid::new_v4(),project.id,None,"solver","Recorded argon",json!({}),None).unwrap();
        std::fs::create_dir(service.directory(source.id).join("trajectory")).unwrap();
        write_json(&service.directory(source.id).join("trajectory/index.json"),&json!({"start_time":0.0,"end_time":10.0})).unwrap();
        let export=service.create(Uuid::new_v4(),project.id,Some(source.id),"export","Argon video",json!({"source_job_id":source.id,"settings":request()}),None).unwrap();
        (directory,service,export)
    }
    #[test] fn frame_count_and_roundoff_are_canonical(){
        let endpoint=9.999999999999897;
        let (canonical,frames)=request().normalized(&json!({"start_time":0.0,"end_time":endpoint})).unwrap();
        assert_eq!(frames,5);assert_eq!(canonical.end_time,endpoint);assert_eq!(worker_settings(&canonical,frames).unwrap()["frame_count"],5);
        let mut invalid=request();invalid.end_time=10.00000001;assert!(invalid.normalized(&json!({"start_time":0.0,"end_time":endpoint})).is_err());
        // Tolerance scales with the actual units; it must not swallow a large
        // relative error when scientific times happen to be extremely small.
        assert!(retained_time(1.001e-20,0.0,1e-20).is_err());
        assert_eq!(retained_time(1e-20+1e-35,0.0,1e-20).unwrap(),1e-20);
    }
    #[test] fn sampler_and_video_limits_match_the_worker(){
        let index=json!({"start_time":0.0,"end_time":10.0});
        for (engine,low,high) in [("eevee",8,128),("cycles",8,512)]{
            for (samples,valid) in [(low-1,false),(low,true),(high,true),(high+1,false)]{let mut r=request();r.renderer=engine.into();r.samples=samples;assert_eq!(r.normalized(&index).is_ok(),valid,"{engine} {samples}");}
        }
        let mut r=request();r.width=318;assert!(r.normalized(&index).is_err());r.width=320;r.height=180;assert!(r.normalized(&index).is_ok());
        r.fps=1;r.playback_duration_seconds=0.1;assert!(r.normalized(&index).is_err());
        r.fps=60;r.playback_duration_seconds=3600.0;assert_eq!(r.normalized(&index).unwrap().1,216000);r.playback_duration_seconds=3601.0;assert!(r.normalized(&index).is_err());
        r=request();r.start_time=f64::NAN;assert!(r.normalized(&index).is_err());
    }
    #[test] fn field_exports_use_recorded_field_endpoints_not_particle_artifacts(){
        let (_directory,service,export)=fixture();let source=export.parent_id.unwrap();
        service.update(source,|job|job.input=json!({"engine":"diffusion_2d"})).unwrap();
        std::fs::create_dir(service.directory(source).join("fields")).unwrap();
        write_json(&service.directory(source).join("fields/index.json"),&json!({"representation":"scalar_field","start_time":-1.0,"end_time":99.0,"frames":[{"time":0.0},{"time":2.56}]})).unwrap();
        let index=source_index(&service,source).unwrap();assert_eq!(index["start_time"],0.0);assert_eq!(index["end_time"],2.56);
        let mut request=request();request.end_time=2.56;assert!(request.normalized(&index).is_ok());request.end_time=10.0;assert!(request.normalized(&index).is_err());
    }
    #[test] fn cancelled_result_is_never_published_or_downloadable(){
        let (_directory,_service,mut job)=fixture();let token=tokio_util::sync::CancellationToken::new();let source=job.parent_id.unwrap();
        let result=json!({"filename":"simulation.mp4"});
        for state in ["paused","cancelled","timed_out","failed","queued"]{job.state=state.into();assert!(!finish_export(&mut job,&token,result.clone(),source));assert_eq!(job.state,state);assert!(ensure_artifact_available(&job,"simulation.mp4").is_err());}
        job.state="running".into();token.cancel();assert!(!finish_export(&mut job,&token,result.clone(),source));
        assert!(ensure_artifact_available(&job,"encoding0001-0005.MP4").is_err());assert!(ensure_artifact_available(&job,"first-frame.png").is_ok());
        let token=tokio_util::sync::CancellationToken::new();assert!(finish_export(&mut job,&token,result,source));assert!(ensure_artifact_available(&job,"simulation.mp4").is_ok());assert!(ensure_artifact_available(&job,"encoding.mp4").is_err());
        assert!(export_status(&job)["download_url"].as_str().is_some());job.state="cancelled".into();assert!(export_status(&job)["download_url"].is_null());
    }
    #[tokio::test] async fn resume_preserves_partial_attempt_and_reuses_a_single_replacement(){
        let (_directory,service,job)=fixture();
        // Keep the renderer in the queue; this test must never launch Blender.
        let permit=service.render_slots.acquire().await.unwrap();
        service.update(job.id,|j|j.state="paused".into()).unwrap();
        let partial=service.directory(job.id).join("encoding.mp4");std::fs::write(&partial,b"retained incomplete bytes").unwrap();
        let deadline=Some(chrono::Utc::now()+chrono::Duration::seconds(30));
        let replacement=service.resume_export(job.id,deadline).unwrap();
        assert_ne!(replacement.id,job.id);assert_eq!(replacement.parent_id,job.parent_id);assert_eq!(replacement.deadline_at,deadline);
        assert_eq!(replacement.input["restarted_from_export_id"],job.id.to_string());assert_eq!(replacement.input["settings"]["frame_count"],5);assert_eq!(replacement.input["settings"]["presentation"],job.input["settings"]["presentation"]);
        assert_eq!(std::fs::read(&partial).unwrap(),b"retained incomplete bytes");assert_eq!(service.get(job.id).unwrap().state,"paused");
        let again=service.resume_export(job.id,deadline).unwrap();assert_eq!(again.id,replacement.id);assert_eq!(service.list(None).unwrap().len(),3);
        service.stop(replacement.id,"cancelled").unwrap();drop(permit);
        for _ in 0..30{if !service.active.lock().contains_key(&replacement.id){break;}tokio::time::sleep(Duration::from_millis(10)).await;}
        assert_eq!(service.get(replacement.id).unwrap().state,"cancelled");assert!(!service.directory(replacement.id).join("simulation.mp4").exists());
    }
}
fn yes()->bool{true} fn renderer()->String{"eevee".into()} fn samples()->u32{64}
impl ExportRequest{
    pub fn normalized(&self,index:&Value)->anyhow::Result<(Self,u64)>{
        anyhow::ensure!((320..=3840).contains(&self.width)&&(180..=2160).contains(&self.height)&&self.width%2==0&&self.height%2==0,"Choose even dimensions within 320–3840 × 180–2160");
        anyhow::ensure!((1..=60).contains(&self.fps)&&self.playback_duration_seconds.is_finite()&&self.playback_duration_seconds>=0.1&&self.playback_duration_seconds<=86400.0,"Frame rate must be 1–60 and playback duration 0.1–86,400 seconds");
        let frames=(self.playback_duration_seconds*self.fps as f64).round() as u64;
        anyhow::ensure!((2..=216000).contains(&frames),"Export requires 2–216,000 frames; select a time window for larger movies");
        anyhow::ensure!(self.start_time.is_finite()&&self.end_time.is_finite()&&self.end_time>self.start_time,"Select an increasing scientific time range");
        let minimum=index["start_time"].as_f64().context("Trajectory start missing")?;
        let maximum=index["end_time"].as_f64().context("Trajectory end missing")?;
        let mut normalized=self.clone();
        normalized.start_time=retained_time(self.start_time,minimum,maximum)?;
        normalized.end_time=retained_time(self.end_time,minimum,maximum)?;
        anyhow::ensure!(normalized.end_time>normalized.start_time,"Select an increasing scientific time range");
        match self.renderer.as_str(){
            "eevee"=>anyhow::ensure!((8..=128).contains(&self.samples),"Eevee requires 8–128 samples"),
            "cycles"=>anyhow::ensure!((8..=512).contains(&self.samples),"Cycles requires 8–512 samples"),
            _=>anyhow::bail!("Choose the Eevee or Cycles renderer"),
        }
        anyhow::ensure!(self.presentation.is_null()||self.presentation.is_object(),"Presentation must be an object");
        Ok((normalized,frames))
    }
}
fn retained_time(value:f64,minimum:f64,maximum:f64)->anyhow::Result<f64>{
    anyhow::ensure!(value.is_finite()&&minimum.is_finite()&&maximum.is_finite()&&maximum>minimum,"The retained trajectory has no increasing time range");
    // Permit only accumulated binary roundoff, e.g. a nominal 10 ps ending at
    // 9.999999999999897 ps. The canonical endpoint is always the retained value.
    let tolerance=64.0*f64::EPSILON*minimum.abs().max(maximum.abs()).max((maximum-minimum).abs()).max(f64::MIN_POSITIVE);
    anyhow::ensure!(value>=minimum-tolerance&&value<=maximum+tolerance,"Requested scientific time range is outside the retained trajectory");
    Ok(value.clamp(minimum,maximum))
}
fn worker_settings(request:&ExportRequest,frames:u64)->anyhow::Result<Value>{
    let mut settings=serde_json::to_value(request)?;
    settings["frame_count"]=json!(frames);
    Ok(settings)
}
pub fn routes()->Router<Arc<AppState>>{
    Router::new().route("/api/laboratory/jobs/:id/exports/estimate",post(estimate))
        .route("/api/laboratory/jobs/:id/exports",post(create))
        .route("/api/laboratory/exports/:id",get(status))
        .route("/api/laboratory/exports/:id/cancel",post(cancel))
}
fn source_index(service:&LaboratoryService,id:Uuid)->anyhow::Result<Value>{
    let job=service.get(id)?;
    anyhow::ensure!(job.kind=="solver","Only recorded numerical results can be exported");
    let mut index=service.read_json(id,if job.input["engine"]=="diffusion_2d"{"fields/index.json"}else{"trajectory/index.json"})?;
    if index["representation"]=="scalar_field" {
        let frames=index["frames"].as_array().context("Recorded field states missing")?;
        let start=frames.first().and_then(|frame|frame["time"].as_f64()).context("Recorded field start missing")?;
        let end=frames.last().and_then(|frame|frame["time"].as_f64()).context("Recorded field end missing")?;
        index["start_time"]=json!(start);index["end_time"]=json!(end);
    }
    Ok(index)
}
fn estimate_value(state:&AppState,id:Uuid,request:&ExportRequest)->anyhow::Result<Value>{
    let job=state.laboratory.get(id)?;anyhow::ensure!(job.kind=="solver","Only numerical trajectories can be exported");
    let index=source_index(&state.laboratory,id)?;let (request,frames)=request.normalized(&index)?;
    let blender=crate::studio::render::find_blender();
    let pixels=request.width as f64*request.height as f64;
    let warmup=16.0;
    let seconds=warmup+1.5+frames as f64*(0.12+0.18*pixels/(1920.0*1080.0))*(request.samples as f64/64.0).max(0.25)*if request.renderer=="cycles"{8.0}else{1.0};
    Ok(json!({"supported":blender.is_some(),"reason":if blender.is_none(){Some("Blender is unavailable; install the supported Blender runtime in Scientific Studio")}else{None},
        "estimated_seconds":seconds,"estimated_warmup_seconds":warmup,"includes_cold_start":true,"frame_count":frames,
        "estimate_basis":if job.input["engine"]=="diffusion_2d"{"Includes cold startup. Field export cost uses the existing renderer throughput approximation; this field shape and hardware need fresh calibration. Queue wait is additional. Exact saved cells use a fixed color range; no solver reruns."}else if job.input["engine"]=="newtonian_nbody"{"Includes cold startup and approximate renderer throughput. This isolated-body count, camera and hardware need calibration for a reliable estimate; queue wait is additional. Rendering uses saved L0/T0 coordinates without rerunning forces."}else{"Includes a 16-second cold-start allowance plus launch and rendering. Measured on this development machine (RTX 4090 Laptop), 108 particles, Eevee 64 samples: 0.21s/frame at 720p and 0.30s/frame at 1080p. Scaling is approximate; scene complexity, Cycles and other systems need fresh calibration. Queue wait is additional. Scientific accuracy is unchanged."},
        "scientific_time_range":[request.start_time,request.end_time],"movie_duration_seconds":frames as f64/request.fps as f64}))
}
async fn estimate(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<ExportRequest>)->Result<Json<Value>,Error>{
    Ok(Json(estimate_value(&state,id,&request)?))
}
async fn create(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<ExportRequest>)->Result<Json<Value>,Error>{
    let estimate=estimate_value(&state,id,&request)?;
    if estimate["supported"]!=true{return Err(anyhow::anyhow!("Blender is unavailable").into());}
    let source=state.laboratory.get(id)?;
    let (request,frames)=request.normalized(&source_index(&state.laboratory,id)?)?;
    let settings=worker_settings(&request,frames)?;
    let job=state.laboratory.create(Uuid::new_v4(),source.project_id,Some(id),"export",&format!("Video · {}",source.title),json!({"source_job_id":id,"settings":settings,"estimate":estimate}),request.time_limit_seconds.map(|s|chrono::Utc::now()+chrono::Duration::seconds(s.min(604800) as i64)))?;
    state.laboratory.start_export(job.id)?;
    Ok(Json(export_status(&job)))
}
fn export_status(job:&LabJob)->Value{
    let id=job.id;
    json!({"id":id,"kind":job.kind,"project_id":job.project_id,"parent_id":job.parent_id,"created_at":job.created_at,"state":job.state,"progress":job.progress,"error":job.error,"result":job.result,
        "status_url":format!("/api/laboratory/exports/{id}"),"cancel_url":format!("/api/laboratory/exports/{id}/cancel"),
        "download_url":if downloadable(job){Some(format!("/api/laboratory/jobs/{id}/artifacts/simulation.mp4"))}else{None}})
}
fn downloadable(job:&LabJob)->bool{job.kind=="export"&&job.state=="completed"&&job.result["filename"]=="simulation.mp4"}
pub(super) fn ensure_artifact_available(job:&LabJob,relative:&str)->anyhow::Result<()>{
    if job.kind=="export"&&std::path::Path::new(relative).extension().and_then(|extension|extension.to_str()).is_some_and(|extension|extension.eq_ignore_ascii_case("mp4")){
        anyhow::ensure!(downloadable(job)&&relative=="simulation.mp4","The video is available only after this export completes successfully");
    }
    Ok(())
}
async fn status(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{let job=state.laboratory.get(id)?;if job.kind!="export"{return Err(anyhow::anyhow!("This job is not a video export").into());}Ok(Json(export_status(&job)))}
async fn cancel(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error>{if state.laboratory.get(id)?.kind!="export"{return Err(anyhow::anyhow!("This job is not a video export").into());}Ok(Json(export_status(&state.laboratory.stop(id,"cancelled")?)))}
fn finish_export(job:&mut LabJob,token:&tokio_util::sync::CancellationToken,result:Value,source:Uuid)->bool{
    if job.state!="running"||token.is_cancelled(){return false;}
    job.state="completed".into();job.result=result;job.progress=json!({"fraction":1.0});
    job.event("completed","Your simulation video is ready to save.",json!({"source_job_id":source}));true
}
impl LaboratoryService{
    pub fn resume_export(&self,id:Uuid,deadline:Option<chrono::DateTime<chrono::Utc>>)->anyhow::Result<LabJob>{
        let original=self.get(id)?;
        anyhow::ensure!(original.kind=="export"&&matches!(original.state.as_str(),"paused"|"failed"|"timed_out"),"Export is not resumable");
        // Reserve the old attempt until its replacement is saved. Concurrent clicks
        // cannot race a worker that is still stopping or create duplicate renders.
        let _reservation=self.acquire(id).context("The export worker is still stopping; retry after it exits")?;
        let outcome=(||{
            let original=self.get(id)?;
            if let Some(restarted)=original.events.iter().rev().find(|event|event.kind=="render_restart_requested"){
                let next=Uuid::parse_str(restarted.data["new_job_id"].as_str().context("Export restart receipt is incomplete")?)?;
                return self.get(next);
            }
            let source=Uuid::parse_str(original.input["source_job_id"].as_str().context("Export source missing")?)?;
            let request:ExportRequest=serde_json::from_value(original.input["settings"].clone())?;
            let (request,frames)=request.normalized(&source_index(self,source)?)?;
            let mut input=original.input.clone();input["settings"]=worker_settings(&request,frames)?;input["restarted_from_export_id"]=json!(id);
            let replacement=self.create(Uuid::new_v4(),original.project_id,Some(source),"export",&original.title,input,deadline)?;
            self.event(id,"render_restart_requested","A separate render attempt will reuse the saved numerical trajectory. This interrupted attempt and its files are preserved.",json!({"new_job_id":replacement.id,"strategy":"restart_render_from_saved_trajectory"}))?;
            self.start_export(replacement.id)?;Ok(replacement)
        })();
        self.release(id);outcome
    }
    pub fn start_export(&self,id:Uuid)->anyhow::Result<()>{
        let token=self.acquire(id)?;let service=self.clone();
        tokio::spawn(async move{
            let job=match service.get(id){Ok(job)=>job,Err(_)=>{service.release(id);return;}};
            let expiry_service=service.clone();let expiry=job.deadline_at.map(|at|tokio::spawn(async move{tokio::time::sleep((at-chrono::Utc::now()).to_std().unwrap_or_default()).await;let _=expiry_service.stop(id,"timed_out");}));
            let result=service.run_export(id,&token).await;
            if let Some(task)=expiry{task.abort();}
            if let Err(error)=result{let _=service.update(id,|j|{if j.active(){j.state=if token.is_cancelled(){"cancelled"}else{"failed"}.into();}j.error=Some(format!("{error:#}"));j.event("render_stopped",format!("{error:#}"),json!({}));});}
            service.release(id);
        });Ok(())
    }
    async fn run_export(&self,id:Uuid,token:&tokio_util::sync::CancellationToken)->anyhow::Result<()>{
        let _slot=tokio::select!{_=token.cancelled()=>anyhow::bail!("Export cancelled in queue"),slot=self.render_slots.acquire()=>slot?};
        let job=self.get(id)?;let source=Uuid::parse_str(job.input["source_job_id"].as_str().context("Export source missing")?)?;
        let directory=self.directory(id);let field=self.get(source)?.input["engine"]=="diffusion_2d";
        std::fs::write(directory.join("trajectory_render.py"),include_str!("../../../tools/trajectory_render.py"))?;
        let worker=if field{let worker=directory.join("field_render.py");std::fs::write(&worker,include_str!("../../../tools/field_render.py"))?;worker}else{directory.join("trajectory_render.py")};
        anyhow::ensure!(job.kind=="export"&&job.state=="queued"&&!token.is_cancelled(),"Export stopped before renderer launch");
        let request:ExportRequest=serde_json::from_value(job.input["settings"].clone())?;
        let (request,frames)=request.normalized(&source_index(self,source)?)?;
        let mut input=worker_settings(&request,frames)?;input["source_directory"]=json!(self.directory(source));
        write_json(&directory.join("render-input.json"),&input)?;
        let blender=crate::studio::render::find_blender().context("Blender runtime is unavailable")?;
        let mut command=process::clean_command(&blender,&directory);
        command.args(["--background","--factory-startup","--disable-autoexec","--python-exit-code","1","--python"]).arg(worker).arg("--").arg("--input").arg(directory.join("render-input.json")).arg("--output").arg(&directory)
            .stdout(std::fs::File::create(directory.join("stdout.log"))?).stderr(std::fs::File::create(directory.join("stderr.log"))?);
        let started=self.update(id,|j|{if j.state=="queued"&&!token.is_cancelled(){j.state="running".into();j.event("render_started","Blender is rendering retained numerical frames. The solver is not being rerun.",json!({"source_job_id":source,"frame_count":frames}));}})?;
        anyhow::ensure!(started.state=="running"&&!token.is_cancelled(),"Export stopped before renderer launch");
        let mut child=process::OwnedProcess::spawn(&mut command,8192)?;
        let progress_service=self.clone();let progress_token=token.clone();
        let progress=tokio::spawn(async move{loop{tokio::select!{_=progress_token.cancelled()=>break,_=tokio::time::sleep(Duration::from_millis(500))=>{}}if let Ok(value)=progress_service.read_json(id,"progress.json"){let _=progress_service.update(id,|j|{if j.active(){j.progress=value;}});}}});
        let status=child.wait_cooperative(token,&directory.join("cancel.request")).await;progress.abort();let status=status?;
        anyhow::ensure!(status.success(),"Blender export failed ({status}); inspect retained stderr.log");
        let result=self.read_json(id,"result.json")?;
        anyhow::ensure!(self.directory(id).join("simulation.mp4").is_file(),"The renderer did not produce the requested video");
        anyhow::ensure!(result["filename"]=="simulation.mp4"&&result["frame_count"].as_u64()==Some(frames)&&result["width"].as_u64()==Some(request.width as u64)&&result["height"].as_u64()==Some(request.height as u64)&&result["fps"].as_u64()==Some(request.fps as u64),"Rendered video does not match the validated export settings");
        let completed=self.update(id,|j|{finish_export(j,token,result,source);})?;
        anyhow::ensure!(completed.state=="completed","Export stopped before its result was published");Ok(())
    }
}
