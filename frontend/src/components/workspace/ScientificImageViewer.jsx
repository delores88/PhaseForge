'use client';
import {useCallback,useEffect,useRef,useState} from 'react';
import {imageKeyCommand,navigateImage} from '@/lib/image-navigation.mjs';
import styles from './ScientificImageViewer.module.css';

/** Inspects a saved raster. Transformations never change its source or provenance. */
export default function ScientificImageViewer({src,alt='Saved scientific image',empty='No saved image is available yet.',onError}){
  const host=useRef(null),panel=useRef(null),drag=useRef(null),size=useRef({width:1,height:1}),image=useRef({width:1,height:1}),loaded=useRef(false),fitted=useRef(true);
  const [view,setView]=useState({scale:1,x:0,y:0}),[ready,setReady]=useState(false),[error,setError]=useState('');
  const navigate=useCallback(command=>{if(!loaded.current)return;fitted.current=!!command.fit;setView(value=>navigateImage(value,command,size.current,image.current));},[]);
  useEffect(()=>{loaded.current=false;fitted.current=true;setReady(false);setError('');setView({scale:1,x:0,y:0});},[src]);
  useEffect(()=>{
    const element=host.current;
    const resize=new ResizeObserver(()=>{if(element.clientWidth<=0||element.clientHeight<=0)return;const previous=size.current;size.current={width:element.clientWidth,height:element.clientHeight};if(loaded.current)setView(value=>fitted.current?navigateImage(value,{fit:true},size.current,image.current):({...value,x:value.x+(size.current.width-previous.width)/2,y:value.y+(size.current.height-previous.height)/2}));});resize.observe(element);
    const wheel=event=>{if(!loaded.current)return;event.preventDefault();event.stopPropagation();const rect=element.getBoundingClientRect();navigate({zoom:Math.exp(-Math.max(-100,Math.min(100,event.deltaY))*.004),anchor:{x:event.clientX-rect.left,y:event.clientY-rect.top}});};
    element.addEventListener('wheel',wheel,{passive:false});return()=>{resize.disconnect();element.removeEventListener('wheel',wheel);};
  },[navigate]);
  const finish=event=>{if(drag.current?.id===event.pointerId){drag.current=null;if(event.currentTarget.hasPointerCapture?.(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);}};
  const action=(label,text,command)=><button type="button" aria-label={label} key={label} disabled={!ready} onClick={()=>navigate(command)}>{text}</button>;
  return <div ref={panel} className={styles.viewer} aria-label="Saved image inspector">
    <div className={styles.controls} role="group" aria-label="Image controls"><div className={styles.group}><span>Zoom</span>{action('Zoom image out','−',{zoom:.8})}<output aria-label="Image magnification">{ready?`${Math.round(view.scale*100)}%`:'—'}</output>{action('Zoom image in','+',{zoom:1.25})}{action('Fit image','Fit / reset',{fit:true})}{action('Show image at actual pixels','1:1 pixels',{actual:true})}</div><div className={styles.group} role="group" aria-label="Pan image"><span>Pan</span>{action('Pan image left','←',{pan:[-60,0]})}{action('Pan image right','→',{pan:[60,0]})}{action('Pan image up','↑',{pan:[0,-60]})}{action('Pan image down','↓',{pan:[0,60]})}</div><button type="button" onClick={()=>{const request=document.fullscreenElement===panel.current?document.exitFullscreen?.():panel.current?.requestFullscreen?.();request?.catch(value=>setError(value.message));}}>Fullscreen</button></div>
    <div ref={host} className={styles.viewport} tabIndex={0} aria-label="Image canvas. Drag to pan, scroll to zoom. Arrow keys pan, plus and minus zoom, Home fits, zero shows actual pixels."
      onDoubleClick={()=>navigate({fit:true})} onKeyDown={event=>{if(event.target!==event.currentTarget)return;const command=imageKeyCommand(event);if(command){event.preventDefault();event.stopPropagation();navigate(command);}}}
      onPointerDown={event=>{if(event.button!==0||!loaded.current)return;event.currentTarget.focus({preventScroll:true});event.currentTarget.setPointerCapture(event.pointerId);drag.current={id:event.pointerId,x:event.clientX,y:event.clientY};}}
      onPointerMove={event=>{if(drag.current?.id!==event.pointerId)return;const previous=drag.current;drag.current={id:event.pointerId,x:event.clientX,y:event.clientY};navigate({pan:[previous.x-event.clientX,previous.y-event.clientY]});}}
      onPointerUp={finish} onPointerCancel={finish} onLostPointerCapture={()=>{drag.current=null;}}>
      {src?<img key={src} src={src} alt={alt} draggable={false} onLoad={event=>{const element=event.currentTarget;image.current={width:element.naturalWidth,height:element.naturalHeight};loaded.current=image.current.width>0&&image.current.height>0;setReady(loaded.current);size.current={width:host.current.clientWidth,height:host.current.clientHeight};navigate({fit:true});}} onError={()=>{loaded.current=false;setReady(false);setError('The saved image could not be loaded.');onError?.('The saved image could not be loaded.');}} style={{width:ready?image.current.width:undefined,height:ready?image.current.height:undefined,visibility:ready?'visible':'hidden',transform:`translate(${view.x}px,${view.y}px) scale(${view.scale})`}}/>:<div className={styles.empty} role="status">{empty}</div>}
    </div>
    <p className={styles.help}>Drag to pan · scroll or +/− to zoom · double-click or Home to fit. The saved image stays unchanged.{error&&<span role="status"> {error}</span>}</p>
  </div>;
}
