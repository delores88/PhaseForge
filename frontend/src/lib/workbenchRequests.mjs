import {durationSeconds,savedDuration} from './chatDuration.mjs';
import {laboratoryViewable} from './laboratory-plot.mjs';
export function snapshotConversationModel(projectId,getSelection,getSelectionError) {
  if(!projectId)throw Error('Create a project first.');
  const problem=getSelectionError(projectId);
  if(problem)throw Error(problem);
  const selection=getSelection(projectId);
  if(!selection?.provider||!selection?.model)throw Error('Choose a model for this conversation.');
  return Object.freeze({provider:selection.provider,model:selection.model,reasoning_effort:selection.reasoning_effort??null,research_mode:!!selection.research_mode});
}

export function selectedLaboratoryContext(projectId,tab,selectedId,jobs) {
  if(tab!=='laboratory'||!selectedId)return null;
  return jobs.find(job=>job.id===selectedId&&job.project_id===projectId&&laboratoryViewable(job))||null;
}

export function conversationTimeLimit(projectId,storage) {
  let saved;try{saved=storage?.getItem(`phaseforge.chat.duration:${projectId}`);}catch{}
  return durationSeconds(savedDuration(saved));
}

// A response from an older visit cannot select a new item after the user leaves
// and returns, even if the project/tab names happen to be the same again.
export function createViewTracker() {
  let key=null,epoch=0;
  return {
    observe(view){const next=JSON.stringify(view);if(next!==key){key=next;epoch+=1;}},
    capture(){return epoch;},
    isCurrent(ticket){return ticket===epoch;},
  };
}

export function createProjectRequestGate() {
  const pending=new Map();
  return {
    begin(projectId,requestId){
      if(pending.has(projectId))throw Error('This conversation is already submitting a request.');
      const ticket=Object.freeze({projectId,requestId});pending.set(projectId,ticket);return ticket;
    },
    pending(projectId){return pending.has(projectId);},
    finish(ticket){if(pending.get(ticket.projectId)===ticket)pending.delete(ticket.projectId);},
  };
}

export function mergeBackgroundRun(rows,run,projectId) {
  if(run.project_id!==projectId)return rows;
  const exists=rows.some(row=>row.id===run.id);
  return exists?rows.map(row=>row.id===run.id?run:row):[...rows,run];
}
