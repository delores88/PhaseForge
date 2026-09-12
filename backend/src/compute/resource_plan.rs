//! Timestamped planning evidence. Budgets are proposals, not reservations or solver validation.
use std::{collections::BTreeMap, path::Path};

use anyhow::ensure;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::HardwareManager;
use crate::domain::HardwareProfile;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const MEMORY_MAX_AGE_SECONDS: i64 = 10;
const CPU_MAX_AGE_SECONDS: i64 = 12;
const GPU_MAX_AGE_SECONDS: i64 = 15;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceScope {
    Host,
    ExecutionRuntime,
}

/// One coherent sample from the trusted caller. A runtime/guest sample must not be
/// passed off as host headroom, or vice versa. Unknown measurements remain None.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceObservations {
    pub sampled_at: DateTime<Utc>,
    pub scope: ResourceScope,
    pub cpu_name: Option<String>,
    pub logical_threads: Option<usize>,
    pub physical_cores: Option<usize>,
    pub total_memory_bytes: Option<u64>,
    pub available_memory_bytes: Option<u64>,
    pub workspace_free_bytes: Option<u64>,
}

/// Historical measured facts only. The coordinator must verify the source receipt
/// bytes and configuration before supplying them; this function validates the shape.
/// No scaling to another grid, backend, hardware, or target duration is inferred.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeasuredPilot {
    pub source_job_id: uuid::Uuid,
    pub receipt_sha256: String,
    pub configuration_sha256: String,
    pub measured_at: DateTime<Utc>,
    pub solver_id: String,
    pub execution_backend: String,
    pub resource_scope: ResourceScope,
    pub cpu_threads: usize,
    pub wall_seconds: f64,
    pub completed_work_units: u64,
    pub work_unit: String,
    pub peak_process_tree_memory_bytes: Option<u64>,
    pub output_bytes: u64,
}

/// Synchronous read-only host sampling. Call on a blocking executor if necessary.
/// Does not start workers, refresh GPU drivers, create directories, or reserve space.
pub fn plan(
    hardware: &HardwareManager,
    telemetry: &Value,
    data_root: &Path,
    deadline_at: Option<DateTime<Utc>>,
    pilot: Option<&MeasuredPilot>,
) -> anyhow::Result<Value> {
    let mut system = sysinfo::System::new();
    system.refresh_cpu();
    system.refresh_memory();
    let total = system.total_memory();
    let available = system.available_memory();
    let observations = ResourceObservations {
        sampled_at: Utc::now(),
        scope: ResourceScope::Host,
        cpu_name: system.cpus().first().map(|cpu| cpu.brand().trim().to_owned()).filter(|name| !name.is_empty()),
        logical_threads: std::thread::available_parallelism().ok().map(|count| count.get()),
        physical_cores: system.physical_core_count(),
        total_memory_bytes: (total > 0).then_some(total),
        available_memory_bytes: (total > 0 && available <= total).then_some(available),
        workspace_free_bytes: workspace_free_bytes(data_root),
    };
    from_observations(hardware.profile(), telemetry, &observations, deadline_at, pilot, Utc::now())
}

