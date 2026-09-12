import {useEffect,useState} from 'react';
import {ArrowUpRight,CheckCircle2,ChevronDown,CircleAlert,Clock3,Download,LoaderCircle,Pause,Play,Square,Wrench,XCircle} from 'lucide-react';
import ChatMarkdown from './ChatMarkdown';
import {api} from '@/lib/api';
import {exportStatusFromJob,exportDownloadName} from '@/lib/laboratory-exports.mjs';
import {labJobPresentation,labElapsed,labDuration} from '@/lib/laboratoryPresentation.mjs';
import {laboratoryRecovery,laboratoryAttention} from '@/lib/laboratory-recovery.mjs';
import styles from './LaboratoryActivity.module.css';

function JobIcon({view}) {
  if(view.active)return <LoaderCircle className={styles.spinner} size={16} aria-label={view.label}/>;
  if(view.unread)return <span className={styles.unread} role="img" aria-label="Completed, unread result"/>;
  if(view.state==='completed')return <CheckCircle2 size={16} aria-label="Completed"/>;
  if(view.state==='paused')return <Pause size={16} aria-label="Paused"/>;
  if(view.state==='timed_out')return <Clock3 size={16} aria-label="Time limit reached"/>;
  if(view.state==='cancelled')return <XCircle size={16} aria-label="Cancelled"/>;
  return <CircleAlert size={16} aria-label={view.label}/>;
}
function EventTimeline({events=[]}) {
  const [count,setCount]=useState(40);
  const ordered=[...events].sort((a,b)=>a.sequence-b.sequence),visible=ordered.slice(-count);
  return <>{ordered.length>count&&<button type="button" className={styles.earlier} onClick={()=>setCount(value=>value+80)}>Show earlier activity ({ordered.length-count})</button>}<ol className={styles.timeline}>{visible.map(event=><li key={event.sequence}>
    <div className={styles.eventHeading}><span>{event.kind?.replaceAll('_',' ')}</span><time dateTime={event.at}>{new Date(event.at).toLocaleTimeString([],{hour:'2-digit',minute:'2-digit',second:'2-digit'})}</time></div>
    <ChatMarkdown>{event.message}</ChatMarkdown>
    {event.data?.tool&&<div className={styles.tool}><Wrench size={12}/>{event.data.tool}</div>}
    {event.data&&Object.keys(event.data).length>0&&<details className={styles.evidence}><summary>Inspect recorded evidence</summary><pre>{JSON.stringify(event.data,null,2)}</pre></details>}
  </li>)}</ol>{!events.length&&<p className={styles.empty}>No activity records yet.</p>}</>;
}
function RecordedActivity({job}) {
  const [open,setOpen]=useState(false),[record,setRecord]=useState(null),[error,setError]=useState('');
  useEffect(()=>{
    if(!open)return;
    const controller=new AbortController();
    api.labJob(job.id,controller.signal).then(value=>{if(!controller.signal.aborted){setRecord(value);setError('');}}).catch(reason=>{if(!controller.signal.aborted)setError(reason.message||'Activity could not be refreshed.');});
    return()=>controller.abort();
  },[open,job.id,job.updated_at]);
  const events=record?.events||(!job.summary_only?job.events:[])||[];
  return <details className={styles.details} onToggle={event=>setOpen(event.currentTarget.open)}><summary><ChevronDown size={14}/>{job.event_count??job.events?.length??0} recorded events</summary>{open&&<>{error&&<p role="status" className={styles.error}>{error} Saved activity remains available; close and reopen to retry.</p>}{!record&&job.summary_only&&!error?<p className={styles.empty}>Loading saved activity…</p>:<EventTimeline events={events}/>}</>}</details>;
}
export default function LaboratoryActivity({jobs=[],onSelect,onControl,onAttentionAction}) {
  const [now,setNow]=useState(Date.now()),[pending,setPending]=useState({}),[errors,setErrors]=useState({}),[resumeBudgets,setResumeBudgets]=useState({});
  const active=jobs.some(job=>labJobPresentation(job).active);
  useEffect(()=>{if(!active)return;const timer=setInterval(()=>setNow(Date.now()),1000);return()=>clearInterval(timer);},[active]);
  async function control(job,action){
    if(pending[job.id])return;
    setPending(value=>({...value,[job.id]:action}));setErrors(value=>({...value,[job.id]:''}));
    const budget=resumeBudgets[job.id];const options=action==='cancel_recovery'?{operation_id:laboratoryRecovery(job)?.operation_id}:action==='resume'&&budget?{time_limit_seconds:budget==='off'?null:Number(budget)}:{};
    try{await onControl?.(job,action,options);}catch(error){setErrors(value=>({...value,[job.id]:error.message||'The job control did not complete.'}));}finally{setPending(value=>({...value,[job.id]:''}));}
  }
  async function attentionAction(job,action){
    setErrors(value=>({...value,[job.id]:''}));
    try{await onAttentionAction?.(job,action);}catch(error){setErrors(value=>({...value,[job.id]:error.message||'The requested view could not be opened.'}));}
  }
  const ordered=[...jobs].sort((a,b)=>Date.parse(b.created_at)-Date.parse(a.created_at));
  return <section className={styles.activity} aria-label="Laboratory activity">
    <header className={styles.heading}><div><span className={styles.eyebrow}>LABORATORY ACTIVITY</span><h2>Research in progress</h2></div><span>{jobs.filter(job=>labJobPresentation(job).active).length} active{jobs.some(job=>laboratoryRecovery(job)?.active)&&` · ${jobs.filter(job=>laboratoryRecovery(job)?.active).length} recovering`} · {jobs.length} saved</span></header>
    {!jobs.length&&<p className={styles.empty}>Your experiments, agent work, and evidence will appear here after you send a request.</p>}
    <div className={styles.jobs}>{ordered.map(job=>{
      const view=labJobPresentation(job),elapsed=labElapsed(job,now),remaining=job.deadline_at?Math.max(0,(Date.parse(job.deadline_at)-now)/1000):null;
      const events=job.events||[],latest=events.at(-1),busy=!!pending[job.id];
      const recovery=laboratoryRecovery(job);
      const attention=laboratoryAttention(job),resumable=view.resumable&&(!attention||attention.actions.some(action=>action.id==='resume'));
      const recoverable=['athenak_gauge_wave','athenak_two_punctures_serial','athenak_two_punctures_cuda'].includes(job.input?.engine)&&!view.active&&['paused','failed','timed_out','cancelled','completed'].includes(job.state);
      return <article key={job.id} className={`${styles.job} ${view.active?styles.isActive:''}`}>
        <header className={styles.jobHeading}><div className={styles.status}><JobIcon view={view}/><span>{view.label}</span>{view.unread&&<strong>New result</strong>}</div><span className={styles.kind}>{job.kind?.replaceAll('_',' ')}</span></header>
        <button type="button" className={styles.title} onClick={()=>onSelect?.(job)}>{job.title||'Laboratory job'}<ArrowUpRight size={15}/></button>
        <div className={styles.meta}><span>{elapsed!==null?`${labDuration(elapsed)} elapsed`:''}</span>{view.active&&<span>{remaining===null?'Timer off':`${labDuration(remaining)} remaining`}</span>}{job.input?.model&&<span>{job.input.model}{job.input.reasoning_effort?` · ${job.input.reasoning_effort}`:''}</span>}</div>
        {view.active&&latest?.message&&<p className={styles.current}>{latest.message}</p>}
        {view.percent!==null&&view.active&&<div className={styles.progressRow}><progress value={view.percent} max="100" aria-label="Reported computation progress"/><span>{view.percent}%</span></div>}
        {job.error&&<p className={styles.error} role="status">{job.error}</p>}
        {attention&&<div className={styles.current} role="status"><strong>Needs direction</strong><p>{attention.reason}</p>{onAttentionAction&&<div className={styles.controls}>{attention.actions.filter(action=>action.id!=='resume').map(action=><button key={action.id} type="button" title={action.description||undefined} onClick={()=>attentionAction(job,action)}>{action.label}</button>)}</div>}</div>}
        {job.kind==='published_simulation'&&['paused','failed','timed_out'].includes(job.state)&&<p className={styles.meta}>Continue the parent agent to finish publishing these retained states.</p>}
        {recoverable&&<p className={styles.meta}>Recover the stopped engine's saved output. This does not restart the calculation or extend its timer.</p>}
        {recovery&&<div role="status" className={recovery.status==='failed'?styles.error:styles.current}>{recovery.active&&<LoaderCircle className={styles.spinner} size={14} aria-hidden="true"/>} {recovery.label}. {recovery.error||'The original experiment remains available.'}</div>}
        {resumable&&!recoverable&&job.kind!=='specialist'&&<label className={styles.meta}>{job.deadline_at&&Date.parse(job.deadline_at)<=now?'Original deadline expired. Choose a new maximum:':'Continuation budget:'}<select aria-label={`Continuation time for ${job.title||'job'}`} value={resumeBudgets[job.id]||''} onChange={event=>setResumeBudgets(values=>({...values,[job.id]:event.target.value}))}><option value="">Keep original budget / inherit active parent</option><option value="900">15 minutes</option><option value="3600">1 hour</option><option value="14400">4 hours</option><option value="off">Timer off</option></select>{job.parent_id&&<span>For a running parent, resume with its budget. Choose a new budget to continue independently after the parent has stopped.</span>}</label>}
        <RecordedActivity job={job}/>
        <footer className={styles.controls}>{job.kind==='export'&&view.state==='completed'&&<a href={`${api.baseUrl}${exportStatusFromJob(job).download_url}`} download={exportDownloadName(job)}><Download size={13}/>Save MP4</a>}{view.active&&onControl&&<><button type="button" onClick={()=>control(job,'pause')} disabled={busy}><Pause size={13}/>Pause</button><button type="button" onClick={()=>control(job,'cancel')} disabled={busy}><Square size={13}/>Stop</button></>}{recoverable&&onControl&&<button type="button" onClick={()=>control(job,'reconcile')} disabled={busy||recovery?.active}><Download size={13}/>{busy||recovery?.active?'Recovering…':recovery?.status==='completed'?'Verify retained output again':'Recover retained output'}</button>}{recovery?.active&&onControl&&<button type="button" onClick={()=>control(job,'cancel_recovery')} disabled={busy||recovery.status==='cancel_requested'}><Square size={13}/>{recovery.status==='cancel_requested'?'Stopping recovery…':'Cancel recovery'}</button>}{resumable&&!recoverable&&onControl&&<button type="button" onClick={()=>control(job,'resume')} disabled={busy}><Play size={13}/>{busy?'Resuming…':'Resume'}</button>}<button type="button" onClick={()=>onSelect?.(job)}>{view.state==='completed'?'Open results':'Inspect job'}<ArrowUpRight size={13}/></button></footer>
        {errors[job.id]&&<p className={styles.error} role="alert">{errors[job.id]}</p>}
      </article>;
    })}</div>
  </section>;
}
