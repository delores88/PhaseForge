import {useEffect,useState} from 'react';
import styles from './CameraControls.module.css';

/** Primary inspection actions stay visible; only saved/exact views are optional. */
export default function CameraControls({engineRef,storageKey='scene',selection=false,disabled=false}){
  const [saved,setSaved]=useState([]),[coordinates,setCoordinates]=useState(null);
  useEffect(()=>{try{const value=JSON.parse(localStorage.getItem(`phaseforge.savedViews.${storageKey}`)||'[]');setSaved(Array.isArray(value)?value.filter(view=>view?.camera&&typeof view.name==='string').slice(-8):[]);}catch{setSaved([]);}},[storageKey]);
  const nav=value=>engineRef.current?.navigate?.(value);
  const save=()=>{const camera=engineRef.current?.cameraState?.();if(!camera)return;const next=[...saved.slice(-7),{name:`View ${saved.length+1}`,camera}];setSaved(next);try{localStorage.setItem(`phaseforge.savedViews.${storageKey}`,JSON.stringify(next));}catch{}};
  const apply=value=>{const current=engineRef.current;if(current?.setCamera)current.setCamera(value);else current?.command?.(value);};
  const button=(label,text,value)=><button type="button" key={label} aria-label={label} title={label} disabled={disabled} onClick={()=>nav(value)}>{text}</button>;
  return <div className={styles.controls} role="group" aria-label="Camera controls">
    <div className={styles.group} role="group" aria-label="Zoom and fit"><span>Zoom</span>{button('Zoom out','−',{zoom:.2})}{button('Zoom in','+',{zoom:-.2})}{button('Reset camera and fit all','Fit all',{home:true})}{selection&&<button type="button" disabled={disabled} onClick={()=>engineRef.current?.focusSelection?.()}>Fit selection</button>}</div>
    <div className={styles.group} role="group" aria-label="Pan camera"><span>Pan</span>{button('Pan left','←',{pan:[-.1,0]})}{button('Pan right','→',{pan:[.1,0]})}{button('Pan up','↑',{pan:[0,.1]})}{button('Pan down','↓',{pan:[0,-.1]})}</div>
    <div className={styles.group} role="group" aria-label="Orbit and tilt"><span>Orbit / tilt</span>{button('Orbit left','←',{orbit:[.18,0]})}{button('Orbit right','→',{orbit:[-.18,0]})}{button('Tilt up','↑',{orbit:[0,-.15]})}{button('Tilt down','↓',{orbit:[0,.15]})}</div>
    <div className={styles.group} role="group" aria-label="Roll camera"><span>Roll</span>{button('Roll left','↶',{roll:-.15})}{button('Roll right','↷',{roll:.15})}</div>
    <details className={styles.views} onToggle={e=>{if(e.currentTarget.open)setCoordinates(engineRef.current?.cameraState?.()||null);}}><summary>Saved views & coordinates</summary><div className={styles.panel}><button type="button" onClick={save} disabled={disabled}>Save this camera</button>{saved.map((view,index)=><button type="button" className={styles.saved} key={index} disabled={disabled} onClick={()=>apply(view.camera)}>{view.name}</button>)}{coordinates&&<div className={styles.precise}>{['position','target'].map(key=><label key={key}>{key}<input aria-label={`Camera ${key}`} key={`${key}-${coordinates[key]}`} defaultValue={coordinates[key]?.join(', ')} onBlur={e=>{const value=e.target.value.split(',').map(Number);if(value.length===3&&value.every(Number.isFinite))apply({[key]:value});}}/></label>)}<small>{coordinates.units}</small></div>}<p>Focus the image: arrows orbit, Shift + arrows pan, +/− zoom, Home fits. Drag to orbit; right-drag to pan.</p></div></details>
  </div>;
}
