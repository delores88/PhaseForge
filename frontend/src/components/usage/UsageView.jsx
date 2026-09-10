import { Download, Pause, Play, Plus, RefreshCw, Square, Trash2, Wallet } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "@/lib/api";
const fmt = (value) => Number(value || 0).toLocaleString();
const money = (value) => value == null ? "Unpriced" : `$${Number(value).toFixed(5)}`;
const defaultRate = { provider:"open_ai", model:"", input_per_million:0, output_per_million:0,
  cached_input_per_million:0, cache_write_5m_per_million:0, cache_write_1h_per_million:0 };
export default function UsageView({ backend }) {
  const [report,setReport]=useState(null);
  const [settings,setSettings]=useState(null);
  const [busy,setBusy]=useState(false);
  const [error,setError]=useState("");
  const [notice,setNotice]=useState("");
  const [providers,setProviders]=useState([]);
  const editing=useRef(false);
  const load=useCallback(async () => {
    if (!backend.connected) return;
    try {
      const data=await api.usage();setReport(data);
      if (!editing.current) setSettings(data.settings);
    } catch(e) { setError(e.message); }
  },[backend.connected]);
  useEffect(()=>{ load();const t=setInterval(load,2500);return()=>clearInterval(t); },[load]);
  useEffect(()=>{ if (backend.connected) api.providers().then(r=>setProviders(r.providers||[])).catch(()=>{}); },[backend.connected]);
  const update=(key,value)=>{ editing.current=true;setSettings(s=>({...s,[key]:value})); };
  const save=async(event)=>{
    event?.preventDefault(); if(!settings)return;
    setBusy(true);setNotice("");setError("");
    try { const next=await api.saveUsageSettings(settings);editing.current=false;setSettings(next);setNotice("Controls saved. They apply to new calls and every repair attempt.");await load(); }
    catch(e){setError(e.message);}finally{setBusy(false);}
  };
  const togglePause=async()=>{
    setBusy(true);
    try { const current=await api.usageSettings();const next=await api.saveUsageSettings({...current,paused:!current.paused});
      setSettings(s=>({...s,paused:next.paused}));await load();setNotice(next.paused?"Paid calls paused. Active agent requests received a cancellation signal.":"Paid calls resumed.");
    }catch(e){setError(e.message);}finally{setBusy(false);}
  };
  const stop=async(id)=>{try{if(id)await api.cancelChatRequest(id);else await api.cancelAllAgents();await load();setNotice("Cancellation requested. Remote providers may still charge work already processed.");}catch(e){setError(e.message);}};
  const addRate=()=>{
    const p=providers.find(x=>x.configured)||providers[0];
    update("rates",[...(settings.rates||[]),{...defaultRate,provider:p?.provider||"open_ai",model:p?.model||""}]);
  };
  const changeRate=(index,key,value)=>update("rates",settings.rates.map((r,i)=>i===index?{...r,[key]:value}:r));
  const exportLedger=()=>{
    const url=URL.createObjectURL(new Blob([JSON.stringify(report,null,2)],{type:"application/json"}));
    const a=document.createElement("a");a.href=url;a.download="phaseforge-usage.json";a.click();setTimeout(()=>URL.revokeObjectURL(url),2000);
  };
  if(!backend.connected) return <section className="usageEmpty"><Wallet size={30}/><h2>Connect the backend to view usage</h2><p>Usage is stored locally with the research database, not in browser storage.</p><button type="button" className="button button--secondary" onClick={backend.refresh}>Retry connection</button></section>;
  if(!report||!settings) return <div className="usageEmpty">{error||"Reading the local usage ledger…"}</div>;
  const totals=report.totals||{};
  const active=report.active_requests||[];
  return <section className="usagePage">
    <div className="usageHeading"><div><span className="eyebrow">LOCAL ACCOUNTING</span><h2>Stay in control of every model call.</h2><p>Provider-reported tokens. Your rate cards. Cancellable research and bounded repairs.</p></div><div className="usageActions">
      <button type="button" className="button button--secondary" onClick={load}><RefreshCw size={15}/>Refresh</button>
      <button type="button" className="button button--secondary" onClick={exportLedger}><Download size={15}/>Export ledger</button>
      <button type="button" className={`button ${report.settings.paused?"button--primary":"button--secondary"}`} onClick={togglePause} disabled={busy}>{report.settings.paused?<Play size={15}/>:<Pause size={15}/>} {report.settings.paused?"Resume paid calls":"Pause paid calls"}</button>
      <button type="button" className="button button--danger" onClick={()=>stop()} disabled={!active.length}><Square size={14}/>Stop all agents</button>
    </div></div>
    {error&&<div className="usageAlert usageAlert--error" role="alert"><span>{error}</span><button type="button" onClick={()=>setError("")}>Dismiss</button></div>}
    {notice&&<div className="usageAlert" role="status">{notice}</div>}
    {report.settings.paused&&<div className="usageAlert usageAlert--warning"><Pause size={15}/>Paid model calls are paused. Local molecular tools and already submitted simulations are separate; cancel simulations on the Runs page.</div>}
    <div className="usageStats">
      <Stat label="Recorded calls" value={fmt(totals.calls)} detail={`${active.length} active requests`}/>
      <Stat label="Provider-reported tokens" value={fmt(totals.total_tokens)} detail={`${fmt(totals.input_tokens)} input · ${fmt(totals.output_tokens)} output`}/>
      <Stat label="Cached input" value={fmt(totals.cached_input_tokens)} detail={`${fmt(totals.reasoning_tokens)} reasoning tokens included in output`}/>
      <Stat label={totals.unpriced_calls?"Priced subtotal (incomplete)":"Estimated cost"} value={totals.unpriced_calls===totals.calls&&totals.calls>0?"Unpriced":money(totals.estimated_cost_usd)} detail={`${totals.unpriced_calls} unpriced · ${totals.unreported_calls} without final usage`}/>
    </div>
    {(report.warnings||[]).map(w=><div className="usageAlert usageAlert--warning" key={w}>{w}</div>)}
    <section className="usagePanel"><header><div><h3>Active requests</h3><p>Stop remains available while the model responds and while proposals are validated or repaired.</p></div></header>
      {!active.length?<div className="usageInlineEmpty">No active agent requests.</div>:<div className="activeRequests">{active.map(r=><article key={r.request_id}><span className="runningDot"/><div><strong>{r.phase.replaceAll("_"," ")}</strong><small>{r.project_id?`World ${r.project_id.slice(0,8)}`:"Connection test"} · {new Date(r.started_at).toLocaleTimeString()}</small></div><button type="button" className="button button--danger" onClick={()=>stop(r.request_id)}><Square size={13}/>Stop</button></article>)}</div>}
    </section>
    <form onSubmit={save} className="usagePanel usageControls"><header><div><h3>Budgets & execution controls</h3><p>These are local admission thresholds, not a provider billing guarantee. Blank limits mean unlimited.</p></div><button className="button button--primary" disabled={busy}>Save controls</button></header>
      <div className="usageFields">
        <Field label="Output tokens per call" note="A provider may impose a lower ceiling."><input type="number" min="512" max="64000" value={settings.max_output_tokens} onChange={e=>update("max_output_tokens",Number(e.target.value))}/></Field>
        <Field label="Repair attempts" note="Each repair is another metered call."><select value={settings.repair_attempts} onChange={e=>update("repair_attempts",Number(e.target.value))}><option value="0">0 — no automatic repair</option><option value="1">1 — default</option><option value="2">2 — maximum</option></select></Field>
        <Field label="Simultaneous provider calls"><input type="number" min="1" max="8" value={settings.max_parallel_calls} onChange={e=>update("max_parallel_calls",Number(e.target.value))}/></Field>
        <Field label="Token limit" note={`${fmt(totals.budget_accounted_tokens)} tokens accounted, including reservations.`}><input type="number" min="1" value={settings.token_limit??""} placeholder="Unlimited" onChange={e=>update("token_limit",e.target.value===""?null:Number(e.target.value))}/></Field>
        <Field label="Estimated USD limit" note={`${money(totals.budget_accounted_cost_usd)} accounted on priced calls. All models need rates for cost enforcement.`}><input type="number" min="0.01" step="0.01" value={settings.cost_limit_usd??""} placeholder="Unlimited" onChange={e=>update("cost_limit_usd",e.target.value===""?null:Number(e.target.value))}/></Field>
        <Field label="Warning at % of limit"><input type="number" min="1" max="100" value={settings.warning_percent} onChange={e=>update("warning_percent",Number(e.target.value))}/></Field>
      </div>
      <label className="usageCheckbox"><input type="checkbox" checked={settings.enforce_limits} onChange={e=>update("enforce_limits",e.target.checked)}/>Block new calls and repairs that would exceed a configured limit</label>
      <p className="usageFootnote">Budget reservations include a conservative input estimate and the full output allowance. Cancelled or interrupted calls without usage retain that reservation. This ledger excludes calls made outside PhaseForge and calls before v0.3.1.</p>
      <div className="rateHeading"><div><h3>Model rate cards</h3><p>USD per 1,000,000 tokens. Enter current provider prices or your contracted rates. No prices are invented or silently updated.</p></div><button type="button" className="button button--secondary" onClick={addRate}><Plus size={14}/>Add model</button></div>
      {!settings.rates.length?<div className="usageInlineEmpty">No rate cards. Tokens will be tracked; dollars stay unpriced until you add rates.</div>:<div className="rateCards">{settings.rates.map((rate,index)=><article key={index}>
        <div className="rateIdentity"><select aria-label="Rate provider" value={rate.provider} onChange={e=>changeRate(index,"provider",e.target.value)}><option value="open_ai">OpenAI</option><option value="anthropic">Anthropic</option></select><input aria-label="Exact model ID" placeholder="Exact requested model ID" value={rate.model} onChange={e=>changeRate(index,"model",e.target.value)}/><button type="button" className="iconButton" aria-label="Remove rate" onClick={()=>update("rates",settings.rates.filter((_,i)=>i!==index))}><Trash2 size={16}/></button></div>
        <div className="rateNumbers">{[["input_per_million","Uncached input"],["output_per_million","Output"],["cached_input_per_million","Cache read"],...(rate.provider==="anthropic"?[["cache_write_5m_per_million","Cache write · 5m"],["cache_write_1h_per_million","Cache write · 1h"]]:[])].map(([key,label])=><Field key={key} label={label}><input type="number" min="0" step="any" value={rate[key]} onChange={e=>changeRate(index,key,Number(e.target.value))}/></Field>)}</div>
      </article>)}</div>}
    </form>
    <section className="usagePanel"><header><div><h3>By provider & model</h3><p>Reasoning and cached tokens are subsets, not double-counted totals.</p></div></header><div className="usageTableWrap"><table className="usageTable"><thead><tr><th>Provider / model</th><th>Calls</th><th>Tokens</th><th>Estimated USD</th></tr></thead><tbody>{(report.models||[]).map(m=><tr key={`${m.provider}:${m.model}`}><td><strong>{m.model}</strong><small>{m.provider==="open_ai"?"OpenAI":"Anthropic"}</small></td><td>{fmt(m.calls)}</td><td>{fmt(m.tokens)}</td><td>{m.unpriced?`${money(m.estimated_cost_usd)} priced subtotal · ${m.unpriced} unpriced`:money(m.estimated_cost_usd)}</td></tr>)}</tbody></table>{!report.models?.length&&<div className="usageInlineEmpty">Your first model call will appear here.</div>}</div></section>
    <section className="usagePanel"><header><div><h3>Recent call ledger</h3><p>Latest {report.ledger_limit} rows; totals include the complete stored ledger. Errors and repairs are retained.</p></div></header><div className="usageTableWrap"><table className="usageTable"><thead><tr><th>Time / purpose</th><th>Model</th><th>Status</th><th>Input</th><th>Output</th><th>Estimated USD</th><th>Details</th></tr></thead><tbody>{(report.records||[]).map(r=><tr key={r.id}>
      <td>{new Date(r.started_at).toLocaleString()}<small>{r.purpose} · attempt {r.attempt+1}</small></td><td><strong>{r.model}</strong><small>{r.project_id?`world ${r.project_id.slice(0,8)}`:"connection test"}</small></td><td><span className={`usageStatus usageStatus--${r.status}`}>{r.status.replaceAll("_"," ")}</span></td><td>{r.usage.reported?fmt(r.usage.input_tokens):"Unknown"}</td><td>{r.usage.reported?fmt(r.usage.output_tokens):"Unknown"}</td><td>{r.usage.reported?money(r.estimated_cost_usd):"Unsettled"}</td><td><details><summary>Inspect</summary><p className="usageRecordError">{r.error||"No error reported."}</p><small>Response: {r.provider_response_id||"not reported"}</small><small>Request: {r.request_id}</small><small>Cached input: {fmt(r.usage.cached_input_tokens)} · reasoning: {fmt(r.usage.reasoning_tokens)}</small><small>Reservation: {fmt(r.reserved_input_tokens+r.reserved_output_tokens)} tokens</small></details></td>
    </tr>)}</tbody></table>{!report.records?.length&&<div className="usageInlineEmpty">No calls recorded yet.</div>}</div></section>
    <p className="usageFootnote">{report.scope} Cancelling closes the local request and prevents subsequent work; it cannot undo tokens a remote provider has already processed. The provider's invoice remains authoritative.</p>
  </section>;
}
function Field({label,note,children}){return <label className="usageField"><span>{label}</span>{children}{note&&<small>{note}</small>}</label>;}
function Stat({label,value,detail}){return <article><span>{label}</span><strong>{value}</strong><small>{detail}</small></article>;}
