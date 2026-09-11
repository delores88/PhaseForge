'use client';

import {useCallback,useEffect,useRef,useState} from 'react';
import {Download,Focus,Maximize,Minus,Plus} from 'lucide-react';
import {api} from '@/lib/api';
import {laboratoryJson,laboratoryUrl} from '@/lib/laboratory-http.mjs';
import {artifactPath,fitPlot,plotSourceId,studyDownloads,verifyPlotPng,zoomPlot} from '@/lib/laboratory-plot.mjs';
import styles from './LaboratoryPlotViewer.module.css';

const format=value=>Number.isFinite(value)?value.toLocaleString(undefined,{maximumSignificantDigits:6}):'—';

export default function LaboratoryPlotViewer({job}){
  const panel=useRef(null),viewport=useRef(null),drag=useRef(null),dimensions=useRef({width:1,height:1});
  const [plot,setPlot]=useState(null),[study,setStudy]=useState(null),[image,setImage]=useState(null),[data,setData]=useState(null),[error,setError]=useState(''),[imageError,setImageError]=useState(''),[view,setView]=useState({scale:1,x:0,y:0}),[retry,setRetry]=useState(0);
  const reset=useCallback(()=>{const {width,height}=dimensions.current;setView(fitPlot(width,height));},[]);
  const zoom=useCallback(factor=>{const {width,height}=dimensions.current;setView(value=>zoomPlot(value,factor,{x:width/2,y:height/2}));},[]);

  useEffect(()=>{
    const controller=new AbortController();
    async function load(){
      try{
        const full=await api.labJob(job.id,controller.signal),id=plotSourceId(full);
        if(!id){if(!controller.signal.aborted){setStudy(full);setPlot(null);setError('');}return;}
        const selected=full.kind==='study_plot'?full:await api.labJob(id,controller.signal);
        if(selected.kind!=='study_plot'||selected.project_id!==job.project_id)throw Error('The chart does not belong to this study project.');
        let parent=full.kind==='ml_study'?full:null;
        if(!parent&&selected.parent_id){
          const candidate=await api.labJob(selected.parent_id,controller.signal);
          if(candidate.kind==='ml_study'&&candidate.project_id===job.project_id)parent=candidate;
        }
        if(!controller.signal.aborted){setPlot(selected);setStudy(parent);setError('');}
      }catch(value){if(value.name!=='AbortError')setError(value.message);}
    }
    load();return()=>controller.abort();
  },[job.id,job.updated_at,job.state,retry]);

  // A plot can finish immediately before its coordinator publishes the dataset
  // links. Refresh that parent without replacing the inspected chart or view.
  useEffect(()=>{
    if(!study?.id||!['queued','running','provisioning','waiting'].includes(study.state))return;
    const controller=new AbortController();let busy=false;
    const timer=setInterval(async()=>{if(busy)return;busy=true;try{const latest=await api.labJob(study.id,controller.signal);if(!controller.signal.aborted&&latest.kind==='ml_study'&&latest.project_id===job.project_id){setStudy(latest);setError('');}}catch(value){if(value.name!=='AbortError')setError(value.message);}finally{busy=false;}},2500);
    return()=>{clearInterval(timer);controller.abort();};
  },[study?.id,study?.state,job.project_id]);

  useEffect(()=>{
    if(plot?.state!=='completed'){setImage(null);setData(null);setImageError('');return;}
    const controller=new AbortController();let objectUrl;
    async function load(){
      try{
        const [report,response]=await Promise.all([laboratoryJson(artifactPath(plot.id,'plot/plot.json'),{signal:controller.signal}),fetch(laboratoryUrl(artifactPath(plot.id,'plot/plot.png')),{signal:controller.signal})]);
        if(!response.ok)throw Error(`Saved chart unavailable (${response.status}).`);
        if(report.schema_version!==1||report.width!==1600||report.height!==1000||report.study_id!==plot.result?.plot?.study_id||report.model_sha256!==plot.result?.plot?.model_sha256||!Array.isArray(report.held_out?.points)||report.held_out.points.length!==8)throw Error('The plot provenance does not match its completed receipt.');
        const bytes=await response.arrayBuffer();await verifyPlotPng(bytes,plot.result?.png_sha256);
        if(controller.signal.aborted)return;
        objectUrl=URL.createObjectURL(new Blob([bytes],{type:'image/png'}));setImage(objectUrl);setData(report);setImageError('');reset();
      }catch(value){if(value.name!=='AbortError')setImageError(value.message);}
    }
    load();return()=>{controller.abort();if(objectUrl)URL.revokeObjectURL(objectUrl);};
  },[plot?.id,plot?.state,plot?.updated_at,retry,reset]);

  useEffect(()=>{
    const host=viewport.current;
    const resize=new ResizeObserver(()=>{dimensions.current={width:host.clientWidth,height:host.clientHeight};reset();});resize.observe(host);
    const wheel=event=>{event.preventDefault();const rect=host.getBoundingClientRect();setView(value=>zoomPlot(value,Math.exp(-Math.max(-100,Math.min(100,event.deltaY))*.004),{x:event.clientX-rect.left,y:event.clientY-rect.top}));};
    host.addEventListener('wheel',wheel,{passive:false});return()=>{resize.disconnect();host.removeEventListener('wheel',wheel);};
  },[reset]);

  const links=studyDownloads(plot,study),state=plot?.state||job.state;
  return <section ref={panel} className={styles.viewer} aria-label="Numerical ML evidence chart">
    <header className={styles.header}><div><span>ML EVALUATION · RETAINED NUMERICAL EVIDENCE</span><h3>{job.title||'Pressure surrogate evaluation'}</h3><p>{data?`${data.synthetic_fixture?'Synthetic component fixture · ':''}Eight held-out conditions · three seeds each · ${data.useful_acceleration?'usefulness gates passed':'usefulness gates rejected'}`:`Study ${state} · the saved chart appears here when its render completes.`}</p></div><div className={styles.tools}>
      <button type="button" onClick={()=>zoom(.8)} disabled={!image} aria-label="Zoom chart out"><Minus size={16}/></button><output aria-label="Chart magnification">{Math.round(view.scale*100)}%</output><button type="button" onClick={()=>zoom(1.25)} disabled={!image} aria-label="Zoom chart in"><Plus size={16}/></button><button type="button" onClick={reset} disabled={!image}><Focus size={15}/>Fit / reset</button><button type="button" aria-label="Fullscreen chart" onClick={()=>{const task=document.fullscreenElement?document.exitFullscreen():panel.current?.requestFullscreen();task?.catch(value=>setError(value.message));}}><Maximize size={16}/></button>
    </div></header>
    <div ref={viewport} className={styles.viewport} tabIndex={0} aria-label="Chart. Drag to pan, scroll to zoom, Home to fit, arrow keys to pan." onDoubleClick={reset}
      onPointerDown={event=>{if(event.button!==0)return;event.currentTarget.focus();event.currentTarget.setPointerCapture(event.pointerId);drag.current={id:event.pointerId,x:event.clientX,y:event.clientY};}}
      onPointerMove={event=>{if(drag.current?.id!==event.pointerId)return;const dx=event.clientX-drag.current.x,dy=event.clientY-drag.current.y;drag.current={id:event.pointerId,x:event.clientX,y:event.clientY};setView(value=>({...value,x:value.x+dx,y:value.y+dy}));}}
      onPointerUp={()=>{drag.current=null;}} onPointerCancel={()=>{drag.current=null;}}
      onKeyDown={event=>{if(event.key==='Home'){event.preventDefault();reset();}else if(['+','=','-'].includes(event.key)){event.preventDefault();zoom(event.key==='-'?.8:1.25);}else if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)){event.preventDefault();setView(value=>({...value,x:value.x+(event.key==='ArrowLeft'?40:event.key==='ArrowRight'?-40:0),y:value.y+(event.key==='ArrowUp'?40:event.key==='ArrowDown'?-40:0)}));}}}>
      {image?<img draggable={false} src={image} width="1600" height="1000" alt="Saved argon pressure evaluation. Measured versus predicted pressure with uncertainty, frozen split coverage and actual OOD solver fallback. Exact values are available below." style={{transform:`translate(${view.x}px,${view.y}px) scale(${view.scale})`}}/>:<div className={styles.pending} role="status">{['failed','cancelled','paused','stopped'].includes(state)?`The ${state} job has no completed chart. Its evidence remains in job history.`:state==='completed'?'Loading and verifying the saved chart…':'Waiting for the saved evaluation plot. You can navigate elsewhere while the study runs.'}</div>}
    </div>
    <footer className={styles.footer}><p>Drag to pan · scroll or +/− to zoom · double-click or Home to fit. Zoom changes the view; the original 1600 × 1000 PNG and numerical values remain unchanged.</p>
      <div className={styles.downloads}>{links.map(link=><a key={link.href} href={laboratoryUrl(link.href)} download={link.filename}><Download size={14}/>{link.label}</a>)}</div>
      {data&&<details><summary>Exact values, uncertainty and source provenance</summary><p>Horizontal intervals: mean ± 4.302653 × SE across three independent seeds (df = 2). Vertical intervals: calibrated model residual bounds. OOD pressure is measured by the direct solver; no OOD model prediction is plotted.</p><div className={styles.table}><table><caption>Held-out pressure, bar</caption><thead><tr>{['Condition','Role','Measured mean','Seed t95 interval','Predicted','Calibrated interval'].map(label=><th key={label}>{label}</th>)}</tr></thead><tbody>{data.held_out.points.map(point=><tr key={point.condition_index}><td>{point.condition_index}</td><td>{point.role}</td><td>{format(point.pressure_bar)}</td><td>{point.seed_t95_bar.map(format).join(' to ')}</td><td>{format(point.prediction_bar)}</td><td>{point.interval_bar.map(format).join(' to ')}</td></tr>)}</tbody></table></div><p>Study {data.study_id} · model SHA-256 <code>{data.model_sha256}</code></p>{Object.entries(data.source_hashes||{}).map(([name,pin])=><p key={name}>{name} SHA-256 <code>{pin.sha256}</code></p>)}<p>{data.scope}</p></details>}
      {(error||imageError)&&<p role="status" className={styles.error}>Chart refresh: {imageError||error} <button type="button" onClick={()=>setRetry(value=>value+1)}>Retry</button></p>}
    </footer>
  </section>;
}
