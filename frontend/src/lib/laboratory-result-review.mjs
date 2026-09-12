export const RESULT_REVIEW_ACTIONS=[
  ['explain','Explain with AI'],['next_steps','Suggest next steps'],['validation','Review numerical checks'],
];
export const reviewableLaboratoryResult=job=>job?.state==='completed'&&['solver','generated','published_simulation','ml_study','study_plot'].includes(job.kind);
export const activeResultReviewSession=(jobs,projectId)=>jobs.find(job=>job.project_id===projectId&&job.kind==='session'&&!['completed','failed','cancelled','paused','timed_out'].includes(job.state))||null;

/** Capture one explicit review before any asynchronous work. No steering or experiment fallback. */
export function captureLaboratoryReview({job,action,view,jobs,snapshotSelection,timeLimit,requestId}){
  if(!reviewableLaboratoryResult(job)||view?.projectId!==job.project_id||view?.jobId!==job.id||view?.tab!=='laboratory')throw Error('Open this completed result in its own project before reviewing it.');
  const current=jobs.find(candidate=>candidate.id===job.id&&candidate.project_id===job.project_id);
  if(!reviewableLaboratoryResult(current)||(current.completed_at&&job.completed_at&&current.completed_at!==job.completed_at))throw Error('The selected result changed. Reload it before starting a review.');
  if(view.busy||activeResultReviewSession(jobs,job.project_id))throw Error('Finish or stop this conversation’s active session before starting a result review.');
  const label=RESULT_REVIEW_ACTIONS.find(([key])=>key===action)?.[1];if(!label)throw Error('Unknown result review action.');
  const selection=Object.freeze({...snapshotSelection(job.project_id)});
  if(!selection.provider||!selection.model)throw Error('Choose a model in this conversation before starting a review.');
  if(timeLimit!==null&&(!Number.isInteger(timeLimit)||timeLimit<10||timeLimit>604800))throw Error('Choose a valid conversation work limit.');
  if(typeof requestId!=='string'||!requestId)throw Error('A result review needs its own request identity.');
  return Object.freeze({projectId:job.project_id,payload:Object.freeze({
    ...selection,request_id:requestId,time_limit_seconds:timeLimit,context_job_id:job.id,output_intent:'explanation',
    content:`${label} for the retained result ${job.id}. Read the saved evidence, identify limitations, and explain any proposed follow-up without executing another experiment.`,
    result_review:Object.freeze({source_job_id:job.id,action}),
  })});
}

const pendingKey=projectId=>`phaseforge.resultReview.pending.v1:${projectId}`;
export function pendingResultReview(storage,projectId){
  const raw=storage.getItem(pendingKey(projectId));if(!raw)return null;
  const request=JSON.parse(raw),payload=request?.payload,review=payload?.result_review;
  if(request.projectId!==projectId||typeof payload?.request_id!=='string'||!payload.request_id||payload.output_intent!=='explanation'||typeof review?.source_job_id!=='string'||review.source_job_id!==payload.context_job_id||!RESULT_REVIEW_ACTIONS.some(([action])=>action===review.action))throw Error('The saved review delivery record could not be verified. Check Agents before starting another review.');
  return request;
}
export function retainResultReview(storage,request){storage.setItem(pendingKey(request.projectId),JSON.stringify(request));}
export function releaseResultReview(storage,request){
  if(pendingResultReview(storage,request.projectId)?.payload.request_id===request.payload.request_id)storage.removeItem(pendingKey(request.projectId));
}
export const uncertainReviewDelivery=error=>!Number.isInteger(error?.status)||![400,401,403,404,409,422].includes(error.status);
