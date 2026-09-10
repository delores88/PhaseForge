// Pure protocol/view helpers: no inference, network, or automatic model calls.
export const terminalVerification = state => !['running','stopping'].includes(state);
export function initialVerification(study,trial) {
  const metrics=[...new Set(study.recipe.objectives.map(g=>g.metric))].slice(0,8);
  return {study_id:study.id,trial_id:trial.id,question:'Does this candidate survive independent implementation checks, new perturbations, and comparison with documented prior results?',
    metrics:metrics.map(metric=>{const descriptor=study.recipe.descriptors.find(d=>d.metric===metric);return {
      metric,scale:descriptor?descriptor.maximum-descriptor.minimum:Math.max(1,Math.abs(trial.metrics[metric]||0)),
      absolute_tolerance:study.recipe.absolute_tolerance,relative_tolerance:study.recipe.relative_tolerance,
      robustness_absolute:Math.max(1e-6,Math.abs(trial.metrics[metric]||0)*.05)};}),
    holdout_replicates:4,perturb_fraction:.01,holdout_seed:(study.recipe.seed+104729)%Number.MAX_SAFE_INTEGER,
    wall_seconds:600,max_rhs_evaluations:5000000,solver_absolute_tolerance:1e-10,solver_relative_tolerance:1e-8,
    duplicate_distance:.01,controls:[],reference_ids:[]};
}
export function validateVerification(p,study) {
  if(!p?.question?.trim()||p.question.trim().length<8)return 'Write the question this verification should answer.';
  if(!p.metrics?.length||p.metrics.length>8||new Set(p.metrics.map(r=>r.metric)).size!==p.metrics.length)return 'Choose 1–8 distinct measured metrics.';
  for(const r of p.metrics){if(!r.metric?.trim()||!Number.isFinite(r.scale)||r.scale<=0)return 'Every metric needs a positive, finite comparison scale.';
    for(const k of ['absolute_tolerance','relative_tolerance','robustness_absolute'])if(!Number.isFinite(r[k])||r[k]<0)return 'Metric tolerances must be finite and nonnegative.';}
  if(!Number.isSafeInteger(p.holdout_seed)||p.holdout_seed<0||p.holdout_seed===study.recipe.seed)return 'Use a new nonnegative integer holdout seed, different from exploration.';
  if(!Number.isInteger(p.holdout_replicates)||p.holdout_replicates<2||p.holdout_replicates>16)return 'Choose 2–16 new perturbations.';
  if(!Number.isFinite(p.perturb_fraction)||p.perturb_fraction<=0||p.perturb_fraction>.25)return 'Perturb by more than zero and no more than 25% of each allowed parameter range.';
  if(!Number.isInteger(p.wall_seconds)||p.wall_seconds<5||p.wall_seconds>3600)return 'Independent-worker wall budget must be 5–3,600 seconds.';
  if(!Number.isInteger(p.max_rhs_evaluations)||p.max_rhs_evaluations<100||p.max_rhs_evaluations>20000000)return 'RHS evaluation budget must be 100–20,000,000.';
  if(![p.solver_absolute_tolerance,p.solver_relative_tolerance].every(v=>Number.isFinite(v)&&v>0&&v<=.01))return 'Adaptive solver tolerances must be positive and no greater than .01.';
  if(!Number.isFinite(p.duplicate_distance)||p.duplicate_distance<0||p.duplicate_distance>1)return 'Comparison threshold must be in [0,1].';
  if(p.controls.length>4||new Set(p.controls.map(c=>c.run_id)).size!==p.controls.length)return 'Use at most four distinct controls.';
  if(p.controls.some(c=>!c.run_id||!c.metric?.trim()||!Number.isFinite(c.minimum)||!Number.isFinite(c.maximum)||c.minimum>c.maximum||c.rationale.trim().length<8))return 'Each control needs a completed run, metric, expected interval and rationale.';
  return '';
}
export const taskProgress=d=>({completed:d?.completed_tasks||0,total:d?.total_tasks||0,determinate:Boolean(d?.total_tasks),label:d?.stage||'Not started'});
export const displayStatus=s=>String(s||'not recorded').replaceAll('_',' ');
export function externalMeasurements(text,rules){
  const obj=JSON.parse(text);if(!obj||Array.isArray(obj)||typeof obj!=='object')throw Error('Enter a JSON object of metric names and numerical values.');
  if(Object.keys(obj).length!==rules.length||rules.some(r=>!Number.isFinite(obj[r.metric])))throw Error('Provide exactly one finite value for each selected metric.');
  return obj;
}
