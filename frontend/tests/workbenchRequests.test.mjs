import {test} from 'node:test';
import assert from 'node:assert/strict';
import {snapshotConversationModel,selectedLaboratoryContext,conversationTimeLimit,createViewTracker,createProjectRequestGate,mergeBackgroundRun} from '../src/lib/workbenchRequests.mjs';

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
