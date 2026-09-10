//! Durable research objects. Recipes are immutable after creation; results never overwrite inputs.
use std::collections::BTreeMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::domain::{ExperimentManifest, ObjectiveGoal, SearchVariableSpec};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Strategy { LatinHypercube, MapElites, NoveltySearch }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricGoal { pub metric: String, pub goal: ObjectiveGoal }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor { pub metric: String, pub minimum: f64, pub maximum: f64, pub bins: usize }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyRecipe {
    pub title: String,
    pub hypothesis: String,
    pub base_manifest_id: Uuid,
    pub strategy: Strategy,
    pub parameters: Vec<SearchVariableSpec>,
    /// First objective selects each cell's elite; remaining objectives form a descriptive Pareto front.
    pub objectives: Vec<MetricGoal>,
    pub descriptors: Vec<Descriptor>,
    pub exploration_trials: usize,
    pub validation_finalists: usize,
    pub wall_seconds: u64,
    pub per_trial_seconds: u64,
    pub seed: u64,
    /// Absolute + relative agreement tolerance, frozen before results are seen.
    pub absolute_tolerance: f64,
    pub relative_tolerance: f64,
    #[serde(default)]
    pub auto_review: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trial {
    pub id: Uuid,
    pub index: usize,
    pub phase: String,
    pub parent_trial_id: Option<Uuid>,
    pub manifest_id: Option<Uuid>,
    pub run_id: Option<Uuid>,
    pub values: Vec<f64>,
    pub state: String,
    pub metrics: BTreeMap<String, f64>,
    pub eligible: bool,
    pub reasons: Vec<String>,
    pub cell: Option<String>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StudyEvent { pub at: DateTime<Utc>, pub kind: String, pub message: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Study {
    pub id: Uuid,
    pub project_id: Uuid,
    pub recipe: StudyRecipe,
    pub recipe_hash: String,
    pub base_manifest: ExperimentManifest,
    pub state: String,
    pub stage: String,
    pub trials: Vec<Trial>,
    pub finalist_ids: Vec<Uuid>,
    pub finalists_frozen: bool,
    pub elapsed_seconds: f64,
    pub review_count: usize,
    pub events: Vec<StudyEvent>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
impl Study {
    pub fn event(&mut self, kind: &str, message: impl Into<String>) {
        let now=Utc::now(); self.updated_at=now;
        self.events.push(StudyEvent{at:now,kind:kind.into(),message:message.into()});
        if self.events.len()>2000 { self.events.remove(0); }
    }
    pub fn planned_trials(&self) -> usize { self.recipe.exploration_trials + 2 * (if self.finalists_frozen {self.finalist_ids.len()} else {self.recipe.validation_finalists}) }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Author {
    pub name: String,
    #[serde(default)] pub orcid: String,
    #[serde(default)] pub contributions: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub id: Uuid,
    pub statement: String,
    /// observation / interpretation / hypothesis — never automatically "discovery".
    pub kind: String,
    pub supporting_runs: Vec<Uuid>,
    pub contradicting_runs: Vec<Uuid>,
    pub limitations: String,
    pub literature_comparison: String,
    pub reviewed_by: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub doi: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<i64>,
    pub publisher: String,
    pub url: String,
    pub source: String,
    pub retrieved_at: DateTime<Utc>,
    #[serde(default)] pub reading_notes: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notebook {
    pub project_id: Uuid,
    pub revision: u64,
    pub title: String,
    pub authors: Vec<Author>,
    pub hypothesis: String,
    pub protocol: String,
    pub notes: String,
    pub claims: Vec<Claim>,
    pub references: Vec<Reference>,
    pub data_availability: String,
    pub code_availability: String,
    pub ai_disclosure: String,
    pub data_license: String,
    pub independent_validation_notes: String,
    pub updated_at: DateTime<Utc>,
}