/// Pure planning builder for deterministic checks and a future independently
/// measured execution-runtime sample. Host telemetry is always labelled host-wide.
pub fn from_observations(
    hardware: &HardwareProfile,
    telemetry: &Value,
    observations: &ResourceObservations,
    deadline_at: Option<DateTime<Utc>>,
    pilot: Option<&MeasuredPilot>,
    now: DateTime<Utc>,
) -> anyhow::Result<Value> {
    validate_observations(observations)?;
    if let Some(pilot) = pilot { validate_pilot(pilot, now)?; }
    let resource_fresh = fresh(Some(observations.sampled_at), now, MEMORY_MAX_AGE_SECONDS);
    let telemetry_at = timestamp(&telemetry["sampled_at"]);
    let cpu_fresh = fresh(telemetry_at, now, CPU_MAX_AGE_SECONDS);
    let gpu_at = timestamp(&telemetry["gpu_sampled_at"]);
    let gpu_fresh = fresh(gpu_at, now, GPU_MAX_AGE_SECONDS);
    let total = resource_fresh.then_some(observations.total_memory_bytes).flatten();
    let available = resource_fresh.then_some(observations.available_memory_bytes).flatten();
    let free = resource_fresh.then_some(observations.workspace_free_bytes).flatten();
    let cpu_threads = observations.logical_threads.map(|count| {
        count.saturating_sub(2).max(1).min(hardware.scheduler.cpu_worker_threads.max(1))
    });
    // At least half of currently available RAM remains outside this proposal.
    // On a pressured host retain a further 2 GiB minimum for other processes.
    let memory_budget = available.map(|bytes| (bytes / 2).min(bytes.saturating_sub(2 * GIB)) / MIB * MIB);
    let output_budget = free.map(|bytes| (bytes / 10).min(bytes.saturating_sub(10 * GIB)).min(8 * GIB) / MIB * MIB);
    let expired = deadline_at.is_some_and(|deadline| deadline <= now);
    let mut reasons = Vec::new();
    if !resource_fresh { reasons.push("The supplied RAM/storage sample is stale or future-dated. Refresh it before admission."); }
    if observations.logical_threads.is_none() { reasons.push("Available logical CPU thread count is unknown."); }
    if available.is_none() || total.is_none() { reasons.push("Fresh available/total RAM is unknown."); }
    if memory_budget.is_some_and(|bytes| bytes < 128 * MIB) { reasons.push("Insufficient available RAM after the interactive reserve."); }
    if free.is_none() { reasons.push("Free space on the actual data-root volume is unknown."); }
    if output_budget.is_some_and(|bytes| bytes < 64 * MIB) { reasons.push("Insufficient workspace storage after the free-space reserve."); }
    if expired { reasons.push("The inherited absolute deadline has elapsed."); }
    let budget_available = reasons.is_empty();
    let initialized = hardware.selected_gpu_index.and_then(|index| hardware.adapters.iter().find(|gpu| gpu.index == index))
        .filter(|_| hardware.gpu_available).map(|gpu| json!({
            "name":gpu.name,"backend":gpu.backend,
            "max_storage_buffer_binding_size":gpu.max_storage_buffer_binding_size,
            "scope":"Initialized legacy ODE evaluator only. A buffer-binding limit is not VRAM capacity."
        }));
    let gpus = gpu_observations(telemetry, gpu_fresh);
    Ok(json!({
        "schema":"phaseforge.resource-plan.v1",
        "sampled_at":now,
        "measurements":{
            "sampled_at":observations.sampled_at,"fresh":resource_fresh,"max_age_seconds":MEMORY_MAX_AGE_SECONDS,
            "resource_scope":observations.scope,
            "cpu":{"name":observations.cpu_name,"logical_threads":observations.logical_threads,"physical_cores":observations.physical_cores,
                "configured_host_worker_threads":hardware.scheduler.cpu_worker_threads},
            "memory":{"total_bytes":total,"available_bytes":available,"scope":observations.scope},
            "workspace":{"free_bytes":free,"scope":"Actual data-root volume in the measured resource scope; free space is not reserved."},
            "host_telemetry":{"sampled_at":telemetry_at,"fresh":cpu_fresh,
                "cpu_percent":if cpu_fresh { percent(&telemetry["cpu_percent"]) } else {None},
                "scope":"Machine-wide utilization; not free CPU capacity or utilization attributable to this job."},
            "gpu":{"sampled_at":gpu_at,"fresh":gpu_fresh,"max_age_seconds":GPU_MAX_AGE_SECONDS,
                "devices":gpus,"initialized_ode_compute":initialized,
                "nr_gpu_eligibility":Value::Null,"execution_runtime_access_verified":false,
                "scope":"Host device-wide counters. Physical IDs are deduplicated; backend aliases and VRAM are never summed. Runtime/solver GPU support requires a separate exact adapter probe."}
        },
        "concurrency":{"laboratory_solver_slots":2,"laboratory_render_slots":1,
            "slots_are_resource_reservations":false,"reserved_cpu_threads":Value::Null,"reserved_memory_bytes":Value::Null,
            "scope":"Configured laboratory semaphore capacity, not available slots or safe concurrent memory allocation. The legacy scheduler has a separate numerical gate."},
        "timer":{"mode":if deadline_at.is_some(){"absolute_deadline"}else{"off"},"deadline_at":deadline_at,
            "remaining_seconds":deadline_at.map(|deadline|(deadline-now).num_milliseconds().max(0) as f64/1000.0),
            "expired":expired,"scope":"Off removes only the total timer; Stop and CPU/RAM/storage limits remain. A child cannot widen its parent's deadline."},
        "budget":{
            "status":if budget_available {"proposal_available"}else{"hold"},"reasons":reasons,
            "cpu_threads_max":cpu_threads,"memory_bytes_max":memory_budget,"output_bytes_max":output_budget,
            "policy":{"cpu_reserve_threads_target":2,"minimum_ram_reserve_bytes":2*GIB,"ram_fraction_of_available_max":0.5,
                "minimum_free_storage_reserve_bytes":10*GIB,"output_fraction_of_free_max":0.1,"output_ceiling_bytes":8*GIB},
            "reserved":false,"execution_admitted":false,"solver_feasibility":"not_assessed",
            "enforcement":{"cpu":"Proposal only. The caller must set and enforce ranks × threads; current native thread environment settings are not a CPU quota.",
                "memory":"Proposal only. Native OwnedProcess currently limits each process; LPAC separately enforces job-wide memory. A multiprocess or guest NR runtime requires its own verified aggregate boundary.",
                "storage":"Proposal only. Existing generated-job monitoring is a sampled soft limit with possible overshoot, not a filesystem quota."},
            "scope":"Recheck and atomically reserve resources before launch. Estimates, scheduler slot counts and current free memory alone do not admit an NR calculation. Lower rendering cost does not establish numerical validity."
        },
        "pilot":pilot,
        "estimate":{"basis":if pilot.is_some(){"historical_measured_pilot_only"}else{"uncalibrated"},
            "target_wall_seconds":Value::Null,"target_peak_memory_bytes":Value::Null,"target_output_bytes":Value::Null,
            "scope":"Pilot facts require caller-verified receipt bytes; only field validation occurs here. No extrapolation across parameters, grid, hardware or execution backend, and no claim of convergence or 0.999c collision feasibility."}
    }))
}

