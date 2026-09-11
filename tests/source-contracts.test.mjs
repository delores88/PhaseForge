import assert from 'node:assert/strict';
import test from 'node:test';
import {credentialSaveContract,cannedPhysicsMarkers} from '../scripts/source-contracts.mjs';

const keyButton=predicate=>`<button onClick={()=>saveKey(id)} disabled={${predicate}}>Save key</button>`;
test('credential save accepts guarded optional/plain key syntax without requiring a model',()=>{
  assert.equal(credentialSaveContract(keyButton('working||!form.apiKey.trim()')),true);
  assert.equal(credentialSaveContract(keyButton('working||!form.apiKey?.trim()')),true);
});
test('credential save rejects model gating and loss of busy/nonblank protections',()=>{
  for(const predicate of ['working||!form.apiKey.trim()||!model','working||!form.apiKey.trim()||!providerReady','!form.apiKey.trim()','working','working||!form.apiKey'])assert.equal(credentialSaveContract(keyButton(predicate)),false,predicate);
  assert.equal(credentialSaveContract('<button disabled={false}>Save key</button>'),false);
});
test('unsupported closed-form capability disclaimer is not a canned implementation',()=>{
  assert.deepEqual(cannedPhysicsMarkers('json!({"scope":"No collisions/mergers, relativity, ephemeris accuracy or general closed-form three-body solution."})'),[]);
});
test('named physics presets and functions remain detected even alongside a disclaimer',()=>{
  const disclaimer='{"scope":"No general closed-form three-body solution."}';
  for(const code of ['fn three_body() {}','const figure_eight = [1,2,3]','{"case":"three-body"}','{"scope":"Use precomputed three-body trajectory"}','{"seed":"known_answer_seed"}','const electron_collapse = true'])assert.equal(cannedPhysicsMarkers(disclaimer+code).length,1,code);
});
