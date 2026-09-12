import test from 'node:test';
import assert from 'node:assert/strict';
import {exportStatusFromJob,exportsForSource,exportDownloadName,createExportRefresh,savedRenderingReady} from '../src/lib/laboratory-exports.mjs';

test('completed publications can render saved frames despite legacy capability flags; uncommitted publications cannot',()=>{
  for(const representation of ['particle_trajectory','scalar_field']){
    const publication={kind:'published_simulation',result:{representation,presentation:{capture:false,exports:false}}};
    for(const state of ['queued','running','validating','failed','cancelled'])assert.equal(savedRenderingReady({...publication,state}),false);
    assert.equal(savedRenderingReady({...publication,state:'completed'}),true);
  }
  assert.equal(savedRenderingReady({kind:'solver',state:'running'}),true,'native solver committed-frame rendering remains available');
});

test('export recovery uses the saved source identity and never offers cancelled/partial results as downloads',()=>{
  const jobs=[{id:'old',kind:'export',state:'completed',parent_id:'source',created_at:'2026-09-11T00:00:00Z'},
    {id:'new',kind:'export',state:'running',input:{source_job_id:'source'},created_at:'2026-09-11T01:00:00Z'},
    {id:'foreign',kind:'export',state:'completed',parent_id:'other',created_at:'2026-09-11T02:00:00Z'}];
  const recovered=exportsForSource(jobs,'source');assert.deepEqual(recovered.map(j=>j.id),['new','old']);assert.equal(recovered[0].download_url,null);assert.match(recovered[1].download_url,/old\/artifacts\/simulation.mp4$/);
  assert.equal(exportStatusFromJob({...jobs[0],state:'cancelled'}).download_url,null);assert.equal(exportDownloadName(jobs[0]),'phaseforge-simulation-old.mp4');
});

test('a temporary backend outage preserves the previous export and clears its reconnect notice on recovery',async()=>{
  const controller=new AbortController(),notices=[];let available=false,current={id:'video',state:'running'},attempts=0;
  const refresh=createExportRefresh({signal:controller.signal,load:async()=>{attempts++;if(!available)throw Object.assign(new Error('Service restarting'),{status:503});return {id:'video',state:'completed'};},onValue:value=>{current=value;},onUnavailable:value=>notices.push(value)});
  await refresh();assert.equal(current.state,'running');assert.deepEqual(notices,[true]);
  available=true;await refresh();assert.equal(current.state,'completed');assert.deepEqual(notices,[true,false]);assert.equal(attempts,2);
});

test('export refresh does not overlap or publish a response after its source viewer closes',async()=>{
  const controller=new AbortController();let resolve,requests=0,updates=0;
  const refresh=createExportRefresh({signal:controller.signal,load:()=>{requests++;return new Promise(done=>{resolve=done;});},onValue:()=>updates++,onUnavailable:()=>updates++});
  const pending=refresh();await refresh();assert.equal(requests,1);controller.abort();resolve({jobs:[]});await pending;assert.equal(updates,0);
});
