'use client';
import {useEffect,useState} from 'react';
import {api} from '@/lib/api';
import {RESULT_REVIEW_ACTIONS,reviewableLaboratoryResult} from '@/lib/laboratory-result-review.mjs';
import styles from './LaboratoryResultActions.module.css';

export default function LaboratoryResultActions({job,busy=false,modelError='',pendingReview=null,onReview}){
  const [source,setSource]=useState(null),[error,setError]=useState(''),[pending,setPending]=useState(''),[retry,setRetry]=useState(0);
  useEffect(()=>{
    setSource(null);setError('');if(!reviewableLaboratoryResult(job))return;
    const controller=new AbortController();
    api.labJob(job.id,controller.signal).then(full=>{
      if(controller.signal.aborted)return;
      if(full.id!==job.id||full.project_id!==job.project_id||!reviewableLaboratoryResult(full))throw Error('This completed result could not be matched to its saved project.');
      setSource(full);
    }).catch(value=>{if(value.name!=='AbortError')setError(value.message);});
    return()=>controller.abort();
  },[job.id,job.project_id,job.state,job.completed_at,retry]);
  if(!reviewableLaboratoryResult(job))return null;
  const retryAction=pendingReview?.payload.result_review.action,retrySource=pendingReview?.payload.result_review.source_job_id;
  async function review(action){if(!source||busy||pending||modelError||(pendingReview&&(retrySource!==job.id||retryAction!==action)))return;setPending(action);setError('');try{await onReview?.(source,action);}catch(value){setError(value.message);}finally{setPending('');}}
  return <section className={styles.actions} aria-label="Review completed laboratory result">
    <header><strong>Review saved results</strong><code title={job.id}>{job.id}</code></header>
    <p>Uses this conversation’s model and work limit. Reads retained results and starts no experiment. Replies appear in chat; progress appears in Agents.</p>
    <div className={styles.buttons}>{RESULT_REVIEW_ACTIONS.map(([action,label])=><button type="button" key={action} disabled={!source||busy||!!pending||!!modelError||!!pendingReview&&(retrySource!==job.id||retryAction!==action)} onClick={()=>review(action)}>{pending===action?'Submitting review…':pendingReview&&retrySource===job.id&&retryAction===action?`Retry ${label.toLowerCase()}`:label}</button>)}</div>
    {pendingReview&&!pending&&<p role="status">Delivery of review <code>{pendingReview.payload.request_id}</code> has not been confirmed. {retrySource===job.id?`Retrying this action reuses the same request, model (${pendingReview.payload.model}) and work limit.`:`Open result ${retrySource} to retry that action.`} Check Agents for an accepted review. No automatic retry occurs.</p>}
    {modelError?<p role="status">{modelError}</p>:busy?<p role="status">Finish or stop this conversation’s active session before starting a review.</p>:!source&&!error&&<p role="status">Checking the saved result…</p>}
    {error&&<p role="status">{error}{!source&&<button type="button" onClick={()=>setRetry(value=>value+1)}>Reload result</button>}</p>}
  </section>;
}
