//! Evidence is separate from optimization. Missing comparisons never become passes.
use std::collections::{BTreeMap, HashMap};
use serde::Serialize;
use crate::{domain::{ConstraintSpec, FalsificationSpec}, sandbox::Expression};

#[derive(Debug, Clone, Serialize)]
pub struct ConstraintResult {
    pub name: String,
    pub expression: String,
    pub value: Option<f64>,
    pub tolerance: f64,
    pub status: String,
    pub note: String,
}

pub fn constraints(specs: &[ConstraintSpec], context: &HashMap<String, f64>) -> Vec<ConstraintResult> {
    specs.iter().map(|spec| {
        let evaluated = Expression::parse(&spec.expression).and_then(|expr| expr.eval(context));
        let (value, status, note) = match evaluated {
            Ok(v) if v.is_finite() => (Some(v), if v <= spec.tolerance { "passed" } else { "failed" }, String::new()),
            Ok(_) => (None, "inconclusive", "Non-finite constraint measurement".to_owned()),
            Err(error) => (None, "inconclusive", format!("Constraint could not be evaluated: {error}")),
        };
        ConstraintResult { name: spec.name.clone(), expression: spec.expression.clone(), value,
            tolerance: spec.tolerance, status: status.to_owned(), note }
    }).collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct ChallengeTrial {
    pub repetition: usize,
    pub perturbation: Option<f64>,
    pub time_step: f64,
    pub reached_end_time: bool,
    pub metrics: BTreeMap<String, f64>,
    pub constraint_results: Vec<ConstraintResult>,
}
#[derive(Debug, Clone, Serialize)]
pub struct MetricComparison {
    pub metric: String,
    pub baseline: Option<f64>,
    pub challenged_min: Option<f64>,
    pub challenged_median: Option<f64>,
    pub challenged_max: Option<f64>,
    pub maximum_absolute_change: Option<f64>,
    pub relative_change: Option<f64>,
    pub expectation: Option<String>,
    pub absolute_tolerance: Option<f64>,
    pub relative_tolerance: Option<f64>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct FalsificationResult {
    pub evidence_version: u32,
    pub name: String,
    pub kind: String,
    pub status: String,
    /// None means not adjudicated, NOT true and not false.
    pub survived: Option<bool>,
    pub comparisons: Vec<MetricComparison>,
    pub trials: Vec<ChallengeTrial>,
    pub note: String,
}

/// Comparisons use declared metrics and explicit absolute/relative tolerances.
/// All repetitions must meet a rule. Medians are display summaries, not pass gates.
pub fn compare(test: &FalsificationSpec, baseline: &BTreeMap<String, f64>, trials: Vec<ChallengeTrial>, note: String) -> FalsificationResult {
    let mut keys: Vec<String> = if test.checks.is_empty() {
        baseline.keys().filter(|k| !k.starts_with("initial_") && !k.starts_with("final_") &&
            !k.starts_with("delta_") && !k.starts_with("abs_delta_") &&
            !["steps", "sample_count", "elapsed_time", "t"].contains(&k.as_str()))
            .take(64).cloned().collect()
    } else { test.checks.iter().map(|c| c.metric.clone()).collect() };
    keys.sort(); keys.dedup();
    let comparisons = keys.iter().map(|key| {
        let base = baseline.get(key).copied().filter(|v| v.is_finite());
        let mut values = trials.iter().filter_map(|trial| trial.metrics.get(key).copied()).filter(|v| v.is_finite()).collect::<Vec<_>>();
        values.sort_by(f64::total_cmp);
        let rule = test.checks.iter().find(|c| c.metric == *key);
        let complete = !trials.is_empty() && values.len() == trials.len() && trials.iter().all(|trial| trial.reached_end_time);
        let maximum_change = base.and_then(|b| values.iter().map(|v| (v-b).abs()).reduce(f64::max));
        let status = if !complete || base.is_none() { "inconclusive" } else if let (Some(b), Some(check)) = (base, rule) {
            let allowed = check.absolute_tolerance + check.relative_tolerance * b.abs();
            let passed = values.iter().all(|v| match check.expectation.as_str() {
                "stable" => (v-b).abs() <= allowed,
                "change" => (v-b).abs() > allowed,
                "decrease" => b-v > allowed,
                "increase" => v-b > allowed,
                _ => false,
            });
            if passed { "passed" } else { "failed" }
        } else { "not_adjudicated" };
        MetricComparison { metric: key.clone(), baseline: base, challenged_min: values.first().copied(),
            challenged_median: values.get(values.len()/2).copied(), challenged_max: values.last().copied(),
            maximum_absolute_change: maximum_change,
            relative_change: base.filter(|v| v.abs() > 1.0e-12).and_then(|b| maximum_change.map(|v| v/b.abs())),
            expectation: rule.map(|c| c.expectation.clone()), absolute_tolerance: rule.map(|c| c.absolute_tolerance),
            relative_tolerance: rule.map(|c| c.relative_tolerance), status: status.to_owned() }
    }).collect::<Vec<_>>();
    let failed_constraint=trials.iter().any(|t|t.constraint_results.iter().any(|c|c.status=="failed"));
    let incomplete_constraint=trials.iter().any(|t|t.constraint_results.iter().any(|c|!matches!(c.status.as_str(),"passed"|"failed")));
    let status = if failed_constraint || comparisons.iter().any(|c| c.status == "failed") { "failed" }
        else if !incomplete_constraint && !test.checks.is_empty() && !comparisons.is_empty() && comparisons.iter().all(|c| c.status == "passed") { "passed" }
        else { "inconclusive" };
    FalsificationResult { evidence_version: 2, name: test.name.clone(), kind: format!("{:?}", test.kind),
        status: status.to_owned(), survived: match status { "passed" => Some(true), "failed" => Some(false), _ => None },
        comparisons, trials,
        note: format!("{note} {} This measures only the stated checks, not truth, novelty, chaos, or convergence order. Near-zero baselines use absolute differences; relative change is omitted.",
            if test.checks.is_empty() { "No explicit comparison rule was authored; measurements are retained and the verdict is INCONCLUSIVE." }
            else { "Every retained repetition must satisfy each declared comparison rule." }) }
}

pub fn reached_end(metrics: &BTreeMap<String, f64>, start: f64, end: f64) -> bool {
    metrics.get("elapsed_time").map(|t| (t-(end-start)).abs() <= 1e-9 * (end-start).abs().max(1.0)).unwrap_or(false)
}

/// Alternating signed perturbations are distinct and fully recorded. No claim of randomness.
pub fn perturbation(magnitude: f64, repetition: usize) -> f64 {
    magnitude * (repetition/2 + 1) as f64 * if repetition % 2 == 0 { 1.0 } else { -1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ChallengeCheck, FalsificationKind};
    fn spec() -> FalsificationSpec { FalsificationSpec { name: "test".into(), kind: FalsificationKind::StepHalving,
        target: None, magnitude: 0.0, repetitions: 1, checks: vec![] } }
    fn trial(value: f64) -> ChallengeTrial { ChallengeTrial { repetition: 1, perturbation: None, time_step: 0.01,
        reached_end_time: true, metrics: BTreeMap::from([("measurement".into(), value)]), constraint_results: vec![] } }
    #[test] fn no_rule_never_passes() {
        let r=compare(&spec(), &BTreeMap::from([("measurement".into(),0.0)]), vec![trial(0.0)], String::new());
        assert_eq!(r.status,"inconclusive"); assert_eq!(r.survived,None);
    }
    #[test] fn explicit_rule_uses_every_trial() {
        let mut s=spec(); s.checks.push(ChallengeCheck { metric:"measurement".into(), expectation:"stable".into(), absolute_tolerance:0.1, relative_tolerance:0.0 });
        let r=compare(&s,&BTreeMap::from([("measurement".into(),1.0)]),vec![trial(1.0),trial(4.0)],String::new());
        assert_eq!(r.status,"failed");
    }
    #[test] fn near_zero_is_not_a_fabricated_percent() {
        let r=compare(&spec(), &BTreeMap::from([("measurement".into(),0.0)]), vec![trial(0.001)], String::new());
        assert_eq!(r.comparisons[0].relative_change,None);
        assert_eq!(r.comparisons[0].maximum_absolute_change,Some(0.001));
    }
    #[test] fn missing_measurement_is_inconclusive() {
        let mut s=spec(); s.checks.push(ChallengeCheck { metric:"missing".into(), expectation:"stable".into(), absolute_tolerance:1.0, relative_tolerance:0.0 });
        let r=compare(&s,&BTreeMap::new(),vec![trial(0.0)],String::new()); assert_eq!(r.status,"inconclusive");
    }
    #[test] fn truncated_run_never_passes() {
        let mut s=spec(); s.checks.push(ChallengeCheck { metric:"measurement".into(), expectation:"stable".into(), absolute_tolerance:1.0, relative_tolerance:0.0 });
        let mut t=trial(0.0); t.reached_end_time=false;
        assert_eq!(compare(&s,&BTreeMap::from([("measurement".into(),0.0)]),vec![t],String::new()).status,"inconclusive");
    }
    #[test] fn actual_constraint_direction_is_preserved() {
        let spec=ConstraintSpec{name:"bound".into(),expression:"x-2".into(),tolerance:0.0};
        assert_eq!(constraints(&[spec],&HashMap::from([("x".into(),3.0)]))[0].status,"failed");
    }
    #[test] fn perturbations_are_distinct() { for (i, expected) in [1e-6,-1e-6,2e-6,-2e-6,3e-6].iter().enumerate() { assert!((perturbation(1e-6,i)-expected).abs()<1e-18); } }
}

/// Derive each level from the original grid. Legacy StepHalving stays unchanged.
pub fn refinement(base:&crate::domain::IntegrationSpec,level:usize,cap:usize)->anyhow::Result<crate::domain::IntegrationSpec>{
    anyhow::ensure!(level<4,"at most four refinement levels");
    let mut next=base.clone();let factor=1usize<<(level+1);
    next.time_step=base.time_step/factor as f64;
    let required=((base.end_time-base.start_time)/next.time_step).ceil();
    anyhow::ensure!(required.is_finite()&&required<=cap as f64&&next.time_step>0.0,"refinement exceeds the solver step budget");
    next.max_steps=(required as usize).saturating_add(1).min(cap);
    next.output_stride=base.output_stride.saturating_mul(factor).max(1);Ok(next)
}
