//! Versioned, locally frozen verification protocols. No field grants arbitrary code execution.
use std::collections::BTreeMap;
use chrono::{DateTime,Utc};
use serde::{Deserialize,Serialize};
use serde_json::Value;
use uuid::Uuid;
use crate::domain::{ExperimentManifest,RunRecord,SearchVariableSpec};
use crate::discovery::types::Reference;

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct MetricRule {
    pub metric:String,
    /// Matching is RMS distance in these explicit, fixed measurement scales.
    pub scale:f64,
    pub absolute_tolerance:f64,
    pub relative_tolerance:f64,
    /// Absolute allowed change from the independently replayed nominal result.
    pub robustness_absolute:f64,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ControlSpec {
    pub run_id:Uuid,
    pub metric:String,
    pub minimum:f64,
    pub maximum:f64,
    pub rationale:String,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct VerificationRequest {
    pub study_id:Uuid,
    pub trial_id:Uuid,
    pub question:String,
    pub metrics:Vec<MetricRule>,
    pub holdout_replicates:usize,
    /// Fraction of each study parameter's full allowed range, NOT a relative change of its value.
    pub perturb_fraction:f64,
    pub holdout_seed:u64,
    pub wall_seconds:u64,
    pub max_rhs_evaluations:usize,
    pub solver_absolute_tolerance:f64,
    pub solver_relative_tolerance:f64,
    pub duplicate_distance:f64,
    #[serde(default)] pub controls:Vec<ControlSpec>,
    #[serde(default)] pub reference_ids:Vec<Uuid>,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ControlSnapshot {pub spec:ControlSpec,pub manifest:ExperimentManifest,pub run:RunRecord}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct CatalogEntry {
    pub id:Uuid,pub project_id:Uuid,pub comparison_space:String,pub title:String,
    pub source:String,pub license:String,pub provenance_kind:String,
    pub source_run_id:Option<Uuid>,pub measurements:BTreeMap<String,f64>,
    pub content_hash:String,pub created_at:DateTime<Utc>,
}
#[derive(Debug,Clone,Deserialize)]
pub struct CatalogImport {
    pub study_id:Uuid,pub metric_rules:Vec<MetricRule>,pub title:String,pub source:String,pub license:String,
    #[serde(default)]pub run_id:Option<Uuid>,
    #[serde(default)]pub measurements:BTreeMap<String,f64>,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct SearchRecord {
    pub id:Uuid,pub dossier_id:Uuid,pub query:String,pub provider:String,
    pub status:String,pub error:Option<String>,pub references:Vec<Reference>,
    pub retrieved_at:DateTime<Utc>,pub scope:String,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ReviewRequest {
    pub reviewer:String,
    /// artifact_or_failed / rediscovery / potentially_distinct / inconclusive
    pub disposition:String,
    pub comparison_notes:String,pub limitations:String,
    pub examined_sources:Vec<String>,
    pub full_text_reviewed:bool,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct ReviewRecord {
    pub id:Uuid,pub dossier_id:Uuid,pub review:ReviewRequest,pub evidence_hash:String,pub recorded_at:DateTime<Utc>,
}
#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Dossier {
    pub id:Uuid,pub project_id:Uuid,pub protocol:VerificationRequest,pub protocol_hash:String,
    pub recipe_hash:String,pub comparison_space:String,
    pub manifest:ExperimentManifest,pub source_run:RunRecord,pub source_run_hash:String,
    pub parameters:Vec<SearchVariableSpec>,pub references:Vec<CatalogEntry>,pub controls:Vec<ControlSnapshot>,
    pub frozen_refinement:Value,pub within_study:Value,
    pub state:String,pub stage:String,pub completed_tasks:usize,pub total_tasks:usize,
    pub worker_source_hash:String,pub reference_source_hash:String,
    pub result:Option<Value>,pub error:Option<String>,
    pub created_at:DateTime<Utc>,pub updated_at:DateTime<Utc>,
}
