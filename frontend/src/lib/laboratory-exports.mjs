export function exportStatusFromJob(job){
  const id=encodeURIComponent(job.id);
  return {...job,status_url:`/api/laboratory/exports/${id}`,cancel_url:`/api/laboratory/exports/${id}/cancel`,
    download_url:job.kind==='export'&&job.state==='completed'?`/api/laboratory/jobs/${id}/artifacts/simulation.mp4`:null};
}
export function exportsForSource(jobs,sourceId){
  return (jobs||[]).filter(job=>job.kind==='export'&&(job.parent_id===sourceId||job.input?.source_job_id===sourceId))
    .sort((a,b)=>Date.parse(b.created_at)-Date.parse(a.created_at)).map(exportStatusFromJob);
}
export function exportDownloadName(job){return `phaseforge-simulation-${job.id}.mp4`;}

/** Export bookkeeping can reconnect without interrupting an already loaded trajectory. */
export function createExportRefresh({load,onValue,onUnavailable,signal}){
  let busy=false;
  return async()=>{
    if(busy||signal.aborted)return;
    busy=true;
    try{const value=await load();if(signal.aborted)return;onValue(value);onUnavailable(false);}
    catch(error){if(!signal.aborted&&error.name!=='AbortError')onUnavailable(true);}
    finally{busy=false;}
  };
}
