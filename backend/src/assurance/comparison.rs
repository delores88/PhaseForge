use std::collections::BTreeMap;
use anyhow::{bail,Context};
use serde::Serialize;
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use crate::{domain::ExperimentManifest,discovery::{math,types::Study}};
use super::types::*;

pub fn hash<T:Serialize>(value:&T)->anyhow::Result<String>{Ok(format!("{:x}",Sha256::digest(serde_json::to_vec(value)?)))}
pub fn text_hash(value:&str)->String{format!("{:x}",Sha256::digest(value.as_bytes()))}
pub fn validate_rules(rules:&[MetricRule])->anyhow::Result<()> {
    if rules.is_empty()||rules.len()>8{bail!("choose 1-8 measured verification metrics");}
    let mut names=std::collections::HashSet::new();
    for r in rules {
        if r.metric.trim().is_empty()||r.metric.len()>160||!names.insert(&r.metric){bail!("metric names must be present and unique");}
        if !r.scale.is_finite()||r.scale<=0.0||![r.absolute_tolerance,r.relative_tolerance,r.robustness_absolute].iter().all(|v|v.is_finite()&&*v>=0.0){bail!("scales must be positive and tolerances finite/nonnegative");}
    }Ok(())
}
/// Exact metadata compatibility, not a symmetry/invariant proof. Only declared search values are masked.
pub fn space(study:&Study,rules:&[MetricRule])->anyhow::Result<String>{
    validate_rules(rules)?;let mut normalized=study.base_manifest.clone();
    for p in &study.recipe.parameters{math::set_parameter(&mut normalized,&p.target,0.0)?;}
    let features=rules.iter().map(|r|json!({"metric":r.metric,"scale":r.scale})).collect::<Vec<_>>();
    let mut targets=study.recipe.parameters.iter().map(|p|p.target.clone()).collect::<Vec<_>>();targets.sort();
    let mut description=json!({"version":"exact-model-feature-space/1","model":normalized.model,"constants":normalized.constants,
        "observables":normalized.observables,"constraints":normalized.constraints,"integration":normalized.integration,
        "search_aliases":normalized.search.variables,"targets":targets,"features":features});
    if !normalized.trajectory.is_empty(){description["trajectory"]=json!(normalized.trajectory);}
    hash(&description)
}
pub fn signature(manifest:&ExperimentManifest,run:&crate::domain::RunRecord,rules:&[MetricRule])->anyhow::Result<BTreeMap<String,f64>>{
    if run.manifest_id!=manifest.id||run.project_id!=manifest.project_id||run.status!=crate::domain::RunStatus::Completed{bail!("completed run and exact manifest provenance are required");}
    let result=run.result.as_ref().context("run has no evidence")?;
    if result.pointer("/numerical/reached_end_time").and_then(Value::as_bool)!=Some(true){bail!("run did not report reaching its endpoint");}
    rules.iter().map(|r|{
        let n=result.get("metrics").and_then(|v|v.get(&r.metric)).and_then(Value::as_f64).filter(|v|v.is_finite()).with_context(||format!("missing finite measurement: {}",r.metric))?;
        Ok((r.metric.clone(),n))
    }).collect()
}
pub fn distance(a:&BTreeMap<String,f64>,b:&BTreeMap<String,f64>,rules:&[MetricRule])->Option<f64>{
    if rules.is_empty(){return None;}
    let mut norm=0.0_f64;
    for r in rules {
        let av=*a.get(&r.metric)?;let bv=*b.get(&r.metric)?;
        if !av.is_finite()||!bv.is_finite()||r.scale<=0.0||!r.scale.is_finite(){return None;}
        let z=(av-bv)/r.scale;if !z.is_finite(){return None;}norm=norm.hypot(z);
    }let d=norm/(rules.len() as f64).sqrt();if d.is_finite(){Some(d)}else{None}
}
pub fn catalog_report(d:&Dossier)->Value{
    let measured=signature(&d.manifest,&d.source_run,&d.protocol.metrics).unwrap_or_default();
    let mut matches=d.references.iter().filter_map(|r|{
        if r.comparison_space!=d.comparison_space||r.source_run_id==Some(d.source_run.id){return None;}
        let dist=distance(&measured,&r.measurements,&d.protocol.metrics)?;
        Some((dist,json!({"reference_id":r.id,"title":r.title,"source":r.source,"provenance_kind":r.provenance_kind,
            "content_hash":r.content_hash,"distance":dist,"near_match":dist<=d.protocol.duplicate_distance,"measurements":r.measurements})))
    }).collect::<Vec<_>>();matches.sort_by(|a,b|a.0.total_cmp(&b.0));
    let nearest=matches.first().map(|v|v.0);let checked=matches.len();
    json!({"status":if nearest.map(|v|v<=d.protocol.duplicate_distance).unwrap_or(false){"near_match"}else if checked==0{"no_comparable_reference"}else{"distinct_in_selected_features"},
        "compared":checked,"excluded":d.references.len()-checked,"threshold":d.protocol.duplicate_distance,"nearest":matches.into_iter().take(12).map(|v|v.1).collect::<Vec<_>>(),
        "boundary":"Distance in selected, scaled observables only. No symmetry equivalence, topological classification, exhaustive catalog coverage, or scientific novelty proof. External values are author supplied."})
}
pub fn within_study(study:&Study,trial_id:uuid::Uuid,rules:&[MetricRule])->Value{
    let Some(target)=study.trials.iter().find(|t|t.id==trial_id) else{return json!({"error":"candidate absent"});};
    let mut distances=study.trials.iter().filter(|t|t.phase=="explore"&&t.eligible&&t.id!=trial_id).filter_map(|t|distance(&target.metrics,&t.metrics,rules).map(|d|(d,t.id))).collect::<Vec<_>>();
    distances.sort_by(|a,b|a.0.total_cmp(&b.0));
    json!({"comparisons":distances.len(),"nearest":distances.iter().take(8).map(|(d,id)|json!({"trial_id":id,"distance":d})).collect::<Vec<_>>(),
        "mean_five_neighbor_distance":if distances.is_empty(){None}else{Some(distances.iter().take(5).map(|v|v.0).sum::<f64>()/distances.len().min(5) as f64)},
        "meaning":"Rarity among completed feasible trials in this study, not novelty to science."})
}
