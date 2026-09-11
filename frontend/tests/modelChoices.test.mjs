import {test} from 'node:test';
import assert from 'node:assert/strict';
import {eligibleModels,shortlistModels,selectModel,selectionProblem,readSelections} from '../src/lib/modelChoices.mjs';

const model=(id,rank,extra={})=>({id,capability_rank:rank,reasoning_efforts:['low','high'],recommended_reasoning:'high',...extra});
test('shortlist uses account membership and capability metadata, then presents six strongest in ascending order',()=>{
  const catalog=[model('alphabetically-first',10),model('z-fast',30),model('strong-1',100),model('strong-2',90),model('strong-3',80),model('strong-4',70),model('strong-5',60),model('strong-6',50),model('image-generator',200)];
  assert.deepEqual(shortlistModels(catalog).map(item=>item.id),['strong-6','strong-5','strong-4','strong-3','strong-2','strong-1']);
  assert.equal(shortlistModels([catalog[0],catalog[1]]).length,2);
});
test('dated aliases and incompatible capabilities cannot crowd out actual distinct eligible models',()=>{
  const catalog=[model('gpt-example',50),model('gpt-example-2026-01-01',50),model('unavailable',200,{available:false}),model('text-only',100,{supports_tools:false}),model('gpt-other-2026-01-01',40),model('no-metadata',undefined)];
  assert.deepEqual(shortlistModels(catalog).map(item=>item.id),['gpt-other-2026-01-01','gpt-example']);
  assert.equal(selectionProblem({model:'gpt-example-2026-01-01',reasoning_effort:null},catalog),'');
  assert.deepEqual(shortlistModels([model('claude-sonnet-4-5',80),model('claude-sonnet-4-5-20250929',80)]).map(item=>item.id),['claude-sonnet-4-5']);
  assert.equal(eligibleModels([model('claude-sonnet-4-5-20250929',80)]).length,1);
});
test('explicit selection materializes its displayed effort and unavailable models do not silently fall back',()=>{
  const choice=model('selected-by-user',50);
  const selection=selectModel({research_mode:true,model:'old',reasoning_effort:'low'},'open_ai',choice);
  assert.deepEqual(selection,{provider:'open_ai',model:'selected-by-user',reasoning_effort:'high',research_mode:true});
  assert.equal(selectionProblem(selection,[choice]),'');
  assert.match(selectionProblem(selection,[model('settings-default',80)]),/no replacement has been selected/);
  assert.match(selectionProblem({...selection,reasoning_effort:'unavailable'},[choice]),/supported reasoning/);
  assert.equal(selectionProblem({...selection,reasoning_effort:null},[choice]),'');
});
test('conversation choices survive serialization without propagating one project into another',()=>{
  const a={provider:'open_ai',model:'model-a',reasoning_effort:'high',research_mode:false};
  const b={provider:'anthropic',model:'model-b',reasoning_effort:null,research_mode:true};
  const saved=readSelections(JSON.stringify({version:2,projects:{'project-a':a,'project-b':b}}));
  assert.deepEqual(saved.projects,{'project-a':a,'project-b':b});
  assert.equal(saved.projects['new-project'],undefined);
  const migration=readSelections('invalid',JSON.stringify(a));
  assert.deepEqual(migration.legacy,a);
  assert.deepEqual(readSelections(JSON.stringify(migration)).legacy,a);
});
