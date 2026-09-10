import {useEffect, useMemo, useState} from 'react';
import {ChevronDown, Sparkles, RefreshCw} from 'lucide-react';
import {api} from '@/lib/api';
import {useModelSelection} from '@/lib/modelSelection';

const cache = new Map();
export default function ModelPicker({providers=[], disabled=false, compact=false}) {
  const {selection,setSelection}=useModelSelection();
  const [open,setOpen]=useState(false), [models,setModels]=useState([]), [error,setError]=useState(''), [loading,setLoading]=useState(false), [revision,setRevision]=useState(0);
  const configured=providers.filter(p=>p.key_configured || p.configured);
  const provider=configured.find(p=>p.provider===selection.provider) || configured.find(p=>p.provider==='open_ai') || configured[0];
  useEffect(()=>{
    if(!provider)return;
    let alive=true;setLoading(true);setError('');
    const key=provider.provider;
    const promise=cache.get(key)||api.providerModels(key);
    cache.set(key,promise);
    promise.then(r=>{if(alive)setModels(r.models||[]);}).catch(e=>{cache.delete(key);if(alive){setModels([]);setError(e.message);}}).finally(()=>{if(alive)setLoading(false);});
    return()=>{alive=false;};
  },[provider?.provider,revision]);
  const choices=useMemo(()=>[...models].sort((a,b)=>(a.capability_rank||0)-(b.capability_rank||0)||a.id.localeCompare(b.id)),[models]);
  const sliderChoices=useMemo(()=>choices.filter(m=>!choices.some(base=>base.id!==m.id&&m.id.startsWith(base.id+'-')&&/^\d{4}-\d{2}-\d{2}$/.test(m.id.slice(base.id.length+1)))),[choices]);
  const selected=choices.find(m=>m.id===(selection.model||provider?.model));
  const recommended=choices.find(m=>m.recommended);
  const label=selected?.display_name || selection.model || provider?.model || 'Choose a model';
  const choose=m=>setSelection({provider:provider.provider,model:m.id,reasoning_effort:m.recommended_reasoning||null});
  useEffect(()=>{
    if(provider&&provider.provider!==selection.provider)setSelection({provider:provider.provider,model:provider.model||'',reasoning_effort:null});
  },[provider?.provider,selection.provider,setSelection]);
  return <div className={`modelPicker ${compact?'modelPicker--compact':''}`}>
    <button type="button" className="modelPickerTrigger" disabled={disabled} onClick={()=>setOpen(v=>!v)} aria-expanded={open} aria-label="Choose model for this turn"><Sparkles size={14}/><span>{label}</span><ChevronDown size={13}/></button>
    {open&&<div className="modelPickerPanel">
      <header><strong>Intelligence for this turn</strong><button type="button" aria-label="Refresh available models" onClick={()=>{cache.delete(provider?.provider);setRevision(v=>v+1);}} disabled={loading}><RefreshCw size={13}/></button></header>
      <div className="modelProviderTabs">{configured.map(p=><button type="button" key={p.provider} className={provider?.provider===p.provider?'active':''} onClick={()=>setSelection({provider:p.provider,model:p.model||'',reasoning_effort:null})}>{p.provider==='open_ai'?'OpenAI':'Anthropic'}</button>)}</div>
      {loading?<p>Loading your available models…</p>:choices.length>0?<>
        <div className="modelRangeLabels"><span>Fast & economical</span><span>Most capable</span></div>
        <input type="range" aria-label="Model capability" min="0" max={Math.max(0,sliderChoices.length-1)} value={Math.max(0,sliderChoices.findIndex(m=>m.id===selected?.id||selected?.id.startsWith(m.id+'-')))} onChange={e=>choose(sliderChoices[Number(e.target.value)])}/>
        <label className="modelSelectLabel">Model<select aria-label="Model for this turn" value={selected?.id||''} onChange={e=>choose(choices.find(m=>m.id===e.target.value))}>{!selected&&<option value="">Select model</option>}{choices.map(m=><option key={m.id} value={m.id}>{m.display_name}{m.recommended?' · Recommended':''}</option>)}</select></label>
        {selected?.description&&<p>{selected.description}</p>}
        {selected?.reasoning_efforts?.length>0&&<label className="modelSelectLabel">Reasoning effort<select aria-label="Reasoning effort" value={selection.reasoning_effort||selected.recommended_reasoning||''} onChange={e=>setSelection(v=>({...v,reasoning_effort:e.target.value||null}))}>{selected.reasoning_efforts.map(e=><option key={e} value={e}>{e.charAt(0).toUpperCase()+e.slice(1)}</option>)}</select></label>}
        {recommended&&<button type="button" className="modelRecommendation" onClick={()=>choose(recommended)}><Sparkles size={14}/><span>For complex simulations<strong>{recommended.display_name}{recommended.recommended_reasoning?` · ${recommended.recommended_reasoning}`:''}</strong></span></button>}
      </>:<p>{error||'Add a provider key in Settings to discover models.'}{provider?.model&&' Your saved model is still selected.'}</p>}
      <small>Changes apply to the next request. Provider API charges apply.</small>
      <button type="button" className="button button--secondary" onClick={()=>setOpen(false)}>Done</button>
    </div>}
  </div>;
}
