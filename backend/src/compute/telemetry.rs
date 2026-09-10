//! Read-only, bounded OS telemetry. No metric is manufactured when a driver cannot report it.
use std::{collections::VecDeque, sync::Arc, time::Duration};
use parking_lot::RwLock;
use serde_json::{json, Value};
use sysinfo::{Pid, System};

#[derive(Clone)]
pub struct Telemetry { inner: Arc<RwLock<Value>> }
impl Telemetry {
    pub fn start() -> Self {
        let inner = Arc::new(RwLock::new(json!({"status":"warming_up","sampled_at":null,"history":[],"gpus":[]})));
        let target = inner.clone();
        tokio::spawn(async move {
            // Retain System across samples: CPU usage is a delta, not a one-shot reading.
            let mut sys=System::new();
            sys.refresh_cpu(); sys.refresh_memory();
            let pid=Pid::from_u32(std::process::id());
            sys.refresh_process(pid);
            let mut history=VecDeque::<Value>::new();
            // Driver probing is independent: a slow GPU utility must not freeze CPU/RAM updates.
            let gpu_state=Arc::new(RwLock::new((Vec::<Value>::new(),String::from("Waiting for the first GPU driver sample"),None::<chrono::DateTime<chrono::Utc>>)));
            let gpu_target=gpu_state.clone();
            tokio::spawn(async move {
                loop {
                    let (values,note)=gpu_metrics().await;
                    *gpu_target.write()=(values,note,Some(chrono::Utc::now()));
                    tokio::time::sleep(Duration::from_secs(6)).await;
                }
            });
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                sys.refresh_cpu(); sys.refresh_memory(); sys.refresh_process(pid);
                let (gpus,gpu_note,gpu_sampled_at)=gpu_state.read().clone();
                let cores=sys.cpus().len().max(1);
                let process=sys.process(pid);
                let total=sys.total_memory();
                let used=total.saturating_sub(sys.available_memory());
                let snapshot=json!({"status":"live","sampled_at":chrono::Utc::now(),"cpu_sample_interval_seconds":2,
                    "gpu_sampled_at":gpu_sampled_at,"gpu_sample_interval_seconds":6,
                    "cpu_percent":finite(sys.global_cpu_info().cpu_usage() as f64),"logical_cores":cores,
                    "ram_total_bytes":total,"ram_used_bytes":used,"ram_available_bytes":sys.available_memory(),
                    "ram_percent":if total>0 {Some(100.0*used as f64/total as f64)} else {None},
                    "backend_pid":std::process::id(),"backend_memory_bytes":process.map(|p|p.memory()),
                    "backend_cpu_percent":process.and_then(|p|finite(p.cpu_usage() as f64/cores as f64)),
                    "gpus":gpus,"gpu_note":gpu_note,
                    "scope":"CPU/RAM and GPU device metrics are machine-wide; backend process metrics exclude browser and external engines. GPU metrics cannot establish this run's utilization. Unavailable counters remain null."});
                history.push_back(json!({"time":snapshot["sampled_at"],"cpu_percent":snapshot["cpu_percent"],"ram_percent":snapshot["ram_percent"],
                    "gpus":gpus.iter().map(|g|json!({"id":g["id"],"utilization_percent":g["utilization_percent"],"memory_used_bytes":g["memory_used_bytes"],"memory_total_bytes":g["memory_total_bytes"]})).collect::<Vec<_>>() }));
                while history.len()>150 {history.pop_front();}
                let mut result=snapshot;
                result["history"]=json!(history);
                *target.write()=result;
            }
        });
        Self{inner}
    }
    pub fn snapshot(&self) -> Value { self.inner.read().clone() }
}
fn finite(v:f64)->Option<f64>{if v.is_finite(){Some(v)}else{None}}
fn parse_number(s:&str)->Option<f64>{s.trim().parse::<f64>().ok().and_then(finite)}
fn bytes(v:Option<f64>)->Option<u64>{v.filter(|n|*n>=0.0).map(|n|(n*1048576.0) as u64)}

pub fn parse_nvidia(text:&str)->Vec<Value>{
    text.lines().filter_map(|line|{
        let fields=line.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len()<7 || fields[0].parse::<u32>().is_err(){return None;}
        Some(json!({"id":fields[2],"name":fields[1],"source":"nvidia-smi","scope":"device-wide",
            "utilization_percent":parse_number(fields[3]).map(|n|n.clamp(0.0,100.0)),
            "memory_used_bytes":bytes(parse_number(fields[4])),"memory_total_bytes":bytes(parse_number(fields[5])),
            "temperature_c":parse_number(fields[6]),"capacity_known":parse_number(fields[5]).map(|n|n>0.0).unwrap_or(false)}))
    }).collect()
}

