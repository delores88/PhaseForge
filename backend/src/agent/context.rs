//! Capability summaries for provider prompts. The complete hardware inventory
//! remains local to the Compute page; identifiers and drivers do not aid planning.
use serde_json::{json, Value};

pub(super) fn compute_summary(hardware: &crate::compute::HardwareManager) -> Value {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    summarize(hardware.profile(), system.available_memory())
}

fn summarize(profile: &crate::domain::HardwareProfile, available_memory_bytes: u64) -> Value {
    let selected_gpu = profile.selected_gpu_index
        .and_then(|index| profile.adapters.iter().find(|adapter| adapter.index == index))
        .filter(|_| profile.gpu_available)
        .map(|adapter| json!({
            "name": adapter.name,
            "backend": adapter.backend,
            "max_compute_workgroups_per_dimension": adapter.max_compute_workgroups_per_dimension,
            "max_storage_buffer_binding_size": adapter.max_storage_buffer_binding_size,
        }));
    json!({
        "cpu_logical_cores": profile.cpu.logical_cores,
        "cpu_worker_threads": profile.scheduler.cpu_worker_threads,
        "available_memory_bytes": available_memory_bytes,
        "selected_gpu": selected_gpu,
        // Scheduler::numerical_gate currently admits one numerical job at once;
        // max_parallel_jobs is dispatcher capacity, not simultaneous solver work.
        "numerical_run_slots": 1,
        "npu_numerical_execution": false,
        "scope": "Local compute only. One numerical run at a time; compatible ODE batches may use the selected GPU. Other native solvers use CPU workers. GPU rendering is separate from numerical execution. Available RAM is a changing admission estimate, not guaranteed capacity.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_compute_is_a_whitelist_with_actual_numerical_capacity() {
        let profile = serde_json::from_value(json!({
            "operating_system":"windows", "architecture":"x86_64",
            "cpu":{"brand":"CPU inventory detail", "logical_cores":24, "total_memory_bytes":64000000000_u64},
            "gpu_available":true, "selected_gpu_index":1,
            "adapters":[
                {"index":0,"name":"Unused adapter inventory","vendor_id":0,"device_id":1,"vendor":"secret vendor",
                 "device_type":"IntegratedGpu","backend":"Vulkan","driver":"secret driver","driver_info":"secret version",
                 "max_compute_workgroups_per_dimension":65535,"max_storage_buffer_binding_size":134217728,"score":1,"selected":false},
                {"index":1,"name":"Selected compute GPU","vendor_id":4318,"device_id":2,"vendor":"secret vendor",
                 "device_type":"DiscreteGpu","backend":"Dx12","driver":"secret driver","driver_info":"secret version",
                 "max_compute_workgroups_per_dimension":65535,"max_storage_buffer_binding_size":134217728,"score":2,"selected":true}
            ],
            "scheduler":{"max_parallel_jobs":6,"cpu_worker_threads":22,"gpu_batch_size":1024,"interactive_reserve":1,"rationale":[]},
            "warnings":["Raw inventory is for local diagnostics"]
        })).unwrap();
        let summary = summarize(&profile, 32000000000);
        assert_eq!(summary["cpu_logical_cores"],24);
        assert_eq!(summary["available_memory_bytes"],32000000000_u64);
        assert_eq!(summary["selected_gpu"]["name"],"Selected compute GPU");
        assert_eq!(summary["selected_gpu"]["backend"],"Dx12");
        assert_eq!(summary["selected_gpu"]["max_storage_buffer_binding_size"],134217728);
        assert_eq!(summary["numerical_run_slots"],1);
        assert_eq!(summary["npu_numerical_execution"],false);
        let serialized = summary.to_string();
        for inventory in ["Unused adapter", "secret", "vendor_id", "device_id", "driver", "max_parallel_jobs", "Raw inventory"] {
            assert!(!serialized.contains(inventory), "Unexpected provider inventory: {inventory}");
        }
        let mut cpu_only = profile;
        cpu_only.gpu_available = false;
        assert!(summarize(&cpu_only, 10)["selected_gpu"].is_null());
    }
}
