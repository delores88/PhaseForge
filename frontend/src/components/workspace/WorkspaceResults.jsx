'use client';
import {useEffect,useRef,useState} from 'react';
import styles from './WorkspaceResults.module.css';

/** Availability is passive. Every view change requires an explicit result click. */
export default function WorkspaceResults({results=[],selectedKey,onOpen,scopeKey}){
  const [open,setOpen]=useState(false),list=useRef(null),scope=useRef(scopeKey);
  useEffect(()=>{if(scope.current!==scopeKey){scope.current=scopeKey;setOpen(false);}},[scopeKey]);
  useEffect(()=>{if(open&&list.current)list.current.scrollLeft=list.current.scrollWidth;},[open]);
  if(!results.length)return null;
  const latest=results.at(-1),viewingLatest=selectedKey===latest.key;
  return <section className={styles.results} aria-label="Completed results">
    <header><button type="button" className={styles.toggle} aria-expanded={open} onClick={()=>setOpen(value=>!value)}>Results ({results.length}) <span aria-hidden="true">{open?'▴':'▾'}</span></button><div className={styles.latest} role="status"><strong>{viewingLatest?'Viewing latest result':'Latest completed result'}</strong><span title={`${latest.type} · ${latest.id}`}>{latest.title}</span></div>{!viewingLatest&&<button type="button" className={styles.openLatest} onClick={()=>onOpen?.(latest)}>Open latest result</button>}</header>
    {open&&<div ref={list} className={styles.list} aria-label="Completed result history, oldest to newest">
      {results.map(result=><button type="button" key={result.key} className={`${styles.card} ${selectedKey===result.key?styles.selected:''}`} aria-pressed={selectedKey===result.key} aria-label={`Open result ${result.title} · ${result.id}`} onClick={()=>onOpen?.(result)}><span>{result.type}{result.key===latest.key?' · Latest':''}</span><strong>{result.title}</strong><small>{result.completedAt?new Date(result.completedAt).toLocaleString():'Completion time not recorded'}</small><code>{result.id}</code></button>)}
    </div>}
  </section>;
}
