import {test} from 'node:test';
import assert from 'node:assert/strict';
import {laboratoryRecovery,laboratoryAttention} from '../src/lib/laboratory-recovery.mjs';
import {labJobPresentation,labElapsed} from '../src/lib/laboratoryPresentation.mjs';

test('full events and compact polling records report the same recovery without changing completion/read state',()=>{
  const data={operation_id:'recovery-1',status:'started',phase:'verifying',error:null};
  const job={state:'completed',created_at:'2026-09-12T00:00:00Z',completed_at:'2026-09-12T00:01:00Z',updated_at:'2026-09-12T01:00:00Z',seen_at:'2026-09-12T00:02:00Z',events:[{kind:'nr_recovery',data}]};
  assert.deepEqual(laboratoryRecovery(job),laboratoryRecovery({...job,events:[],recovery:data}));
  assert.equal(laboratoryRecovery(job).label,'Verifying saved files');
  assert.equal(labJobPresentation(job).state,'completed');assert.equal(labJobPresentation(job).active,false);
  assert.equal(labJobPresentation(job).unread,false);assert.equal(labElapsed(job),60);
});

test('terminal recovery failures remain explicit and latest operation controls supersede older successful ones',()=>{
  const events=[{kind:'nr_recovery',data:{operation_id:'old',status:'completed'}},{kind:'nr_recovery',data:{operation_id:'new',status:'failed',error:'Recorded hash mismatch'}}];
  const value=laboratoryRecovery({events});assert.equal(value.operation_id,'new');assert.equal(value.active,false);assert.equal(value.label,'Recovery failed');assert.equal(value.error,'Recorded hash mismatch');
  assert.equal(laboratoryRecovery({recovery:{operation_id:'new',status:'cancel_requested'}}).active,true);
  assert.equal(laboratoryRecovery({events:[]}),null);
});

test('attention exposes only supported recorded actions and never invents Resume for a capability gap',()=>{
  const job={result:{attention:{schema:'phaseforge.agent-attention.v1',status:'needs_direction',kind:'capability_gap',reason:'Registered solver unavailable',actions:[{id:'revise_request',label:'Revise request'},{id:'inspect_capabilities',label:'Inspect supported engines'},{id:'invented_tool',label:'Run arbitrary tool'}]}}};
  assert.deepEqual(laboratoryAttention(job).actions.map(action=>action.id),['revise_request','inspect_capabilities']);
  assert.equal(laboratoryAttention({...job,result:{attention:{schema:'unknown',status:'needs_direction'}}}),null);
});
