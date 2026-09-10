//! Direct experiment intent and deterministic next-run actions. Research task
//! progress is NOT an execution dependency. Scientific claims remain separate.
pub mod api;
use anyhow::{bail, Context};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use crate::domain::{ExperimentManifest,ExperimentManifestDraft};

#[derive(Debug,Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildOptions {
    /// Permission to use declared hypothetical parameter assumptions, never to
    /// fabricate observations or claim empirical/clinical validation.
    #[serde(default)] pub assumptions_allowed: bool,
    pub max_wall_seconds: u64,
    pub max_memory_mb: usize,
    pub max_candidates: usize,
}
impl Default for BuildOptions {
    fn default()->Self {Self{assumptions_allowed:false,max_wall_seconds:180,max_memory_mb:1024,max_candidates:64}}
}
impl BuildOptions {
    pub fn validate(&self)->anyhow::Result<()> {
        if !(1..=86400).contains(&self.max_wall_seconds)||!(128..=262144).contains(&self.max_memory_mb)||!(1..=131072).contains(&self.max_candidates) {bail!("Invalid build budget: 1–86400 seconds, 128–262144 MiB, 1–131072 candidates");} Ok(())
    }
    pub fn validate_draft(&self,m:&ExperimentManifestDraft)->anyhow::Result<()> {
        if m.compute.max_wall_seconds>self.max_wall_seconds||m.compute.max_memory_mb>self.max_memory_mb||m.compute.candidate_count>self.max_candidates {bail!("The proposed experiment exceeds the USER budget: {}s, {} MiB, {} candidates. Revise the proposal within those caps; do not silently alter the physics at admission.",self.max_wall_seconds,self.max_memory_mb,self.max_candidates);}
        // max_candidates is a TOTAL candidate-evaluation ceiling on this direct
        // build, in addition to the native per-generation allocation limit.
        if m.search.enabled && m.search.population.saturating_mul(m.search.generations)>self.max_candidates {bail!("Search population × generations exceeds the user-approved total candidate budget {}",self.max_candidates);}
        if m.search.enabled && m.search.population>m.compute.candidate_count {bail!("Search population exceeds its declared compute allocation; declare an honest candidate budget");}
        Ok(())
    }
}
/// A response may identify a precise missing engine/input, but it may not turn a
/// direct experiment request into another research plan. This is schema routing,
/// not a provider safety override. Provider refusals remain terminal responses.
pub fn validate_action(intent:&str,action:&str,has_plan:bool)->anyhow::Result<()> {
    if intent=="experiment" {
        if !["create_manifest","revise_manifest","capability_gap"].contains(&action)||has_plan {bail!("Experiment mode requires a runnable manifest, or a concise capability_gap for the exact unavailable operation with research_plan=null. Do not return another plan or impose literature, publication, or task-completion gates on exploratory simulation.");}
    }
    if intent=="review" && (has_plan||!["explain","capability_gap"].contains(&action)) {bail!("Review mode requires the actual analysis in assistant_message, or a precise missing-input gap; not another research plan or simulation");}
    Ok(())
}
pub fn instructions(options:&BuildOptions)->Value {
    json!({"mode":"experiment","budget":options,"instruction":include_str!("experiment-guide.md"),
        "research_plan_dependencies":"advisory context only; not required for execution",
        "assumption_policy":if options.assumptions_allowed {"The user explicitly permits declared hypothetical/scaled parameters for computational exploration, not invented measured data. Record assumptions and sensitivity limitations in the manifest."}else{"Use supplied inputs or established model definitions. Do not invent specific empirical inputs; ask for the minimum missing input or offer a scoped hypothesis-mode alternative."},
        "execution":"Only the user's auto_run flag authorizes one accepted experiment. It never authorizes an autonomous series or bypasses budget, schema, safety or cancellation checks."})
}
#[derive(Debug,Clone,Copy,Serialize,Deserialize,PartialEq,Eq)]
#[serde(rename_all="snake_case")]
pub enum NextOperation {Replay, FinerSteps, LongerHorizon}
/// Creates a new revision from the EXACT selected manifest. Re-evaluates the
/// complete seeded experiment: not continuation from its last state and not an
/// independent-solver verification. Never modifies the original revision.
pub fn next_draft(base:&ExperimentManifest,operation:NextOperation)->anyhow::Result<ExperimentManifestDraft> {
    if operation==NextOperation::Replay {bail!("Replay uses the original immutable manifest, not a new draft");}
    let mut draft:ExperimentManifestDraft=serde_json::from_value(serde_json::to_value(base)?)?;
    match operation {
        NextOperation::FinerSteps=>{draft.integration.time_step*=0.5;draft.integration.output_stride=draft.integration.output_stride.saturating_mul(2);},
        NextOperation::LongerHorizon=>{draft.integration.end_time=base.integration.start_time+2.0*(base.integration.end_time-base.integration.start_time);},
        NextOperation::Replay=>unreachable!(),
    }
    let steps=((draft.integration.end_time-draft.integration.start_time)/draft.integration.time_step).ceil();
    if !steps.is_finite()||steps>2_000_000.0||draft.integration.time_step<=0.0 {bail!("Requested change exceeds the numerical step ceiling; edit the horizon or timestep in Setup");}
    draft.integration.max_steps=draft.integration.max_steps.max(steps as usize);
    let suffix=if operation==NextOperation::FinerSteps{"finer steps"}else{"longer horizon"};
    draft.title=format!("{} · {suffix}",base.title.chars().take(170).collect::<String>());
    draft.limitations.push(format!("User-requested {suffix} of revision {}. Re-evaluates the complete seeded experiment from its declared initial conditions, retaining its original acceptance rules and compute caps. A different search winner is possible; this is not independent confirmation or a physical validation claim.",base.revision));
    let candidate=ExperimentManifest::from_draft(base.project_id,Some(base.id),base.revision+1,draft.clone(),"direct experiment change");
    crate::sandbox::validate_manifest(&candidate).context("The changed experiment is not executable within runtime limits")?;
    Ok(draft)
}
#[cfg(test)] mod tests {
    use super::*;
    fn base()->ExperimentManifest {let d:ExperimentManifestDraft=serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();ExperimentManifest::from_draft(uuid::Uuid::nil(),None,1,d,"test")}
    #[test]fn direct_intent_cannot_loop_into_plan(){assert!(validate_action("experiment","research_plan",true).is_err());assert!(validate_action("experiment","explain",false).is_err());validate_action("experiment","create_manifest",false).unwrap();validate_action("experiment","capability_gap",false).unwrap();}
    #[test]fn review_produces_analysis_not_more_planning(){assert!(validate_action("review","research_plan",true).is_err());validate_action("review","explain",false).unwrap();}
    #[test]fn no_assumptions_without_permission(){assert!(!BuildOptions::default().assumptions_allowed);}
    #[test]fn changed_grid_is_exact_and_original_immutable(){let b=base();let d=next_draft(&b,NextOperation::FinerSteps).unwrap();assert_eq!(d.integration.time_step,b.integration.time_step/2.0);assert_eq!(d.integration.end_time,b.integration.end_time);assert_eq!(d.compute.max_wall_seconds,b.compute.max_wall_seconds);assert_eq!(d.search.seed,b.search.seed);}
    #[test]fn longer_restarts_same_initial_state(){let b=base();let d=next_draft(&b,NextOperation::LongerHorizon).unwrap();assert_eq!(d.integration.end_time,2.0*b.integration.end_time-b.integration.start_time);assert_eq!(serde_json::to_value(d.model).unwrap(),serde_json::to_value(b.model).unwrap());}
    #[test]fn excessive_budget_rejected(){let mut o=BuildOptions::default();o.max_wall_seconds=0;assert!(o.validate().is_err());}
    #[test]fn candidate_cap_includes_generations(){let b=base();let mut d:ExperimentManifestDraft=serde_json::from_value(serde_json::to_value(b).unwrap()).unwrap();d.search.enabled=true;d.search.population=32;d.search.generations=3;let mut o=BuildOptions::default();o.max_wall_seconds=86400;o.max_memory_mb=262144;assert!(o.validate_draft(&d).is_err());}
}
