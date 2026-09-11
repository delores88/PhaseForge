//! Trusted data-only PNG plotting. Generated code never gains access to the
//! host scientific environment merely to obtain image libraries.
use super::{generated::SourceImport,ml_study::sha,process,write_json,LaboratoryService};
use anyhow::{ensure,Context};
use serde_json::json;
use std::fs;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub(super) fn source_hash()->String{sha(include_bytes!("../../../tools/ml_plot_worker.py"))}
impl LaboratoryService{
    pub(super) fn start_ml_plot(&self,id:Uuid)->anyhow::Result<()>{
        let job=self.get(id)?;ensure!(job.kind=="study_plot"&&job.state=="queued"&&job.input["worker_sha256"]==source_hash(),"Plot identity or shipped renderer changed");
        let parent=job.parent_id.context("A plot needs its registered coordinator")?;
        let token=self.acquire_child(id,&self.execution_token(parent).context("Plot coordinator is not executing")?)?;
        let service=self.clone();tokio::spawn(async move{
            let outcome=service.execute_ml_plot(id,&token).await;
            if let Err(error)=outcome{let _=service.update(id,|job|{if job.active(){job.state=if token.is_cancelled(){"cancelled"}else{"failed"}.into();}job.error=Some(format!("{error:#}"));job.event("plot_stopped","The data plot did not complete; its input pins and logs are retained.",json!({"error":format!("{error:#}")}));});}
            service.release(id);
        });Ok(())
    }
    async fn execute_ml_plot(&self,id:Uuid,token:&CancellationToken)->anyhow::Result<()>{
        let job=self.get(id)?;let directory=self.directory(id);
        let sources:Vec<SourceImport>=serde_json::from_value(job.input["sources"].clone())?;
        ensure!(sources.len()==3,"The study plot requires exactly evaluation, split and OOD data");
        let imports=directory.join("imports");fs::create_dir(&imports)?;
        let mut total=0usize;let mut config=job.input["config"].clone();let mut pins=vec![];
        for source in sources{
            self.ensure_study_import_access(&job,source.job_id)?;
            super::safe_relative(&source.destination)?;
            ensure!(!source.destination.contains(['/', '\\'])&&source.sha256.is_some(),"Plot imports are explicitly pinned single files");
            let origin=self.get(source.job_id)?;ensure!(origin.project_id==job.project_id&&origin.state=="completed"&&!self.executing(origin.id),"Plot source is not a completed project artifact");
            let bytes=if origin.kind=="generated"{self.read_generated_artifact(origin.id,&source.path)?}else{fs::read(self.path(origin.id,&source.path)?)?};
            total+=bytes.len();ensure!(total<=16*1024*1024&&Some(sha(&bytes))==source.sha256,"Plot input exceeded the bound or changed bytes");
            fs::write(imports.join(&source.destination),&bytes)?;
            pins.push(json!({"job_id":source.job_id,"path":source.path,"sha256":source.sha256,"destination":source.destination,"bytes":bytes.len()}));
        }
        for key in ["evaluation","split","ood"]{let name=config[key]["path"].as_str().context("Missing plot source descriptor")?;super::safe_relative(name)?;ensure!(!name.contains(['/', '\\']),"Plot source must be a named imported file");config[key]["path"]=json!(format!("imports/{name}"));}
        write_json(&directory.join("plot-input.json"),&config)?;
        write_json(&directory.join("source-pins.json"),&pins)?;
        let worker=directory.join("ml_plot_worker.py");fs::write(&worker,include_bytes!("../../../tools/ml_plot_worker.py"))?;
        self.update(id,|job|{if job.active(){job.state="provisioning".into();job.event("plot_environment","Checking the pinned Pillow environment for the data-only plotter.",json!({"worker_sha256":source_hash()}));}})?;
        let python=self.ensure_environment(id,token).await?;
        ensure!(!token.is_cancelled()&&self.get(id)?.active(),"Plot stopped before rendering");
        let output=directory.join("plot");fs::create_dir(&output)?;
        let mut command=process::clean_command(&python,&directory);
        command.args(["-I", "-B"]).arg(&worker).arg("--input").arg(directory.join("plot-input.json")).arg("--output").arg(&output).stdout(fs::File::create(directory.join("stdout.log"))?).stderr(fs::File::create(directory.join("stderr.log"))?);
        self.update(id,|job|{if job.active(){job.state="running".into();}})?;
        let mut child=process::OwnedProcess::spawn(&mut command,512)?;
        self.event(id,"plot_started","Rendering measured values, uncertainty and held-out predictions from pinned data.",json!({"pid":child.id(),"scientific_rerun":false}))?;
        let status=child.wait(token).await?;ensure!(status.success(),"Data plot worker failed: {}",fs::read_to_string(directory.join("stderr.log")).unwrap_or_default().chars().take(4000).collect::<String>());
        ensure!(!token.is_cancelled(),"Plot stopped before publication");
        let manifest=self.read_json(id,"plot/plot.json")?;let png=fs::read(self.path(id,"plot/plot.png")?)?;
        ensure!(png.len()<=16*1024*1024&&png.starts_with(b"\x89PNG\r\n\x1a\n"),"Plotter did not produce a bounded actual PNG");
        let result=json!({"status":"completed","renderer":"trusted_data_only_pillow","worker_sha256":source_hash(),"png_sha256":sha(&png),"png":"plot/plot.png","data":"plot/plot.json","source_pins":pins,"scientific_rerun":false,"plot":manifest});
        write_json(&directory.join("result.json"),&result)?;
        self.update(id,|job|{if job.active()&&!token.is_cancelled(){job.state="completed".into();job.result=result;job.event("completed","Measured and predicted values are plotted with their original source hashes and explicit uncertainty labels.",json!({"png":"plot/plot.png"}));}})?;Ok(())
    }
}
