import {laboratoryAttention} from './laboratory-recovery.mjs';
import {registeredArtifacts} from './laboratory-artifacts.mjs';
import {artifactPath} from './laboratory-plot.mjs';

/** Only a current, server-listed direction may choose a new view. */
export function attentionDestination(job,action,projectId){
  if(!job||job.project_id!==projectId)throw Error('Open this attempt in its own project before choosing a next step.');
  const attention=laboratoryAttention(job);
  if(!attention?.actions.some(candidate=>candidate.id===action?.id))throw Error('This direction is no longer available. Reload the attempt to see its current options.');
  if(action.id==='revise_request')return {type:'chat'};
  if(action.id==='inspect_capabilities')return {type:'capabilities'};
  if(action.id!=='inspect_evidence')throw Error('Use the explicit job control for this action.');
  const ids=Array.isArray(attention.evidence_job_ids)?attention.evidence_job_ids:[];
  if(ids.some(id=>typeof id!=='string'||!id||/[/?#\\]/.test(id)))throw Error('The saved evidence identities are invalid; inspect the original attempt record.');
  return {type:'evidence',sourceId:job.id,projectId,jobIds:[...new Set([...ids,job.id])],hasReferencedEvidence:ids.length>0};
}

/** Links come from explicit registered paths, never an inferred directory scan. */
export function attentionArtifactLinks(job){
  const rows=[...registeredArtifacts(job)];
  const add=(path,sha256,bytes)=>{if(typeof path!=='string'||!/^[a-f0-9]{64}$/.test(sha256||''))return;try{artifactPath(job.id,path);rows.push({path,sha256,bytes});}catch{}};
  const diagnostics=job.result?.diagnostics;
  add(diagnostics?.path,diagnostics?.sha256);
  add(job.result?.process_receipt,job.result?.process_receipt_sha256);
  return [...new Map(rows.map(row=>[row.path,row])).values()];
}

export function boundedEvidenceText(value,limit=65536){
  const text=JSON.stringify(value??null,null,2);
  return {text:text.slice(0,limit),truncated:text.length>limit};
}
