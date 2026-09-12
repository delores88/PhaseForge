import {test} from 'node:test';
import assert from 'node:assert/strict';
import {loadObservedCompletedJobs,mergeAcknowledgedJobs,observedCompletedJobs} from '../src/lib/laboratory-read-state.mjs';
import {labJobPresentation} from '../src/lib/laboratoryPresentation.mjs';
const completed='2026-09-12T10:00:00Z';
const job={id:'a',project_id:'p',state:'completed',completed_at:completed,seen_at:null};
test('a project click acknowledges only its already-completed unread snapshot',()=>{
  assert.deepEqual(observedCompletedJobs([job,{...job,id:'running',state:'running'},{...job,id:'foreign',project_id:'q'},{...job,id:'seen',seen_at:completed}],'p'),[{id:'a',completed_at:completed}]);
});
test('stale polling cannot restore a blue dot, while a later completion becomes unread',()=>{
  const watermarks=new Map([['a',completed]]);
  const stale=mergeAcknowledgedJobs([job],watermarks)[0];
  assert.equal(labJobPresentation(stale).unread,false);
  const later=mergeAcknowledgedJobs([{...job,completed_at:'2026-09-12T10:05:00Z'}],watermarks)[0];
  assert.equal(labJobPresentation(later).unread,true);
  assert.equal(later.completed_at,'2026-09-12T10:05:00Z');
});

test('first-load acknowledgement leaves work completed after the opening click unread',()=>{
  const later={...job,id:'later',completed_at:'2026-09-12T10:00:02Z'};
  assert.deepEqual(observedCompletedJobs([job,later],'p',Date.parse('2026-09-12T10:00:01Z')),[{id:'a',completed_at:completed}]);
});

test('a completion during delayed project loading stays unread with either a loaded poll or a first job fetch',async()=>{
  const clickedAt=Date.parse('2026-09-12T10:00:01Z');
  let finishProjectLoad;
  const projectLoad=new Promise(resolve=>{finishProjectLoad=resolve;});
  const later={...job,id:'later',state:'running',completed_at:null};
  const snapshot=[job,later,{...job,id:'foreign',project_id:'q'}];
  const open=async jobs=>{
    // The caller captures the action boundary before awaiting project data.
    const viewedAt=clickedAt;
    await projectLoad;
    return loadObservedCompletedJobs({projectId:'p',viewedAt,jobs,fetchJobs:async id=>{
      assert.equal(id,'p');return {jobs:snapshot};
    }});
  };
  const cached=open(snapshot),firstLoad=open(null);
  later.state='completed';later.completed_at='2026-09-12T10:00:02Z';
  finishProjectLoad();
  const expected=[{id:'a',completed_at:completed}];
  assert.deepEqual(await cached,expected);assert.deepEqual(await firstLoad,expected);
  const retained=mergeAcknowledgedJobs(snapshot,new Map(expected.map(row=>[row.id,row.completed_at])));
  assert.equal(labJobPresentation(retained[0]).unread,false);
  assert.equal(labJobPresentation(retained[1]).unread,true);
  // Explicitly opening that job's completed progress still acknowledges it.
  assert.deepEqual(observedCompletedJobs([retained[1]],'p'),[{id:'later',completed_at:later.completed_at}]);
});

test('a delayed first job response cannot expand its supplied opening timestamp',async()=>{
  let finishFetch;
  const response=new Promise(resolve=>{finishFetch=resolve;});
  const pending=loadObservedCompletedJobs({projectId:'p',viewedAt:Date.parse('2026-09-12T10:00:01Z'),jobs:null,fetchJobs:()=>response});
  finishFetch({jobs:[job,{...job,id:'during-fetch',completed_at:'2026-09-12T10:00:03Z'}]});
  assert.deepEqual(await pending,[{id:'a',completed_at:completed}]);
});