fn validate_observations(value: &ResourceObservations) -> anyhow::Result<()> {
    ensure!(value.logical_threads.is_none_or(|count| count > 0), "Logical thread count must be positive or unknown");
    ensure!(value.physical_cores.is_none_or(|count| count > 0), "Physical core count must be positive or unknown");
    ensure!(value.total_memory_bytes.is_none_or(|bytes| bytes > 0), "Total memory must be positive or unknown");
    if let (Some(total), Some(available)) = (value.total_memory_bytes, value.available_memory_bytes) {
        ensure!(available <= total, "Available memory exceeds measured total memory");
    }
    ensure!(value.available_memory_bytes.is_none() || value.total_memory_bytes.is_some(), "Available memory needs its measured total");
    ensure!(value.cpu_name.as_ref().is_none_or(|name| !name.trim().is_empty() && name.len() <= 512), "Invalid CPU name");
    Ok(())
}

fn validate_pilot(value: &MeasuredPilot, now: DateTime<Utc>) -> anyhow::Result<()> {
    let digest = |text: &str| text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    ensure!(!value.source_job_id.is_nil() && digest(&value.receipt_sha256) && digest(&value.configuration_sha256), "Pilot requires exact source and SHA-256 provenance");
    ensure!(value.measured_at <= now, "Pilot measurement is future-dated");
    ensure!(value.wall_seconds.is_finite() && value.wall_seconds > 0.0 && value.completed_work_units > 0 && value.cpu_threads > 0, "Invalid measured pilot work/time/thread values");
    ensure!(value.peak_process_tree_memory_bytes.is_none_or(|bytes| bytes > 0), "Pilot peak memory must be positive or unknown");
    for text in [&value.solver_id, &value.execution_backend, &value.work_unit] {
        ensure!(!text.trim().is_empty() && text.len() <= 256 && !text.chars().any(char::is_control), "Invalid pilot descriptor");
    }
    Ok(())
}

