import {test} from 'node:test';
import assert from 'node:assert/strict';
import {snapshotConversationModel,selectedLaboratoryContext,conversationTimeLimit,resolveRunInspection,createViewTracker,createProjectRequestGate,mergeBackgroundRun} from '../src/lib/workbenchRequests.mjs';

test('admission snapshots the target conversation even when the visible picker has moved elsewhere',async()=>{
  const choices={a:{provider:'open_ai',model:'selected-a',reasoning_effort:'high'},b:{provider:'anthropic',model:'selected-b',reasoning_effort:null}};
  const a=snapshotConversationModel('a',id=>choices[id],()=>null);
  choices.a={provider:'open_ai',model:'next-request-a',reasoning_effort:'low'};
  await Promise.resolve();
  assert.equal(a.model,'selected-a');assert.equal(a.reasoning_effort,'high');
  assert.equal(snapshotConversationModel('b',id=>choices[id],()=>null).reasoning_effort,null);
  assert.throws(()=>snapshotConversationModel('a',id=>choices[id],()=> 'Account access changed'),/Account access changed/);
  assert.throws(()=>snapshotConversationModel('missing',id=>choices[id],()=>null),/Choose a model/);
});
test('a laboratory context must be explicitly selected, visible and owned by the target project',()=>{
  const jobs=[{id:'latest-a',project_id:'a',kind:'solver'},{id:'selected-b',project_id:'b',kind:'solver'}];
  assert.equal(selectedLaboratoryContext('a','laboratory',null,jobs),null);
  assert.equal(selectedLaboratoryContext('a','findings','latest-a',jobs),null);
  assert.equal(selectedLaboratoryContext('a','laboratory','selected-b',jobs),null);
  assert.equal(selectedLaboratoryContext('a','laboratory','latest-a',jobs),jobs[0]);
});
test('ordinary chat receives the selected illustration context only in its own project and laboratory view',()=>{
  const illustration={id:'illustration-a',project_id:'a',kind:'illustration'},jobs=[illustration,{id:'export-a',project_id:'a',kind:'export'}];
  assert.equal(selectedLaboratoryContext('a','laboratory','illustration-a',jobs),illustration);
  assert.equal(selectedLaboratoryContext('b','laboratory','illustration-a',jobs),null);
  assert.equal(selectedLaboratoryContext('a','sessions','illustration-a',jobs),null);
  assert.equal(selectedLaboratoryContext('a','laboratory','export-a',jobs),null);
});
test('image observation uses the target conversation’s saved duration including timer Off',()=>{
  const values={'phaseforge.chat.duration:a':'off','phaseforge.chat.duration:b':'28800'};
  const storage={getItem:key=>values[key]};
  assert.equal(conversationTimeLimit('a',storage),null);
  assert.equal(conversationTimeLimit('b',storage),28800);
  assert.equal(conversationTimeLimit('new',storage),900);
  values['phaseforge.chat.duration:a']='nonsense';assert.equal(conversationTimeLimit('a',storage),900);
});
test('late completion cannot select content after navigation, including leaving and returning',async()=>{
  const view=createViewTracker();view.observe(['a','world','run-1']);const before=view.capture();
  await Promise.resolve();assert.equal(view.isCurrent(before),true);
  view.observe(['a','findings','run-1']);assert.equal(view.isCurrent(before),false);
  view.observe(['a','world','run-1']);assert.equal(view.isCurrent(before),false);
  const current=view.capture();view.observe(['a','world','run-2']);assert.equal(view.isCurrent(current),false);
});
test('submission receipts are isolated by project and an older response cannot clear a newer submission',async()=>{
  const gate=createProjectRequestGate();const a=gate.begin('a','first');const b=gate.begin('b','second');
  assert.throws(()=>gate.begin('a','duplicate'),/already submitting/);
  gate.finish(a);assert.equal(gate.pending('a'),false);assert.equal(gate.pending('b'),true);
  const next=gate.begin('a','third');await Promise.resolve();gate.finish(a);assert.equal(gate.pending('a'),true);
  gate.finish(b);assert.equal(gate.pending('a'),true);gate.finish(next);assert.equal(gate.pending('a'),false);
});
test('background updates preserve inspected ordering and reject another project’s completed run',()=>{
  const rows=[{id:'old',project_id:'a',status:'completed'},{id:'new',project_id:'a',status:'running'}];
  const merged=mergeBackgroundRun(rows,{id:'new',project_id:'a',status:'completed'},'a');
  assert.deepEqual(merged.map(row=>row.id),['old','new']);assert.equal(merged[1].status,'completed');
  assert.equal(mergeBackgroundRun(rows,{id:'foreign',project_id:'b'},'a'),rows);
  assert.equal(mergeBackgroundRun(rows,{id:'later',project_id:'a'},'a')[0],rows[0]);
});

test('delayed session inspection cannot select a run after project navigation, return, or a newer load',async()=>{
  for(const change of ['project','away-and-back','load','result']){
    const view=createViewTracker();view.observe(['a','sessions','old']);let project='a',generation=1;
    const captured=view.capture(),loadGeneration=generation;let finish;
    const pending=resolveRunInspection({id:'run-a',projectId:'a',loadRun:()=>new Promise(resolve=>{finish=resolve;}),isCurrent:()=>project==='a'&&generation===loadGeneration&&view.isCurrent(captured)});
    if(change==='project'){project='b';view.observe(['b','laboratory',null]);}
    if(change==='away-and-back'){view.observe(['b','laboratory',null]);view.observe(['a','sessions','old']);}
    if(change==='load')generation++;
    if(change==='result')view.observe(['a','findings','new']);
    finish({id:'run-a',project_id:'a',status:'completed'});
    assert.equal(await pending,null,change);
  }
});

test('session inspection requires exact returned run/project identity and opens an unchanged visit',async()=>{
  const run={id:'run-a',project_id:'a',status:'completed'},options={id:run.id,projectId:run.project_id,isCurrent:()=>true};
  assert.equal(await resolveRunInspection({...options,loadRun:async()=>run}),run);
  await assert.rejects(resolveRunInspection({...options,loadRun:async()=>({...run,id:'different'})}),/saved project/);
  await assert.rejects(resolveRunInspection({...options,loadRun:async()=>({...run,project_id:'b'})}),/saved project/);
});

test('review and observation admission validate the visible timer draft rather than silently using old saved minutes',()=>{
  const storage={getItem:()=> '1800'};
  for(const value of ['',0,-1,1.5,10081,'invalid'])assert.throws(()=>conversationTimeLimit('a',storage,{mode:'custom',value}),/Custom time/);
  assert.equal(conversationTimeLimit('a',storage,{mode:'custom',value:'47'}),2820);
  assert.equal(conversationTimeLimit('a',storage,{mode:'preset',value:'off'}),null);
  assert.equal(conversationTimeLimit('a',storage,{mode:'preset',value:'900'}),900);
  assert.equal(conversationTimeLimit('b',storage),1800);
});
