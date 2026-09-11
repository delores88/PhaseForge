import {CheckCircle2,CircleAlert,Eye,EyeOff,KeyRound,LockKeyhole,RefreshCw,Save,Trash2,Wallet,Cpu} from 'lucide-react';
import {useCallback,useEffect,useState} from 'react';
import Link from 'next/link';
import {ErrorState,LoadingState} from '@/components/shared/AsyncState';
import {api} from '@/lib/api';
import {invalidateModelCatalog} from '@/lib/modelCatalog.mjs';
import {version as appVersion} from '../../../package.json';

const labels={open_ai:{name:'OpenAI',keyPlaceholder:'sk-…'},anthropic:{name:'Anthropic',keyPlaceholder:'sk-ant-…'}};
const emptyForm=status=>({apiKey:'',baseUrl:status?.base_url||''});
export default function SettingsView({backend,onHardware,onEventState}) {
  const [providers,setProviders]=useState([]),[forms,setForms]=useState({}),[visible,setVisible]=useState({}),[busy,setBusy]=useState({}),[messages,setMessages]=useState({}),[error,setError]=useState(null),[loading,setLoading]=useState(true);
  const load=useCallback(async()=>{
    if(!backend.connected)return;
    try{
      setError(null);
      const [response,hardware]=await Promise.all([api.providers(),api.hardware()]);
      const rows=response.providers||[];setProviders(rows);
      setForms(current=>Object.fromEntries(rows.map(provider=>[provider.provider,{...emptyForm(provider),...current[provider.provider],apiKey:''}])));
      onHardware?.(hardware);
    }catch(value){setError(value.message);}finally{setLoading(false);}
  },[backend.connected,onHardware]);
  useEffect(()=>{load();},[load]);
  useEffect(()=>{onEventState?.(backend.connected?'http':'disconnected');},[backend.connected,onEventState]);
  const update=(provider,key,value)=>setForms(current=>({...current,[provider]:{...emptyForm(),...current[provider],[key]:value}}));
  const notify=(provider,text,tone='info')=>setMessages(current=>({...current,[provider]:{text,tone}}));
  const replace=updated=>{
    setProviders(current=>current.map(provider=>provider.provider===updated.provider?updated:provider));
    setForms(current=>({...current,[updated.provider]:emptyForm(updated)}));
    invalidateModelCatalog();window.dispatchEvent(new CustomEvent('phaseforge:providers-changed'));
  };
  async function action(provider,kind,work){
    if(busy[provider])return;
    setBusy(current=>({...current,[provider]:kind}));
    try{await work();}catch(value){notify(provider,value.message,'error');}finally{setBusy(current=>({...current,[provider]:''}));}
  }
  const saveKey=provider=>action(provider,'save',async()=>{
    const form=forms[provider]||emptyForm();
    const updated=await api.saveProviderKey(provider,{api_key:form.apiKey,base_url:form.baseUrl||null});
    replace(updated);notify(provider,'Credential and endpoint saved. Choose this account’s model in your conversation.','success');
  });
  const check=provider=>action(provider,'check',async()=>{
    notify(provider,'Checking this account’s model catalog…');
    const result=await api.providerModels(provider);
    invalidateModelCatalog();window.dispatchEvent(new CustomEvent('phaseforge:providers-changed'));
    notify(provider,`Connection verified. The account returned ${result.models?.length||0} models. No generation request was made.`,'success');
  });
  const remove=provider=>action(provider,'remove',async()=>{
    replace(await api.deleteProviderKey(provider));notify(provider,'Credential removed. Conversation choices and saved research remain available.','success');
  });
  if(loading&&backend.connected)return <LoadingState label="Loading local settings…"/>;
  if(error)return <ErrorState message={error} onRetry={load}/>;
  return <section className="settingsView settingsView--providersV3">
    <section className="surfacePanel settingsAbout" aria-label="About PhaseForge">
      <div className="aboutBrand"><img className="brandDark" src="/brand/app-icon-dark.png?v=1.1" alt="PhaseForge"/><img className="brandLight" src="/brand/app-icon-light.png?v=1.1" alt="PhaseForge"/><div><strong className="alphaNotice">Version {appVersion}</strong><h2>Serious research starts with curiosity.</h2></div></div>
      <p>Explore scientific questions, run experiments, and inspect their evidence with your AI research team and local compute.</p>
      <p>PhaseForge is free. Connect an OpenAI or Anthropic API account for AI features; the provider charges for its usage. Existing saved research and local calculations remain available without a generation request.</p>
    </section>
    <div className="securityCallout"><LockKeyhole size={20}/><div><strong>Provider connections</strong><p>Credentials stay in your operating system’s credential store. Choose models and reasoning effort in chat, separately for each conversation.</p></div></div>
    <div className="providerGrid providerGrid--wide">{providers.map(provider=>{
      const id=provider.provider,meta=labels[id]||{name:id,keyPlaceholder:'API key'},form=forms[id]||emptyForm(provider),working=!!busy[id],message=messages[id];
      return <article className="providerCard providerCard--workflow" key={id}>
        <header><div className="providerCard__mark"><KeyRound size={18}/></div><div><h2>{meta.name}</h2><div className="providerStateRow"><span className={provider.key_configured?'configured':''}>{provider.key_configured?<CheckCircle2 size={13}/>:<CircleAlert size={13}/>}Credential {provider.key_configured?'saved':'not saved'}</span></div></div></header>
        <section className="providerStep"><div className="providerStep__number"><KeyRound size={14}/></div><div className="providerStep__body">
          <label><span>API key</span><div className="secretInput"><input type={visible[id]?'text':'password'} value={form.apiKey} onChange={event=>update(id,'apiKey',event.target.value)} placeholder={provider.key_configured?'Paste to replace the key or update its endpoint':meta.keyPlaceholder} autoComplete="off" spellCheck="false" disabled={working}/><button type="button" className="iconButton" onClick={()=>setVisible(current=>({...current,[id]:!current[id]}))} aria-label={visible[id]?'Hide API key':'Show API key'}>{visible[id]?<EyeOff size={15}/>:<Eye size={15}/>}</button></div></label>
          <details className="providerAdvanced"><summary>Endpoint</summary><label><span>Base URL</span><input value={form.baseUrl} onChange={event=>update(id,'baseUrl',event.target.value)} placeholder={provider.base_url} spellCheck="false" disabled={working}/></label><small>Use the provider’s HTTPS endpoint or a trusted compatible service. Re-enter the key when changing its destination.</small></details>
          <div className="providerStep__actions">{provider.key_configured&&<button type="button" className="button button--dangerGhost" onClick={()=>remove(id)} disabled={working}><Trash2 size={14}/>Remove key</button>}<button type="button" className="button button--primary" onClick={()=>saveKey(id)} disabled={working||!form.apiKey.trim()}><Save size={14}/>{busy[id]==='save'?'Saving…':provider.key_configured?'Save connection':'Save key securely'}</button></div>
          <div className="providerStep__actions"><button type="button" className="button button--secondary" onClick={()=>check(id)} disabled={working||!provider.key_configured}><RefreshCw size={14}/>{busy[id]==='check'?'Checking…':'Check connection'}</button><small>Reads account access without generating an answer.</small></div>
        </div></section>
        {message?.text&&<div className={`providerMessage providerMessage--${message.tone}`} role={message.tone==='error'?'alert':'status'}>{message.tone==='error'?<CircleAlert size={15}/>:<CheckCircle2 size={15}/>}<span>{message.text}</span></div>}
      </article>;
    })}</div>
    <section className="surfacePanel settingsPolicy"><header className="surfacePanel__header"><div><h2>Usage and local resources</h2><p>Manage spending and compute independently of your conversation’s model.</p></div></header><div className="boundaryGrid"><div><Wallet size={20}/><h3>Usage & cost</h3><p>Inspect requests, set spending limits, or pause paid work.</p><Link href="/usage/" className="button button--secondary">Open usage controls</Link></div><div><Cpu size={20}/><h3>Local compute</h3><p>Inspect available processors, memory, and engine readiness.</p><Link href="/hardware/" className="button button--secondary">Open hardware</Link></div></div></section>
  </section>;
}
