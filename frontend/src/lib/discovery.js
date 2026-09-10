export const terminalStudy = state => !["running", "pausing"].includes(state);
export const fmt = v => typeof v === "number" && Number.isFinite(v) ? (Math.abs(v) >= 1e5 || (Math.abs(v) < .001 && v !== 0) ? v.toExponential(4) : Number(v.toPrecision(6)).toString()) : "—";
export function targets(manifest) {
  const constants = (manifest?.constants || []).map(c => ({target:`constant:${c.name}`, label:`Constant · ${c.name}`, value:c.value}));
  if (manifest?.model?.kind === "state_vector_ode") return [...constants,...manifest.model.variables.map(v => ({target:`initial:${v.name}`,label:`Initial state · ${v.name}`,value:v.initial}))];
  if (manifest?.model?.kind === "pairwise_particles") return [...constants,
    {target:"population.mass",label:"Particle mass",value:manifest.model.population.mass},
    {target:"interaction.softening",label:"Interaction softening",value:manifest.model.interaction.softening},
    {target:"interaction.linear_damping",label:"Linear damping",value:manifest.model.interaction.linear_damping}];
  return constants;
}
export function initialRecipe(manifest) {
  const observables=[...(manifest.observables||[]),...(manifest.trajectory||[]).filter(t=>!(manifest.observables||[]).some(o=>o.name===t.name))];
  const goals=(manifest.search?.objectives||[]).filter(g=>observables.some(o=>o.name===g.expression||o.name===g.name)).slice(0,3).map(g=>({metric:observables.some(o=>o.name===g.expression)?g.expression:g.name,goal:g.goal}));
  return {title:`${manifest.title.slice(0,150)} — exploration`, hypothesis:manifest.hypothesis,
    base_manifest_id:manifest.id, strategy:"latin_hypercube", parameters:manifest.search?.variables?.map(v=>({...v}))||[],
    objectives:goals.length?goals:observables[0] ? [{metric:observables[0].name,goal:"minimize"}] : [], descriptors:[],
    exploration_trials:32, validation_finalists:2, wall_seconds:900, per_trial_seconds:120, seed:42,
    absolute_tolerance:1e-6, relative_tolerance:.001, auto_review:false};
}
export function validateRecipe(r) {
  if (!r?.title?.trim()) return "Name the study.";
  if(r.parameters.length>12||r.objectives.length>3||r.descriptors.length>2) return "Use up to 12 parameters, 3 ranking metrics, and 2 descriptor axes.";
  if (!r.parameters.length) return "Choose at least one parameter and its scientifically justified bounds.";
  if (!r.objectives.length || r.objectives.some(g=>!g.metric?.trim())) return "Choose at least one measured metric to rank candidates.";
  if (new Set(r.parameters.map(p=>p.target)).size !== r.parameters.length) return "Each parameter target must be unique.";
  if (r.parameters.some(p=>!Number.isFinite(p.minimum)||!Number.isFinite(p.maximum)||p.maximum<=p.minimum||!Number.isFinite(p.maximum-p.minimum))) return "Every parameter needs finite, increasing bounds.";
  if (["map_elites","novelty_search"].includes(r.strategy)&&!r.descriptors.length) return "Diversity/novelty search needs at least one measured behavioral descriptor.";
  if(r.descriptors.some(d=>!d.metric?.trim()||!Number.isFinite(d.minimum)||!Number.isFinite(d.maximum)||d.maximum<=d.minimum||!Number.isFinite(d.maximum-d.minimum)||!Number.isInteger(d.bins)||d.bins<2||d.bins>64)) return "Descriptor ranges must be finite and increasing, with 2–64 bins.";
  if (!Number.isInteger(r.exploration_trials)||r.exploration_trials<4||r.exploration_trials>512) return "Use 4–512 exploration trials.";
  if (!Number.isInteger(r.validation_finalists)||r.validation_finalists<0||r.validation_finalists>6) return "Use 0–6 validation finalists.";
  if (!Number.isInteger(r.wall_seconds)||r.wall_seconds<10||r.wall_seconds>86400) return "Campaign time must be 10–86,400 seconds.";
  if (!Number.isInteger(r.per_trial_seconds)||r.per_trial_seconds<1||r.per_trial_seconds>3600) return "Per-trial time must be 1–3,600 seconds.";
  if (!Number.isFinite(r.absolute_tolerance)||!Number.isFinite(r.relative_tolerance)||r.absolute_tolerance<0||r.relative_tolerance<0) return "Choose nonnegative finite tolerances.";
  if (!Number.isSafeInteger(r.seed)||r.seed<0) return "Use a nonnegative integer seed.";
  return "";
}
export function activeTrial(study) { return [...(study?.trials||[])].reverse().find(t=>["queued","running"].includes(t.state)) || null; }
export function eligiblePoints(study) { return (study?.trials||[]).filter(t=>t.phase==="explore"&&t.eligible); }
export function suggestedParameter(option, index) {
  const width=Math.max(Math.abs(option.value)*.2, .1);
  return {name:`p${index+1}`,target:option.target,minimum:option.value-width,maximum:option.value+width};
}
export function downloadBlob(name, blob) {
  const url=URL.createObjectURL(blob); const link=document.createElement("a");link.href=url;link.download=name;
  document.body.appendChild(link);link.click();link.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
