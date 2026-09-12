'use client';
import {useEffect,useMemo,useRef} from 'react';
import {laboratoryLineage} from '@/lib/laboratory-lineage.mjs';
import styles from './LaboratoryLineage.module.css';

/** Selection belongs to the workbench; background completion never changes it. */
export default function LaboratoryLineage({jobs=[],selectedJobId,onSelect,scopeKey=''}){
  const items=useMemo(()=>laboratoryLineage(jobs),[jobs]),scroll=useRef(null),revealed=useRef(null);
  const scope=scopeKey||items[0]?.job.project_id||'laboratory';
  useEffect(()=>{if(items.length&&scroll.current&&revealed.current!==scope){scroll.current.scrollLeft=scroll.current.scrollWidth;revealed.current=scope;}},[scope,items.length]);
  if(!items.length)return null;
  return <section className={styles.lineage} aria-label="Experiment lineage"><header><div><strong>Experiment history</strong><span>{items.length} retained · oldest to newest</span></div><div><button type="button" aria-label="Scroll experiment history left" onClick={()=>scroll.current?.scrollBy({left:-320,behavior:'auto'})}>←</button><button type="button" aria-label="Show newest experiment" onClick={()=>{onSelect?.(items.at(-1).job);if(scroll.current)scroll.current.scrollLeft=scroll.current.scrollWidth;}}>Newest →</button></div></header>
    <div ref={scroll} className={styles.scroll} tabIndex={0} aria-label="Retained experiments, oldest on the left and newest on the right" onKeyDown={event=>{if(event.target!==event.currentTarget||event.altKey||event.ctrlKey||event.metaKey)return;if(event.key==='End'){event.preventDefault();event.currentTarget.scrollLeft=event.currentTarget.scrollWidth;}else if(event.key==='Home'){event.preventDefault();event.currentTarget.scrollLeft=0;}}}>
      {items.map(({job,derivation,type},index)=>{const active=['queued','running','provisioning','waiting','waiting_for_model_slot'].includes(job.state),selected=job.id===selectedJobId,latest=index===items.length-1;return <button type="button" key={job.id} className={`${styles.card} ${selected?styles.selected:''}`} aria-pressed={selected} aria-label={`Open ${job.title||type}${latest?' (newest)':''}`} onClick={()=>onSelect?.(job)}><span className={styles.top}><span>{type}</span>{latest&&<b>Newest</b>}{selected&&<b>Selected</b>}</span><strong>{job.title||type}</strong><span className={styles.status}><i className={active?styles.spinning:''} data-state={job.state}/>{(job.state||'retained').replaceAll('_',' ')}{job.created_at&&<time dateTime={job.created_at}>{new Date(job.created_at).toLocaleString(undefined,{month:'short',day:'numeric',hour:'2-digit',minute:'2-digit'})}</time>}</span>{derivation?<small title={`${derivation.label} ${derivation.sourceId}`}>{derivation.label} <code>{derivation.sourceId}</code></small>:<small title={job.id}>Original output · <code>{job.id}</code></small>}</button>;})}
    </div>
  </section>;
}
