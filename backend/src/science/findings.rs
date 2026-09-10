//! Human-readable, deterministic findings. An LLM may explain these but cannot change them.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use crate::domain::{ExperimentManifest, RunRecord};

pub fn build(run: &RunRecord, manifest: &ExperimentManifest) -> Value {
    let result = run.result.as_ref().unwrap_or(&Value::Null);
    let version = result.get("evidence_version").and_then(Value::as_u64).unwrap_or(1);
    let constraints = result.get("constraint_results").and_then(Value::as_array).cloned().unwrap_or_default();
    let tests = result.get("falsification").and_then(Value::as_array).cloned().unwrap_or_default();
    let passed = constraints.iter().filter(|c|c["status"]=="passed").count();
    let failed = constraints.iter().filter(|c|c["status"]=="failed").count();
    let challenge_passed = tests.iter().filter(|t|t["evidence_version"]==2 && t["status"]=="passed").count();
    let challenge_failed = tests.iter().filter(|t|t["evidence_version"]==2 && t["status"]=="failed").count();
    let challenge_unknown = tests.len() - challenge_passed - challenge_failed;
    let complete = result.pointer("/numerical/reached_end_time").and_then(Value::as_bool);
    let verdict = if failed>0 || challenge_failed>0 { "checks_failed" }
        else if version<2 || constraints.len()!=manifest.constraints.len() || tests.len()!=manifest.falsification.len() || complete!=Some(true) || run.status!=crate::domain::RunStatus::Completed ||
            challenge_unknown>0 || constraints.iter().any(|c|c["status"]!="passed") || (constraints.is_empty() && tests.is_empty()) { "inconclusive" }
        else { "checks_passed" };
    let title = match verdict {
        "checks_passed" => "The declared numerical checks passed",
        "checks_failed" => "At least one declared check failed",
        _ => "Useful measurements; validation is incomplete",
    };
    let summary = if run.result.is_none() { "This run has no completed evidence package yet.".to_owned() }
        else { format!("{} baseline constraints passed; {} failed. {} challenge checks passed, {} failed, and {} need a valid comparison rule or more evidence. These are checks of the authored model, not proof of the scientific hypothesis.",passed,failed,challenge_passed,challenge_failed,challenge_unknown) };
    let metrics = result.get("metrics").cloned().unwrap_or_else(||json!({}));
    let mut primary = manifest.observables.iter().map(|o|json!({"name":o.name,"label":o.name.replace('_'," "),"unit":o.unit,
        "expression":o.expression,"value":metrics.get(&o.name)})).collect::<Vec<_>>();
    primary.extend(manifest.trajectory.iter().filter(|m|!manifest.observables.iter().any(|o|o.name==m.name)).map(|m|json!({"name":m.name,"label":m.name.replace('_'," "),"unit":m.unit,"expression":m.expression,"reducer":m.reducer,"value":metrics.get(&m.name),"event_observed":metrics.get(&format!("{}_observed",m.name))})));
    if primary.is_empty() {
        primary = metrics.as_object().into_iter().flatten().filter(|(k,_)| !k.starts_with("final_") && !k.starts_with("delta_") && !k.starts_with("abs_delta_")).take(8)
            .map(|(k,v)|json!({"name":k,"label":k.replace('_'," "),"unit":"","value":v,"expression":""})).collect();
    }
    let mut gaps = Vec::<String>::new();
    if version<2 { gaps.push("Legacy result: objective-score survival flags are not accepted as scientific validation. Replay the original manifest to capture metric comparisons and constraint results.".into()); }
    if complete==Some(false) { gaps.push("The solver stopped before the requested end time. Increase the allowed step count or shorten the interval before comparing endpoints.".into()); }
    if manifest.constraints.is_empty() { gaps.push("No baseline constraint was authored. Good-looking measurements alone do not define success.".into()); }
    if tests.is_empty() { gaps.push("No numerical challenge was executed.".into()); }
    if challenge_unknown>0 { gaps.push("One or more challenges lack explicit comparison rules or complete measurements; green success flags are withheld.".into()); }
    if result.get("warnings").and_then(Value::as_array).map(|a|!a.is_empty()).unwrap_or(false) { gaps.push("The numerical runtime reported warnings. Review Numerical details.".into()); }
    if !manifest.search.enabled {gaps.push("This is one trajectory, not a discovery portfolio. Use Research plan or Discovery to define meaningful search bounds and retain diverse candidates.".into());}
    if !manifest.trajectory.is_empty(){gaps.push("Trajectory reducers use each integration-step endpoint, not exact continuous extrema or collision roots. First-passage values are censored at the horizon unless NAME_observed=1. Refine narrow encounters before drawing conclusions.".into());}
    for test in &tests {
        if test["kind"]=="StepHalving" {
            let steps=test["trials"].as_array().into_iter().flatten().filter_map(|t|t["time_step"].as_f64()).collect::<Vec<_>>();
            if steps.len()>1&&steps.iter().all(|h|(*h-steps[0]).abs()<steps[0].abs()*1e-12){gaps.push("A legacy challenge repeated the same half-step grid. Its name does not establish h/h2/h4 evidence. Create a new ResolutionLadder revision; historical results are not rewritten.".into());}
        }
    }
    gaps.push("Novelty has not been assessed against literature or a solution catalogue. No novelty claim is made.".into());
    let next_steps = vec![
        json!({"id":"finer","label":"Finer steps & run","kind":"direct","operation":"finer_steps","role":"builder",
            "reason":"No model call. Re-evaluates this seeded experiment from the same initial conditions using half-size integration steps, within the same compute caps. Not independent confirmation.","prompt":""}),
        json!({"id":"longer","label":"Longer horizon & run","kind":"direct","operation":"longer_horizon","role":"builder",
            "reason":"No model call. Starts this seeded experiment again over twice the horizon, within the same compute caps. Not continuation from the last state; original acceptance rules remain visible.","prompt":""}),
        json!({"id":"replay","label":"Replay with complete evidence","kind":"run","role":"falsifier",
            "reason":"Re-evaluate the original immutable manifest with the corrected evidence recorder. No model tokens; local compute only.",
            "prompt":""}),
        json!({"id":"validate","label":"Build stronger checks","kind":"proposal","role":"falsifier",
            "reason":"Choose meaningful observables, explicit constraints, and per-challenge metric checks. A proposed revision waits for your approval.",
            "prompt":"Review the selected run's measured evidence and the original immutable manifest. Repair incomplete validation without changing its scientific question. Give each challenge explicit checks: metric, expectation (stable/change/decrease/increase), absolute_tolerance and relative_tolerance. Use absolute tolerances for near-zero baselines; justify the chosen limits rather than applying a universal percentage. A single step-halving comparison is not proof of convergence order. Preserve a bounded baseline and distinguish numerical verification from hypothesis and novelty. Author a full revised manifest, but do not run it yet."}),
        json!({"id":"explore","label":"Build a changed experiment","kind":"proposal","role":"explorer",
            "reason":"Propose a small search around defensible parameters after reviewing the validation gaps. Nothing is submitted until you approve.",
            "prompt":"Use the selected run as the baseline. Propose an informative bounded discovery search with explicit objectives, constraints, search ranges, compute budget and comparison checks. Explain what new evidence this could yield and what it cannot establish. Do not conflate a known-example reproduction with discovery. Preserve original inputs and create a new revision. Do not run yet."}),
        json!({"id":"refine","label":"Check numerical resolution","kind":"proposal","role":"builder",
            "reason":"Plan a resolution/refinement study with the same physical setup and reported metric differences.",
            "prompt":"Build a resolution study from the selected run's immutable manifest. Preserve the governing equations and initial state, complete the requested time horizon, and compare actual observables and endpoint states rather than the optimization penalty score. Use kind=resolution_ladder with repetitions=2 for distinct h/2 and h/4 replays plus the baseline h. Include explicit comparisons and justify tolerances; legacy step_halving repeats h/2. Use visualization.entities for all relevant bodies/state traces and velocity expressions where meaningful. Do not execute yet."}),
    ];
    let evidence_hash = format!("{:x}",Sha256::digest(serde_json::to_vec(&json!({"run_id":run.id,"manifest_id":manifest.id,
        "metrics":metrics,"constraints":constraints,"challenges":tests})).unwrap_or_default()));
    json!({"schema_version":"phaseforge-findings/2","run_id":run.id,"manifest_id":manifest.id,"revision":manifest.revision,
        "hypothesis":manifest.hypothesis,"question":manifest.question,"scientific_boundary":manifest.scientific_boundary,
        "title":title,"verdict":verdict,"summary":summary,"primary_metrics":primary,
        "constraint_results":constraints,"challenges":tests,"validation_gaps":gaps,"next_steps":next_steps,
        "novelty":"not_assessed","source_evidence_version":version,"evidence_hash":evidence_hash,
        "run_status":run.status,"execution_backend":run.backend_used,
        "scope":"Numerical checks of the authored model only. No scientific truth or discovery adjudication.",
        "requested_end_time":manifest.integration.end_time,"actual_end_time":result.pointer("/numerical/actual_end_time"),
        "search_enabled":manifest.search.enabled,"objective_count":manifest.search.objectives.len()})
}
