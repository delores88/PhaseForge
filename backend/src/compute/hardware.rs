use std::sync::Arc;

use sysinfo::System;

use crate::{
    config::AppConfig,
    domain::{CpuInfo, GpuAdapterInfo, HardwareProfile, SchedulerProfile},
};

#[cfg(feature = "gpu")]
use super::gpu::GpuOdeEvaluator;

#[derive(Clone)]
pub struct HardwareManager {
    profile: HardwareProfile,
    neural_accelerators: serde_json::Value,
    #[cfg(feature = "gpu")]
    gpu_ode_evaluator: Option<Arc<GpuOdeEvaluator>>,
}

impl HardwareManager {
    pub async fn discover(config: &AppConfig) -> anyhow::Result<Self> {
        let neural_accelerators = super::accelerators::neural_inventory().await;
        let mut system = System::new_all();
        system.refresh_all();

        let logical_cores = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1);
        let cpu_brand = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown CPU".to_owned());

        let cpu = CpuInfo {
            brand: cpu_brand,
            logical_cores,
            total_memory_bytes: system.total_memory(),
        };

        let mut warnings = Vec::new();
        if neural_accelerators["devices"].as_array().map(|v| !v.is_empty()).unwrap_or(false) {
            warnings.push("The OS reports a neural/AI accelerator. No compatible NPU numerical solver is installed, so it is not counted as simulation compute.".to_owned());
        }
        let mut adapter_infos = Vec::new();
        let mut selected_gpu_index = None;

        #[cfg(feature = "gpu")]
        let mut gpu_ode_evaluator = None;

        #[cfg(feature = "gpu")]
        if config.gpu_enabled {
            let instance = wgpu::Instance::default();
            let mut adapters = instance.enumerate_adapters(wgpu::Backends::all());

            for (index, adapter) in adapters.iter().enumerate() {
                adapter_infos.push(describe_adapter(index, adapter));
            }

            if let Some(index) = select_adapter(&adapter_infos, &config.gpu_preference) {
                let selected_info = adapter_infos[index].clone();
                if let Some(info) = adapter_infos.get_mut(index) {
                    info.selected = true;
                }

                let adapter = adapters.swap_remove(index);
                match GpuOdeEvaluator::new(
                    adapter,
                    selected_info.clone(),
                    recommended_gpu_batch(&selected_info),
                )
                .await
                {
                    Ok(evaluator) => {
                        selected_gpu_index = Some(index);
                        gpu_ode_evaluator = Some(Arc::new(evaluator));
                    }
                    Err(error) => {
                        warnings.push(format!(
                            "The selected GPU was detected but compute initialization failed: {error:#}. CPU fallback remains enabled."
                        ));
                        if let Some(info) = adapter_infos.get_mut(index) {
                            info.selected = false;
                        }
                    }
                }
            } else {
                warnings.push(
                    "No hardware GPU adapter met the compute selection policy; PhaseForge will use CPU workers."
                        .to_owned(),
                );
            }
        }

        #[cfg(not(feature = "gpu"))]
        warnings.push(
            "The backend was compiled without the `gpu` feature; CPU execution is active.".to_owned(),
        );

        if !config.gpu_enabled {
            warnings.push("GPU execution is disabled by configuration.".to_owned());
        }

        let selected_info = selected_gpu_index.and_then(|index| adapter_infos.get(index));
        let max_parallel_jobs = if config.max_parallel_jobs > 0 {
            config.max_parallel_jobs.clamp(1, 64)
        } else {
            (logical_cores.saturating_sub(2) / 3).clamp(1, 8)
        };
        let gpu_batch_size = selected_info.map(recommended_gpu_batch).unwrap_or(0);

        let mut rationale = vec![
            format!(
                "{} logical CPU cores detected; {} retained for OS/UI responsiveness.",
                logical_cores,
                logical_cores.min(2)
            ),
            format!(
                "The dispatcher will allow up to {max_parallel_jobs} active research jobs."
            ),
        ];
        if let Some(info) = selected_info {
            rationale.push(format!(
                "{} through {} is selected for compatible batched compute; GPU dispatches are serialized to avoid memory oversubscription.",
                info.name, info.backend
            ));
            rationale.push(format!(
                "Auto GPU batches begin near {gpu_batch_size} candidates and adapt from measured dispatch time."
            ));
        } else {
            rationale.push(
                "Compatible experiments transparently fall back to bounded Rayon CPU batches."
                    .to_owned(),
            );
        }

        let profile = HardwareProfile {
            operating_system: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            cpu,
            gpu_available: selected_gpu_index.is_some(),
            selected_gpu_index,
            adapters: adapter_infos,
            scheduler: SchedulerProfile {
                max_parallel_jobs,
                cpu_worker_threads: logical_cores.saturating_sub(2).max(1),
                gpu_batch_size,
                interactive_reserve: 1,
                rationale,
            },
            warnings,
        };

