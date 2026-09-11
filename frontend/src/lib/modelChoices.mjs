export const MODEL_SELECTION_KEY = 'phaseforge.modelSelections.v2';
export const EMPTY_SELECTION = Object.freeze({provider:'open_ai',model:'',reasoning_effort:null,research_mode:false});

// Catalog membership establishes access; the backend's maintained capability
// metadata establishes ordering. Names only exclude incompatible API families.
export function eligibleModels(models = []) {
  const seen = new Set();
  const eligible = models.filter(model => {
    if (!model || typeof model.id !== 'string' || seen.has(model.id)) return false;
    seen.add(model.id);
    if (!Number.isFinite(model.capability_rank) || model.capability_rank <= 0) return false;
    if (model.available === false || model.supports_tools === false || model.supports_text === false) return false;
    return !/(embedding|moderation|whisper|\btts\b|audio|realtime|transcrib|dall-e|image|sora|search|deep-research)/i.test(model.id);
  });
  return eligible;
}

export function shortlistModels(models = [], count = 6) {
  // IDs break equal-rank ties only; they never establish capability.
  const eligible=eligibleModels(models),ids=new Set(eligible.map(model=>model.id));
  return eligible.filter(model=>{const base=model.id.replace(/-(?:\d{4}-\d{2}-\d{2}|\d{8})$/,'');return base===model.id||!ids.has(base);})
    .sort((a,b) => b.capability_rank-a.capability_rank || a.id.localeCompare(b.id))
    .slice(0,Math.max(1,Math.min(6,count)))
    .sort((a,b) => a.capability_rank-b.capability_rank || a.id.localeCompare(b.id));
}

export function selectModel(selection, provider, model) {
  const efforts = model.reasoning_efforts || [];
  const preferred = efforts.includes(model.recommended_reasoning) ? model.recommended_reasoning : efforts.at(-1) || null;
  return {...EMPTY_SELECTION,...selection,provider,model:model.id,reasoning_effort:preferred};
}

export function selectionProblem(selection, models, catalogReady = true) {
  if (!selection?.model) return 'Choose a model for this conversation.';
  if (!catalogReady) return 'Checking model access…';
  const model = eligibleModels(models).find(item => item.id === selection.model);
  if (!model) return `${selection.model} is no longer in the eligible account catalog. Choose an available model; no replacement has been selected.`;
  if (selection.reasoning_effort && !(model.reasoning_efforts || []).includes(selection.reasoning_effort)) return `Choose a supported reasoning effort for ${model.display_name || model.id}.`;
  return '';
}

export function normalizeSelection(value) {
  if (!value || !['open_ai','anthropic'].includes(value.provider)) return {...EMPTY_SELECTION};
  return {provider:value.provider,model:typeof value.model === 'string' ? value.model : '',
    reasoning_effort:typeof value.reasoning_effort === 'string' && value.reasoning_effort ? value.reasoning_effort : null,
    research_mode:value.research_mode === true};
}

export function readSelections(raw, legacy) {
  try {
    const parsed = JSON.parse(raw || 'null');
    if (parsed?.version === 2 && parsed.projects && typeof parsed.projects === 'object' && !Array.isArray(parsed.projects)) {
      return {version:2,projects:Object.fromEntries(Object.entries(parsed.projects).map(([id,value]) => [id,normalizeSelection(value)])),legacy:parsed.legacy?.model?normalizeSelection(parsed.legacy):null};
    }
  } catch { /* Corrupt browser preferences do not erase backend evidence. */ }
  let migration = null;
  try { const parsed = JSON.parse(legacy || 'null'); if(parsed?.model) migration = normalizeSelection(parsed); } catch {}
  return {version:2,projects:{},legacy:migration};
}
