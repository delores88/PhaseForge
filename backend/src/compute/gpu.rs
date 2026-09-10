use std::{
    borrow::Cow,
    collections::HashMap,
    mem::size_of,
    sync::{Arc, atomic::{AtomicBool, Ordering}},
    time::Instant,
};

use anyhow::{Context, bail};
use async_trait::async_trait;
use bytemuck::{Pod, Zeroable};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use wgpu::util::DeviceExt;

use crate::{
    domain::{
        ComputePolicy, ComputeRequest, ExperimentManifest, GpuAdapterInfo, IntegrationMethod,
        ObjectiveGoal,
    },
    science::ode::{
        OdeBatchEvaluator, OdeCandidateInput, OdeProgram, gpu_supported_identifiers,
    },
};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GpuConfig {
    candidate_count: u32,
    padding0: u32,
    padding1: u32,
    padding2: u32,
}

pub(crate) struct GpuOdeEvaluator {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    adapter_info: GpuAdapterInfo,
    recommended_batch_size: usize,
    gate: Mutex<()>,
    pipelines: Mutex<HashMap<String, Arc<wgpu::ComputePipeline>>>,
    healthy: AtomicBool,
    recovery_notes: parking_lot::Mutex<Vec<String>>,
}

impl GpuOdeEvaluator {
    pub(crate) async fn new(
        adapter: wgpu::Adapter,
        adapter_info: GpuAdapterInfo,
        recommended_batch_size: usize,
    ) -> anyhow::Result<Self> {
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("PhaseForge generic compute device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults(),
                },
                None,
            )
            .await
            .context("unable to request a wgpu compute device")?;
        Ok(Self {
            device: Arc::new(device),
            queue: Arc::new(queue),
            adapter_info,
            recommended_batch_size,
            gate: Mutex::new(()),
            pipelines: Mutex::new(HashMap::new()),
            healthy: AtomicBool::new(true),
            recovery_notes: parking_lot::Mutex::new(Vec::new()),
        })
    }

    async fn pipeline_for(
        &self,
        source: &str,
    ) -> anyhow::Result<Arc<wgpu::ComputePipeline>> {
        let key = format!("{:x}", Sha256::digest(source.as_bytes()));
        if let Some(pipeline) = self.pipelines.lock().await.get(&key).cloned() {
            return Ok(pipeline);
        }

        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("PhaseForge generated state-vector shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(source.to_owned())),
        });
        let pipeline = self.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("PhaseForge generated state-vector pipeline"),
            layout: None,
            module: &shader,
            entry_point: "main",
        });
        let validation = self.device.pop_error_scope().await;
        let allocation = self.device.pop_error_scope().await;
        if let Some(error) = allocation {
            return Err(super::recovery::ResourcePressure(format!("GPU pipeline allocation failed: {error}")).into());
        }
        if let Some(error) = validation {
            bail!("generated WGSL validation failed: {error}");
        }
        let pipeline = Arc::new(pipeline);
        let mut pipelines = self.pipelines.lock().await;
        if pipelines.len() >= 32 { pipelines.clear(); }
        pipelines.insert(key, Arc::clone(&pipeline));
        Ok(pipeline)
    }

    async fn evaluate_chunk(
        &self,
        pipeline: &wgpu::ComputePipeline,
        candidates: &[OdeCandidateInput],
    ) -> anyhow::Result<Vec<f64>> {
        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let result = self.evaluate_chunk_inner(pipeline, candidates).await;
        let validation = self.device.pop_error_scope().await;
        let allocation = self.device.pop_error_scope().await;
        if let Some(error) = allocation {
            return Err(super::recovery::ResourcePressure(format!("GPU buffer allocation failed: {error}")).into());
        }
        if let Some(error) = validation { bail!("GPU dispatch validation failed: {error}"); }
        result
    }

    async fn evaluate_chunk_inner(
        &self,
        pipeline: &wgpu::ComputePipeline,
        candidates: &[OdeCandidateInput],
    ) -> anyhow::Result<Vec<f64>> {
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        let stride = candidates[0].state.len() + candidates[0].constants.len();
        let mut flat = Vec::<f32>::new();
        flat.try_reserve_exact(candidates.len().saturating_mul(stride))
            .map_err(|e| super::recovery::ResourcePressure(format!("GPU staging allocation failed: {e}")))?;
        for candidate in candidates {
            if candidate.state.len() + candidate.constants.len() != stride {
                bail!("candidate input stride changed inside a GPU batch");
            }
            flat.extend(candidate.state.iter().map(|value| *value as f32));
            flat.extend(candidate.constants.iter().map(|value| *value as f32));
        }

        let input = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PhaseForge state-vector candidates"),
            contents: bytemuck::cast_slice(&flat),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let output_size = (candidates.len() * size_of::<f32>()) as u64;
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("PhaseForge state-vector scores"),
            size: output_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("PhaseForge state-vector score readback"),
            size: output_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let config = GpuConfig {
            candidate_count: candidates.len() as u32,
            padding0: 0,
            padding1: 0,
            padding2: 0,
        };
        let uniform = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("PhaseForge state-vector batch configuration"),
            contents: bytemuck::bytes_of(&config),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let layout = pipeline.get_bind_group_layout(0);
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("PhaseForge state-vector bindings"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("PhaseForge state-vector encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("PhaseForge state-vector pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(((candidates.len() as u32) + 63) / 64, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_size);
        self.queue.submit(Some(encoder.finish()));

        let slice = readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let readback_started = Instant::now();
        loop {
            self.device.poll(wgpu::Maintain::Poll);
            match receiver.try_recv() {
                Ok(result) => { result.context("GPU readback mapping failed")?; break; }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => bail!("GPU readback callback was dropped"),
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
            if readback_started.elapsed() > std::time::Duration::from_secs(30) {
                self.healthy.store(false, Ordering::Relaxed);
                bail!("GPU readback exceeded 30 seconds; this device is disabled for further numerical dispatches until restart");
            }
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        let mapped = slice.get_mapped_range();
        let values = bytemuck::cast_slice::<u8, f32>(&mapped)
            .iter()
            .map(|value| {
                let score = *value as f64;
                if score.is_finite() { score } else { f64::INFINITY }
            })
            .collect::<Vec<_>>();
        drop(mapped);
        readback.unmap();
        Ok(values)
    }
}

#[async_trait]
impl OdeBatchEvaluator for GpuOdeEvaluator {
    fn take_execution_notes(&self) -> Vec<String> {
        std::mem::take(&mut *self.recovery_notes.lock())
    }
    fn label(&self) -> String {
        format!(
            "{} / {} via wgpu",
            self.adapter_info.name, self.adapter_info.backend
        )
    }

    fn compatibility(
        &self,
        program: &OdeProgram,
        manifest: &ExperimentManifest,
    ) -> anyhow::Result<()> {
        if !self.healthy.load(Ordering::Relaxed) { bail!("GPU device was disabled after a dispatch timeout; CPU execution is available"); }
        if !manifest.trajectory.is_empty() {bail!("trajectory reductions require CPU/f64 scoring; GPU lowering must not omit them");}
        if program.state_names.len() > 24 || program.constant_names.len() > 32 {
            bail!("state or constant dimensions exceed the generic GPU lowerer limits");
        }
        if !matches!(
            manifest.integration.method,
            IntegrationMethod::Euler | IntegrationMethod::Rk4
        ) {
            bail!("integration method cannot be lowered to the state-vector GPU runtime");
        }
        if !program.constraints.is_empty() {
            bail!("constraints currently require CPU scoring");
        }
        if program.objectives.is_empty() {
            bail!("GPU candidate scoring requires at least one objective");
        }
        let steps = ((manifest.integration.end_time - manifest.integration.start_time)
            / manifest.integration.time_step)
            .ceil()
            .max(1.0) as usize;
        if steps > 500_000 {
            bail!("generated GPU integration exceeds the 500,000-step safety limit");
        }
        let allowed = gpu_supported_identifiers(program);
        for derivative in &program.derivatives {
            let unknown = derivative
                .identifiers()
                .into_iter()
                .filter(|name| !allowed.contains(name))
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                bail!("derivative references unsupported GPU identifiers: {}", unknown.join(", "));
            }
        }
        for objective in &program.objectives {
            let unknown = objective
                .expression
                .identifiers()
                .into_iter()
                .filter(|name| !allowed.contains(name))
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                bail!("objective requires CPU-only aggregate identifiers: {}", unknown.join(", "));
            }
        }
        let _ = generate_shader(program, manifest)?;
        Ok(())
    }

    async fn evaluate(
        &self,
        program: &OdeProgram,
        manifest: &ExperimentManifest,
        candidates: &[OdeCandidateInput],
        compute: &ComputeRequest,
        cancellation: &CancellationToken,
    ) -> anyhow::Result<Vec<f64>> {
        let _guard = self.gate.lock().await;
        self.recovery_notes.lock().clear();
        if !self.healthy.load(Ordering::Relaxed) { bail!("GPU device is unavailable after a dispatch timeout"); }
        if candidates.is_empty() {
            return Ok(Vec::new());
        }
        if cancellation.is_cancelled() {
            bail!("run cancelled");
        }
        let source = generate_shader(program, manifest)?;
        let pipeline = self.pipeline_for(&source).await?;

        let policy_batch = match compute.policy {
            ComputePolicy::Interactive => (self.recommended_batch_size / 4).max(64),
            ComputePolicy::Balanced => self.recommended_batch_size,
            ComputePolicy::Throughput => self.recommended_batch_size.saturating_mul(2),
        };
        let limits = self.device.limits();
        let stride_bytes = (candidates[0].state.len() + candidates[0].constants.len()).max(1) * size_of::<f32>();
        let hard_limit = self.recommended_batch_size.saturating_mul(4).max(1)
            .min(limits.max_storage_buffer_binding_size as usize / stride_bytes)
            .min(limits.max_buffer_size as usize / stride_bytes)
            .min(limits.max_compute_workgroups_per_dimension as usize * 64).max(1);
        let configured_batch = if compute.batch_size == 0 {
            policy_batch.min(hard_limit)
        } else {
            compute.batch_size.clamp(1, hard_limit)
        };

        let mut scores = Vec::with_capacity(candidates.len());
        let mut offset = 0usize;
        let mut batch_size = configured_batch;
        let mut retries = 0;
        while offset < candidates.len() {
            if cancellation.is_cancelled() {
                bail!("run cancelled");
            }
            let end = (offset + batch_size).min(candidates.len());
            let started = Instant::now();
            match self.evaluate_chunk(&pipeline, &candidates[offset..end]).await {
                Ok(values) => { scores.extend(values); offset = end; }
                Err(error) if error.downcast_ref::<super::recovery::ResourcePressure>().is_some() && batch_size > 1 && retries < 4 => {
                    batch_size = (batch_size / 2).max(1);
                    retries += 1;
                    self.recovery_notes.lock().push(format!("GPU allocation pressure: resumed the uncompleted candidate batch with {batch_size} candidates (retry {retries}/4)."));
                    continue;
                }
                Err(error) => return Err(error),
            }
            // Adapt down even for an admitted allocation. Never grow beyond the
            // approved allocation, and keep laptop/desktop watchdog headroom.
            let target_ms = match compute.policy {
                ComputePolicy::Interactive => 140.0,
                ComputePolicy::Balanced => 350.0,
                ComputePolicy::Throughput => 700.0,
            };
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            if elapsed_ms > target_ms && batch_size > 1 {
                batch_size = ((batch_size as f64 * target_ms / elapsed_ms) as usize).max(1);
            }
        }
        Ok(scores)
    }
}

fn generate_shader(
    program: &OdeProgram,
    manifest: &ExperimentManifest,
) -> anyhow::Result<String> {
    let state_count = program.state_names.len();
    let constant_count = program.constant_names.len().max(1);
    let stride = state_count + program.constant_names.len();
    let steps = ((manifest.integration.end_time - manifest.integration.start_time)
        / manifest.integration.time_step)
        .ceil()
        .max(1.0) as u32;
    let state_index = program
        .state_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let constant_index = program
        .constant_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index))
        .collect::<HashMap<_, _>>();

    let derivative_resolver = |name: &str| -> Option<String> {
        if name == "t" {
            Some("t".to_owned())
        } else if let Some(index) = state_index.get(name) {
            Some(format!("state[{index}]"))
        } else {
            constant_index
                .get(name)
                .map(|index| format!("constants[{index}]"))
        }
    };
    let objective_resolver = |name: &str| -> Option<String> {
        if name == "t" {
            return Some("t".to_owned());
        }
        if let Some(index) = state_index.get(name) {
            return Some(format!("state[{index}]"));
        }
        if let Some(name) = name.strip_prefix("final_") {
            return state_index.get(name).map(|index| format!("state[{index}]"));
        }
        if let Some(name) = name.strip_prefix("initial_") {
            return state_index
                .get(name)
                .map(|index| format!("initial_state[{index}]"));
        }
        if let Some(name) = name.strip_prefix("delta_") {
            return state_index.get(name).map(|index| {
                format!("(state[{index}] - initial_state[{index}])")
            });
        }
        constant_index
            .get(name)
            .map(|index| format!("constants[{index}]"))
    };

    let mut functions = String::new();
    for (index, expression) in program.derivatives.iter().enumerate() {
        functions.push_str(&format!(
            "fn derivative_{index}(state: array<f32, {state_count}>, constants: array<f32, {constant_count}>, t: f32) -> f32 {{\n    return {};\n}}\n\n",
            expression.to_wgsl(&derivative_resolver)?
        ));
    }

    let mut load = String::new();
    for index in 0..state_count {
        load.push_str(&format!(
            "    state[{index}] = candidates[base + {index}u];\n    initial_state[{index}] = state[{index}];\n"
        ));
    }
    for index in 0..program.constant_names.len() {
        load.push_str(&format!(
            "    constants[{index}] = candidates[base + {}u];\n",
            state_count + index
        ));
    }
    if program.constant_names.is_empty() {
        load.push_str("    constants[0] = 0.0;\n");
    }

    let mut step_body = String::new();
    match manifest.integration.method {
        IntegrationMethod::Euler => {
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        k1[{index}] = derivative_{index}(state, constants, t);\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        state[{index}] = state[{index}] + step_dt * k1[{index}];\n"
                ));
            }
        }
        IntegrationMethod::Rk4 => {
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        k1[{index}] = derivative_{index}(state, constants, t);\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        temporary[{index}] = state[{index}] + 0.5 * step_dt * k1[{index}];\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        k2[{index}] = derivative_{index}(temporary, constants, t + 0.5 * step_dt);\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        temporary[{index}] = state[{index}] + 0.5 * step_dt * k2[{index}];\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        k3[{index}] = derivative_{index}(temporary, constants, t + 0.5 * step_dt);\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        temporary[{index}] = state[{index}] + step_dt * k3[{index}];\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        k4[{index}] = derivative_{index}(temporary, constants, t + step_dt);\n"
                ));
            }
            for index in 0..state_count {
                step_body.push_str(&format!(
                    "        state[{index}] = state[{index}] + step_dt * (k1[{index}] + 2.0 * k2[{index}] + 2.0 * k3[{index}] + k4[{index}]) / 6.0;\n"
                ));
            }
        }
        IntegrationMethod::VelocityVerlet => {
            bail!("velocity-Verlet cannot be generated for state-vector ODEs")
        }
    }

    let mut objective = String::from("    var score: f32 = 0.0;\n");
    for compiled in &program.objectives {
        let expression = compiled.expression.to_wgsl(&objective_resolver)?;
        let sign = match compiled.goal {
            ObjectiveGoal::Minimize => 1.0,
            ObjectiveGoal::Maximize => -1.0,
        };
        objective.push_str(&format!(
            "    score = score + {} * ({});\n",
            wgsl_number(compiled.weight * sign),
            expression
        ));
    }
    objective.push_str("    scores[index] = score;\n");

    Ok(format!(
        r#"struct Config {{
    candidate_count: u32,
    padding0: u32,
    padding1: u32,
    padding2: u32,
}}

