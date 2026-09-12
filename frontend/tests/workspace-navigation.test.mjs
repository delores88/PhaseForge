import {test} from 'node:test';
import assert from 'node:assert/strict';
import {initialLaboratorySelection,newestLaboratoryOutput,workspaceTools} from '../src/lib/workspace-navigation.mjs';
const jobs=[
  {id:'old',project_id:'p',kind:'solver',created_at:'2026-09-11T10:00:00Z'},
  {id:'new',project_id:'p',kind:'illustration',created_at:'2026-09-12T10:00:00Z'},
  {id:'session',project_id:'p',kind:'session',created_at:'2026-09-12T11:00:00Z'},
  {id:'foreign',project_id:'q',kind:'solver',created_at:'2026-09-12T12:00:00Z'},
];
test('open the latest applicable result, preserving deliberate older selection across completion',()=>{
  assert.equal(newestLaboratoryOutput(jobs,'p').id,'new');
  assert.equal(initialLaboratorySelection(jobs,'p',null),'new');
  assert.equal(initialLaboratorySelection(jobs,'p','old'),'old');
  assert.equal(initialLaboratorySelection(jobs,'p','foreign'),'new');
  assert.equal(initialLaboratorySelection(jobs,'empty','old'),null);
});
test('irrelevant equation results/setup tabs are absent while real tools remain reachable',()=>{
  const empty=workspaceTools().map(([id])=>id);
  assert.deepEqual(empty,['capabilities','equations','studio']);
  assert.ok(!empty.includes('findings')&&!empty.includes('manifest'));
  const saved=workspaceTools({runs:[{}],manifests:[{}],structures:[{}]}).map(([id])=>id);
  for(const id of ['world','manifest','findings','evidence','advanced','molecules'])assert.ok(saved.includes(id));
});
