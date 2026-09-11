//! Native unit tests: executed by Cargo/CI, never selectable application experiments.
use super::{types::*,math,service,notebook};
use crate::{domain::{ExperimentManifest,ExperimentManifestDraft,ObjectiveGoal,SearchVariableSpec},persistence::Database};
use chrono::Utc;
use uuid::Uuid;
fn study()->Study {
    let draft:ExperimentManifestDraft=serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
    let base=ExperimentManifest::from_draft(Uuid::new_v4(),None,1,draft,"test only");let now=Utc::now();
    Study{id:Uuid::new_v4(),project_id:base.project_id,recipe:StudyRecipe{title:"Contract test".into(),hypothesis:"No scientific claim".into(),base_manifest_id:base.id,strategy:Strategy::MapElites,
        parameters:vec![SearchVariableSpec{name:"p".into(),target:"initial:x".into(),minimum:0.0,maximum:1.0}],objectives:vec![MetricGoal{metric:"state".into(),goal:ObjectiveGoal::Minimize}],
        descriptors:vec![Descriptor{metric:"behavior".into(),minimum:0.0,maximum:1.0,bins:4}],exploration_trials:16,validation_finalists:2,wall_seconds:60,per_trial_seconds:10,seed:17,absolute_tolerance:1e-8,relative_tolerance:1e-3,auto_review:false,review_model:None},
        recipe_hash:"fixture".into(),base_manifest:base,state:"draft".into(),stage:"".into(),trials:vec![],finalist_ids:vec![],finalists_frozen:false,elapsed_seconds:0.0,review_count:0,events:vec![],created_at:now,updated_at:now}
}
fn trial(s:&Study,index:usize,score:f64,behavior:f64)->Trial {
    let metrics=std::collections::BTreeMap::from([("state".into(),score),("behavior".into(),behavior)]);
    Trial{id:Uuid::new_v4(),index,phase:"explore".into(),parent_trial_id:None,manifest_id:None,run_id:None,values:vec![behavior],state:"completed".into(),cell:math::cell(&s.recipe,&metrics),metrics,eligible:true,reasons:vec![],created_at:Utc::now()}
}
#[test] fn lhs_stratified_reproducible(){let s=study();let a=(0..16).map(|i|math::lhs(&s.recipe,i)).collect::<Vec<_>>();let b=(0..16).map(|i|math::lhs(&s.recipe,i)).collect::<Vec<_>>();assert_eq!(a,b);let mut bins=a.iter().map(|v|(16.0*v[0]).floor() as usize).collect::<Vec<_>>();bins.sort();assert_eq!(bins,(0..16).collect::<Vec<_>>());}
#[test] fn descriptor_edges_and_outside(){let s=study();assert_eq!(trial(&s,1,0.0,0.0).cell,Some("0".into()));assert_eq!(trial(&s,1,0.0,1.0).cell,Some("3".into()));assert_eq!(trial(&s,1,0.0,1.01).cell,None);}
#[test] fn archive_preserves_best_per_cell(){let mut s=study();s.trials=vec![trial(&s,1,2.0,0.1),trial(&s,2,1.0,0.12),trial(&s,3,9.0,0.8)];let a=math::archive(&s);assert_eq!(a["0"],1);assert_eq!(a["3"],2);}
#[test] fn invalid_candidate_cannot_be_elite(){let mut s=study();s.trials=vec![trial(&s,1,2.0,0.1),trial(&s,2,1.0,0.12)];s.trials[1].eligible=false;assert_eq!(math::archive(&s)["0"],0);}
#[test] fn pareto_direction_and_ties(){let mut s=study();s.recipe.objectives.push(MetricGoal{metric:"behavior".into(),goal:ObjectiveGoal::Maximize});s.trials=vec![trial(&s,1,1.0,0.1),trial(&s,2,2.0,0.8),trial(&s,3,3.0,0.1)];let p=math::pareto(&s);assert!(p.contains(&s.trials[0].id));assert!(p.contains(&s.trials[1].id));assert!(!p.contains(&s.trials[2].id));}
#[test] fn map_mutation_bounded_reproducible(){let mut s=study();s.trials=vec![trial(&s,1,1.0,0.3)];let a=math::next_values(&s,8);assert_eq!(a,math::next_values(&s,8));assert!((0.0..=1.0).contains(&a[0]));}
#[test] fn missing_refinements_are_inconclusive(){let mut s=study();s.trials=vec![trial(&s,1,0.0,0.1)];assert_eq!(math::validation(&s,s.trials[0].id)["status"],"inconclusive");}
#[test] fn near_zero_refinement_uses_absolute_tolerance(){let mut s=study();let parent=trial(&s,1,0.0,0.1);let mut half=trial(&s,2,1e-10,0.1);half.phase="half_step".into();half.parent_trial_id=Some(parent.id);let mut quarter=half.clone();quarter.id=Uuid::new_v4();quarter.phase="quarter_step".into();quarter.metrics.insert("state".into(),2e-10);let id=parent.id;s.trials=vec![parent,half,quarter];assert_eq!(math::validation(&s,id)["status"],"agreement");s.trials[2].eligible=false;assert_eq!(math::validation(&s,id)["status"],"inconclusive");}
#[test] fn missing_target_not_silently_added(){let mut s=study();assert!(math::set_parameter(&mut s.base_manifest,"initial:missing",1.0).is_err());assert!(math::set_parameter(&mut s.base_manifest,"authored_by",1.0).is_err());}
#[test] fn parameter_materialization_does_not_mutate_source(){let s=study();let before=serde_json::to_value(&s.base_manifest).unwrap();let mut clone=s.base_manifest.clone();math::set_parameter(&mut clone,"initial:x",0.4).unwrap();assert_eq!(serde_json::to_value(&s.base_manifest).unwrap(),before);assert_ne!(serde_json::to_value(clone).unwrap(),before);}
#[test] fn malformed_recipe_rejected(){let mut s=study();assert!(service::validate_recipe(&s.recipe,&s.base_manifest).is_ok());s.recipe.parameters[0].maximum=s.recipe.parameters[0].minimum;assert!(service::validate_recipe(&s.recipe,&s.base_manifest).is_err());}
#[test] fn atomic_revision_reservations_are_unique(){let db=Database::open(std::path::Path::new(":memory:")).unwrap();let project=Uuid::new_v4();assert_eq!(db.next_manifest_revision(project).unwrap(),1);assert_eq!(db.next_manifest_revision(project).unwrap(),2);}
#[test] fn notebook_revisions_reject_stale_updates(){let db=Database::open(std::path::Path::new(":memory:")).unwrap();let p=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:Some("test".into()),question:"test notebook revisions".into()});db.put_project(&p).unwrap();let old=notebook::load(&db,p.id).unwrap();let saved=notebook::save(&db,p.id,old.clone()).unwrap();assert_eq!(saved.revision,1);assert!(notebook::save(&db,p.id,old).is_err());}
#[test] fn completed_label_does_not_certify_novelty(){let s=study();assert_eq!(math::summary(&s)["novelty"],"not_assessed");}

