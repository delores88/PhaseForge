import {laboratoryViewable} from './laboratory-plot.mjs';

export function newestLaboratoryOutput(jobs,projectId){
  return jobs.filter(job=>job.project_id===projectId&&laboratoryViewable(job))
    .sort((a,b)=>Date.parse(b.created_at)-Date.parse(a.created_at)||b.id.localeCompare(a.id))[0]||null;
}

// A saved output is selected once on entry. A later completion must not replace
// the iteration that the researcher is inspecting.
export function initialLaboratorySelection(jobs,projectId,selectedId){
  if(jobs.some(job=>job.id===selectedId&&job.project_id===projectId&&laboratoryViewable(job)))return selectedId;
  return newestLaboratoryOutput(jobs,projectId)?.id||null;
}

export function workspaceTools({runs=[],manifests=[],structures=[]}={}){
  const tools=[['capabilities','Solver coverage'],['equations','Equation editor'],['studio','CAD, PCB & scene tools']];
  if(structures.length)tools.push(['molecules','Molecular files']);
  if(manifests.length){tools.push(['world','Equation experiments'],['manifest','Equation setup'],['advanced','Batch runs & verification']);}
  if(runs.length)tools.push(['findings','Equation measurements'],['evidence','Equation evidence']);
  return tools;
}

export function latestDeliverable(jobs){
  return jobs.filter(job=>job.kind==='session'&&job.result?.deliverable)
    .sort((a,b)=>Date.parse(b.created_at)-Date.parse(a.created_at))[0]||null;
}
