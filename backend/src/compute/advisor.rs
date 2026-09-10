//! Structural admission estimates, not benchmarks or hard OS memory isolation.
//! Only allocation changes: no equation, timestep, horizon or candidate-budget edits.
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use crate::{compute::HardwareManager,domain::{ComputePreference,ComputeRequest,ComputePolicy,ExperimentManifest,ModelSpec,IntegrationMethod,FalsificationKind}};
#[derive(Debug,Clone,Serialize,Deserialize)]pub struct Advice {
    pub sampled_at:chrono::DateTime<chrono::Utc>,pub available_memory_bytes:u64,pub cpu_workers:usize,
    pub gpu_eligible:bool,pub route:String,pub reason:String,pub admissible:bool,
    pub estimated_peak_bytes:u64,pub candidate_evaluations:usize,pub estimated_step_work:f64,
    pub recommended_compute:ComputeRequest,pub warnings:Vec<String>,pub scope:String,
}
pub fn snapshot(hw:&HardwareManager)->Value {let mut system=sysinfo::System::new();system.refresh_memory();json!({"hardware":hw.profile(),"neural_accelerators":hw.neural_accelerators(),"available_memory_bytes":system.available_memory(),"sampled_at":chrono::Utc::now(),"note":"Available memory and initialized devices, not utilization or measured throughput. Review solver eligibility before selecting GPU. Neural devices are inventoried separately and are not counted as numerical capacity without an installed solver."})}
pub fn advise(m:&ExperimentManifest,hw:&HardwareManager)->anyhow::Result<Advice>{
    let mut system=sysinfo::System::new();system.refresh_memory();
    let (eligible,reason)=if matches!(&m.model,ModelSpec::StateVectorOde{..}){
        match hw.gpu_ode_evaluator(){Some(gpu)=>match crate::science::ode::compile_program(m).and_then(|p|gpu.compatibility(&p,m)){
            Ok(())=>(m.search.enabled,"GPU/f32 screening when search is enabled; final replay remains CPU/f64.".into()),
            Err(e)=>(false,format!("CPU/f64 required by this experiment: {e:#}")),
        },None=>(false,"No initialized GPU compute evaluator; CPU/f64 is available.".into())}
    }else{(false,"This particle solver runs on CPU; 3D rendering may independently use the GPU.".into())};
    let mut advice=calculate(m,hw.profile().scheduler.cpu_worker_threads.max(1),system.available_memory(),eligible,reason);
    if eligible && advice.recommended_compute.preference != ComputePreference::Cpu && m.compute.batch_size == 0 {
        advice.recommended_compute.batch_size = hw.profile().scheduler.gpu_batch_size.max(1)
            .min(advice.recommended_compute.batch_size);
    }
    Ok(advice)
}
pub fn worker_memory_bytes(m:&ExperimentManifest)->u64 {
    let states=match &m.model {
        ModelSpec::StateVectorOde{variables,..}=>variables.len() as u64,
        ModelSpec::PairwiseParticles{population,dimensions,..}=>(population.count as u64).saturating_mul(*dimensions as u64).saturating_mul(2),
    };
    states.saturating_mul(4096).saturating_add((m.trajectory.len() as u64).saturating_mul(8192)).saturating_add(1048576)
}
pub fn calculate(m:&ExperimentManifest,workers:usize,available:u64,gpu:bool,reason:String)->Advice{
    let mut c=m.compute.clone();let mut warnings=Vec::new();
    let steps=((m.integration.end_time-m.integration.start_time)/m.integration.time_step).ceil().max(1.0);
    let (states,entities,factor)=match &m.model{ModelSpec::StateVectorOde{variables,..}=>(variables.len(),m.visualization.entities.len().max(variables.len()/3).max(1),if m.integration.method==IntegrationMethod::Rk4{4.0}else{1.0}),ModelSpec::PairwiseParticles{population,dimensions,..}=>(population.count*dimensions*2,population.count,population.count as f64)};
    let count=if m.search.enabled{m.search.population.min(c.candidate_count.max(2)).max(2)}else{1};
    let evaluations=count.saturating_mul(if m.search.enabled{m.search.generations.max(1)}else{1});
    let output=(m.visualization.max_frames as u64).saturating_mul(entities as u64*1536+m.observables.len() as u64*256).saturating_add(8*1048576);
    let all_candidates=count as u64*(states as u64*32+m.search.variables.len() as u64*64+4096);
    let per_worker=worker_memory_bytes(m);
    let allowed=(available/2).min(c.max_memory_mb as u64*1048576);
    let policy_workers=match c.policy{ComputePolicy::Interactive=>(workers/2).max(1),ComputePolicy::Balanced=>workers.saturating_sub(1).max(1),ComputePolicy::Throughput=>workers};
    let capacity=allowed.saturating_sub(output+all_candidates+16*1048576)/per_worker.max(1);
    let gpu_route=gpu&&c.preference!=ComputePreference::Cpu;
    let target=if c.batch_size>0{c.batch_size}else if gpu_route{32768}else{policy_workers};
    // GPU candidates are compact input vectors, not whole CPU workers. The estimate
    // reserves a full CPU fallback wave separately from GPU transfer allocations.
    c.batch_size=if gpu_route && capacity as usize >= workers {target.min(count).max(1)} else {target.min(policy_workers).min(capacity as usize).min(count).max(1)};
    c.max_memory_mb=(allowed/1048576).min(c.max_memory_mb as u64) as usize;
    if !gpu {c.preference=ComputePreference::Cpu;}
    let workers_in_flight=if gpu_route {workers.min(c.batch_size).max(1)} else {c.batch_size};
    let peak=output+all_candidates+16*1048576+per_worker*workers_in_flight as u64;
    let search_budget_ok=!m.search.enabled || c.candidate_count >= if m.search.algorithm==crate::domain::SearchAlgorithm::DifferentialEvolution{4}else{2};
    let admissible=c.max_memory_mb>=128&&capacity>=1&&peak<=allowed&&search_budget_ok;
    if !search_budget_ok{warnings.push("The approved candidate allocation is below this search algorithm's minimum; revise the budget explicitly instead of running extra candidates.".into());}
    if count==1{warnings.push("Single trajectory: there is no initial-condition search or diversity portfolio in this run. A time integration is sequential; independent trials create parallel work.".into());}
    if !m.trajectory.is_empty(){warnings.push("Per-step trajectory measurements use CPU/f64. GPU scoring cannot omit them merely to raise GPU utilization.".into());}
    if !admissible{warnings.push("Insufficient memory headroom for estimated resident/output data. Reduce retained frames or explicitly redesign the workload; physical equations will not be silently simplified.".into());}
    let refine:f64=m.falsification.iter().map(|t|match t.kind{FalsificationKind::ResolutionLadder=>(0..t.repetitions.min(4)).map(|i|(1usize<<(i+1)) as f64).sum(),FalsificationKind::StepHalving=>2.0*t.repetitions as f64,_=>t.repetitions as f64}).sum();
    Advice{sampled_at:chrono::Utc::now(),available_memory_bytes:available,cpu_workers:policy_workers,gpu_eligible:gpu,
        route:if gpu&&c.preference!=ComputePreference::Cpu{"GPU/f32 screen + CPU/f64 replay"}else{"CPU/f64"}.into(),reason,admissible,
        estimated_peak_bytes:peak,candidate_evaluations:evaluations,estimated_step_work:steps*(evaluations as f64+1.0+refine)*states as f64*factor,
        recommended_compute:c,warnings,scope:"Heuristic host-memory and work estimates, not measured peak RSS or an ETA. Half of available RAM remains reserved; this is not an OS memory sandbox. Admission never expands token/time/candidate limits. VRAM is not inferred from system RAM.".into()}
}
#[cfg(test)]mod tests{use super::*;fn fixture()->ExperimentManifest{let d=serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();ExperimentManifest::from_draft(uuid::Uuid::nil(),None,1,d,"test")}
#[test]fn respects_approved_budget(){let m=fixture();let a=calculate(&m,22,18*1024*1024*1024,false,"cpu".into());assert!(a.recommended_compute.max_memory_mb<=m.compute.max_memory_mb);assert_eq!(a.recommended_compute.max_wall_seconds,m.compute.max_wall_seconds);assert_eq!(a.recommended_compute.candidate_count,m.compute.candidate_count);}
#[test]fn no_free_ram_rejected(){let a=calculate(&fixture(),22,64*1048576,false,"cpu".into());assert!(!a.admissible);}
#[test]fn device_presence_not_eligibility(){let a=calculate(&fixture(),8,8*1024*1024*1024,false,"unsupported lowering".into());assert_eq!(a.recommended_compute.preference,ComputePreference::Cpu);}
}
