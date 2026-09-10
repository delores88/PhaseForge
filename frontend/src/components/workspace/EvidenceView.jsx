import {useState} from "react";
import {AlertTriangle,CheckCircle2,HelpCircle,ShieldCheck,XCircle} from "lucide-react";
import {number,challengeStatus,statusText,exportJson} from "@/lib/lab";
import MetricChart from "./MetricChart";
export function StatusBadge({status}){
  const Icon=status==="passed"||status==="checks_passed"?CheckCircle2:status==="failed"||status==="checks_failed"?XCircle:HelpCircle;
  return <span className={`labStatus labStatus--${status}`}><Icon size={13}/>{statusText(status)}</span>;
}
export function ConstraintTable({rows=[],legacy=false}){
  return <div className="labTableWrap"><table className="labTable"><thead><tr><th>Declared constraint</th><th>Measured expression</th><th>Must be ≤</th><th>Result</th></tr></thead><tbody>{rows.map((c,i)=><tr key={`${c.name}-${i}`}><td><strong>{c.name?.replaceAll("_"," ")}</strong><small>{c.expression}</small>{c.note&&<small>{c.note}</small>}</td><td>{legacy?"Not captured":number(c.value)}</td><td>{number(c.tolerance)}</td><td><StatusBadge status={legacy?"inconclusive":c.status}/></td></tr>)}</tbody></table>{!rows.length&&<p className="labNotice">No baseline success criterion was authored.</p>}</div>;
}
export function Challenges({tests=[]}){
  return <div className="labChallenges">{tests.map((test,i)=>{
    const status=challengeStatus(test);const legacy=test.evidence_version!==2;
    return <article key={`${test.name}-${i}`} className="labChallenge"><header><div><h3>{test.name.replaceAll("_"," ")}</h3><small>{legacy?"Legacy scoring — not accepted as a validation result":`${test.trials?.length||0} recorded trials · ${test.kind||"challenge"}`}</small></div><StatusBadge status={status}/></header>
      {legacy?<p className="labNotice labNotice--warning">This older run compared objective/penalty scores. Its “survived” flag and 0% value do not establish numerical convergence or sensitivity. Replay to capture actual metric differences.</p>:<><p>{test.note}</p><div className="labTableWrap"><table className="labTable"><thead><tr><th>Measurement</th><th>Baseline</th><th>Challenged median</th><th>Largest absolute change</th><th>Comparison rule</th><th>Result</th></tr></thead><tbody>{(test.comparisons||[]).map((c,j)=><tr key={`${c.metric}-${j}`}><td>{c.metric.replaceAll("_"," ")}</td><td>{number(c.baseline)}</td><td>{number(c.challenged_median)}<small>range {number(c.challenged_min)}–{number(c.challenged_max)}</small></td><td>{number(c.maximum_absolute_change)}</td><td>{c.expectation?`${c.expectation}; abs ${number(c.absolute_tolerance)} + rel ${number(c.relative_tolerance)} × |baseline|`:"No explicit rule"}</td><td><StatusBadge status={c.status}/></td></tr>)}</tbody></table></div>
        <details className="labDetails"><summary>Inspect individual trials and their constraints</summary>{(test.trials||[]).map((trial,j)=><div className="labTrial" key={j}><strong>Trial {trial.repetition}</strong> · step {number(trial.time_step)} · perturbation {trial.perturbation==null?"none":number(trial.perturbation)} · {trial.reached_end_time?"end time reached":"END TIME NOT REACHED"}<ConstraintTable rows={trial.constraint_results||[]}/><details><summary>Raw trial measurements</summary><pre>{JSON.stringify(trial.metrics,null,2)}</pre></details></div>)}</details></>}
    </article>;
  })}{!tests.length&&<p className="labNotice">No numerical challenge was executed. A completed simulation is not a robustness verdict.</p>}</div>;
}
export default function EvidenceView({run,manifest}){
  if(!run?.result)return <div className="evidenceEmpty"><ShieldCheck size={24}/><strong>No evidence package yet</strong><p>Complete a run to inspect actual measurements and declared comparison rules.</p></div>;
  const result=run.result;const legacy=result.evidence_version!==2;
  const rows=legacy?(manifest?.constraints||[]):(result.constraint_results||[]);
  return <section className="labEvidence"><header className="labSectionHeader"><div><span className="eyebrow">REPRODUCIBLE EVIDENCE</span><h2>Inspect what was measured</h2><p>Run {run.id.slice(0,8)} · revision {run.manifest_revision} · {run.backend_used||"execution backend not recorded"}</p></div><button type="button" className="button button--secondary" onClick={()=>exportJson(`phaseforge-run-${run.id}.json`,{run,manifest})}>Export run + manifest</button></header>
    {legacy&&<p className="labNotice labNotice--warning">Legacy measurements remain readable. Old challenge badges are intentionally downgraded to inconclusive, not silently re-certified.</p>}
    <h3>Did the declared baseline checks pass?</h3><ConstraintTable rows={rows} legacy={legacy}/>
    <MetricChart series={result.series||[]}/>
    <h3>What happened when we challenged it?</h3><Challenges tests={result.falsification||[]}/>
    {!!result.warnings?.length&&<div className="labWarningList">{result.warnings.map((w,i)=><p key={i}><AlertTriangle size={14}/>{w}</p>)}</div>}
    <details className="labDetails"><summary>Advanced numerical measurements ({Object.keys(result.metrics||{}).length})</summary><div className="metricMatrix">{Object.entries(result.metrics||{}).map(([key,v])=><div key={key}><span>{key.replaceAll("_"," ")}</span><strong>{number(v)}</strong></div>)}</div></details>
    <details className="labDetails"><summary>Numerical provenance and search history</summary><pre>{JSON.stringify({numerical:result.numerical,objective_score:result.objective_score,best_candidate:result.best_candidate,search_history:result.search_history},null,2)}</pre></details>
  </section>;
}
