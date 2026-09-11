'use client';
import {useCallback,useEffect,useRef,useState} from 'react';
import {PresentationWriter,presentationPatch} from '@/lib/presentation-writer.mjs';
import {laboratoryJson} from '@/lib/laboratory-http.mjs';

/** A renderer edits only presentation keys; solver/illustration source artifacts stay immutable. */
export default function useLaboratoryPresentation(jobId,defaults){
  const [settings,setSettings]=useState(defaults),[ready,setReady]=useState(false),[warning,setWarning]=useState(''),[history,setHistory]=useState({undo:[],redo:[]});
  const value=useRef(defaults),writer=useRef(null),timer=useRef(null),historyValue=useRef(history);historyValue.current=history;
  const replace=useCallback(next=>{value.current=next;setSettings(next);try{localStorage.setItem(`phaseforge.lab.presentation.${jobId}`,JSON.stringify(next));}catch{}},[jobId]);
  useEffect(()=>{
    const controller=new AbortController();let stopped=false,busy=false,localWriter=null;
    setReady(false);setWarning('');setHistory({undo:[],redo:[]});replace(defaults);
    let local={};try{local=JSON.parse(localStorage.getItem(`phaseforge.lab.presentation.${jobId}`)||'{}');}catch{}
    replace({...defaults,...local});
    async function refresh(){if(busy||stopped)return;busy=true;try{
      if(localWriter)await localWriter.retry();
      const saved=await laboratoryJson(`/api/laboratory/jobs/${jobId}/presentation`,{signal:controller.signal});if(stopped)return;
      if(localWriter)localWriter.acceptRemote(saved);else{
        localWriter=new PresentationWriter({revision:saved.revision,settings:saved.settings||{},send:body=>laboratoryJson(`/api/laboratory/jobs/${jobId}/presentation`,{method:'PUT',body,keepalive:true}),onState:(next,_revision,meta)=>{
          if(stopped)return;
          if(meta?.remoteKeys?.length){const previous=value.current;setHistory(h=>({undo:[...h.undo.slice(-39),Object.fromEntries(meta.remoteKeys.map(key=>[key,previous[key]??defaults[key]??null]))],redo:[]}));}
          replace({...defaults,...next});setWarning('');
        },onError:error=>{if(!stopped)setWarning(`Presentation edits are queued: ${error.message}`);}});
        writer.current=localWriter;replace({...defaults,...saved.settings});setReady(true);
      }
      if(!localWriter?.inFlight&&!Object.keys(localWriter?.pending||{}).length)setWarning('');
    }catch(error){if(!stopped&&error.name!=='AbortError')setWarning('Reconnecting to saved presentation. The view remains available.');}finally{busy=false;}}
    refresh();const interval=setInterval(refresh,4000);
    return()=>{stopped=true;clearInterval(interval);clearTimeout(timer.current);controller.abort();localWriter?.detach();if(writer.current===localWriter)writer.current=null;};
  },[jobId,defaults,replace]);
  const update=useCallback((patch,{record=true}={})=>{
    if(!writer.current)return;
    const previous=value.current,next={...previous,...patch},changed=presentationPatch(previous,next);if(!Object.keys(changed).length)return;
    if(record)setHistory(h=>({undo:[...h.undo.slice(-39),Object.fromEntries(Object.keys(changed).map(key=>[key,previous[key]??null]))],redo:[]}));
    replace(next);writer.current.queue(changed);clearTimeout(timer.current);timer.current=setTimeout(()=>writer.current?.flush(),300);
  },[replace]);
  const restore=useCallback(direction=>{const h=historyValue.current,other=direction==='undo'?'redo':'undo',patch=h[direction].at(-1);if(!patch)return;setHistory({...h,[direction]:h[direction].slice(0,-1),[other]:[...h[other].slice(-39),Object.fromEntries(Object.keys(patch).map(key=>[key,value.current[key]??null]))]});update(patch,{record:false});},[update]);
  return {settings,ready,warning,update,undo:()=>restore('undo'),redo:()=>restore('redo'),canUndo:!!history.undo.length,canRedo:!!history.redo.length};
}
