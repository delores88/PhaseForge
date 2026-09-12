import test from 'node:test';
import assert from 'node:assert/strict';
import {captureLaboratoryReview,activeResultReviewSession,reviewableLaboratoryResult,pendingResultReview,retainResultReview,releaseResultReview,uncertainReviewDelivery} from '../src/lib/laboratory-result-review.mjs';
const job={id:'retained-source',project_id:'conversation-a',kind:'generated',state:'completed',completed_at:'2026-09-12T01:00:00Z'};
const options=()=>({job,action:'explain',view:{projectId:job.project_id,jobId:job.id,tab:'laboratory',busy:false},jobs:[job],snapshotSelection:()=>({provider:'open_ai',model:'chosen-model',reasoning_effort:'high',research_mode:false}),timeLimit:900,requestId:'review-attempt'});

test('all review actions capture exact retained identity, explicit chat model, reasoning and work limit without steering or experiments',()=>{
  for(const action of ['explain','next_steps','validation']){
    const request=captureLaboratoryReview({...options(),action});
    assert.equal(request.projectId,job.project_id);assert.deepEqual(request.payload.result_review,{source_job_id:job.id,action});assert.equal(request.payload.context_job_id,job.id);
    assert.equal(request.payload.provider,'open_ai');assert.equal(request.payload.model,'chosen-model');assert.equal(request.payload.reasoning_effort,'high');assert.equal(request.payload.time_limit_seconds,900);assert.equal(request.payload.request_id,'review-attempt');assert.equal(request.payload.output_intent,'explanation');
    assert.equal(Object.hasOwn(request.payload,'auto_run'),false);assert.equal(Object.hasOwn(request.payload,'steer'),false);
  }
  assert.equal(captureLaboratoryReview({...options(),timeLimit:null}).payload.time_limit_seconds,null);
});

test('model changes or project navigation after admission cannot alter the already captured request',async()=>{
  let selection={provider:'open_ai',model:'first-model',reasoning_effort:'medium'};
  const request=captureLaboratoryReview({...options(),snapshotSelection:()=>selection});let resolve;
  const pending=new Promise(done=>{resolve=done;});selection.model='other-model';selection.provider='other-provider';
  resolve(request);const sent=await pending;assert.equal(sent.payload.model,'first-model');assert.equal(sent.payload.provider,'open_ai');assert.equal(sent.projectId,'conversation-a');assert.ok(Object.isFrozen(sent.payload.result_review));
});

test('stale callbacks, unfinished/changed sources, missing models and invalid actions are rejected before submission',()=>{
  for(const change of [{view:{...options().view,projectId:'conversation-b'}},{view:{...options().view,jobId:'different'}},{view:{...options().view,tab:'sessions'}},{job:{...job,state:'running'}},{jobs:[{...job,completed_at:'2026-09-12T02:00:00Z'}]},{jobs:[]},{action:'run'},{view:{...options().view,busy:true}},{snapshotSelection:()=>({provider:'open_ai',model:''})}])assert.throws(()=>captureLaboratoryReview({...options(),...change}));
});

test('an active same-project session blocks a new review even while waiting for a model slot; another project does not',()=>{
  const active={id:'existing-session',kind:'session',project_id:job.project_id,state:'waiting_for_model_slot'};
  assert.equal(activeResultReviewSession([active],job.project_id),active);assert.throws(()=>captureLaboratoryReview({...options(),jobs:[job,active]}),/active session/);
  assert.doesNotThrow(()=>captureLaboratoryReview({...options(),jobs:[job,{...active,project_id:'conversation-b'}]}));
  assert.doesNotThrow(()=>captureLaboratoryReview({...options(),jobs:[job,{...active,state:'completed'}]}));
});

test('reviews cover completed numerical and plot sources while illustration and unfinished output actions remain separate',()=>{
  for(const kind of ['solver','generated','published_simulation','ml_study','study_plot'])assert.equal(reviewableLaboratoryResult({...job,kind}),true);
  for(const kind of ['illustration','session','observation','export'])assert.equal(reviewableLaboratoryResult({...job,kind}),false);
  assert.equal(reviewableLaboratoryResult({...job,state:'running'}),false);
});

test('uncertain delivery survives a UI reload with the exact original request ID, model, limit and payload',()=>{
  const values=new Map(),storage={getItem:key=>values.get(key)||null,setItem:(key,value)=>values.set(key,value),removeItem:key=>values.delete(key)},request=captureLaboratoryReview(options());
  retainResultReview(storage,request);const reloaded=pendingResultReview(storage,request.projectId);
  assert.equal(JSON.stringify(reloaded),JSON.stringify(request));assert.equal(reloaded.payload.request_id,'review-attempt');
  assert.equal(pendingResultReview(storage,'another-project'),null);
  releaseResultReview(storage,{...request,payload:{...request.payload,request_id:'stale-response'}});assert.ok(pendingResultReview(storage,request.projectId));
  releaseResultReview(storage,request);assert.equal(pendingResultReview(storage,request.projectId),null);
});

test('network and server failures retain delivery uncertainty while a missing dedicated endpoint is a definite rejection',()=>{
  assert.equal(uncertainReviewDelivery(new Error('Network interrupted')),true);
  for(const status of [500,502,503,408])assert.equal(uncertainReviewDelivery({status}),true);
  for(const status of [400,401,403,404,409,422])assert.equal(uncertainReviewDelivery({status}),false);
});

test('the dedicated API route fails closed on an older backend without a normal-chat fallback',async()=>{
  const original=globalThis.fetch,calls=[];globalThis.fetch=async(url,options)=>{calls.push({url,options});return new Response(JSON.stringify({error:{message:'Not found'}}),{status:404});};
  try{
    const {api}=await import('../src/lib/api.js'),payload=captureLaboratoryReview(options()).payload;
    await assert.rejects(api.labReview(job.id,payload),error=>error.status===404);
    assert.equal(calls.length,1);assert.ok(calls[0].url.endsWith(`/api/laboratory/jobs/${job.id}/review`));assert.equal(calls[0].options.method,'POST');assert.deepEqual(JSON.parse(calls[0].options.body),payload);
    assert.equal(calls.some(call=>call.url.includes('/laboratory/chat')||call.url.includes('/steer')),false);
  }finally{globalThis.fetch=original;}
});
