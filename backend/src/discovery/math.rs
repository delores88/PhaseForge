//! Deterministic campaign sampling, behavioral archives and numerical agreements.
use std::collections::BTreeMap;
use anyhow::{bail, Context};
use rand::{Rng, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha20Rng;
use serde_json::{json, Value};
use uuid::Uuid;
use crate::domain::{ExperimentManifest, ObjectiveGoal};
use super::types::*;

pub fn oriented(value: f64, goal: ObjectiveGoal) -> f64 {
    if goal == ObjectiveGoal::Minimize {value} else {-value}
}
pub fn cell(recipe: &StudyRecipe, metrics: &BTreeMap<String,f64>) -> Option<String> {
    if recipe.descriptors.is_empty() { return None; }
    recipe.descriptors.iter().map(|d|{
        let value=*metrics.get(&d.metric)?;
        if !value.is_finite() || value<d.minimum || value>d.maximum {return None;}
        let bin=(((value-d.minimum)/(d.maximum-d.minimum)*d.bins as f64).floor() as usize).min(d.bins-1);
        Some(bin.to_string())
    }).collect::<Option<Vec<_>>>().map(|v|v.join(":"))
}
pub fn archive(study: &Study) -> BTreeMap<String,usize> {
    let mut cells=BTreeMap::<String,usize>::new(); let goal=&study.recipe.objectives[0];
    for (i,t) in study.trials.iter().enumerate().filter(|(_,t)|t.phase=="explore" && t.eligible) {
        if let Some(key)=&t.cell {
            let replace=cells.get(key).map(|old|oriented(t.metrics[&goal.metric],goal.goal)<oriented(study.trials[*old].metrics[&goal.metric],goal.goal)).unwrap_or(true);
            if replace {cells.insert(key.clone(),i);}
        }
    } cells
}
/// Every dimension uses its own seeded permutation; each planned stratum is sampled once.
pub fn lhs(recipe: &StudyRecipe,index:usize) -> Vec<f64> {
    recipe.parameters.iter().enumerate().map(|(j,p)|{
        let mut rng=ChaCha20Rng::seed_from_u64(recipe.seed.wrapping_add((j as u64+1).wrapping_mul(7919)));
        let mut bins=(0..recipe.exploration_trials).collect::<Vec<_>>(); bins.shuffle(&mut rng);
        let jitter=(0..=index).map(|_|rng.gen::<f64>()).last().unwrap_or(0.5); // independent reproducible jitter per stratum
        p.minimum+(p.maximum-p.minimum)*(bins[index] as f64+jitter)/recipe.exploration_trials as f64
    }).collect()
}
pub fn next_values(study:&Study,index:usize)->Vec<f64>{
    if study.recipe.strategy==Strategy::NoveltySearch{return novelty_values(study,index);}
    let archive=archive(study);
    let warmup=(study.recipe.exploration_trials/4).clamp(4,16);
    if study.recipe.strategy==Strategy::LatinHypercube || index<warmup || archive.is_empty(){return lhs(&study.recipe,index);}
    let mut rng=ChaCha20Rng::seed_from_u64(study.recipe.seed.wrapping_add((index as u64+1).wrapping_mul(104729)));
    if rng.gen::<f64>()<0.15 {return lhs(&study.recipe,index);}
    let entries=archive.values().copied().collect::<Vec<_>>();
    let parent=&study.trials[entries[rng.gen_range(0..entries.len())]];
    parent.values.iter().zip(&study.recipe.parameters).map(|(x,p)|{
        let noise=(0..6).map(|_|rng.gen::<f64>()).sum::<f64>()-3.0;
        (x+noise*0.12*(p.maximum-p.minimum)).clamp(p.minimum,p.maximum)
    }).collect()
}
/// Whitelisted numerical targets only. No reflection into identity, code, or storage settings.
pub fn set_parameter(manifest:&mut ExperimentManifest,target:&str,value:f64)->anyhow::Result<()> {
    if !value.is_finite(){bail!("parameter value must be finite");}
    if let Some(name)=target.strip_prefix("constant:") {
        manifest.constants.iter_mut().find(|p|p.name==name).context("unknown constant target")?.value=value; return Ok(());
    }
    if let Some(name)=target.strip_prefix("initial:") {
        if let crate::domain::ModelSpec::StateVectorOde{variables,..}=&mut manifest.model {
            variables.iter_mut().find(|p|p.name==name).context("unknown initial-state target")?.initial=value; return Ok(());
        }
        bail!("initial: targets require a state-vector ODE");
    }
    // Particle scalar parameters supported by the native manifest, including explicit distribution scalars.
    let pointer=match target {
        "population.mass"=>"/population/mass".to_owned(),
        "interaction.softening"=>"/interaction/softening".to_owned(),
        "interaction.linear_damping"=>"/interaction/linear_damping".to_owned(),
        "position.radius"|"position.thickness"|"position.spacing"|"position.jitter"|
        "velocity.radius"|"velocity.thickness"|"velocity.spacing"|"velocity.jitter"=>format!("/population/{}",target.replace('.',"/")),
        _=>bail!("unsupported study parameter target: {target}; choose a constant, ODE initial state, or supported particle scalar"),
    };
    if !matches!(&manifest.model,crate::domain::ModelSpec::PairwiseParticles{..}){bail!("particle target on non-particle model");}
    let mut model=serde_json::to_value(&manifest.model)?;
    let slot=model.pointer_mut(&pointer).context("target does not exist in this distribution")?;
    if !slot.is_number(){bail!("target is not a numerical scalar");}
    *slot=json!(value); manifest.model=serde_json::from_value(model)?; Ok(())
}
pub fn pareto(study:&Study)->Vec<Uuid>{
    let candidates=study.trials.iter().filter(|t|t.phase=="explore" && t.eligible).collect::<Vec<_>>();
    candidates.iter().filter(|a|!candidates.iter().any(|b|{
        if a.id==b.id{return false;}
        let mut strict=false;
        for g in &study.recipe.objectives {
            let av=oriented(a.metrics[&g.metric],g.goal); let bv=oriented(b.metrics[&g.metric],g.goal);
            if bv>av{return false;} if bv<av{strict=true;}
        } strict
    })).map(|t|t.id).collect()
}
pub fn validation(study:&Study,parent:Uuid)->Value{
    let base=study.trials.iter().find(|t|t.id==parent);
    let half=study.trials.iter().find(|t|t.parent_trial_id==Some(parent)&&t.phase=="half_step");
    let quarter=study.trials.iter().find(|t|t.parent_trial_id==Some(parent)&&t.phase=="quarter_step");
    let mut checks=Vec::new();
    for g in &study.recipe.objectives {
        let a=base.and_then(|t|t.metrics.get(&g.metric)).copied();
        let b=half.and_then(|t|t.metrics.get(&g.metric)).copied();
        let c=quarter.and_then(|t|t.metrics.get(&g.metric)).copied();
        let delta_ab=a.zip(b).map(|(a,b)|(a-b).abs()).filter(|v|v.is_finite()); let delta_bc=b.zip(c).map(|(b,c)|(b-c).abs()).filter(|v|v.is_finite());
        let limit=a.zip(b).zip(c).map(|((a,b),c)|study.recipe.absolute_tolerance+study.recipe.relative_tolerance*a.abs().max(b.abs()).max(c.abs())).filter(|v|v.is_finite());
        let measured=base.map(|x|x.eligible).unwrap_or(false)&&half.map(|x|x.eligible).unwrap_or(false)&&quarter.map(|x|x.eligible).unwrap_or(false)&&limit.is_some()&&delta_ab.is_some()&&delta_bc.is_some();
        let agrees=measured&&delta_ab.zip(delta_bc).zip(limit).map(|((ab,bc),tol)|ab<=tol&&bc<=tol).unwrap_or(false);
        checks.push(json!({"metric":g.metric,"baseline":a,"half_step":b,"quarter_step":c,"baseline_half_difference":delta_ab,"half_quarter_difference":delta_bc,"threshold":limit,"status":if agrees{"agreement"}else if measured{"disagreement"}else{"inconclusive"}}));
    }
    let passed=!checks.is_empty()&&checks.iter().all(|c|c["status"]=="agreement");
    json!({"trial_id":parent,"status":if passed{"agreement"}else if checks.iter().any(|c|c["status"]=="disagreement"){"disagreement"}else{"inconclusive"},"checks":checks,"boundary":"Same-engine numerical refinement; NOT independent replication, convergence-order proof, dynamical stability, or novelty evidence."})
}
pub fn summary(study:&Study)->Value {
    let eligible=study.trials.iter().filter(|t|t.phase=="explore"&&t.eligible).count();
    let rejected=study.trials.iter().filter(|t|t.phase=="explore"&&t.state!="queued"&&t.state!="running"&&!t.eligible).count();
    let cells=archive(study);let capacity=study.recipe.descriptors.iter().map(|d|d.bins).product::<usize>();
    json!({"eligible":eligible,"rejected":rejected,"completed":study.trials.iter().filter(|t|t.state!="queued"&&t.state!="running").count(),
        "planned":study.planned_trials(),"archive":cells.iter().map(|(c,i)|json!({"cell":c,"trial_id":study.trials[*i].id})).collect::<Vec<_>>(),
        "coverage":if study.recipe.descriptors.is_empty(){None}else{Some(cells.len() as f64/capacity as f64)},
        "pareto_trial_ids":pareto(study),"validation":study.finalist_ids.iter().map(|id|validation(study,*id)).collect::<Vec<_>>(),
        "behavioral_novelty":novelty_scores(study).iter().map(|(id,score)|json!({"trial_id":id,"knn_distance":score})).collect::<Vec<_>>(),"novelty":"not_assessed","execution":"sequential f64 numerical trials on the existing local scheduler; no multi-node or multi-GPU campaign execution"})
}

/// RMS k-neighbor sparsity in the recipe's *fixed* descriptor ranges. Out-of-range
/// candidates remain in the archive ledger but do not define valid novelty niches.
pub fn novelty_scores(study:&Study)->BTreeMap<Uuid,f64>{
    let candidates=study.trials.iter().filter(|t|t.phase=="explore"&&t.eligible&&cell(&study.recipe,&t.metrics).is_some()).collect::<Vec<_>>();
    let mut out=BTreeMap::new();
    if study.recipe.descriptors.is_empty(){return out;}
    for a in &candidates {
        let mut distances=candidates.iter().filter(|b|b.id!=a.id).filter_map(|b|{
            let mut norm=0.0_f64;
            for d in &study.recipe.descriptors{let delta=(a.metrics.get(&d.metric)?-b.metrics.get(&d.metric)?)/(d.maximum-d.minimum);if !delta.is_finite(){return None;}norm=norm.hypot(delta);}
            Some(norm/(study.recipe.descriptors.len() as f64).sqrt())
        }).collect::<Vec<_>>();
        distances.sort_by(f64::total_cmp);let k=distances.len().min(5);
        if k>0{out.insert(a.id,distances.iter().take(k).sum::<f64>()/k as f64);}
    }out
}
/// Exploration follows sparse measured behaviors with a persistent broad-search
/// fraction. This is k-NN novelty search, NOT a calibrated Bayesian posterior.
pub fn novelty_values(study:&Study,index:usize)->Vec<f64>{
    let scores=novelty_scores(study);let warmup=(study.recipe.exploration_trials/4).clamp(4,16);
    if index<warmup||scores.len()<2{return lhs(&study.recipe,index);}
    let mut rng=ChaCha20Rng::seed_from_u64(study.recipe.seed.wrapping_add((index as u64+1).wrapping_mul(32452843)));
    if rng.gen::<f64>()<0.30{return lhs(&study.recipe,index);}
    let mut pool=study.trials.iter().filter(|t|scores.contains_key(&t.id)).collect::<Vec<_>>();
    pool.sort_by(|a,b|scores[&b.id].total_cmp(&scores[&a.id]).then_with(||a.index.cmp(&b.index)));
    let upper=pool.len().min(6);let parent=pool[rng.gen_range(0..upper)];
    parent.values.iter().zip(&study.recipe.parameters).map(|(v,p)|{
        let width=p.maximum-p.minimum;
        let noise=(0..6).map(|_|rng.gen::<f64>()).sum::<f64>()-3.0;
        let raw=(v-p.minimum)/width+noise*0.10;
        // Reflection in normalized coordinates avoids overflow and clipped duplicates.
        let folded=raw.rem_euclid(2.0);
        p.minimum+width*if folded<=1.0{folded}else{2.0-folded}
    }).collect()
}
/// Preserve diverse finalists: strongest quality control first, then farthest
/// measured behavior from already selected candidates. Not a novelty certificate.
pub fn diverse_finalists(study:&Study)->Vec<Uuid>{
    let g=&study.recipe.objectives[0];
    let scores=novelty_scores(study);
    let mut pool=study.trials.iter().filter(|t|t.phase=="explore"&&t.eligible&&scores.contains_key(&t.id)).collect::<Vec<_>>();
    pool.sort_by(|a,b|oriented(a.metrics[&g.metric],g.goal).total_cmp(&oriented(b.metrics[&g.metric],g.goal)).then_with(||a.index.cmp(&b.index)));
    let mut selected=Vec::new();
    if study.recipe.validation_finalists==0{return selected;}
    if let Some(first)=pool.first(){selected.push(first.id);}
    while selected.len()<study.recipe.validation_finalists && selected.len()<pool.len(){
        let mut best:Option<(f64,usize,Uuid)>=None;
        for candidate in &pool {
            if selected.contains(&candidate.id){continue;}
            let mut separation=f64::INFINITY;
            for id in &selected {
                if let Some(prior)=pool.iter().find(|p|p.id==*id) {
                    let mut norm=0.0_f64;
                    for d in &study.recipe.descriptors { norm=norm.hypot((candidate.metrics[&d.metric]-prior.metrics[&d.metric])/(d.maximum-d.minimum)); }
                    separation=separation.min(norm);
                }
            }
            let replace=best.map(|(score,index,_)|separation>score||(separation==score&&candidate.index<index)).unwrap_or(true);
            if replace {best=Some((separation,candidate.index,candidate.id));}
        }
        if let Some((_,_,id))=best{selected.push(id);}else{break;}
    }selected
}