fn timestamp(value: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.as_str()?).ok().map(|at| at.with_timezone(&Utc))
}
fn fresh(at: Option<DateTime<Utc>>, now: DateTime<Utc>, max_seconds: i64) -> bool {
    at.is_some_and(|at| { let age = (now - at).num_milliseconds(); age >= 0 && age <= max_seconds * 1000 })
}
fn percent(value: &Value) -> Option<f64> {
    value.as_f64().filter(|number| number.is_finite() && (0.0..=100.0).contains(number))
}
fn gpu_observations(telemetry: &Value, fresh: bool) -> Vec<Value> {
    let mut devices: BTreeMap<String, Value> = BTreeMap::new();
    for row in telemetry["gpus"].as_array().into_iter().flatten().take(64) {
        let Some(id) = row["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 256) else { continue };
        let name = row["name"].as_str().filter(|name| !name.is_empty() && name.len() <= 512);
        let source = row["source"].as_str().filter(|source| source.len() <= 128);
        let total = if fresh && row["capacity_known"] == true { row["memory_total_bytes"].as_u64().filter(|bytes| *bytes > 0) } else { None };
        let used = if fresh { row["memory_used_bytes"].as_u64().filter(|bytes| total.is_none_or(|total| *bytes <= total)) } else { None };
        let mut candidate = json!({"physical_id":id,"name":name,"source":source,"scope":"host device-wide",
            "memory_total_bytes":total,"memory_used_bytes":used,"unallocated_bytes":total.zip(used).map(|(total,used)|total-used),
            "utilization_percent":if fresh { percent(&row["utilization_percent"]) } else {None},
            "capacity_known":total.is_some(),"counter_conflict":false});
        if let Some(previous) = devices.get(id) {
            // Conflicting observations for one physical ID are unknown, never summed or selected optimistically.
            if previous != &candidate {
                candidate["memory_total_bytes"] = Value::Null; candidate["memory_used_bytes"] = Value::Null;
                candidate["unallocated_bytes"] = Value::Null; candidate["utilization_percent"] = Value::Null;
                candidate["capacity_known"] = json!(false); candidate["counter_conflict"] = json!(true);
            }
        }
        devices.insert(id.to_owned(), candidate);
    }
    devices.into_values().collect()
}

#[cfg(windows)]
fn workspace_free_bytes(data_root: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let root = data_root.canonicalize().ok()?;
    if !root.is_dir() { return None; }
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut available = 0;
    let success = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(wide.as_ptr(), &mut available, std::ptr::null_mut(), std::ptr::null_mut()) };
    (success != 0).then_some(available)
}

