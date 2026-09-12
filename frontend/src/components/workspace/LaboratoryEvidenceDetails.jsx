'use client';
import {useEffect,useState} from 'react';
import {api} from '@/lib/api';
import {artifactPath,laboratoryViewable} from '@/lib/laboratory-plot.mjs';
import {attentionArtifactLinks,boundedEvidenceText} from '@/lib/laboratory-attention-navigation.mjs';
import ChatMarkdown from './ChatMarkdown';

function RecordedJson({value,label}){
  const preview=boundedEvidenceText(value);
  return <details className="labDetails"><summary>{label}</summary><pre style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere',maxHeight:360,overflow:'auto'}}>{preview.text}</pre>{preview.truncated&&<p>Showing the first 65,536 characters. Download the complete job record below.</p>}</details>;
}

/** A selected attempt's evidence, separate from the unrelated current viewer. */
export default function LaboratoryEvidenceDetails({request,jobs=[],onOpen,onBack}){
  const [selectedId,setSelectedId]=useState(request.jobIds[0]),[record,setRecord]=useState(null),[error,setError]=useState(''),[retry,setRetry]=useState(0),[count,setCount]=useState(40);
  useEffect(()=>{
    const controller=new AbortController();setRecord(null);setError('');setCount(40);
    api.labJob(selectedId,controller.signal).then(value=>{
      if(controller.signal.aborted)return;
      if(value.id!==selectedId||value.project_id!==request.projectId)throw Error('The evidence record does not belong to this selected project and attempt.');
      setRecord(value);
    }).catch(value=>{if(!controller.signal.aborted)setError(value.message||'The saved evidence could not be loaded.');});
    return()=>controller.abort();
  },[request.projectId,selectedId,retry]);
  const files=record?attentionArtifactLinks(record):[],events=[...(record?.events||[])].sort((a,b)=>a.sequence-b.sequence),visible=events.slice(-count);
  return <section className="labEvidence" aria-label="Selected attempt evidence" style={{padding:20,overflow:'auto',height:'100%',minHeight:0}}>
    <header className="labSectionHeader"><div><h2>Saved evidence</h2><p>Direction from attempt <code>{request.sourceId}</code>. This view reads saved records and starts no work.</p></div><button type="button" className="button button--secondary" onClick={onBack}>Back to Agents</button></header>
    {!request.hasReferencedEvidence&&<p className="labNotice">This direction listed no separate evidence jobs. The attempt's own events, result and artifact references are shown.</p>}
    <label>Evidence record <select aria-label="Saved evidence record" value={selectedId} onChange={event=>setSelectedId(event.target.value)}>{request.jobIds.map(id=><option key={id} value={id}>{jobs.find(job=>job.id===id&&job.project_id===request.projectId)?.title||'Saved record'} · {id}{id===request.sourceId?' · Original attempt':''}</option>)}</select></label>
    {error&&<p role="alert">{error} <button type="button" onClick={()=>setRetry(value=>value+1)}>Retry evidence loading</button></p>}
    {!record&&!error&&<p role="status">Loading the exact saved evidence record…</p>}
    {record&&<><h3>{record.title||'Saved attempt'}</h3><p><code>{record.id}</code> · {record.kind} · {record.state}</p>{record.error&&<p className="labNotice labNotice--warning" role="status">{record.error}</p>}
      <div className="expActions">{laboratoryViewable(record)&&<button type="button" className="button button--secondary" onClick={()=>onOpen?.(record)}>Open this saved output</button>}<a href={`${api.baseUrl}/api/laboratory/jobs/${encodeURIComponent(record.id)}`} download={`phaseforge-job-${record.id}.json`}>Download complete job record</a></div>
      <h3>Registered artifact references</h3>{files.length?<ul>{files.map(file=><li key={file.path}><a href={`${api.baseUrl}${artifactPath(record.id,file.path)}`} download>{file.path}</a> <code>{file.sha256}</code></li>)}</ul>:<p>No separate registered file list is present in this record. Original tool receipts and result data remain below.</p>}
      <RecordedJson label="Recorded result and artifact references" value={record.result}/><RecordedJson label="Original request" value={record.input}/>
      <h3>Recorded activity ({events.length})</h3>{events.length>count&&<button type="button" onClick={()=>setCount(value=>value+80)}>Show earlier activity ({events.length-count})</button>}
      <ol>{visible.map(event=><li key={event.sequence}><strong>{event.kind?.replaceAll('_',' ')}</strong> <time dateTime={event.at}>{new Date(event.at).toLocaleString()}</time><ChatMarkdown>{event.message}</ChatMarkdown>{event.data&&<RecordedJson label="Recorded tool or evidence data" value={event.data}/>}</li>)}</ol>{!events.length&&<p>No activity events were recorded for this attempt.</p>}
    </>}
  </section>;
}
