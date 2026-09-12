//! Read-only observation of saved native fields, owned by the same NR job.
use std::{path::{Path, PathBuf}, time::Duration};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;
use super::{numerical_relativity::{bounded_file, hash}, process::OwnedProcess};

pub const MEMORY_MIB: u64 = 512;
pub struct LiveGuard { child: OwnedProcess, directory: PathBuf, stop_file: PathBuf }

fn sources() -> [(&'static str, &'static [u8]); 3] {
    [("nr_live_guard.py",include_bytes!("../../../tools/nr_live_guard.py")),
     ("nr_black_hole_result.py",include_bytes!("../../../tools/nr_black_hole_result.py")),
     ("athenak_decode.py",include_bytes!("../../../tools/athenak_decode.py"))]
}

impl LiveGuard {
    pub async fn start(python:&Path, job:&Path, native_work:&Path, token:&CancellationToken)->anyhow::Result<Self>{
        ensure!(!token.is_cancelled(),"NR request stopped before monitor startup");
        let worker=job.join("nr-monitor-tools");std::fs::create_dir_all(&worker)?;
        for(name,bytes)in sources(){super::retain_worker_source(&worker.join(name),bytes)?;}
        let directory=job.join("nr-monitor");let stop_file=job.join("nr-monitor.stop");
        ensure!(!directory.exists()&&!stop_file.exists(),"NR monitor attempts cannot reuse earlier output or stop markers");
        let mut command=super::process::clean_command(python,job);
        command.args(["-I","-B","-c","import runpy,sys; sys.path.insert(0,sys.argv.pop(1)); sys.argv=sys.argv[1:]; runpy.run_path(sys.argv[0],run_name='__main__')"])
            .arg(&worker).arg(worker.join("nr_live_guard.py")).arg("--run").arg(native_work)
            .arg("--output").arg(&directory).arg("--stop-file").arg(&stop_file)
            .stdout(std::fs::File::create(job.join("nr-monitor.log"))?)
            .stderr(std::fs::File::create(job.join("nr-monitor-stderr.log"))?);
        let mut guard=Self{child:OwnedProcess::spawn(&mut command,MEMORY_MIB as usize)?,directory,stop_file};
        let readiness=async {
            loop {
                ensure!(!token.is_cancelled(),"NR request stopped during monitor startup");
                if let Some(state)=guard.state()? {
                    ensure!(state["exit_code"].is_null()&&matches!(state["status"].as_str(),Some("waiting"|"monitoring")),"Live numerical monitor rejected startup: {}",state["reason"]);
                    return Ok::<(),anyhow::Error>(());
                }
                tokio::select!{
                    status=guard.child.wait(token)=>anyhow::bail!("Live numerical monitor stopped before readiness: {:?}",status?),
                    _=tokio::time::sleep(Duration::from_millis(100))=>{}
                }
            }
        };
        tokio::time::timeout(Duration::from_secs(5),readiness).await.context("Live numerical monitor did not become ready")??;
        Ok(guard)
    }
    pub fn id(&self)->u32{self.child.id()}
    pub fn state(&self)->anyhow::Result<Option<Value>>{
        let path=self.directory.join("state.json");if !path.exists(){return Ok(None);}
        let state:Value=serde_json::from_slice(&bounded_file(&path,64*1024)?)?;
        validate_state(&state)?;Ok(Some(state))
    }
    pub async fn wait(&mut self,token:&CancellationToken)->anyhow::Result<std::process::ExitStatus>{self.child.wait(token).await}
    // The solver is already stopped here. This bounded final scan never grants
    // another solver deadline or starts new scientific evolution.
    pub async fn finish(mut self)->Value{
        let outcome=async{
            std::fs::write(&self.stop_file,b"caller stopped numerical worker\n")?;
            let cleanup=CancellationToken::new();
            let status=tokio::time::timeout(Duration::from_secs(20),self.child.wait(&cleanup)).await.context("Numerical monitor final scan exceeded its cleanup budget")??;
            let bytes=bounded_file(&self.directory.join("receipt.json"),64*1024)?;
            let receipt:Value=serde_json::from_slice(&bytes)?;validate_state(&receipt)?;
            ensure!(receipt["exit_code"]==json!(status.code()),"Numerical monitor receipt and process exit disagree");
            let observations=bounded_file(&self.directory.join("observations.jsonl"),8*1024*1024)?;
            ensure!(receipt["observations"]["path"]=="observations.jsonl"&&receipt["observations"]["sha256"]==hash(&observations)&&receipt["observations"]["bytes"]==observations.len(),"Numerical monitor observation pin differs");
            let passed=status.success()&&receipt["status"]=="stopped"&&receipt["reason"]=="caller_stopped"&&receipt["observed_pair_count"].as_u64().is_some_and(|v|v>0);
            Ok::<Value,anyhow::Error>(json!({"path":"nr-monitor/receipt.json","sha256":hash(&bytes),"passed":passed,"status":receipt["status"],"reason":receipt["reason"],"observed_pair_count":receipt["observed_pair_count"],"last_time":receipt["last_time"],"last_cycle":receipt["last_cycle"],"latest_observation":receipt["latest_observation"],"guard_thresholds":receipt["guard_thresholds"],"accuracy_validated":false}))
        }.await;
        outcome.unwrap_or_else(|error|json!({"passed":false,"status":"failed","reason":"monitor_receipt_unavailable_or_invalid","detail":format!("{error:#}"),"accuracy_validated":false}))
    }
}

fn validate_state(state:&Value)->anyhow::Result<()>{
    ensure!(state["schema"]=="phaseforge.nr-live-guard.v1"&&state["accuracy_validated"]==false,"Unexpected numerical monitor contract");
    for(name,bytes)in sources(){ensure!(state["source_sha256"][name]==hash(bytes),"Numerical monitor source {name} differs from this application");}
    let thresholds=&state["guard_thresholds"];
    ensure!(thresholds["expected_blocks"]==624&&thresholds["static_mesh"]==true&&thresholds["excise_chi"]==0.0625&&thresholds["growth_factor"]==100.0&&thresholds["initial_rms_floor"]==1e-10&&thresholds["poll_interval_seconds"]==1.0,"Numerical monitor changed the admitted guard thresholds");
    Ok(())
}
