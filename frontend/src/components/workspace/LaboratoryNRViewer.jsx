'use client';
import {useEffect,useMemo,useRef,useState} from 'react';
import {Camera,Focus,Maximize,Minus,Pause,Play,Plus,RotateCcw} from 'lucide-react';
import {laboratoryUrl} from '@/lib/laboratory-http.mjs';
import {loadNRDiagnostics,nrCellAt,nrColor,nrRelative,nrTransform} from '@/lib/nr-diagnostics.mjs';
import useLaboratoryPresentation from './useLaboratoryPresentation';
import styles from './LaboratoryNRViewer.module.css';

const DEFAULTS={nrChannel:null,nrView:null,nrBlockOutlines:true,playbackDurationSeconds:10};
const number=value=>typeof value==='number'&&Number.isFinite(value)?value.toLocaleString(undefined,{maximumSignificantDigits:7}):'—';
export default function LaboratoryNRViewer({job}){
  const panel=useRef(null),canvas=useRef(null),drag=useRef(null),drawState=useRef(null),wheelZoom=useRef(null);
  const [index,setIndex]=useState(null),[frame,setFrame]=useState(null),[frameNumber,setFrameNumber]=useState(0),[playing,setPlaying]=useState(false);
  const [error,setError]=useState(''),[loading,setLoading]=useState(false),[size,setSize]=useState({width:800,height:440}),[view,setView]=useState(null),[point,setPoint]=useState(null);
  const presentation=useLaboratoryPresentation(job.id,DEFAULTS),settings=presentation.settings;
  useEffect(()=>{if(!drag.current)setView(settings.nrView);},[settings.nrView]);
  useEffect(()=>{
    const controller=new AbortController();setIndex(null);setFrame(null);setFrameNumber(0);setPlaying(false);setPoint(null);setError('');
    if(!job.result?.diagnostics?.sha256)return()=>controller.abort();
    setLoading(true);
    loadNRDiagnostics(job,async path=>{
      nrRelative(path);const url=`/api/laboratory/jobs/${encodeURIComponent(job.id)}/artifacts/${path.split('/').map(encodeURIComponent).join('/')}`;
      const response=await fetch(laboratoryUrl(url),{signal:controller.signal});
      if(!response.ok)throw Error(`Retained NR artifact unavailable (${response.status})`);
      return response.arrayBuffer();
    }).then(value=>{if(!controller.signal.aborted){setIndex(value);setLoading(false);}}).catch(e=>{if(e.name!=='AbortError'){setError(e.message);setLoading(false);}});
    return()=>controller.abort();
  },[job.id,job.result?.diagnostics?.sha256]);
  useEffect(()=>{
    if(!index)return;let stale=false;setLoading(true);setFrame(null);
    index.loadFrame(frameNumber).then(value=>{if(!stale){setFrame(value);setLoading(false);setError('');}}).catch(e=>{if(!stale&&e.name!=='AbortError'){setError(e.message);setLoading(false);setPlaying(false);}});
    return()=>{stale=true;};
  },[index,frameNumber]);
  useEffect(()=>{
    const observer=new ResizeObserver(entries=>{const rect=entries[0].contentRect;if(rect.width>0&&rect.height>0)setSize({width:rect.width,height:rect.height});});
    if(canvas.current)observer.observe(canvas.current);return()=>observer.disconnect();
  },[]);
  useEffect(()=>{
    const element=canvas.current;if(!element)return;
    const wheel=event=>{event.preventDefault();wheelZoom.current?.(event.deltaY<0?1.15:1/1.15);};
    element.addEventListener('wheel',wheel,{passive:false});
    return()=>element.removeEventListener('wheel',wheel);
  },[]);
  useEffect(()=>{
    if(!playing||!index||loading)return;
    const duration=Math.max(.5,Math.min(86400,Number(settings.playbackDurationSeconds)||10));
    const timer=setTimeout(()=>setFrameNumber(value=>{if(value>=index.frames.length-1){setPlaying(false);return value;}return value+1;}),duration*1000/Math.max(1,index.frames.length-1));
    return()=>clearTimeout(timer);
  },[playing,index,frameNumber,loading,settings.playbackDurationSeconds]);
  const channel=index?.channels?.[settings.nrChannel]?settings.nrChannel:index?.primary_channel;
  const selected=useMemo(()=>point&&frame?nrCellAt(frame,point.x,point.y):null,[point,frame]);
  useEffect(()=>{
    const element=canvas.current;if(!element||!frame||!channel)return;
    const ratio=Math.min(window.devicePixelRatio||1,2);element.width=Math.round(size.width*ratio);element.height=Math.round(size.height*ratio);
    const ctx=element.getContext('2d');ctx.setTransform(ratio,0,0,ratio,0,0);ctx.fillStyle='#080f20';ctx.fillRect(0,0,size.width,size.height);
    const transform=nrTransform(frame.bounds,size.width,size.height,view);drawState.current=transform;
    const range=index.ranges[channel];ctx.imageSmoothingEnabled=false;
    for(const block of frame.blocks){
      for(let y=0;y<block.y.length;y++)for(let x=0;x<block.x.length;x++){
        const [left,top]=transform.toScreen(block.x_edges[x],block.y_edges[y+1]);
        const [right,bottom]=transform.toScreen(block.x_edges[x+1],block.y_edges[y]);
        ctx.fillStyle=nrColor(block.channels[channel][y][x],range);ctx.fillRect(left,top,right-left,bottom-top);
      }
      if(settings.nrBlockOutlines){const [x,y]=transform.toScreen(block.x_edges[0],block.y_edges.at(-1));ctx.strokeStyle='rgba(245,246,255,.42)';ctx.lineWidth=.7;ctx.strokeRect(x,y,(block.x_edges.at(-1)-block.x_edges[0])*transform.scale,(block.y_edges.at(-1)-block.y_edges[0])*transform.scale);}
    }
    if(selected){const [x,y]=transform.toScreen(selected.x,selected.y);ctx.strokeStyle='#fff';ctx.lineWidth=1.5;ctx.beginPath();ctx.arc(x,y,5,0,Math.PI*2);ctx.stroke();}
    ctx.fillStyle='#e7eaf2';ctx.font='12px system-ui';ctx.fillText('x → [L]   y ↑ [L]',16,size.height-15);
    ctx.fillText(`t = ${number(frame.time)} L/c  ·  ${index.channels[channel].label}`,16,22);
  },[frame,index,channel,size,view,settings.nrBlockOutlines,selected]);
  const updateView=next=>{setView(next);presentation.update({nrView:next},{record:false});};
  const zoom=factor=>{if(!frame)return;const t=drawState.current||nrTransform(frame.bounds,size.width,size.height,view);updateView({x:t.cx,y:t.cy,zoom:Math.max(.05,Math.min(100,(view?.zoom||1)*factor))});};
  wheelZoom.current=zoom;
  const location=event=>{const rect=canvas.current.getBoundingClientRect();return [event.clientX-rect.left,event.clientY-rect.top];};
  const pointerDown=event=>{if(!frame||!drawState.current)return;const [x,y]=location(event);canvas.current.setPointerCapture(event.pointerId);drag.current={x,y,view:{x:drawState.current.cx,y:drawState.current.cy,zoom:view?.zoom||1},scale:drawState.current.scale,moved:false};};
  const pointerMove=event=>{const value=drag.current;if(!value)return;const [x,y]=location(event);if(Math.hypot(x-value.x,y-value.y)>3)value.moved=true;if(value.moved)setView({...value.view,x:value.view.x-(x-value.x)/value.scale,y:value.view.y+(y-value.y)/value.scale});};
  const pointerUp=event=>{const value=drag.current;if(!value)return;drag.current=null;if(value.moved)presentation.update({nrView:view},{record:false});else{const [x,y]=drawState.current.toData(...location(event));setPoint({x,y});}};
  const save=()=>{if(!frame)return;const link=document.createElement('a');link.href=canvas.current.toDataURL('image/png');link.download=`phaseforge-nr-diagnostic-${job.id}-${frame.time}.png`;link.click();};
  const seek=value=>{setPlaying(false);setFrameNumber(value);};
  const partial=index&&!index.gauge&&!index.execution?.complete;
  const range=index?.ranges?.[channel];
  return <section ref={panel} className={styles.viewer} aria-label="Numerical relativity diagnostic viewer">
    <header><div><span className={styles.eyebrow}>RETAINED EINSTEIN EVOLUTION · DIAGNOSTIC SLICE</span><h3>{job.title||'Numerical relativity diagnostics'}</h3></div><div className={styles.controls}><button type="button" onClick={save} disabled={!frame} aria-label="Save NR diagnostic image"><Camera size={16}/></button><button type="button" onClick={()=>document.fullscreenElement?document.exitFullscreen?.():panel.current?.requestFullscreen?.()} aria-label="Fullscreen NR diagnostics"><Maximize size={16}/></button></div></header>
    {index&&<p className={styles.scope}>{index.gauge?'Flat spacetime in a harmonic coordinate gauge. No black-hole collision or physical gravitational radiation.':`Bowen–York initial data. ${partial?`Partial execution · ${index.execution?.termination_reason||'stopped'} · retained ${number(index.end)} L/c, requested ${number(index.execution?.time_target)} L/c.`:'Recorded time target reached; numerical accuracy remains unvalidated.'} Physical boost unknown; 0.999c collision and merger are not validated.`}</p>}
    {index&&<div className={styles.toolbar}><label>Diagnostic field <select value={channel} onChange={event=>presentation.update({nrChannel:event.target.value})}>{Object.entries(index.channels).map(([name,spec])=><option key={name} value={name}>{spec.label} [{spec.unit}]</option>)}</select></label><div className={styles.controls}><button type="button" onClick={()=>updateView(null)}><Focus size={15}/>Fit</button><button type="button" aria-label="Zoom into NR diagnostic" onClick={()=>zoom(1.4)}><Plus size={15}/></button><button type="button" aria-label="Zoom out of NR diagnostic" onClick={()=>zoom(1/1.4)}><Minus size={15}/></button><label><input type="checkbox" checked={settings.nrBlockOutlines} onChange={e=>presentation.update({nrBlockOutlines:e.target.checked})}/>Block boundaries</label></div></div>}
    <div className={styles.plot}><canvas ref={canvas} aria-label="Actual retained AMR numerical cells. Drag to pan, scroll to zoom, click to inspect." onPointerDown={pointerDown} onPointerMove={pointerMove} onPointerUp={pointerUp} onPointerCancel={()=>{drag.current=null;}}/>{loading&&<div className={styles.loading} role="status">Verifying retained numerical values…</div>}{!index&&!loading&&!error&&<div className={styles.loading} role="status">No retained diagnostic slices are available for this attempt yet.</div>}</div>
    {frame&&index&&<><div className={styles.legend}><div><strong>{index.channels[channel].label} [{index.channels[channel].unit}]</strong><span className={styles.gradient}/><span>{number(range.min)} → {number(range.max)} · fixed across retained times</span></div><p>{frame.blocks.length} leaf blocks · {frame.cellCount.toLocaleString()} cells. Actual z: {number(frame.plane.actual_z_min)}–{number(frame.plane.actual_z_max)} L. {frame.plane.actual_z_varies_by_block?'Different native z centres across AMR levels. ':''}No interpolation or 3D spacetime embedding.</p></div>
      {selected&&<aside className={styles.inspector} aria-label="Selected NR cell"><strong>{number(selected.values[channel])} {index.channels[channel].unit}</strong><span>x {number(selected.x)}, y {number(selected.y)}, z {number(selected.z)} L · native level {selected.level} · block [{selected.logical.join(', ')}]</span><span>t {number(frame.time)} L/c · cycle {frame.cycle}. {frame.reductionMask&&selected.values.chi<frame.reductionMask.value?`This cell is displayed but excluded from the recorded chi ≥ ${number(frame.reductionMask.value)} RMS reduction.`:''}</span></aside>}
      <footer><div className={styles.timeline}><button type="button" aria-label="Restart NR playback" onClick={()=>seek(0)}><RotateCcw size={16}/></button><button type="button" aria-label={playing?'Pause NR playback':'Play retained NR states'} disabled={index.frames.length<2} onClick={()=>{if(frameNumber>=index.frames.length-1)setFrameNumber(0);setPlaying(v=>!v);}}>{playing?<Pause size={17}/>:<Play size={17}/>}</button><input type="range" aria-label="Recorded NR state" min={0} max={index.frames.length-1} step={1} value={frameNumber} onChange={event=>seek(Number(event.target.value))}/><code>{number(frame.time)} / {number(index.end)} L/c</code></div><div className={styles.footerRow}><label>Playback seconds <input type="number" min=".5" max="86400" value={settings.playbackDurationSeconds} onChange={event=>{const value=Number(event.target.value);if(value>=.5&&value<=86400)presentation.update({playbackDurationSeconds:value});}}/></label><span>Drag to pan · scroll to zoom · click a cell · {index.frames.length} actual saved times</span></div></footer>
      {!index.gauge&&<details className={styles.horizons}><summary>Horizon diagnostics · no independently validated horizon properties</summary>{index.horizons?.map(h=><p key={h.horizon}>Search {h.horizon}: latest {h.latest_status||'unknown'}; {h.failure_count} failed searches, {h.stale_summary_count} stale summary rows. Mass-change stopping flags do not validate a zero-expansion surface.</p>)}</details>}
    </>}
    {presentation.warning&&<p role="status">{presentation.warning}</p>}{error&&<p className={styles.error} role="alert">{error}</p>}
  </section>;
}
