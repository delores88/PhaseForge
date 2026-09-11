import {test} from 'node:test';
import assert from 'node:assert/strict';
import {labJobPresentation,labElapsed} from '../src/lib/laboratoryPresentation.mjs';
const job={state:'completed',created_at:'2026-01-01T00:00:00Z',updated_at:'2026-01-01T00:02:00Z',seen_at:null,progress:{}};
test('completion remains unread per job until its own completed result was seen',()=>{
  assert.equal(labJobPresentation(job).unread,true);
  assert.equal(labJobPresentation({...job,seen_at:'2026-01-01T00:01:00Z'}).unread,true);
  assert.equal(labJobPresentation({...job,seen_at:job.updated_at}).unread,false);
  assert.equal(labJobPresentation({...job,state:'paused'}).unread,false);
});
test('progress only reports actual computation fraction and never elapsed budget',()=>{
  assert.equal(labJobPresentation({...job,state:'running',deadline_at:'2026-01-01T00:10:00Z'}).percent,null);
  assert.equal(labJobPresentation({...job,state:'running',progress:{fraction:.4}}).percent,40);
  assert.equal(labElapsed(job,Date.parse('2026-01-01T00:10:00Z')),120);
  assert.equal(labJobPresentation({...job,state:'timed_out'}).resumable,true);
  assert.equal(labJobPresentation({...job,state:'cancelled'}).resumable,false);
});
test('viewing or updating presentation after completion does not re-create unread results or add runtime',()=>{
  const completed_at=job.updated_at;
  const seen_at='2026-01-01T00:03:00Z';
  const updated_at='2026-01-01T00:04:00Z';
  assert.equal(labJobPresentation({...job,completed_at,seen_at,updated_at}).unread,false);
  assert.equal(labElapsed({...job,completed_at,seen_at,updated_at}),120);
  const events=[{kind:'completed',at:completed_at},{kind:'presentation_updated',at:updated_at}];
  assert.equal(labJobPresentation({...job,events,seen_at,updated_at}).unread,false);
  assert.equal(labJobPresentation({...job,events,seen_at:'2026-01-01T00:01:00Z',updated_at}).unread,true);
});
