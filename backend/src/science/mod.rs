pub mod trajectory;
pub mod findings;
pub mod evidence;
pub use evidence::FalsificationResult;
pub mod engines;
pub mod molecular;
pub(crate) mod ode;
mod particles;
pub(crate) mod search;

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Context;
use serde::Serialize;
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    compute::HardwareManager,
    domain::{ComputePreference, ComputeRequest, ExperimentManifest, ModelSpec},
};

pub type ProgressCallback = Arc<dyn Fn(f32, &str) + Send + Sync>;

#[derive(Debug, Clone, Serialize)]
pub struct TimeSeries {
    pub name: String,
    pub unit: String,
    pub points: Vec<[f64; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisualEntity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub radius: Option<f64>,
    pub id: String,
    pub position: [f64; 3],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub velocity: Option<[f64; 3]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scalar: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisualFrame {
    pub time: f64,
    pub entities: Vec<VisualEntity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct VisualizationOutput {
    pub kind: String,
    pub point_size: f32,
    pub trails: bool,
    pub frames: Vec<VisualFrame>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExecutionOutput {
    pub evidence_version: u32,
    pub constraint_results: Vec<evidence::ConstraintResult>,
    pub summary: String,
    pub capability_id: String,
    pub metrics: BTreeMap<String, f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objective_score: Option<f64>,
    pub best_candidate: BTreeMap<String, f64>,
    pub search_history: Vec<search::SearchGeneration>,
    pub series: Vec<TimeSeries>,
    pub visualization: VisualizationOutput,
    pub falsification: Vec<FalsificationResult>,
    pub warnings: Vec<String>,
    pub numerical: BTreeMap<String, Value>,
}

pub struct ExecutionOutcome {
    pub value: Value,
    pub backend_used: String,
}

pub async fn execute(
    manifest: ExperimentManifest,
    compute: ComputeRequest,
    hardware: HardwareManager,
    cancellation: CancellationToken,
    progress: ProgressCallback,
) -> anyhow::Result<ExecutionOutcome> {
    execute_checkpointed(manifest, compute, hardware, cancellation, progress, None).await
}

pub(crate) async fn execute_checkpointed(
    manifest: ExperimentManifest,
    compute: ComputeRequest,
    hardware: HardwareManager,
    cancellation: CancellationToken,
    progress: ProgressCallback,
    checkpoint: Option<crate::compute::recovery::CheckpointStore>,
) -> anyhow::Result<ExecutionOutcome> {
    let capability = manifest.capability_id().to_owned();
    let scene = manifest.visualization.scene.clone();
    let outcome = match &manifest.model {
        ModelSpec::StateVectorOde { .. } => {
            #[cfg(feature = "gpu")]
            let gpu = if compute.preference == ComputePreference::Cpu {
                None
            } else {
                hardware.gpu_ode_evaluator()
            };
            #[cfg(not(feature = "gpu"))]
            let gpu = None;
            ode::execute_checkpointed(manifest, compute, gpu, cancellation, progress, checkpoint).await?
        }
        ModelSpec::PairwiseParticles { .. } => {
            if compute.preference == ComputePreference::Gpu {
                tracing::warn!(
                    "pairwise particle GPU lowering is not complete; using bounded CPU execution"
                );
            }
            let task = tokio::task::spawn_blocking(move || {
                particles::execute_checkpointed(&manifest, &compute, &cancellation, &progress, checkpoint)
            });
            task.await
                .context("pairwise particle worker panicked")??
        }
    };

    let backend_used = outcome.backend_used.clone();
    let mut value = serde_json::to_value(outcome.output)
        .with_context(|| format!("unable to serialize {capability} result"))?;
    if let Some(scene) = scene {
        value["visualization"]["scene"] = scene;
    }
    Ok(ExecutionOutcome {
        value,
        backend_used,
    })
}

pub(crate) struct InternalOutcome {
    pub output: ExecutionOutput,
    pub backend_used: String,
}
