import {useEffect,useId,useMemo,useRef,useState} from 'react';
import {createPortal} from 'react-dom';
import {ChevronDown,Sparkles,RefreshCw,Globe2,X} from 'lucide-react';
import {useModelSelection} from '@/lib/modelSelection';
import {eligibleModels,selectModel,shortlistModels} from '@/lib/modelChoices.mjs';
import {invalidateModelCatalog,loadModelCatalog} from '@/lib/modelCatalog.mjs';
import styles from './ModelPicker.module.css';

const providerName=id=>id==='open_ai'?'OpenAI':id==='anthropic'?'Anthropic':id;
export default function ModelPicker({providers=[],disabled=false,compact=false}) {
  const {selection,setSelection,setModelCatalog,selectionError,selectionReady}=useModelSelection();
  const [open,setOpen]=useState(false),[models,setModels]=useState([]),[error,setError]=useState(''),[loading,setLoading]=useState(false),[revision,setRevision]=useState(0);
  const [position,setPosition]=useState({top:16,left:16});
  const trigger=useRef(null),panel=useRef(null),headingId=useId();
  const configured=providers.filter(item=>item.key_configured||item.configured);
  const provider=configured.find(item=>item.provider===selection.provider);
  useEffect(()=>{
    const changed=()=>{invalidateModelCatalog();setRevision(value=>value+1);};
    window.addEventListener('phaseforge:providers-changed',changed);
    return()=>window.removeEventListener('phaseforge:providers-changed',changed);
  },[]);
  useEffect(()=>{
    setModels([]);setError('');
    if(!provider){setLoading(false);setModelCatalog(selection.provider,[],'Connect the selected provider in Settings, or choose another account.');return;}
    let alive=true;setLoading(true);
    const promise=loadModelCatalog(provider.provider,provider.base_url);
    promise.then(response=>{
      if(!alive)return;
      const catalog=response.models||[];setModels(catalog);setModelCatalog(provider.provider,catalog);
    }).catch(value=>{
      if(alive){setError(value.message);setModelCatalog(provider.provider,[],value.message);}
    }).finally(()=>{if(alive)setLoading(false);});
    return()=>{alive=false;};
  },[provider?.provider,provider?.base_url,selection.provider,revision,setModelCatalog]);
  const choices=useMemo(()=>shortlistModels(models),[models]);
  const selected=useMemo(()=>eligibleModels(models).find(model=>model.id===selection.model),[models,selection.model]);
  const selectedIndex=choices.findIndex(model=>model.id===selection.model);
  const label=selected?.display_name||selection.model||'Choose a model';
  const effort=selection.reasoning_effort||'Provider default';
  const close=()=>{setOpen(false);trigger.current?.focus();};
  useEffect(()=>{
    if(!open)return;
    const key=event=>{
      if(event.key==='Escape'){event.preventDefault();setOpen(false);trigger.current?.focus();}
      if(event.key==='Tab'){
        const items=Array.from(panel.current?.querySelectorAll('button:not(:disabled),input:not(:disabled),select:not(:disabled),a[href]')||[]);
        if(!items.length)return;
        if(event.shiftKey&&document.activeElement===items[0]){event.preventDefault();items.at(-1).focus();}
        else if(!event.shiftKey&&document.activeElement===items.at(-1)){event.preventDefault();items[0].focus();}
      }
    };
    panel.current?.querySelector('input[type="range"],button')?.focus();
    window.addEventListener('keydown',key);
    return()=>window.removeEventListener('keydown',key);
  },[open]);
  const choose=model=>{if(model&&provider)setSelection(value=>selectModel(value,provider.provider,model));};
  function toggle(){
    const bounds=trigger.current?.getBoundingClientRect();
    setPosition({top:Math.max(12,Math.min((bounds?.bottom||8)+8,window.innerHeight-510)),left:Math.max(12,Math.min(bounds?.left||12,window.innerWidth-376))});
    setOpen(value=>!value);
  }
  return <div className={`${styles.root} ${compact?styles.compact:''}`}>
    <button ref={trigger} type="button" className={styles.trigger} disabled={disabled||!selectionReady} onClick={toggle} aria-expanded={open} aria-haspopup="dialog" aria-label={`Choose model for this conversation: ${label}; reasoning ${effort}`}><Sparkles size={14}/><span>{label}<small>{selection.model?effort:'For this conversation'}</small></span><ChevronDown size={13}/></button>
    <button type="button" className={`${styles.research} ${selection.research_mode?styles.active:''}`} disabled={disabled} aria-pressed={!!selection.research_mode} title="Retrieve public scientific sources for this request" onClick={()=>setSelection(value=>({...value,research_mode:!value.research_mode}))}><Globe2 size={14}/><span>Research {selection.research_mode?'on':'off'}</span></button>
    {open&&createPortal(<><div className={styles.backdrop} onClick={close}/><section ref={panel} className={styles.panel} style={position} role="dialog" aria-modal="true" aria-labelledby={headingId}>
      <header><strong id={headingId}>Model for this conversation</strong><button type="button" onClick={close} aria-label="Close model selection"><X size={17}/></button></header>
      <div className={styles.providers} aria-label="AI provider">{configured.map(item=><button type="button" key={item.provider} aria-pressed={item.provider===selection.provider} onClick={()=>{if(item.provider!==selection.provider)setSelection(value=>({...value,provider:item.provider,model:'',reasoning_effort:null}));}}>{providerName(item.provider)}</button>)}</div>
      {!configured.length?<p>Save a provider key in Settings to discover your account’s models.</p>:!provider?<p>Your selected provider is unavailable. Choose a connected provider above.</p>:<>
        <div className={styles.modelLabel}><strong>{label}</strong><span>{selection.model?effort:'Select a position below'}</span></div>
        {loading?<p role="status">Checking available models…</p>:choices.length>0?<>
          <div className={styles.sliderShell}>
            <div className={styles.stops} aria-hidden="true" style={{gridTemplateColumns:`repeat(${choices.length},1fr)`}}>{choices.map(model=><i key={model.id}/>)}</div>
            <input className={styles.slider} type="range" min="0" max={Math.max(0,choices.length-1)} step="1" value={Math.max(0,selectedIndex)} onChange={event=>choose(choices[Number(event.target.value)])} aria-label="Model capability" aria-valuetext={selectedIndex>=0?`${selectedIndex+1} of ${choices.length}: ${label}, reasoning ${effort}`:`Choose one of ${choices.length} available models`} disabled={disabled||choices.length===1}/>
          </div>
          <div className={styles.scale}><span>Fast & economical</span><span>Most capable</span></div>
          <div className={styles.modelChoices}>{choices.map((model,index)=><button type="button" key={model.id} onClick={()=>choose(model)} aria-pressed={model.id===selection.model} title={model.id}><span>{index+1}</span><strong>{model.display_name||model.id}</strong></button>)}</div>
          {selected&&<label className={styles.effort}>Reasoning effort<select value={selection.reasoning_effort||''} onChange={event=>setSelection(value=>({...value,reasoning_effort:event.target.value||null}))} disabled={disabled}><option value="">Provider default</option>{(selected.reasoning_efforts||[]).map(value=><option value={value} key={value}>{value.charAt(0).toUpperCase()+value.slice(1)}</option>)}</select></label>}
          {selected?.description&&<p>{selected.description}</p>}
          {selected&&selectedIndex<0&&<p>Your saved model is still available. The slider shows the current strongest {choices.length} choices; it has not replaced your selection.</p>}
        </>:<p>{error||'No eligible research models were returned by this account.'}</p>}
        {!!selectionError&&!loading&&<p className={styles.error} role="status">{selectionError}</p>}
        <footer><span>{choices.length} eligible choices · future requests only</span><button type="button" onClick={()=>{invalidateModelCatalog();setRevision(value=>value+1);}} disabled={loading} aria-label="Refresh account models"><RefreshCw size={14}/>Refresh</button></footer>
      </>}
    </section></>,document.body)}
  </div>;
}
