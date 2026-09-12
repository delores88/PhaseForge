'use client';
import {useEffect,useState} from 'react';
import {Download} from 'lucide-react';
import {api} from '@/lib/api';
import {laboratoryUrl} from '@/lib/laboratory-http.mjs';
import {artifactPath} from '@/lib/laboratory-plot.mjs';
import {outputArtifacts,previewableArtifact,registeredArtifacts,verifyArtifactImage,numericalArtifact,jsonArtifact,verifyArtifactJson,boundedArtifactBytes,JSON_PREVIEW_BYTES} from '@/lib/laboratory-artifacts.mjs';
import styles from './LaboratoryPlotViewer.module.css';
import dataStyles from './LaboratoryArtifactViewer.module.css';
import ScientificImageViewer from './ScientificImageViewer';

export default function LaboratoryArtifactViewer({job}){
  const [full,setFull]=useState(job),[selected,setSelected]=useState(''),[image,setImage]=useState(null),[error,setError]=useState(''),[retry,setRetry]=useState(0),[selectedData,setSelectedData]=useState(''),[data,setData]=useState(null),[mode,setMode]=useState('image'),[imageError,setImageError]=useState(''),[dataError,setDataError]=useState('');
  const outputs=outputArtifacts(full),all=registeredArtifacts(full),images=outputs.filter(previewableArtifact),artifact=images.find(item=>item.path===selected)||images[0];
  const dataFiles=outputs.filter(jsonArtifact),dataArtifact=dataFiles.find(item=>item.path===selectedData)||numericalArtifact(full),showData=dataFiles.length>0&&(mode==='data'||!images.length),previewError=error||(showData?dataError:imageError);
  const description=typeof full.result?.reported_result?.summary==='string'?full.result.reported_result.summary:'Files retained by the completed local job.',scope=typeof full.result?.reported_result?.scope==='string'?full.result.reported_result.scope:'These are recorded job outputs. Successful execution alone does not establish scientific validity.';
  useEffect(()=>{const controller=new AbortController();api.labJob(job.id,controller.signal).then(value=>{if(!controller.signal.aborted){setFull(value);setError('');}}).catch(error=>{if(error.name!=='AbortError')setError(error.message);});return()=>controller.abort();},[job.id,job.updated_at,retry]);
  useEffect(()=>{
    setImage(null);setImageError('');if(!artifact)return;const controller=new AbortController();let objectUrl;
    async function load(){try{if(artifact.bytes>32*1024*1024)throw Error('This image exceeds the 32 MiB preview limit. Download the original file below.');const response=await fetch(laboratoryUrl(artifactPath(job.id,artifact.path)),{signal:controller.signal});if(!response.ok)throw Error(`Saved image unavailable (${response.status}).`);const bytes=await response.arrayBuffer(),mime=await verifyArtifactImage(bytes,artifact);if(controller.signal.aborted)return;objectUrl=URL.createObjectURL(new Blob([bytes],{type:mime}));setImage(objectUrl);setImageError('');}catch(error){if(error.name!=='AbortError')setImageError(error.message);}}
    load();return()=>{controller.abort();if(objectUrl)URL.revokeObjectURL(objectUrl);};
  },[job.id,artifact?.path,artifact?.sha256,retry]);
  useEffect(()=>{
    setData(null);setDataError('');if(!dataArtifact)return;const controller=new AbortController();
    async function load(){try{if(dataArtifact.bytes>JSON_PREVIEW_BYTES)throw Error('This result exceeds the 1 MiB JSON preview limit. Download the original file below.');const response=await fetch(laboratoryUrl(artifactPath(job.id,dataArtifact.path)),{signal:controller.signal});if(!response.ok)throw Error(`Saved result unavailable (${response.status}).`);const bytes=await boundedArtifactBytes(response),verified=await verifyArtifactJson(bytes,dataArtifact);if(!controller.signal.aborted){setData(verified);setDataError('');}}catch(error){if(error.name!=='AbortError')setDataError(error.message);}}
    load();return()=>controller.abort();
  },[job.id,dataArtifact?.path,dataArtifact?.sha256,retry]);
  const link=item=><a key={item.path} href={laboratoryUrl(artifactPath(job.id,item.path))} download={`phaseforge-${job.id}-${item.path.split('/').at(-1)}`}><Download size={14}/>{item.path.split('/').at(-1)}</a>;
  return <section className={`${styles.viewer} ${dataStyles.artifactViewer}`} aria-label="Saved output files">
    <header className={styles.header}><div><span>SAVED OUTPUT</span><h3 className={dataStyles.title} title={job.title||'Generated files'}>{job.title||'Generated files'}</h3><p>{description}</p></div></header>
    {images.length>0&&dataFiles.length>0&&<div className={dataStyles.mode} role="group" aria-label="Saved output view"><button type="button" aria-pressed={!showData} onClick={()=>setMode('image')}>Image</button><button type="button" aria-pressed={showData} onClick={()=>setMode('data')}>Numerical result</button></div>}
    {images.length>1&&!showData&&<label>Preview image <select aria-label="Preview saved output image" value={artifact?.path||''} onChange={event=>setSelected(event.target.value)}>{images.map(item=><option key={item.path} value={item.path}>{item.path.split('/').at(-1)}</option>)}</select></label>}
    {images.length>0&&<div className={dataStyles.imagePanel} hidden={showData}><ScientificImageViewer src={image} alt={artifact?.path.split('/').at(-1)||'Saved output'} empty="Loading and verifying the saved image…"/></div>}
    {(showData||!images.length)&&<div className={dataStyles.result}>
      {dataFiles.length>1&&<label>Saved result <select aria-label="Preview saved JSON result" value={dataArtifact?.path||''} onChange={event=>setSelectedData(event.target.value)}>{dataFiles.map(item=><option key={item.path} value={item.path}>{item.path}</option>)}</select></label>}
      {dataArtifact?<><h4>{dataArtifact.path}</h4>{data?<><pre tabIndex={0} aria-label="Verified saved JSON result">{data.text}</pre>{data.truncated&&<p>Showing the first 65,536 characters. Download the original file for the complete result.</p>}</>:!previewError&&<p role="status">Loading and verifying the saved result…</p>}</>:<p>{full.state==='completed'?'Saved files are available below.':'Saved files appear when the job completes.'}</p>}
    </div>}
    <footer className={styles.footer}><p>{scope}</p><div className={styles.downloads}>{outputs.map(link)}</div><details><summary>All retained files & provenance</summary><div className={styles.downloads}>{all.map(link)}</div>{artifact&&!showData&&<p>Displayed image SHA-256 <code>{artifact.sha256}</code></p>}{showData&&dataArtifact&&data&&<p>Displayed JSON SHA-256 <code>{dataArtifact.sha256}</code></p>}</details>{previewError&&<p className={styles.error} role="status">{previewError} <button type="button" onClick={()=>setRetry(value=>value+1)}>Retry</button></p>}</footer>
  </section>;
}
