import {laboratoryLineage} from './laboratory-lineage.mjs';

const timestamp=record=>{
  for(const value of [record.completed_at,record.finished_at,record.updated_at,record.created_at,record.queued_at]){
    const parsed=Date.parse(value||'');if(Number.isFinite(parsed))return parsed;
  }
  return 0;
};

/** One project-scoped index across both retained execution stores. Never selects a view. */
export function completedWorkspaceResults({jobs=[],runs=[],projectId}={}){
  if(!projectId)return [];
  const laboratory=laboratoryLineage(jobs.filter(job=>job.project_id===projectId&&job.state==='completed'))
    .map(({job,type})=>({key:`laboratory:${job.id}`,id:job.id,projectId,source:'laboratory',title:job.title||type,type,record:job,completedAt:timestamp(job)}));
  const equations=runs.filter(run=>run.project_id===projectId&&run.status==='completed')
    .map(run=>({key:`run:${run.id}`,id:run.id,projectId,source:'run',title:run.name||'Equation experiment',type:'Equation measurements',record:run,completedAt:timestamp(run)}));
  return [...new Map([...laboratory,...equations].map(result=>[result.key,result])).values()]
    .sort((a,b)=>a.completedAt-b.completedAt||a.key.localeCompare(b.key));
}

/** Resolve clicks against the current project snapshot, not a stale rendered closure. */
export function currentWorkspaceResult(results,key,projectId){
  return results.find(result=>result.key===key&&result.projectId===projectId)||null;
}

export const WORKSPACE_PANELS=new Set(['laboratory','capabilities','world','manifest','findings','evidence','advanced','sessions','research','molecules','equations','studio']);
export function linkedRunPanel(panel){return WORKSPACE_PANELS.has(panel)?panel:'findings';}

export function unhandledRunLink(id,panel,handledKey){
  if(typeof id!=='string'||!id)return null;
  const key=`${id}:${typeof panel==='string'?panel:''}`;
  return key===handledKey?null:{id,key,panel:linkedRunPanel(panel)};
}