@group(0) @binding(0) var<storage, read> candidates: array<f32>;
@group(0) @binding(1) var<storage, read_write> scores: array<f32>;
@group(0) @binding(2) var<uniform> config: Config;

{functions}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {{
    let index = invocation.x;
    if (index >= config.candidate_count) {{
        return;
    }}
    let base = index * {stride}u;
    var state: array<f32, {state_count}>;
    var initial_state: array<f32, {state_count}>;
    var constants: array<f32, {constant_count}>;
    var k1: array<f32, {state_count}>;
    var k2: array<f32, {state_count}>;
    var k3: array<f32, {state_count}>;
    var k4: array<f32, {state_count}>;
    var temporary: array<f32, {state_count}>;
{load}
    var t: f32 = {start_time};
    for (var step: u32 = 0u; step < {steps}u; step = step + 1u) {{
        let step_dt = min({time_step}, {end_time} - t);
{step_body}
        t = t + step_dt;
    }}
{objective}
}}
"#,
        start_time = wgsl_number(manifest.integration.start_time),
        end_time = wgsl_number(manifest.integration.end_time),
        time_step = wgsl_number(manifest.integration.time_step),
    ))
}

fn wgsl_number(value: f64) -> String {
    let mut rendered = format!("{value:.12}");
    while rendered.contains('.') && rendered.ends_with('0') {
        rendered.pop();
    }
    if rendered.ends_with('.') {
        rendered.push('0');
    }
    if !rendered.contains('.') && !rendered.contains('e') && !rendered.contains('E') {
        rendered.push_str(".0");
    }
    rendered
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::{compute::HardwareManager, config::AppConfig, domain::ExperimentManifestDraft};

    /// Explicit hardware validation, not a CI assumption about adapter availability.
    #[tokio::test]
    #[ignore = "requires an initialized hardware GPU; run explicitly on the target machine"]
    async fn gpu_one_candidate_batches_match_analytic_decay() {
        let hardware = HardwareManager::discover(&AppConfig::default()).await.unwrap();
        let evaluator = hardware.gpu_ode_evaluator().expect("hardware GPU required for this smoke test");
        let mut draft: ExperimentManifestDraft = serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
        draft.search.objectives = serde_json::from_value(serde_json::json!([{"name":"endpoint","expression":"x*x","goal":"minimize","weight":1.0}])).unwrap();
        let manifest = ExperimentManifest::from_draft(uuid::Uuid::nil(), None, 1, draft, "analytic GPU verification");
        let program = crate::science::ode::compile_program(&manifest).unwrap();
        evaluator.compatibility(&program, &manifest).unwrap();
        let candidates = [0.5, 1.0, 2.0, 3.0].into_iter().map(|x| OdeCandidateInput { state: vec![x], constants: vec![] }).collect::<Vec<_>>();
        let mut compute = manifest.compute.clone();
        compute.batch_size = 1;
        let scores = evaluator.evaluate(&program, &manifest, &candidates, &compute, &CancellationToken::new()).await.unwrap();
        assert_eq!(scores.len(), candidates.len());
        for (score, input) in scores.iter().zip(candidates) {
            let analytic = input.state[0].powi(2) * (-2.0_f64).exp();
            assert!((score - analytic).abs() < 2e-5, "GPU {score} vs analytic {analytic}");
        }
        let cancelled = CancellationToken::new(); cancelled.cancel();
        assert!(evaluator.evaluate(&program, &manifest, &[OdeCandidateInput{state:vec![1.0],constants:vec![]}], &compute, &cancelled).await.is_err());
        println!("Verified numerical GPU execution on {}", evaluator.label());
    }
}