#[test] fn novelty_knn_is_reproducible_and_bounded(){let mut s=study();s.recipe.strategy=Strategy::NoveltySearch;s.trials=vec![trial(&s,1,3.0,0.01),trial(&s,2,2.0,0.02),trial(&s,3,1.0,0.03),trial(&s,4,5.0,0.9)];let a=math::novelty_scores(&s);assert_eq!(a,math::novelty_scores(&s));assert!(a[&s.trials[3].id]>a[&s.trials[1].id]);}
#[test] fn novelty_excludes_infeasible_and_outside_behavior(){let mut s=study();s.recipe.strategy=Strategy::NoveltySearch;s.trials=vec![trial(&s,1,0.0,0.1),trial(&s,2,0.0,0.4),trial(&s,3,0.0,1.2),trial(&s,4,0.0,0.8)];s.trials[3].eligible=false;let a=math::novelty_scores(&s);assert_eq!(a.len(),2);assert!(!a.contains_key(&s.trials[3].id));}
#[test] fn novelty_retains_quality_control_and_diverse_finalist(){let mut s=study();s.recipe.strategy=Strategy::NoveltySearch;s.trials=vec![trial(&s,1,1.0,0.01),trial(&s,2,2.0,0.02),trial(&s,3,3.0,0.9)];let chosen=math::diverse_finalists(&s);assert_eq!(chosen,vec![s.trials[0].id,s.trials[2].id]);}
#[test] fn novelty_mutations_stay_inside_recipe(){let mut s=study();s.recipe.strategy=Strategy::NoveltySearch;s.trials=vec![trial(&s,1,1.0,0.1),trial(&s,2,2.0,0.9)];for index in 4..s.recipe.exploration_trials{let a=math::next_values(&s,index);assert_eq!(a,math::next_values(&s,index));assert!((0.0..=1.0).contains(&a[0]));}}
#[test] fn novelty_requires_declared_behavior(){let mut s=study();s.recipe.strategy=Strategy::NoveltySearch;s.recipe.descriptors.clear();assert!(service::validate_recipe(&s.recipe,&s.base_manifest).is_err());}
#[test] fn novelty_zero_finalist_budget_is_obeyed(){let mut s=study();s.recipe.strategy=Strategy::NoveltySearch;s.recipe.validation_finalists=0;s.trials=vec![trial(&s,1,1.,0.1),trial(&s,2,2.,0.9)];assert!(math::diverse_finalists(&s).is_empty());}