pub(crate) async fn command_output(program:&std::path::Path,args:&[&str])->Option<String>{
    let mut command=tokio::process::Command::new(program);
    command.args(args).kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)] command.creation_flags(0x08000000);
    let output=tokio::time::timeout(Duration::from_secs(4),command.output()).await.ok()?.ok()?;
    if !output.status.success() || output.stdout.len()>2*1024*1024{return None;}
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn gpu_metrics()->(Vec<Value>,String){
    let mut candidates=vec![std::path::PathBuf::from("nvidia-smi")];
    #[cfg(windows)] {
        if let Some(root)=std::env::var_os("SystemRoot") { candidates.push(std::path::PathBuf::from(root).join("System32/nvidia-smi.exe")); }
        if let Some(root)=std::env::var_os("ProgramFiles") { candidates.push(std::path::PathBuf::from(root).join("NVIDIA Corporation/NVSMI/nvidia-smi.exe")); }
    }
    // PATH entry preferred, then conventional driver locations on Windows.
    for program in candidates.drain(..){
        if let Some(text)=command_output(&program,&["--query-gpu=index,name,uuid,utilization.gpu,memory.used,memory.total,temperature.gpu","--format=csv,noheader,nounits"]).await {
            let values=parse_nvidia(&text);
            if !values.is_empty(){return (values,"NVIDIA device-wide driver counters. Other vendors are not sampled by this provider; per-process VRAM is not inferred under WDDM.".to_owned());}
        }
    }
    #[cfg(windows)] {
        // Fixed read-only command: no user input enters a shell or command line.
        let script=r#"$ErrorActionPreference='Stop'; $eng=Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine; $mem=Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUAdapterMemory; $out=@(); foreach($g in ($eng | Group-Object { if($_.Name -match '(luid_.+?_phys_\d+)'){ $matches[1] }else{'unknown'} })) { $busy=0; foreach($e in ($g.Group | Group-Object { if($_.Name -match 'eng_(\d+)'){ $matches[1] }else{$_.Name} })) { $sum=($e.Group | Measure-Object -Property UtilizationPercentage -Sum).Sum; if($sum -gt $busy){$busy=$sum} }; $m=$mem | Where-Object {$_.Name -like ('*'+$g.Name+'*')} | Select-Object -First 1; $used=$null; if($m){$used=[double]$m.DedicatedUsage}; $out+=@{id=$g.Name;name=('Windows adapter '+$g.Name);source='Windows WDDM counters';scope='device-wide busiest engine';utilization_percent=[Math]::Min(100,[Math]::Max(0,$busy));memory_used_bytes=$used;memory_total_bytes=$null;temperature_c=$null;capacity_known=$false} }; ConvertTo-Json -InputObject @($out) -Depth 4 -Compress"#;
        if let Some(text)=command_output(std::path::Path::new("powershell.exe"),&["-NoProfile","-NonInteractive","-Command",script]).await {
            if let Ok(value)=serde_json::from_str::<Value>(&text){
                let values=value.as_array().cloned().unwrap_or_default();
                if !values.is_empty(){return (values,"Windows device-wide busiest-engine counters. Dedicated allocation may be reported; reliable capacity is unavailable, so no VRAM percent is invented.".into());}
            }
        }
    }
    #[cfg(target_os="linux")] {
        let values=amd_linux();
        if !values.is_empty(){return (values,"Linux AMD driver sysfs counters; device-wide, fields depend on driver support.".into());}
    }
    (vec![],"GPU utilization/VRAM counters unavailable. Hardware detection alone is not utilization data. Rendering and CPU execution remain usable.".into())
}
#[cfg(target_os="linux")]
fn amd_linux()->Vec<Value>{
    let Ok(entries)=std::fs::read_dir("/sys/class/drm") else{return vec![];};
    entries.filter_map(Result::ok).filter_map(|entry|{
        let name=entry.file_name().to_string_lossy().to_string();
        let suffix=name.strip_prefix("card")?;
        if suffix.is_empty() || !suffix.chars().all(|c|c.is_ascii_digit()){return None;}
        let device=entry.path().join("device");
        let read=|field:&str|std::fs::read_to_string(device.join(field)).ok().and_then(|s|parse_number(&s));
        let used=read("mem_info_vram_used");let total=read("mem_info_vram_total");let busy=read("gpu_busy_percent");
        if used.is_none() && busy.is_none(){return None;}
        Some(json!({"id":name,"name":format!("AMD {}",name),"source":"Linux DRM sysfs","scope":"device-wide",
            "utilization_percent":busy,"memory_used_bytes":used,"memory_total_bytes":total,
            "temperature_c":null,"capacity_known":total.map(|v|v>0.0).unwrap_or(false)}))
    }).collect()
}
#[cfg(test)]mod tests{
    use super::*;
    #[test]fn unavailable_is_not_zero(){let rows=parse_nvidia("0, GPU, GPU-id, [N/A], 512, 8192, N/A");assert!(rows[0]["utilization_percent"].is_null());assert!(rows[0]["temperature_c"].is_null());assert_eq!(rows[0]["memory_used_bytes"],536870912_u64);}
    #[test]fn malformed_is_rejected(){assert!(parse_nvidia("GPU probe failed").is_empty());assert!(parse_number("NaN").is_none());}
    #[test]fn capacity_and_usage_are_different(){let rows=parse_nvidia("0, GPU, GPU-id, 93, 4096, 8192, 55");assert_eq!(rows[0]["utilization_percent"],93.0);assert_eq!(rows[0]["memory_total_bytes"],8589934592_u64);}
}
