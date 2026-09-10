import {test} from 'node:test';
import assert from 'node:assert/strict';
import {chatPresentation} from '../src/lib/chatPresentation.mjs';

test('session attribution keeps raw audit data out of the displayed conversation',()=>{
  const message={role:'user',content:'Study transport',metadata:{source:'research_session',origin_task_id:'saved-task',origin_task_cycle:2,session_objective:'Study transport',internal_prompt:'Private orchestration and machine inventory'}};
  const view=chatPresentation(message);
  assert.deepEqual(view,{session:true,label:'RESEARCH TEAM · CYCLE 2',content:'Study transport'});
  assert.equal(message.metadata.internal_prompt,'Private orchestration and machine inventory');
  assert.equal(chatPresentation({...message,role:'assistant',content:'The measured result is ready.'}).content,'The measured result is ready.');
});
test('old session briefings display their objective without rewriting the saved message',()=>{
  const content='Build the next concrete experiment for this researcher-authorized bounded task.\nTASK AND SHARED ARTIFACTS:\n'+JSON.stringify({objective:'Compare trajectories',cycle:1,compute:{raw:'inventory'}});
  const message={role:'user',content};
  assert.equal(chatPresentation(message).content,'Compare trajectories');
  assert.equal(chatPresentation(message).session,true);
  assert.equal(message.content,content);
  assert.deepEqual(chatPresentation({...message,metadata:{source:'user'}}),{session:false,label:'YOU',content});
});
test('ordinary user text and incomplete legacy prefixes remain ordinary messages',()=>{
  for(const content of ['Explain TASK AND SHARED ARTIFACTS:\n{}','Build the next concrete experiment for this researcher-authorized bounded task.\nTASK AND SHARED ARTIFACTS:\nmalformed']) {
    assert.deepEqual(chatPresentation({role:'user',content}),{session:false,label:'YOU',content});
  }
});
