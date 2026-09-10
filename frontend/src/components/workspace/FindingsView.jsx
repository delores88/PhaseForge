import Link from "next/link";
import {useEffect,useMemo,useRef,useState} from "react";
import {ArrowRight,BookOpen,Download,FlaskConical,LoaderCircle,ShieldAlert,Sparkles,X} from "lucide-react";
import {api} from "@/lib/api";
import {number,exportJson,finite} from "@/lib/lab";
import {ConstraintTable,StatusBadge} from "./EvidenceView";
import ChatMarkdown from "./ChatMarkdown";
import MetricChart from "./MetricChart";
export default function FindingsView({run,manifest,runs=[],manifests=[],onExplain,onContinue,onReplay,onInspect,busy,explaining,refreshToken=0,autoExplain,onAutoExplain,providerReady}){
  const [report,setReport]=useState(null),[loading,setLoading]=useState(false),[error,setError]=useState("");
  const [action,setAction]=useState(null),[instruction,setInstruction]=useState("");
  const dialogRef=useRef(null);const [retry,setRetry]=useState(0);
  useEffect(()=>{
    if(!action)return;const prior=document.activeElement;
    const node=dialogRef.current;const first=node?.querySelector('button,textarea');first?.focus();
    const key=e=>{if(e.key==='Escape'){setAction(null);return;}
      if(e.key==='Tab'){const items=[...(node?.querySelectorAll('button:not(:disabled),textarea:not(:disabled),input:not(:disabled)')||[])];
        if(!items.length)return;const a=items[0],b=items[items.length-1];
        if(e.shiftKey&&document.activeElement===a){e.preventDefault();b.focus();}else if(!e.shiftKey&&document.activeElement===b){e.preventDefault();a.focus();}}};
    document.addEventListener('keydown',key);return()=>{document.removeEventListener('keydown',key);prior?.focus?.();};
  },[action]);
  useEffect(()=>{
    setReport(null);setError("");setAction(null);
    if(!run?.id||run.status!=="completed")return;
    const controller=new AbortController();setLoading(true);
    api.findings(run.id,controller.signal).then(setReport).catch(e=>{if(e.name!=="AbortError")setError(e.message);}).finally(()=>{if(!controller.signal.aborted)setLoading(false);});
    return()=>controller.abort();
  },[run?.id,run?.status,refreshToken,retry]);
  const begin=a=>{setAction(a);setInstruction(a.prompt||"");};
  const approve=async()=>{const pending=action;setAction(null);if(pending.kind==="run")await onReplay(run);else await onContinue(run,{...pending,prompt:instruction});};
  if(!run)return <div className="findingsEmpty"><FlaskConical size={30}/><h2>Your findings will appear here</h2><p>Create a research world, ask for an experiment, review the proposed equations, and run it. The lab will distinguish measurements, validation results, AI interpretation, and your next decision.</p></div>;
  if(run.status!=="completed")return <div className="findingsEmpty"><LoaderCircle size={28} className={["queued","running"].includes(run.status)?"spin":""}/><h2>{run.status==="failed"?"The experiment did not complete":run.status==="cancelled"?"Experiment stopped":"Waiting for measurements"}</h2><p>{run.error||run.phase}</p><p>No findings are invented for unfinished work. Progress and machine resources appear above.</p></div>;
  return <section className="findingsView">
    <header className="labSectionHeader"><div><span className="eyebrow">EXPERIMENT BRIEF · REVISION {run.manifest_revision}</span><h2>{report?.title||"Reading the evidence package…"}</h2><p>{manifest?.title||run.name}</p></div>{report&&<StatusBadge status={report.verdict}/>}</header>
    {(loading||error)&&<p className="labNotice">{error||"Loading measured results and validation criteria…"}{error&&<button type="button" onClick={()=>setRetry(v=>v+1)}>Retry evidence loading</button>}</p>}
    {report&&<>
      <div className="findingsHeadline"><BookOpen size={20}/><div><strong>What we can say now</strong><p>{report.summary}</p><small>Novelty: not assessed · {report.execution_backend||"backend not recorded"} · run {run.id.slice(0,8)}</small></div></div>
      <div className="findingsMetricGrid">{report.primary_metrics.slice(0,8).map(m=><article key={m.name}><span>{m.label}</span><strong>{number(m.value)} <small>{m.unit||"model units"}</small></strong><details><summary>Definition</summary><code>{m.expression||m.name}</code></details></article>)}</div>
      <section className="aiFindings"><header><div><Sparkles size={17}/><h3>Explain this in plain language</h3></div>{!report.ai_analysis&&<button type="button" className="button button--primary" disabled={busy||!providerReady} onClick={()=>onExplain(run)}>{explaining?<LoaderCircle className="spin" size={14}/>:<Sparkles size={14}/>}Explain findings with AI</button>}</header>
        {report.ai_analysis?<><ChatMarkdown>{report.ai_analysis.text}</ChatMarkdown><footer>AI interpretation · {report.ai_analysis.model} · bound to this run and evidence hash. The measured values and check verdicts below are authoritative; AI prose is advisory.</footer></>:<><p>{explaining?"The analyst is reading this run's evidence. Its explanation will appear here and in chat.":"One paid model call will explain the measurements, limitations, and next steps. It cannot run another experiment or change the numerical verdict."}</p>{!providerReady&&<p>Configure a provider and model in Settings first. Deterministic findings remain available without an API.</p>}</>}
        <label className="labOptIn"><input type="checkbox" checked={autoExplain} onChange={e=>onAutoExplain(e.target.checked)} disabled={busy}/>Automatically explain newly completed runs I start (one paid call per run)</label>
      </section>
      <div className="findingsColumns"><section><h3>What still needs checking</h3><div className="labWarningList">{report.validation_gaps.map((gap,i)=><p key={i}><ShieldAlert size={14}/>{gap}</p>)}</div><details className="labDetails"><summary>Original hypothesis and scientific boundary</summary><p>{report.hypothesis}</p><p>{report.scientific_boundary}</p></details></section><section><h3>What should we do next?</h3><p className="labCaption">You decide. Choose a local replay now, or build a revised experiment. Research-plan completion is not required.</p><div className="nextStepGrid">{report.next_steps.map(a=><button type="button" key={a.id} onClick={()=>begin(a)} disabled={busy||(!providerReady&&!["run","direct"].includes(a.kind))}><strong>{a.label}<ArrowRight size={14}/></strong><span>{a.reason}</span></button>)}</div></section></div>
      <h3>Declared baseline criteria</h3><ConstraintTable rows={report.source_evidence_version<2?(manifest?.constraints||[]):report.constraint_results} legacy={report.source_evidence_version<2}/>
      <MetricChart series={run.result?.series||[]}/>
      <RunComparison run={run} manifest={manifest} runs={runs} manifests={manifests}/>
      <footer className="findingsFooter"><Link className="button button--primary" href={`/?panel=advanced&project=${run.project_id}&manifest=${run.manifest_id}`}>Build a discovery campaign from this run</Link><button type="button" className="button button--secondary" onClick={onInspect}>Inspect all evidence and challenges</button><button type="button" className="button button--secondary" onClick={()=>exportJson(`phaseforge-findings-${run.id}.json`,{findings:report,run,manifest})}><Download size={14}/>Export research bundle</button><small>Report version {report.schema_version} · source {report.evidence_hash?.slice(0,12)}</small></footer>
    </>}
    {action&&<div className="labModalBackdrop" onClick={()=>setAction(null)}><section className="labApprovalDialog" ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby="approval-title" onClick={e=>e.stopPropagation()}><header><h2 id="approval-title">{action.label}</h2><button type="button" aria-label="Close approval" onClick={()=>setAction(null)}><X size={18}/></button></header><p>{action.reason}</p><div className="labNotice">Source run {run.id.slice(0,8)} · immutable revision {run.manifest_revision}. {action.kind==="direct"?"This creates and runs a changed copy of the complete source experiment, using its original initial conditions, seed, acceptance rules and compute caps. No model tokens; no plan prerequisite. A longer horizon starts again from time zero/declared start, not the final frame.":action.kind==="run"?"This replays that revision unchanged. It uses local compute, not model tokens. Undefined challenge thresholds remain inconclusive.":"This requests a new executable setup with auto-run OFF, not another research plan. Use Run saved setup when ready. Existing token and repair limits still apply."}</div>{!["run","direct"].includes(action.kind)&&<label className="labInstruction">Change or add to the research direction<textarea rows={7} value={instruction} onChange={e=>setInstruction(e.target.value)}/></label>}<footer><button className="button button--secondary" type="button" onClick={()=>setAction(null)}>Cancel</button><button className="button button--primary" type="button" disabled={busy||(!["run","direct"].includes(action.kind)&&!instruction.trim())} onClick={approve}>{action.kind==="direct"?"Run this changed copy":action.kind==="run"?"Run local replay":"Build revised setup"}</button></footer></section></div>}
  </section>;
}
function RunComparison({run,manifest,runs,manifests}){
  const [reference,setReference]=useState("");const candidates=runs.filter(r=>r.id!==run.id&&r.status==="completed"&&r.result);
  const other=candidates.find(r=>r.id===reference)||null;
  const otherManifest=manifests.find(m=>m.id===other?.manifest_id);
  const rows=(manifest?.observables||[]).map(m=>{const old=otherManifest?.observables?.find(o=>o.name===m.name);const a=finite(other?.result?.metrics?.[m.name]),b=finite(run.result?.metrics?.[m.name]);const compatible=old&&old.expression===m.expression&&old.unit===m.unit&&other?.capability_id===run.capability_id;return {name:m.name,a,b,delta:compatible&&a!==null&&b!==null?b-a:null,compatible};});
  if(!candidates.length)return null;
  return <section className="runComparison"><header><h3>Compare against an earlier run</h3><select aria-label="Comparison baseline" value={reference} onChange={e=>setReference(e.target.value)}><option value="">Choose a recorded baseline</option>{candidates.map(r=><option key={r.id} value={r.id}>Revision {r.manifest_revision} · {r.id.slice(0,8)}</option>)}</select></header>{other&&<><p className="labCaption">Differences are descriptive, not improvement scores. Changed metric definitions or units are not compared.</p><div className="labTableWrap"><table className="labTable"><thead><tr><th>Measurement</th><th>Reference run</th><th>This run</th><th>Difference</th></tr></thead><tbody>{rows.map(row=><tr key={row.name}><td>{row.name.replaceAll("_"," ")}</td><td>{number(row.a)}</td><td>{number(row.b)}</td><td>{row.compatible?number(row.delta):"Definition differs / unavailable"}</td></tr>)}</tbody></table></div></>}</section>;
}