        Ok(Self {
            profile,
            neural_accelerators,
            #[cfg(feature = "gpu")]
            gpu_ode_evaluator,
        })
    }

    pub fn profile(&self) -> &HardwareProfile {
        &self.profile
    }

    pub fn neural_accelerators(&self) -> &serde_json::Value { &self.neural_accelerators }

    #[cfg(feature = "gpu")]
    pub(crate) fn gpu_ode_evaluator(
        &self,
    ) -> Option<Arc<dyn crate::science::ode::OdeBatchEvaluator>> {
        self.gpu_ode_evaluator.as_ref().map(|value| {
            Arc::clone(value) as Arc<dyn crate::science::ode::OdeBatchEvaluator>
        })
    }

    #[cfg(not(feature = "gpu"))]
    pub(crate) fn gpu_ode_evaluator(
        &self,
    ) -> Option<Arc<dyn crate::science::ode::OdeBatchEvaluator>> {
        None
    }
}

#[cfg(feature = "gpu")]
fn describe_adapter(index: usize, adapter: &wgpu::Adapter) -> GpuAdapterInfo {
    let info = adapter.get_info();
    let limits = adapter.limits();
    let score = adapter_score(&info);
    GpuAdapterInfo {
        index,
        name: info.name,
        vendor_id: info.vendor,
        device_id: info.device,
        vendor: vendor_name(info.vendor).to_owned(),
        device_type: format!("{:?}", info.device_type),
        backend: format!("{:?}", info.backend),
        driver: info.driver,
        driver_info: info.driver_info,
        max_compute_workgroups_per_dimension: limits.max_compute_workgroups_per_dimension,
        max_storage_buffer_binding_size: limits.max_storage_buffer_binding_size,
        score,
        selected: false,
    }
}

#[cfg(feature = "gpu")]
fn adapter_score(info: &wgpu::AdapterInfo) -> i32 {
    let type_score = match info.device_type {
        wgpu::DeviceType::DiscreteGpu => 1000,
        wgpu::DeviceType::IntegratedGpu => 650,
        wgpu::DeviceType::VirtualGpu => 300,
        wgpu::DeviceType::Cpu => 25,
        _ => 100,
    };
    let backend_score = match info.backend {
        #[cfg(target_os = "windows")]
        wgpu::Backend::Dx12 => 220,
        wgpu::Backend::Vulkan => 190,
        wgpu::Backend::Metal => 180,
        wgpu::Backend::Gl => 50,
        _ => 0,
    };
    let vendor_score = match info.vendor {
        0x10DE => 30,
        0x1002 | 0x1022 => 25,
        0x8086 => 15,
        _ => 0,
    };
    type_score + backend_score + vendor_score
}

#[cfg(feature = "gpu")]
fn select_adapter(adapters: &[GpuAdapterInfo], preference: &str) -> Option<usize> {
    let preference = preference.trim().to_ascii_lowercase();
    if let Ok(index) = preference.parse::<usize>() {
        if adapters
            .get(index)
            .map(|adapter| !adapter.device_type.eq_ignore_ascii_case("Cpu"))
            .unwrap_or(false)
        {
            return Some(index);
        }
    }

    adapters
        .iter()
        .filter(|adapter| {
            if adapter.device_type.eq_ignore_ascii_case("Cpu") {
                return false;
            }
            preference.is_empty()
                || preference == "auto"
                || adapter.name.to_ascii_lowercase().contains(&preference)
                || adapter.vendor.to_ascii_lowercase().contains(&preference)
        })
        .max_by_key(|adapter| adapter.score)
        .map(|adapter| adapter.index)
}

fn recommended_gpu_batch(adapter: &GpuAdapterInfo) -> usize {
    let base = if adapter.device_type.contains("Discrete") {
        8192
    } else if adapter.device_type.contains("Integrated") {
        2048
    } else {
        512
    };
    let limit_based = (adapter.max_storage_buffer_binding_size as usize / 256).max(64);
    base.min(limit_based).max(64)
}

fn vendor_name(vendor_id: u32) -> &'static str {
    match vendor_id {
        0x10DE => "NVIDIA",
        0x1002 | 0x1022 => "AMD",
        0x8086 => "Intel",
        0x106B => "Apple",
        0x1414 => "Microsoft",
        _ => "Other",
    }
}

#[cfg(all(test, feature = "gpu"))]
mod tests {
    use super::*;

    fn adapter(index: usize, name: &str, vendor: &str, device_type: &str, score: i32) -> GpuAdapterInfo {
        GpuAdapterInfo {
            index,
            name: name.to_owned(),
            vendor_id: 0,
            device_id: 0,
            vendor: vendor.to_owned(),
            device_type: device_type.to_owned(),
            backend: "Dx12".to_owned(),
            driver: String::new(),
            driver_info: String::new(),
            max_compute_workgroups_per_dimension: 65_535,
            max_storage_buffer_binding_size: 134_217_728,
            score,
            selected: false,
        }
    }

    #[test]
    fn automatic_selection_prefers_highest_scored_hardware_gpu() {
        let adapters = vec![
            adapter(0, "Software Adapter", "Microsoft", "Cpu", 5_000),
            adapter(1, "Integrated", "Intel", "IntegratedGpu", 800),
            adapter(2, "Discrete", "NVIDIA", "DiscreteGpu", 1_200),
        ];
        assert_eq!(select_adapter(&adapters, "auto"), Some(2));
    }

    #[test]
    fn vendor_preference_filters_before_scoring() {
        let adapters = vec![
            adapter(0, "Fast NVIDIA", "NVIDIA", "DiscreteGpu", 1_300),
            adapter(1, "AMD Radeon", "AMD", "DiscreteGpu", 1_200),
        ];
        assert_eq!(select_adapter(&adapters, "amd"), Some(1));
    }
}
