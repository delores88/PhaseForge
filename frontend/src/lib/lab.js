// Pure view-model helpers: no generated scientific claims and no remote calls.
export const finite = value => value !== null && value !== undefined && (typeof value === "number" || (typeof value === "string" && value.trim() !== "")) && Number.isFinite(Number(value)) ? Number(value) : null;
export function percent(value) { const n=finite(value); return n===null?null:Math.min(100,Math.max(0,n)); }
export function number(value) {
  const n=finite(value); if(n===null)return "Not available";
  if(n===0)return "0";
  return Math.abs(n)<.001||Math.abs(n)>=1e7?n.toExponential(3):new Intl.NumberFormat(undefined,{maximumSignificantDigits:6}).format(n);
}
export function bytes(value) {
  let n=finite(value);if(n===null||n<0)return "Not available";
  const units=["B","KiB","MiB","GiB","TiB"];let i=0;while(n>=1024&&i<4){n/=1024;i++;}
  return `${new Intl.NumberFormat(undefined,{maximumFractionDigits:i>1?2:0}).format(n)} ${units[i]}`;
}
export function memoryPercent(used,total) {
  const u=finite(used),t=finite(total);return u===null||t===null||t<=0?null:percent(100*u/t);
}
export function challengeStatus(test) {
  if(test?.evidence_version!==2)return "inconclusive";
  return ["passed","failed","inconclusive"].includes(test.status)?test.status:"inconclusive";
}
export function statusText(value) {
  return ({checks_passed:"Declared checks passed",checks_failed:"Checks need attention",inconclusive:"Validation incomplete",not_adjudicated:"No pass rule",passed:"Passed",failed:"Failed"})[value]||String(value||"Not assessed").replaceAll("_"," ");
}
export function phaseDetails(workflow,projectId,run,hasManifest,explaining=false,pendingRevision=false) {
  const req=workflow?.requests?.find(r=>r.project_id===projectId);
  const phase=req?.phase||"";
  const activeRun=(workflow?.runs||[]).find(r=>r.project_id===projectId&&["queued","running"].includes(r.status))||run;
  if(explaining||["explaining_evidence","saving_findings","waiting_to_explain"].includes(phase))return {stage:4,label:phase==="waiting_to_explain"?"Waiting for the explanation slot":"Explaining measured evidence",busy:true,detail:"One metered interpretation. No next experiment will be launched.",started:req?.started_at};
  if(req)return {stage:phase.includes("validat")||phase==="saving_revision"?2:1,label:({preparing:"Preparing context",requesting_model:"Model is authoring a proposal",repairing_proposal:"Repairing a rejected proposal",validating_proposal:"Checking the proposal",saving_revision:"Saving the immutable revision"})[phase]||"Preparing request",busy:true,started:req.started_at,detail:"Provider generation has no reliable percentage or ETA. Stop remains available."};
  if(activeRun&&["queued","running"].includes(activeRun.status))return {stage:3,label:activeRun.phase||"Queued",busy:true,started:activeRun.started_at||activeRun.queued_at,run:activeRun,progress:percent(100*activeRun.progress),detail:"Numerical work-plan progress, not a wall-clock ETA. Model generation and simulation are separate stages."};
  if(pendingRevision)return {stage:2,label:"Setup ready to run",busy:false,detail:"Run the saved setup or edit it. Research notes are optional."};
  if(activeRun?.status==="completed")return {stage:4,label:"Measurements ready · choose the next step",busy:false,detail:"Replay, run with finer steps, extend the horizon, or build a changed setup. No automatic follow-on run."};
  if(["failed","cancelled"].includes(activeRun?.status))return {stage:3,label:activeRun.status==="failed"?"Run failed · review the error":"Run stopped",busy:false,detail:activeRun.error||"No automatic retry is scheduled."};
  if(hasManifest)return {stage:2,label:"Setup ready to run",busy:false,detail:"Use Run saved setup in Experiment. No research checklist is required."};
  return {stage:0,label:"Ready to build an experiment",busy:false,detail:"Describe a test in Experiment or enter your own equations. Build saves a setup; Build & run starts one bounded simulation."};
}
export function sampleFrames(frames,time) {
  if(!frames?.length)return {left:null,right:null,alpha:0,index:0};
  if(time<=frames[0].time)return {left:frames[0],right:frames[0],alpha:0,index:0};
  const last=frames.length-1;if(time>=frames[last].time)return {left:frames[last],right:frames[last],alpha:0,index:last};
  let lo=0,hi=last;while(lo+1<hi){const mid=(lo+hi)>>1;if(frames[mid].time<=time)lo=mid;else hi=mid;}
  const span=frames[hi].time-frames[lo].time;
  return {left:frames[lo],right:frames[hi],alpha:span>0?(time-frames[lo].time)/span:0,index:lo};
}
export function interpolateEntity(a,b,alpha) {
  if(!a)return b?.position||[0,0,0];
  if(!b||a.id!==b.id)return a.position;
  return a.position.map((v,i)=>v+(b.position[i]-v)*alpha);
}
export function normalizedFrames(source,maxFrames=4000,maxEntities=12000,totalBudget=240000) {
  // Rendering is bounded independently of solver evidence; first/final frames are retained.
  const valid=(Array.isArray(source)?source:[]).filter(f=>finite(f.time)!==null&&Array.isArray(f.entities)).slice().sort((a,b)=>Number(a.time)-Number(b.time));
  if(!valid.length)return [];
  const entityLimit=Math.max(1,Math.min(maxEntities,Math.floor(totalBudget/2)));
  let widest=1;for(const frame of valid)widest=Math.max(widest,Math.min(entityLimit,frame.entities.length));
  const frameLimit=Math.max(2,Math.min(maxFrames,Math.floor(totalBudget/widest)));
  const indices=valid.length<=frameLimit?valid.map((_,i)=>i):Array.from({length:frameLimit},(_,i)=>Math.round(i*(valid.length-1)/(frameLimit-1)));
  const normalized=indices.map(i=>valid[i]).map(f=>({time:Number(f.time),entities:f.entities.slice(0,entityLimit).filter(e=>e.position?.length===3&&e.position.every(v=>finite(v)!==null)).map(e=>({...e,id:String(e.id),position:e.position.map(Number),velocity:e.velocity?.length===3&&e.velocity.every(v=>finite(v)!==null)?e.velocity.map(Number):null}))}));
  return normalized.some(frame=>frame.entities.length)?normalized:[];
}
export function exportJson(name,value){const url=URL.createObjectURL(new Blob([JSON.stringify(value,null,2)],{type:"application/json"}));const a=document.createElement("a");a.href=url;a.download=name;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);}
