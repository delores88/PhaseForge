import ModelPicker from './ModelPicker';
import {useModelSelection} from '@/lib/modelSelection';
import {chatPresentation} from '@/lib/chatPresentation.mjs';
import Link from "next/link";
import { ArrowDown, ArrowUp, Bot, Check, Copy, FlaskConical, GripVertical, History, Maximize2, Minimize2, Paperclip, PanelRightClose, PanelRightOpen, Pencil, Plus, RotateCcw, Settings, ShieldAlert, Sparkles, Square, Wallet, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatDate, statusLabel } from "@/lib/format";
import { api } from "@/lib/api";
import ChatMarkdown from "./ChatMarkdown";
import ConversationHistory from "./ConversationHistory";
import drawerStyles from './ChatDrawer.module.css';
import {ATTACHMENT_FORMATS as ALLOWED,readAttachmentBatch} from '@/lib/attachmentIntake.mjs';
import {CHAT_DURATION_PRESETS,durationSeconds,savedDuration,customMinutesToSeconds} from '@/lib/chatDuration.mjs';
const key=(id)=>`phaseforge.chat.draft:${id}`;
function newId(){
  if(globalThis.crypto?.randomUUID)return crypto.randomUUID();
  const bytes=new Uint8Array(16);for(let i=0;i<16;i++)bytes[i]=Math.floor(Math.random()*256);
  bytes[6]=(bytes[6]&15)|64;bytes[8]=(bytes[8]&63)|128;
  const h=Array.from(bytes,b=>b.toString(16).padStart(2,"0")).join("");
  return `${h.slice(0,8)}-${h.slice(8,12)}-${h.slice(12,16)}-${h.slice(16,20)}-${h.slice(20)}`;
}
export default function ChatDrawer({containerRef,messages,providers,busy,activeJob,error,project,projects,structures,selectedStructure,onStructureSelect,onProjectSelect,onNewProject,onRenameProject,onDeleteProject,onSend,onCancel,activeRequestId,findingsRun,onOpenFindings}){
  const {selection,selectionValid,selectionError,selectionProjectId}=useModelSelection();
  const [collapsed,setCollapsed]=useState(false),[width,setWidth]=useState(34),[dragging,setDragging]=useState(false);
  const [historyOpen,setHistoryOpen]=useState(false);
  const [duration,setDuration]=useState('900');
  const [customDuration,setCustomDuration]=useState(false),[customMinutes,setCustomMinutes]=useState('30'),[submitting,setSubmitting]=useState(false);
  const sendingMessage=useRef(false);
  const [content,setContent]=useState(""),[attachments,setAttachments]=useState([]);
  const [branch,setBranch]=useState(null),[localError,setLocalError]=useState(""),[dragOver,setDragOver]=useState(false);
  const [copied,setCopied]=useState(null),[nearBottom,setNearBottom]=useState(true),[phase,setPhase]=useState("");
  const [policy,setPolicy]=useState(null);
  const transcript=useRef(null),textarea=useRef(null),file=useRef(null),draftOwner=useRef(null);
  const readingFiles=useRef(false);
  const bottom=useRef(true),lastProject=useRef(null);
  const configured=useMemo(()=>providers.filter(p=>p.key_configured||p.configured),[providers]);
  const projectSelectionError=selectionProjectId!==project?.id?'Loading this conversation’s model selection.':selectionError;
  const blocked=Boolean(policy?.paused)||!configured.length||!selectionValid||!!projectSelectionError;
  const [restored,setRestored]=useState(false);
  useEffect(()=>{
    try{const v=localStorage.getItem("phaseforge.chat.widthPercent");if(v!==null&&Number.isFinite(Number(v)))setWidth(Math.max(10,Math.min(60,Number(v))));setCollapsed(localStorage.getItem("phaseforge.chat.collapsed")==="true");}catch{}
    setRestored(true);
  },[]);
  useEffect(()=>{if(restored)try{localStorage.setItem("phaseforge.chat.widthPercent",String(width));localStorage.setItem("phaseforge.chat.collapsed",String(collapsed));}catch{}},[width,collapsed,restored]);
  useEffect(()=>{
    lastProject.current=project?.id||null;
    let value="";try{value=project?.id?localStorage.getItem(key(project.id))||"":"";}catch{}
    draftOwner.current=null;setContent(value);setAttachments([]);setBranch(null);setLocalError("");
    let valueDuration='900';try{valueDuration=savedDuration(localStorage.getItem(`phaseforge.chat.duration:${project?.id}`));}catch{}setDuration(valueDuration);setCustomDuration(!CHAT_DURATION_PRESETS.some(([value])=>value===valueDuration));setCustomMinutes(valueDuration==='off'?'30':String(Number(valueDuration)/60));
    // Separate the restore from the write effect; never write another world's draft.
    const t=setTimeout(()=>{draftOwner.current=project?.id||null;},0);return()=>clearTimeout(t);
  },[project?.id]);
  useEffect(()=>{if(project?.id&&draftOwner.current===project.id)try{localStorage.setItem(key(project.id),content);}catch{}},[content,project?.id]);
  const jump=useCallback(()=>{const el=transcript.current;if(el){el.scrollTo({top:el.scrollHeight,behavior:"smooth"});bottom.current=true;setNearBottom(true);}},[]);
  useEffect(()=>{if(bottom.current)jump();},[messages,busy,jump]);
  useEffect(()=>{const el=textarea.current;if(el){el.style.height="auto";el.style.height=`${Math.min(210,Math.max(76,el.scrollHeight))}px`;}},[content,collapsed]);
  useEffect(()=>{
    const toggle=()=>setCollapsed(v=>!v),focus=()=>{setCollapsed(false);setTimeout(()=>textarea.current?.focus(),60);};
    window.addEventListener("phaseforge:toggle-chat",toggle);window.addEventListener("phaseforge:focus-chat",focus);
    return()=>{window.removeEventListener("phaseforge:toggle-chat",toggle);window.removeEventListener("phaseforge:focus-chat",focus);};
  },[]);
  useEffect(()=>{
    if(!dragging)return;
    const move=e=>{const b=containerRef.current?.getBoundingClientRect();if(b?.width)setWidth(Math.max(10,Math.min(60,(b.right-e.clientX)/b.width*100)));};
    const stop=()=>setDragging(false);document.body.classList.add("is-resizing-chat");
    window.addEventListener("pointermove",move);window.addEventListener("pointerup",stop);window.addEventListener("pointercancel",stop);
    return()=>{document.body.classList.remove("is-resizing-chat");window.removeEventListener("pointermove",move);window.removeEventListener("pointerup",stop);window.removeEventListener("pointercancel",stop);};
  },[dragging,containerRef]);
  useEffect(()=>{
    let disposed=false;
    const poll=async()=>{try{const r=activeRequestId?await api.usage():null;if(!disposed){setPhase(r?.active_requests?.find(v=>v.request_id===activeRequestId)?.phase||"");if(r)setPolicy(r.settings);else setPolicy(await api.usageSettings());}}catch{}};
    poll();const t=setInterval(poll,activeRequestId?1500:7000);return()=>{disposed=true;clearInterval(t);};
  },[activeRequestId]);
  const addFiles=async(files)=>{
    if(readingFiles.current){setLocalError("Wait for the current files to finish reading.");return;}
    const owner=project?.id;readingFiles.current=true;setLocalError("");
    try{const next=await readAttachmentBatch(files,attachments);if(lastProject.current===owner)setAttachments(current=>[...current,...next]);}
    catch(e){if(lastProject.current===owner)setLocalError(e.message);}
    finally{readingFiles.current=false;}
  };
  const submit=async(event,override=null)=>{
    event?.preventDefault?.();const text=override?.content??content.trim();const files=override?[]:attachments;
    if((busy&&!activeJob)||sendingMessage.current||!project||(!text&&!files.length))return;
    if(blocked&&!activeJob){setLocalError(policy?.paused?"Paid calls are paused. Resume them in Usage & cost.":!configured.length?"Save a provider key in Settings, then choose a model here.":projectSelectionError);return;}
    let limit;try{limit=customDuration?customMinutesToSeconds(customMinutes):durationSeconds(duration);}catch(error){if(!activeJob){setLocalError(error.message);return;}}
    const original={content,attachments,branch};const world=project.id;
    sendingMessage.current=true;setSubmitting(true);
    setContent("");setAttachments([]);setBranch(null);setLocalError("");bottom.current=true;
    try{await onSend({content:text,request_id:newId(),provider:selection.provider,model:selection.model,reasoning_effort:selection.reasoning_effort,
      agent_role:override?.agent_role||'builder',study_intent:'auto',experiment_options:null,auto_run:false,time_limit_seconds:limit,attachments:files,structure_id:selectedStructure?.id||null,
      branch_from_message_id:override?.branch_from_message_id||branch,reply_to_message_id:override?.reply_to_message_id||null});
      if(lastProject.current===world){try{localStorage.removeItem(key(world));}catch{}}
    }catch(e){if(lastProject.current===world){setContent(original.content||text);setAttachments(original.attachments);setBranch(original.branch);setLocalError(e.message);}}
    finally{sendingMessage.current=false;setSubmitting(false);}
  };
  const repeat=(message,index)=>{
    const source=message.role==="user"?message:[...messages.slice(0,index)].reverse().find(m=>m.role==="user");
    if(source&&!chatPresentation(source).session)submit(null,{content:source.content,agent_role:message.agent_role||source.agent_role,reply_to_message_id:source.id,branch_from_message_id:source.id});
  };
  const copy=async(m)=>{try{await navigator.clipboard.writeText(m.content);setCopied(m.id);setTimeout(()=>setCopied(null),1300);}catch{setLocalError("Clipboard unavailable in this browser.");}};
  const edit=m=>{setContent(chatPresentation(m).content);setBranch(m.id);setCollapsed(false);setTimeout(()=>textarea.current?.focus(),20);};
  if(collapsed)return <button type="button" className="researchChatReveal" onClick={()=>setCollapsed(false)} aria-label="Open research chat" title="Open research chat"><PanelRightOpen size={19}/><span>Research chat</span></button>;
  return <aside className={`researchChat ${drawerStyles.drawer} ${dragging?"researchChat--dragging":""}`} style={{'--chat-width':`${width}%`}} aria-label="Research chat" onDragOver={e=>{e.preventDefault();setDragOver(true);}} onDragLeave={e=>{if(!e.currentTarget.contains(e.relatedTarget))setDragOver(false);}} onDrop={e=>{e.preventDefault();setDragOver(false);addFiles(e.dataTransfer.files);}}>
    <div role="separator" tabIndex={0} aria-orientation="vertical" aria-label="Resize research chat" aria-valuemin={10} aria-valuemax={60} aria-valuenow={Math.round(width)} className="researchChatResize" onPointerDown={e=>{e.preventDefault();setDragging(true);}} onDoubleClick={()=>setWidth(42)} onKeyDown={e=>{if(e.key==="ArrowLeft")setWidth(v=>Math.min(60,v+2));if(e.key==="ArrowRight")setWidth(v=>Math.max(10,v-2));}}><GripVertical size={14}/></div>
    {dragOver&&<div className="researchDrop"><Paperclip size={28}/><strong>Drop research files</strong><span>Text, data, or molecular structures · 2 MiB each</span></div>}
    <header className="researchChatHeader"><div className="researchIdentity"><Sparkles size={19}/><div><strong>{project?.name||"Research assistant"}</strong><small>{project?"Persistent conversation · evidence-linked":"Configure a provider to begin"}</small></div></div><div className="researchHeaderTools">
      <button type="button" title="Research history" aria-label="Research history" onClick={()=>setHistoryOpen(v=>!v)}><History size={17}/></button>
      <button type="button" title="New research world" aria-label="New research world" onClick={onNewProject}><Plus size={17}/></button>
      <button type="button" title={width>=59?"Restore chat width":"Expand chat to 60%"} aria-label="Resize chat" onClick={()=>setWidth(width>=59?42:60)}>{width>=59?<Minimize2 size={16}/>:<Maximize2 size={16}/>}</button>
      <button type="button" title="Collapse chat" aria-label="Collapse chat" onClick={()=>setCollapsed(true)}><PanelRightClose size={17}/></button>
    </div></header>
    {historyOpen&&<div className="researchHistoryOverlay"><div className="researchHistoryClose"><button type="button" onClick={()=>setHistoryOpen(false)}><X size={15}/>Close history</button></div><ConversationHistory projects={projects} activeId={project?.id} onSelect={id=>{onProjectSelect(id);setHistoryOpen(false);}} onNew={()=>{onNewProject();setHistoryOpen(false);}} onRename={onRenameProject} onDelete={id=>{if(!busy)onDeleteProject(id);}}/></div>}
    <div className="researchContext"><label className={drawerStyles.duration}>{activeJob?'Next session limit':'Work limit'}<select aria-label="Maximum work time for the next new session" value={customDuration?'custom':duration} onChange={event=>{const value=event.target.value;if(value==='custom'){setCustomDuration(true);setCustomMinutes('30');setDuration('1800');try{if(project?.id)localStorage.setItem(`phaseforge.chat.duration:${project.id}`,'1800');}catch{}}else{setCustomDuration(false);setDuration(value);try{if(project?.id)localStorage.setItem(`phaseforge.chat.duration:${project.id}`,value);}catch{}}}}>{CHAT_DURATION_PRESETS.map(([value,label])=><option key={value} value={value}>{label}</option>)}<option value="custom">Custom…</option></select></label>{customDuration&&<label className={drawerStyles.duration}><input type="number" min="1" max="10080" step="1" aria-label="Custom work time in minutes" value={customMinutes} onChange={event=>{setCustomMinutes(event.target.value);try{const seconds=customMinutesToSeconds(event.target.value);setDuration(String(seconds));if(project?.id)localStorage.setItem(`phaseforge.chat.duration:${project.id}`,String(seconds));}catch{}}}/>minutes</label>}<Link href="/usage/" title="Token usage, cost controls, and stopping all calls"><Wallet size={14}/><span>Usage controls</span></Link></div>
    {structures.length>0&&<div className="researchStructureContext"><FlaskConical size={14}/><select aria-label="Molecular structure context" value={selectedStructure?.id||""} onChange={e=>onStructureSelect?.(e.target.value||null)}><option value="">No structure selected</option>{structures.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></div>}
    <div className="researchTranscript" ref={transcript} onScroll={()=>{const el=transcript.current;const close=el.scrollHeight-el.scrollTop-el.clientHeight<90;bottom.current=close;setNearBottom(close);}}>
      {!messages.length&&<div className="researchChatEmpty"><span><Sparkles size={25}/></span><h3>{project?"What should we investigate?":"Your research starts here."}</h3><p>{project?"Ask a scientific question, run an experiment, or inspect its evidence. Work continues when you navigate elsewhere.":"Create a research world, connect your API account, and choose a model."}</p>{!configured.length?<Link href="/settings/" className="button button--secondary"><Settings size={15}/>Connect provider</Link>:<div className="researchSuggestions">{["Design and run an experiment","Explain the current evidence","Inspect the selected molecular structure"].map(text=><button key={text} type="button" onClick={()=>setContent(text)} disabled={!project}>{text}</button>)}</div>}</div>}
      {messages.map((m,index)=>{const user=m.role==="user";const meta=m.metadata||{};const presentation=chatPresentation(m);const source=user?m:[...messages.slice(0,index)].reverse().find(item=>item.role==="user");const sessionOwned=presentation.session||chatPresentation(source).session;return <article key={m.id} className={`researchMessage ${user?"researchMessage--user":"researchMessage--assistant"} ${m.kind==="error"?"researchMessage--error":""}`}>
        {!user&&<div className="assistantIdentity"><Bot size={16}/><strong>{statusLabel(m.agent_role||"assistant")}</strong><time>{formatDate(m.created_at)}</time></div>}
        <div className="researchMessageBody">{user&&<div className="userMessageMeta">{presentation.label} <time>{formatDate(m.created_at)}</time></div>}<ChatMarkdown>{presentation.content}</ChatMarkdown>{meta.attachments?.length>0&&<div className="researchMessageFiles">{meta.attachments.map((a,i)=><span key={a.id||i}><Paperclip size={12}/>{a.name}</span>)}</div>}</div>
        {(m.manifest_id||meta.submitted_run_id||meta.capability_gap||meta.research_plan_id)&&<div className="researchArtifacts">{meta.research_plan_id&&<span><Sparkles size={13}/>Research plan ready</span>}{m.manifest_id&&<span><FlaskConical size={13}/>Experiment revision saved</span>}{meta.submitted_run_id&&<span>Run submitted</span>}{meta.capability_gap&&<span><ShieldAlert size={13}/>Capability gap recorded</span>}</div>}
        <footer className="researchMessageActions"><button type="button" onClick={()=>copy({...m,content:presentation.content})} title="Copy message">{copied===m.id?<Check size={13}/>:<Copy size={13}/>}<span>{copied===m.id?"Copied":"Copy"}</span></button>{user&&!sessionOwned&&<button type="button" onClick={()=>edit(m)} disabled={busy}><Pencil size={13}/><span>Edit & branch</span></button>}{!sessionOwned&&<button type="button" onClick={()=>repeat(m,index)} disabled={busy||blocked} title="Sends a new metered model request"><RotateCcw size={13}/><span>{user||m.kind==="error"?"Retry":"Regenerate"}</span></button>}{sessionOwned&&<small>Research session · progress and controls in Agents</small>}{!user&&meta.model&&<small>{meta.model}</small>}</footer>
      </article>;})}
      {activeJob&&<div className="researchProgress" role="status"><Bot size={18}/><div><strong>{statusLabel(activeJob.state)}</strong><ChatMarkdown>{activeJob.events?.at(-1)?.message||activeJob.title}</ChatMarkdown><small>{activeJob.input?.model} · Model and time controls apply to your next request.</small><div className="progressPulse"><i/><i/><i/></div></div><button type="button" onClick={onCancel}><Square size={13}/>Stop</button></div>}
      {busy&&!activeJob&&<div className="researchProgress" role="status"><Bot size={18}/><div><strong>{phase?statusLabel(phase):"Working on this request…"}</strong><p>{phase==="repairing_proposal"?"Repairing only the rejected proposal within your configured budget.":phase==="validating_proposal"?"Checking the schema and numerical constraints before execution.":"A bounded model call. Stop remains available throughout the request."}</p><div className="progressPulse"><i/><i/><i/></div></div><button type="button" onClick={onCancel} disabled={!activeRequestId}><Square size={13}/>Stop</button></div>}
    </div>
    {!nearBottom&&<button type="button" className="jumpToLatest" onClick={jump}><ArrowDown size={13}/>Latest messages</button>}
    {(localError||error)&&<div className="researchChatError" role="alert"><ShieldAlert size={15}/><span>{localError||error}</span>{localError&&<button type="button" onClick={()=>setLocalError("")} aria-label="Dismiss error"><X size={13}/></button>}</div>}
    {findingsRun&&!busy&&<div className="chatFindingsReady"><ShieldAlert size={15}/><div><strong>Measured evidence is ready</strong><span>Run {findingsRun.id.slice(0,8)} · review findings and choose the next experiment.</span></div><button type="button" onClick={onOpenFindings}>Open findings</button></div>}
    <div className="researchComposerArea">
      {blocked&&<div className="researchSetupHint">{policy?.paused?<>Paid calls paused. <Link href="/usage/">Resume in Usage & cost</Link></>:!configured.length?<>No provider connected. <Link href="/settings/">Save a key</Link></>:projectSelectionError}</div>}
      {branch&&<div className="researchBranch"><RotateCcw size={13}/><span>New branch from an earlier message. Original history is retained.</span><button type="button" onClick={()=>setBranch(null)} aria-label="Clear branch"><X size={13}/></button></div>}
      <form className="researchComposer" onSubmit={submit}>
        {!!attachments.length&&<div className="researchAttachmentTray">{attachments.map((a,i)=><span key={`${a.name}-${i}`}><Paperclip size={13}/>{a.name}<button type="button" onClick={()=>setAttachments(arr=>arr.filter((_,j)=>i!==j))} aria-label={`Remove ${a.name}`}><X size={12}/></button></span>)}</div>}
        <textarea ref={textarea} value={content} onChange={e=>{draftOwner.current=project?.id||null;setContent(e.target.value);}} placeholder={project?'Ask a question or describe an experiment…':"Create a research world to start a conversation"} disabled={!project} rows={3} onKeyDown={e=>{if(e.key==="Enter"&&!e.shiftKey&&!e.nativeEvent.isComposing){e.preventDefault();submit(e);}}}/>
        <div className={`researchComposerToolbar ${drawerStyles.composerToolbar}`}><div className={drawerStyles.composerTools}>
          <input type="file" multiple hidden ref={file} accept={ALLOWED.map(x=>`.${x}`).join(",")} onChange={e=>{addFiles(e.target.files);e.target.value="";}}/>
          <button className="attachButton" type="button" aria-label="Attach research files" title="Attach research files" disabled={!project||(busy&&!activeJob)||submitting} onClick={()=>file.current?.click()}><Paperclip size={18}/></button>
          <ModelPicker providers={providers} compact/>
        </div><div className={drawerStyles.sendActions}>{busy&&<button type="button" className={`researchSend researchSend--stop ${drawerStyles.composerAction}`} aria-label="Stop active request" title="Stop active request" disabled={!activeRequestId} onClick={onCancel}><Square size={15} fill="currentColor" aria-hidden="true"/></button>}{(!busy||activeJob)&&<button type="submit" className={`researchSend ${drawerStyles.composerAction}`} aria-label={activeJob?'Send update to active session':'Send message'} title={activeJob?'Send update · Enter':'Send · Enter'} disabled={!project||submitting||(!activeJob&&blocked)||(!content.trim()&&!attachments.length)}><ArrowUp size={21} strokeWidth={2.5} aria-hidden="true"/></button>}</div></div>
      </form>
      {activeJob&&<p className={drawerStyles.steeringNote}>Your update joins this session at its next action boundary. It keeps {activeJob.input?.model||'its current model'} and the existing deadline; running calculations retain their original inputs.</p>}
      <div className="researchComposerNote"><span>Enter to {activeJob?'send an update':'send'} · Shift+Enter for a new line</span><Link href="/usage/">{policy?`${fmtLimit(policy.max_output_tokens)} output cap · ${policy.repair_attempts} repair${policy.repair_attempts===1?"":"s"}`:"Usage & cost"}</Link></div>
    </div>
  </aside>;
}
function fmtLimit(n){return Number(n).toLocaleString();}
