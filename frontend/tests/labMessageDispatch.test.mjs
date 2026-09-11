import test from 'node:test';import assert from 'node:assert/strict';
import {dispatchLabMessage} from '../src/lib/labMessageDispatch.mjs';
const activeSession={id:'active',project_id:'project',kind:'session',state:'running',input:{model:'captured-A'}};
const payload={request_id:'update',content:'Keep the original goal; use the supplied units.',attachments:[],provider:'openai',model:'next-B',reasoning_effort:'high',time_limit_seconds:2820};
test('active update sends constraints without changing the running model or time policy',async()=>{
  let sent;
  const result=await dispatchLabMessage({projectId:'project',activeSession,payload,snapshotSelection:()=>{throw Error('Next model unavailable');},submitUpdate:async(id,request)=>{sent={id,request};return{id};},submitSession:()=>assert.fail('No new generation/session')});
  assert.equal(result.kind,'update');assert.equal(sent.id,'active');assert.deepEqual(Object.keys(sent.request).sort(),['attachments','content','request_id']);
});
test('finish race starts a new session using captured next choice and retained source references',async()=>{
  let model='next-B',sent;const ref={job_id:'data',path:'source.csv',sha256:'pin'};
  await dispatchLabMessage({projectId:'project',activeSession,payload:{...payload,attachments:[{name:'source.csv',content:'x\n1'}]},snapshotSelection:()=>({provider:'openai',model,reasoning_effort:'high'}),submitUpdate:async()=>{model='later-C';throw{status:409,body:{error:{code:'session_finished',retained_files:[ref]}}};},submitSession:async(project,request)=>{sent={project,request};return{};}});
  assert.equal(sent.project,'project');assert.equal(sent.request.model,'next-B');assert.equal(sent.request.time_limit_seconds,2820);assert.deepEqual(sent.request.attachments,[ref]);
});
test('foreign projects never receive updates and uncertain errors never retry as new work',async()=>{
  let next=false;await dispatchLabMessage({projectId:'other',activeSession,payload,snapshotSelection:()=>({model:'chosen'}),submitUpdate:()=>assert.fail('Foreign session'),submitSession:async()=>{next=true;}});assert.ok(next);
  await assert.rejects(dispatchLabMessage({projectId:'project',activeSession,payload,snapshotSelection:()=>({model:'chosen'}),submitUpdate:async()=>{throw Error('Connection lost');},submitSession:()=>assert.fail('Uncertain update must not duplicate work')}),/Connection lost/);
});