#[cfg(not(windows))]
fn workspace_free_bytes(data_root: &Path) -> Option<u64> {
    let root = data_root.canonicalize().ok()?;
    if !root.is_dir() { return None; }
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks.iter().filter(|disk| root.starts_with(disk.mount_point())).max_by_key(|disk| disk.mount_point().components().count()).map(|disk| disk.available_space())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> DateTime<Utc> { "2026-09-12T15:00:00Z".parse().unwrap() }
    fn hardware() -> HardwareProfile {
        serde_json::from_value(json!({"operating_system":"windows","architecture":"x86_64","cpu":{"brand":"CPU","logical_cores":22,"total_memory_bytes":32*GIB},"gpu_available":true,"selected_gpu_index":0,
            "adapters":[{"index":0,"name":"RTX test","vendor_id":4318,"device_id":1,"vendor":"NVIDIA","device_type":"DiscreteGpu","backend":"Dx12","driver":"","driver_info":"","max_compute_workgroups_per_dimension":65535,"max_storage_buffer_binding_size":2147483648_u32,"score":1,"selected":true},
            {"index":1,"name":"RTX test","vendor_id":4318,"device_id":1,"vendor":"NVIDIA","device_type":"DiscreteGpu","backend":"Vulkan","driver":"","driver_info":"","max_compute_workgroups_per_dimension":65535,"max_storage_buffer_binding_size":2147483648_u32,"score":1,"selected":false}],
            "scheduler":{"max_parallel_jobs":6,"cpu_worker_threads":20,"gpu_batch_size":8192,"interactive_reserve":1,"rationale":[]},"warnings":[]})).unwrap()
    }
    fn observed() -> ResourceObservations { ResourceObservations { sampled_at:now(),scope:ResourceScope::Host,cpu_name:Some("CPU".into()),logical_threads:Some(22),physical_cores:Some(16),total_memory_bytes:Some(32*GIB),available_memory_bytes:Some(10*GIB),workspace_free_bytes:Some(80*GIB) } }
    fn telemetry() -> Value { json!({"sampled_at":now(),"gpu_sampled_at":now(),"cpu_percent":80,"gpus":[{"id":"gpu-0","name":"RTX test","source":"nvidia-smi","capacity_known":true,"memory_total_bytes":16*GIB,"memory_used_bytes":3*GIB,"utilization_percent":20}]}) }
    fn build(value: &ResourceObservations, t: &Value) -> Value { from_observations(&hardware(),t,value,None,None,now()).unwrap() }

    #[test]
    fn off_does_not_remove_resource_caps_or_create_a_reservation() {
        let plan=build(&observed(),&telemetry());
        assert_eq!(plan["schema"],"phaseforge.resource-plan.v1");assert_eq!(plan["timer"]["mode"],"off");assert!(plan["timer"]["deadline_at"].is_null());
        assert_eq!(plan["budget"]["memory_bytes_max"],5*GIB);assert_eq!(plan["budget"]["cpu_threads_max"],20);assert_eq!(plan["budget"]["output_bytes_max"],8*GIB);
        assert_eq!(plan["budget"]["reserved"],false);assert_eq!(plan["budget"]["execution_admitted"],false);assert_eq!(plan["concurrency"]["laboratory_solver_slots"],2);
    }
    #[test]
    fn memory_pressure_and_low_disk_hold_even_when_total_capacity_is_large() {
        let mut value=observed();value.available_memory_bytes=Some(GIB);value.workspace_free_bytes=Some(9*GIB);
        let plan=build(&value,&telemetry());assert_eq!(plan["budget"]["status"],"hold");assert_eq!(plan["budget"]["memory_bytes_max"],0);assert_eq!(plan["budget"]["output_bytes_max"],0);
    }
    #[test]
    fn stale_or_future_measurements_are_unknown_without_borrowing_telemetry_ram() {
        for delta in [-11,1] { let mut value=observed();value.sampled_at=now()+chrono::Duration::seconds(delta);let plan=build(&value,&telemetry());assert!(plan["measurements"]["memory"]["available_bytes"].is_null());assert!(plan["budget"]["memory_bytes_max"].is_null());assert_eq!(plan["budget"]["status"],"hold"); }
        let mut t=telemetry();t["sampled_at"]=json!(now()-chrono::Duration::seconds(13));t["gpu_sampled_at"]=json!(now()-chrono::Duration::seconds(16));let plan=build(&observed(),&t);
        assert!(plan["measurements"]["host_telemetry"]["cpu_percent"].is_null());assert!(plan["measurements"]["gpu"]["devices"][0]["memory_total_bytes"].is_null());assert_eq!(plan["budget"]["memory_bytes_max"],5*GIB);
    }
    #[test]
    fn aliases_are_not_summed_and_gpu_presence_never_establishes_nr_support() {
        let mut t=telemetry();let duplicate=t["gpus"][0].clone();t["gpus"].as_array_mut().unwrap().push(duplicate);let plan=build(&observed(),&t);
        assert_eq!(plan["measurements"]["gpu"]["devices"].as_array().unwrap().len(),1);assert_eq!(plan["measurements"]["gpu"]["devices"][0]["memory_total_bytes"],16*GIB);assert!(plan["measurements"]["gpu"]["nr_gpu_eligibility"].is_null());assert_eq!(plan["measurements"]["gpu"]["execution_runtime_access_verified"],false);
        t["gpus"][1]["memory_used_bytes"]=json!(4*GIB);let plan=build(&observed(),&t);assert!(plan["measurements"]["gpu"]["devices"][0]["memory_total_bytes"].is_null());assert_eq!(plan["measurements"]["gpu"]["devices"][0]["counter_conflict"],true);
    }
    #[test]
    fn unavailable_vram_and_invalid_counters_never_become_zero_capacity() {
        let mut t=telemetry();t["gpus"][0]["capacity_known"]=json!(false);t["gpus"][0]["memory_total_bytes"]=Value::Null;t["gpus"][0]["utilization_percent"]=json!(-5);let plan=build(&observed(),&t);let gpu=&plan["measurements"]["gpu"]["devices"][0];assert!(gpu["memory_total_bytes"].is_null());assert!(gpu["unallocated_bytes"].is_null());assert!(gpu["utilization_percent"].is_null());assert_eq!(gpu["memory_used_bytes"],3*GIB);
    }
    #[test]
    fn absolute_deadline_is_preserved_and_expired_budget_holds() {
        for seconds in [-1,0,25] { let deadline=now()+chrono::Duration::seconds(seconds);let plan=from_observations(&hardware(),&telemetry(),&observed(),Some(deadline),None,now()).unwrap();assert_eq!(plan["timer"]["deadline_at"],json!(deadline));assert_eq!(plan["timer"]["remaining_seconds"],json!(seconds.max(0) as f64));assert_eq!(plan["timer"]["expired"],seconds<=0); }
    }
    #[test]
    fn runtime_resources_stay_separate_from_host_gpu_and_unknowns_hold() {
        let mut value=observed();value.scope=ResourceScope::ExecutionRuntime;value.logical_threads=Some(4);value.available_memory_bytes=Some(6*GIB);let plan=build(&value,&telemetry());assert_eq!(plan["measurements"]["memory"]["scope"],"execution_runtime");assert_eq!(plan["budget"]["cpu_threads_max"],2);assert_eq!(plan["budget"]["memory_bytes_max"],3*GIB);assert_eq!(plan["measurements"]["gpu"]["devices"][0]["scope"],"host device-wide");
        value.workspace_free_bytes=None;value.physical_cores=None;let plan=build(&value,&json!({}));assert_eq!(plan["budget"]["status"],"hold");assert!(plan["measurements"]["cpu"]["physical_cores"].is_null());
    }
    #[test]
    fn malformed_observations_and_pilot_facts_are_rejected_and_pilot_has_no_eta() {
        let mut value=observed();value.available_memory_bytes=Some(33*GIB);assert!(from_observations(&hardware(),&telemetry(),&value,None,None,now()).is_err());
        let mut pilot=MeasuredPilot{source_job_id:uuid::Uuid::new_v4(),receipt_sha256:"a".repeat(64),configuration_sha256:"b".repeat(64),measured_at:now(),solver_id:"nr-test".into(),execution_backend:"cpu".into(),resource_scope:ResourceScope::ExecutionRuntime,cpu_threads:2,wall_seconds:1.5,completed_work_units:5,work_unit:"grid updates".into(),peak_process_tree_memory_bytes:Some(GIB),output_bytes:1024};
        let plan=from_observations(&hardware(),&telemetry(),&observed(),None,Some(&pilot),now()).unwrap();assert_eq!(plan["pilot"]["wall_seconds"],1.5);assert!(plan["estimate"]["target_wall_seconds"].is_null());
        pilot.wall_seconds=f64::NAN;assert!(from_observations(&hardware(),&telemetry(),&observed(),None,Some(&pilot),now()).is_err());pilot.wall_seconds=1.0;pilot.receipt_sha256="bad".into();assert!(from_observations(&hardware(),&telemetry(),&observed(),None,Some(&pilot),now()).is_err());
    }
    #[test]
    fn actual_volume_query_is_read_only_and_missing_directory_is_unknown() {
        let root=std::env::current_dir().unwrap();assert!(workspace_free_bytes(&root).is_some());assert_eq!(workspace_free_bytes(&root.join(format!("not-created-{}",uuid::Uuid::new_v4()))),None);
    }
}
