use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComputePreference {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComputePolicy {
    Interactive,
    #[default]
    Balanced,
    Throughput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunPriority {
    Interactive,
    #[default]
    Research,
    Validation,
    Background,
}

impl RunPriority {
    pub fn queue_weight(self) -> u8 {
        match self {
            Self::Interactive => 4,
            Self::Validation => 3,
            Self::Research => 2,
            Self::Background => 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ComputeRequest {
    pub preference: ComputePreference,
    pub policy: ComputePolicy,
    pub candidate_count: usize,
    pub batch_size: usize,
    pub max_wall_seconds: u64,
    pub max_memory_mb: usize,
}

impl Default for ComputeRequest {
    fn default() -> Self {
        Self {
            preference: ComputePreference::Auto,
            policy: ComputePolicy::Balanced,
            candidate_count: 256,
            batch_size: 0,
            max_wall_seconds: 900,
            max_memory_mb: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub brand: String,
    pub logical_cores: usize,
    pub total_memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuAdapterInfo {
    pub index: usize,
    pub name: String,
    pub vendor_id: u32,
    pub device_id: u32,
    pub vendor: String,
    pub device_type: String,
    pub backend: String,
    pub driver: String,
    pub driver_info: String,
    pub max_compute_workgroups_per_dimension: u32,
    pub max_storage_buffer_binding_size: u32,
    pub score: i32,
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerProfile {
    pub max_parallel_jobs: usize,
    pub cpu_worker_threads: usize,
    pub gpu_batch_size: usize,
    pub interactive_reserve: usize,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareProfile {
    pub operating_system: String,
    pub architecture: String,
    pub cpu: CpuInfo,
    pub gpu_available: bool,
    pub selected_gpu_index: Option<usize>,
    pub adapters: Vec<GpuAdapterInfo>,
    pub scheduler: SchedulerProfile,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Draft,
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProject {
    pub id: Uuid,
    pub name: String,
    pub question: String,
    pub status: ProjectStatus,
    #[serde(default)]
    pub active_manifest_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    pub question: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub status: Option<ProjectStatus>,
}

impl ResearchProject {
    pub fn new(request: CreateProjectRequest) -> Self {
        let now = Utc::now();
        let question = request.question.trim().to_owned();
        let derived_name = question
            .trim_end_matches(|character: char| matches!(character, '?' | '.' | '!'))
            .chars()
            .take(72)
            .collect::<String>();
        let name = request
            .name
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                if derived_name.is_empty() {
                    "Untitled research world".to_owned()
                } else {
                    derived_name
                }
            });
        Self {
            id: Uuid::new_v4(),
            name,
            question,
            status: ProjectStatus::Active,
            active_manifest_id: None,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestStatus {
    Draft,
    Ready,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationMethod {
    Euler,
    Rk4,
    VelocityVerlet,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationSpec {
    pub method: IntegrationMethod,
    pub start_time: f64,
    pub end_time: f64,
    pub time_step: f64,
    pub output_stride: usize,
    pub max_steps: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateVariableSpec {
    pub name: String,
    #[serde(default)]
    pub unit: String,
    pub initial: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivativeSpec {
    pub variable: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstantSpec {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DistributionSpec {
    Explicit { values: Vec<Vec<f64>> },
    Uniform { min: Vec<f64>, max: Vec<f64> },
    Normal { mean: Vec<f64>, std_dev: Vec<f64> },
    Grid {
        extent: Vec<usize>,
        spacing: f64,
        jitter: f64,
    },
    Sphere { radius: f64, thickness: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticlePopulationSpec {
    pub count: usize,
    pub mass: f64,
    pub position: DistributionSpec,
    pub velocity: DistributionSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairInteractionSpec {
    pub radial_force: String,
    #[serde(default)]
    pub cutoff: Option<f64>,
    #[serde(default)]
    pub softening: f64,
    #[serde(default)]
    pub linear_damping: f64,
    #[serde(default)]
    pub external_acceleration: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BoundarySpec {
    Open,
    Periodic { min: Vec<f64>, max: Vec<f64> },
    Reflective { min: Vec<f64>, max: Vec<f64> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelSpec {
    StateVectorOde {
        variables: Vec<StateVariableSpec>,
        derivatives: Vec<DerivativeSpec>,
    },
    PairwiseParticles {
        dimensions: usize,
        population: ParticlePopulationSpec,
        interaction: PairInteractionSpec,
        boundary: BoundarySpec,
    },
}

impl ModelSpec {
    pub fn capability_id(&self) -> &'static str {
        match self {
            Self::StateVectorOde { .. } => "state_vector_ode",
            Self::PairwiseParticles { .. } => "pairwise_particles",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchAlgorithm {
    None,
    Random,
    Evolutionary,
    LatinHypercube,
    DifferentialEvolution,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchVariableSpec {
    pub name: String,
    pub target: String,
    pub minimum: f64,
    pub maximum: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObjectiveGoal {
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveSpec {
    pub name: String,
    pub expression: String,
    pub goal: ObjectiveGoal,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSpec {
    pub enabled: bool,
    pub algorithm: SearchAlgorithm,
    #[serde(default)]
    pub variables: Vec<SearchVariableSpec>,
    #[serde(default)]
    pub objectives: Vec<ObjectiveSpec>,
    pub population: usize,
    pub generations: usize,
    pub elite_fraction: f64,
    pub mutation_scale: f64,
    pub seed: u64,
}

impl Default for SearchSpec {
    fn default() -> Self {
        Self {
            enabled: false,
            algorithm: SearchAlgorithm::None,
            variables: Vec::new(),
            objectives: Vec::new(),
            population: 1,
            generations: 1,
            elite_fraction: 0.15,
            mutation_scale: 0.12,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservableSpec {
    pub name: String,
    pub expression: String,
    #[serde(default)]
    pub unit: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintSpec {
    pub name: String,
    pub expression: String,
    #[serde(default)]
    pub tolerance: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VisualizationKind {
    Trajectory,
    ParticleCloud,
    Scatter,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualMapping {
    /// Nominal sphere radius in model units; this does not implement contact physics.
    #[serde(default, skip_serializing_if = "String::is_empty")] pub radius: String,
    pub id: String,
    pub x: String,
    pub y: String,
    #[serde(default)] pub z: String,
    #[serde(default)] pub vx: String,
    #[serde(default)] pub vy: String,
    #[serde(default)] pub vz: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualizationSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<serde_json::Value>,
    #[serde(default)]
    pub entities: Vec<VisualMapping>,
    pub kind: VisualizationKind,
    #[serde(default)]
    pub x: String,
    #[serde(default)]
    pub y: String,
    #[serde(default)]
    pub z: String,
    #[serde(default = "default_point_size")]
    pub point_size: f32,
    #[serde(default = "default_max_frames")]
    pub max_frames: usize,
    #[serde(default = "default_true")]
    pub trails: bool,
}

fn default_point_size() -> f32 { 0.08 }
fn default_max_frames() -> usize { 1200 }
fn default_true() -> bool { true }

impl Default for VisualizationSpec {
    fn default() -> Self {
        Self {
            scene: None,
            kind: VisualizationKind::Trajectory,
            entities: Vec::new(),
            x: String::new(),
            y: String::new(),
            z: String::new(),
            point_size: default_point_size(),
            max_frames: default_max_frames(),
            trails: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FalsificationKind {
    ResolutionLadder,
    StepHalving,
    SeedReplication,
    InitialPerturbation,
    ParameterPerturbation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeCheck {
    pub metric: String,
    /// stable, change, decrease, or increase. Thresholds are authored, never inferred from score.
    pub expectation: String,
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FalsificationSpec {
    #[serde(default)]
    pub checks: Vec<ChallengeCheck>,
    pub name: String,
    pub kind: FalsificationKind,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub magnitude: f64,
    #[serde(default = "default_repetitions")]
    pub repetitions: usize,
}

fn default_repetitions() -> usize { 3 }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrajectoryReducer {
    Minimum, Maximum, Range, TimeMean, Integral, DurationBelow, DurationAbove,
    EntriesBelow, EntriesAbove, FirstBelow, FirstAbove,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrajectoryMeasure {
    pub name: String, pub expression: String, pub reducer: TrajectoryReducer,
    #[serde(default)] pub unit: String,
    #[serde(default)] pub threshold: f64,
    #[serde(default)] pub hysteresis: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentManifestDraft {
    pub title: String,
    pub question: String,
    pub scientific_boundary: String,
    pub hypothesis: String,
    pub model: ModelSpec,
    #[serde(default)]
    pub constants: Vec<ConstantSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trajectory: Vec<TrajectoryMeasure>,
    pub integration: IntegrationSpec,
    #[serde(default)]
    pub search: SearchSpec,
    #[serde(default)]
    pub observables: Vec<ObservableSpec>,
    #[serde(default)]
    pub constraints: Vec<ConstraintSpec>,
    #[serde(default)]
    pub visualization: VisualizationSpec,
    #[serde(default)]
    pub falsification: Vec<FalsificationSpec>,
    #[serde(default)]
    pub compute: ComputeRequest,
    #[serde(default)]
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentManifest {
    pub id: Uuid,
    pub project_id: Uuid,
    #[serde(default)]
    pub parent_manifest_id: Option<Uuid>,
    pub revision: u32,
    pub status: ManifestStatus,
    pub title: String,
    pub question: String,
    pub scientific_boundary: String,
    pub hypothesis: String,
    pub model: ModelSpec,
    pub constants: Vec<ConstantSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub trajectory: Vec<TrajectoryMeasure>,
    pub integration: IntegrationSpec,
    pub search: SearchSpec,
    pub observables: Vec<ObservableSpec>,
    pub constraints: Vec<ConstraintSpec>,
    pub visualization: VisualizationSpec,
    pub falsification: Vec<FalsificationSpec>,
    pub compute: ComputeRequest,
    pub limitations: Vec<String>,
    pub authored_by: String,
    pub created_at: DateTime<Utc>,
}

impl ExperimentManifest {
    pub fn from_draft(
        project_id: Uuid,
        parent_manifest_id: Option<Uuid>,
        revision: u32,
        draft: ExperimentManifestDraft,
        authored_by: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            parent_manifest_id,
            revision,
            status: ManifestStatus::Ready,
            title: draft.title.trim().to_owned(),
            question: draft.question.trim().to_owned(),
            scientific_boundary: draft.scientific_boundary.trim().to_owned(),
            hypothesis: draft.hypothesis.trim().to_owned(),
            model: draft.model,
            constants: draft.constants,
            trajectory: draft.trajectory,
            integration: draft.integration,
            search: draft.search,
            observables: draft.observables,
            constraints: draft.constraints,
            visualization: draft.visualization,
            falsification: draft.falsification,
            compute: draft.compute,
            limitations: draft.limitations,
            authored_by: authored_by.into(),
            created_at: Utc::now(),
        }
    }

    pub fn capability_id(&self) -> &'static str {
        self.model.capability_id()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityGap {
    pub requested_capability: String,
    pub reason: String,
    #[serde(default)]
    pub suggested_extension: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCapability {
    pub id: String,
    pub name: String,
    pub description: String,
    pub gpu_acceleration: String,
    pub limits: Value,
}

pub fn runtime_capabilities() -> Vec<RuntimeCapability> {
    vec![
        RuntimeCapability {
            id: "state_vector_ode".to_owned(),
            name: "State-vector dynamics".to_owned(),
            description: "First-order ordinary differential systems authored from bounded mathematical expressions, with deterministic integration and optional parameter search.".to_owned(),
            gpu_acceleration: "Batched candidate scoring is lowered to a generated WGSL compute shader when the expression set is compatible; the winning candidate is replayed in f64 on CPU.".to_owned(),
            limits: serde_json::json!({
                "state_variables": 128,
                "gpu_state_variables": 24,
                "trajectory_measures": 64,
                "measurement_sampling": "every fixed integration step; CPU f64",
                "constants": 32,
                "maximum_steps": 2_000_000,
                "expression_language": "bounded arithmetic and scientific functions"
            }),
        },
        RuntimeCapability {
            id: "pairwise_particles".to_owned(),
            name: "Pairwise particle dynamics".to_owned(),
            description: "One- to three-dimensional particles governed by an authored radial interaction expression, optional external acceleration, configurable boundaries, and ensemble search.".to_owned(),
            gpu_acceleration: "The browser uses the available graphics adapter for rendering. Numerical execution currently uses bounded Rayon CPU batches; a generic tiled GPU lowerer is scaffolded but not advertised as complete.".to_owned(),
            limits: serde_json::json!({
                "dimensions": [1, 2, 3],
                "particles_per_candidate": 2048,
                "maximum_steps": 500_000,
                "interaction": "radial pair force plus external acceleration"
            }),
        },
    ]
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    Builder,
    Explorer,
    Falsifier,
    Theorist,
    BiomolecularArchitect,
    QmmmPlanner,
}

impl Default for AgentRole {
    fn default() -> Self { Self::Builder }
}

impl AgentRole {
    pub fn label(self) -> &'static str {
        match self {
            Self::Builder => "Builder",
            Self::Explorer => "Explorer",
            Self::Falsifier => "Falsifier",
            Self::Theorist => "Theorist",
            Self::BiomolecularArchitect => "Biomolecular architect",
            Self::QmmmPlanner => "QM/MM planner",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Chat,
    Proposal,
    Status,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub id: Uuid,
    pub project_id: Uuid,
    pub role: ConversationRole,
    pub kind: MessageKind,
    pub content: String,
    #[serde(default)]
    pub agent_role: Option<AgentRole>,
    #[serde(default)]
    pub manifest_id: Option<Uuid>,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

impl ConversationMessage {
    pub fn new(
        project_id: Uuid,
        role: ConversationRole,
        kind: MessageKind,
        content: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            project_id,
            role,
            kind,
            content: content.into(),
            agent_role: None,
            manifest_id: None,
            metadata: Value::Null,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachmentInput {
    pub name: String,
    #[serde(default)]
    pub mime_type: String,
    #[serde(default)]
    pub size_bytes: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatAttachmentRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub message_id: Uuid,
    pub name: String,
    pub mime_type: String,
    pub size_bytes: usize,
    pub sha256: String,
    pub content: String,
    #[serde(default)]
    pub imported_structure_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SendMessageRequest {
    #[serde(default)] pub research_mode: bool,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)] pub experiment_options: Option<crate::experiment::BuildOptions>,
    #[serde(default)] pub context_manifest_id: Option<Uuid>,
    #[serde(default)] pub study_intent: Option<String>,
    #[serde(default)]
    pub source_run_id: Option<Uuid>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub request_id: Option<Uuid>,
    #[serde(default)]
    pub provider: Option<ProviderKind>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub agent_role: AgentRole,
    #[serde(default)]
    pub auto_run: bool,
    #[serde(default)]
    pub attachments: Vec<ChatAttachmentInput>,
    #[serde(default)]
    pub structure_id: Option<Uuid>,
    #[serde(default)]
    pub reply_to_message_id: Option<Uuid>,
    #[serde(default)]
    pub branch_from_message_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub request_id: Uuid,
    pub user_message: ConversationMessage,
    pub assistant_message: ConversationMessage,
    #[serde(default)]
    pub manifest: Option<ExperimentManifest>,
    #[serde(default)]
    pub capability_gap: Option<CapabilityGap>,
    #[serde(default)]
    pub submitted_run_id: Option<Uuid>,
    #[serde(default)]
    pub imported_structure_ids: Vec<Uuid>,
    pub notice: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    OpenAi,
    Anthropic,
}

impl ProviderKind {
    pub fn account_name(self) -> &'static str {
        match self {
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub provider: ProviderKind,
    pub configured: bool,
    pub key_configured: bool,
    pub model_configured: bool,
    pub model: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveProviderKeyRequest {
    pub api_key: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveProviderModelRequest {
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModel {
    #[serde(default)]
    pub capability_rank: u32,
    #[serde(default)]
    pub reasoning_efforts: Vec<String>,
    #[serde(default)]
    pub recommended_reasoning: Option<String>,
    #[serde(default)]
    pub description: String,
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub owned_by: Option<String>,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderModelList {
    pub provider: ProviderKind,
    pub models: Vec<ProviderModel>,
    pub fetched_at: DateTime<Utc>,
    pub current_model: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MolecularFormat {
    #[default]
    Auto,
    Pdb,
    Sdf,
    Mol,
    Xyz,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportStructureRequest {
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub name: String,
    #[serde(default)]
    pub format: MolecularFormat,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MolecularAtom {
    pub id: u32,
    #[serde(default)]
    pub serial: Option<i32>,
    pub name: String,
    pub element: String,
    pub position: [f64; 3],
    #[serde(default)]
    pub residue_name: String,
    #[serde(default)]
    pub residue_id: String,
    #[serde(default)]
    pub chain_id: String,
    pub hetero: bool,
    #[serde(default)]
    pub formal_charge: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MolecularBond {
    pub atom_a: u32,
    pub atom_b: u32,
    pub order: f32,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LigandGroup {
    pub id: String,
    pub name: String,
    pub chain_id: String,
    pub residue_id: String,
    pub atom_ids: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MolecularDiagnostics {
    pub atom_count: usize,
    pub heavy_atom_count: usize,
    pub bond_count: usize,
    pub residue_count: usize,
    pub chain_count: usize,
    pub ligand_count: usize,
    pub formula: String,
    pub molecular_mass_da: f64,
    pub center_of_mass: [f64; 3],
    pub bounding_min: [f64; 3],
    pub bounding_max: [f64; 3],
    pub radius_of_gyration: f64,
    pub potential_hbond_donors: usize,
    pub potential_hbond_acceptors: usize,
    pub approximate_hbond_contacts: usize,
    pub approximate_rotatable_bonds: usize,
    pub approximate_ring_bonds: usize,
    pub suspicious_close_contacts: usize,
    pub lipinski_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MolecularStructure {
    pub id: Uuid,
    #[serde(default)]
    pub project_id: Option<Uuid>,
    pub name: String,
    pub format: MolecularFormat,
    pub atoms: Vec<MolecularAtom>,
    pub bonds: Vec<MolecularBond>,
    pub ligands: Vec<LigandGroup>,
    pub diagnostics: MolecularDiagnostics,
    pub warnings: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QmmmRegionRequest {
    #[serde(default)]
    pub ligand_id: Option<String>,
    #[serde(default)]
    pub residues: Vec<String>,
    #[serde(default)]
    pub atom_ids: Vec<u32>,
    #[serde(default)]
    pub center: Option<[f64; 3]>,
    #[serde(default = "default_qmmm_radius")]
    pub radius: f64,
    #[serde(default)]
    pub include_solvent: bool,
    #[serde(default = "default_true")]
    pub preserve_residues: bool,
    #[serde(default = "default_qmmm_max_atoms")]
    pub max_atoms: usize,
    #[serde(default)]
    pub net_charge: i32,
    #[serde(default = "default_multiplicity")]
    pub multiplicity: u32,
}

fn default_qmmm_radius() -> f64 { 5.0 }
fn default_qmmm_max_atoms() -> usize { 400 }
fn default_multiplicity() -> u32 { 1 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QmmmRegionPlan {
    pub id: Uuid,
    pub structure_id: Uuid,
    pub selected_atom_ids: Vec<u32>,
    pub selected_residues: Vec<String>,
    pub estimated_electrons: i64,
    pub net_charge: i32,
    pub multiplicity: u32,
    pub radius: f64,
    pub warnings: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CampaignPlanRequest {
    pub goal: String,
    #[serde(default)]
    pub qmmm_plan_id: Option<Uuid>,
    #[serde(default = "default_candidate_count")]
    pub candidate_count: usize,
    #[serde(default)]
    pub thorough: bool,
}

fn default_candidate_count() -> usize { 64 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignStage {
    pub id: String,
    pub title: String,
    pub description: String,
    pub required_engines: Vec<String>,
    pub readiness: String,
    pub compute_profile: String,
    pub evidence_gate: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputationalCampaign {
    pub id: Uuid,
    pub structure_id: Uuid,
    pub goal: String,
    pub candidate_count: usize,
    pub thorough: bool,
    #[serde(default)]
    pub qmmm_plan_id: Option<Uuid>,
    pub stages: Vec<CampaignStage>,
    pub warnings: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScientificEngineStatus {
    pub id: String,
    pub name: String,
    pub available: bool,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub capabilities: Vec<String>,
    pub accelerator_support: String,
    pub install_hint: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImportManifestRequest {
    pub manifest: ExperimentManifestDraft,
    #[serde(default = "default_true")]
    pub auto_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRequest {
    pub manifest_id: Uuid,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub priority: RunPriority,
    #[serde(default)]
    pub compute: Option<ComputeRequest>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunRecord {
    pub id: Uuid,
    pub project_id: Uuid,
    pub manifest_id: Uuid,
    pub manifest_revision: u32,
    pub capability_id: String,
    pub name: String,
    pub status: RunStatus,
    pub priority: RunPriority,
    pub compute: ComputeRequest,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub progress: f32,
    pub phase: String,
    pub backend_used: Option<String>,
    pub queued_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl RunRecord {
    pub fn queued(request: RunRequest, manifest: &ExperimentManifest) -> Self {
        let compute = request.compute.unwrap_or_else(|| manifest.compute.clone());
        let name = request
            .name
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| manifest.title.clone());
        Self {
            id: Uuid::new_v4(),
            project_id: manifest.project_id,
            manifest_id: manifest.id,
            manifest_revision: manifest.revision,
            capability_id: manifest.capability_id().to_owned(),
            name,
            status: RunStatus::Queued,
            priority: request.priority,
            compute,
            result: None,
            error: None,
            progress: 0.0,
            phase: "Queued".to_owned(),
            backend_used: None,
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEvent {
    pub id: Uuid,
    pub kind: String,
    pub run_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
    pub payload: Value,
}

impl ServerEvent {
    pub fn new(kind: impl Into<String>, run_id: Option<Uuid>, payload: Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: kind.into(),
            run_id,
            occurred_at: Utc::now(),
            payload,
        }
    }
}
