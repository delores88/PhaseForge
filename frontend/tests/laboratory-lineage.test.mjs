import test from 'node:test';
import assert from 'node:assert/strict';
import {laboratoryLineage,laboratoryDerivation} from '../src/lib/laboratory-lineage.mjs';

test('chronology is creation order, not polling or last-update order; helpers do not become history cards',()=>{
  const first={id:'first',kind:'solver',created_at:'2026-09-12T01:00:00Z',updated_at:'2026-09-12T03:00:00Z',state:'running'},last={id:'last',kind:'illustration',created_at:'2026-09-12T02:00:00Z',state:'completed',result:{source_illustration_id:'prior-render'}};
  const rows=laboratoryLineage([last,{id:'session',kind:'session'},first,{id:'export',kind:'export'},{id:'observation',kind:'observation'},{id:'analysis',kind:'generated',state:'completed',input:{ml_stage:'fit'}}]);
  assert.deepEqual(rows.map(row=>row.job.id),['first','last']);assert.equal(rows[1].derivation.sourceId,'prior-render');assert.equal(rows[0].job,first);
});
test('one published simulation replaces its raw source card while unrelated outputs and immutable derivation IDs remain',()=>{
  const rows=laboratoryLineage([{id:'source',kind:'generated',state:'completed'},{id:'pub',kind:'published_simulation',input:{source_job_id:'source'}},{id:'image',kind:'generated',state:'completed'}]);
  assert.deepEqual(rows.map(row=>row.job.id),['image','pub']);assert.deepEqual(rows[1].derivation,{label:'Published from',sourceId:'source'});
  assert.deepEqual(laboratoryDerivation({input:{restarted_from_illustration_id:'timeout-id',source_illustration_id:'base-id'}}),{label:'Restart of',sourceId:'timeout-id'});
});

test('published summaries retain source lineage from result when full admission input is absent',()=>{
  for(const representation of ['particle_trajectory','scalar_field']){
    const source={id:'raw-generated',kind:'generated',state:'completed'},published={id:'published',kind:'published_simulation',state:'completed',result:{source_job_id:source.id,representation,presentation:{capture:false,exports:false}}};
    const [card]=laboratoryLineage([source,published]);assert.equal(card.job.id,published.id);assert.deepEqual(card.derivation,{label:'Published from',sourceId:source.id});assert.equal(laboratoryLineage([source,published]).length,1);
  }
});
