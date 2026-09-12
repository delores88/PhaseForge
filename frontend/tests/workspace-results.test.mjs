import test from 'node:test';
import assert from 'node:assert/strict';
import {completedWorkspaceResults,currentWorkspaceResult,linkedRunPanel,unhandledRunLink} from '../src/lib/workspace-results.mjs';

const run={id:'equation',project_id:'p',status:'completed',name:'Recorded equation',queued_at:'2026-09-11T01:00:00Z',finished_at:'2026-09-11T03:00:00Z',result:{series:[{name:'measurement',values:[1,2]}]}};
const still={id:'still',project_id:'p',kind:'illustration',state:'completed',title:'Retained illustration',completed_at:'2026-09-11T02:00:00Z'};

test('completed legacy runs and new laboratory outputs share one correctly ordered project results index',()=>{
  const rows=completedWorkspaceResults({projectId:'p',runs:[run,{...run,id:'foreign',project_id:'q'}],jobs:[still,{...still,id:'pending',state:'running'}]});
  assert.deepEqual(rows.map(row=>row.key),['laboratory:still','run:equation']);
  assert.equal(rows.at(-1).completedAt,Date.parse(run.finished_at));assert.equal(rows.at(-1).record,run);
  assert.equal(rows.at(-1).type,'Equation measurements');
});

test('completion becomes available without changing an older result identity, and reloading recovers the same history',()=>{
  const state={projectId:'p',jobs:[still],runs:[run]},selected='laboratory:still';
  const before=completedWorkspaceResults(state),solver={id:'solver',project_id:'p',kind:'solver',state:'completed',completed_at:'2026-09-11T04:00:00Z'};
  const after=completedWorkspaceResults({...state,jobs:[...state.jobs,solver]});
  assert.equal(currentWorkspaceResult(after,selected,'p').id,'still');assert.equal(after.at(-1).id,'solver');
  assert.deepEqual(completedWorkspaceResults(JSON.parse(JSON.stringify({...state,jobs:[...state.jobs,solver]}))).map(row=>row.key),after.map(row=>row.key));
  assert.deepEqual(before.map(row=>row.key),['laboratory:still','run:equation']);
});

test('stale result clicks cannot select another project or a no-longer-completed result',()=>{
  const rows=completedWorkspaceResults({projectId:'p',runs:[run]});
  assert.equal(currentWorkspaceResult(rows,'run:equation','q'),null);
  assert.equal(currentWorkspaceResult(completedWorkspaceResults({projectId:'q',runs:[run]}),'run:equation','q'),null);
  assert.equal(currentWorkspaceResult(completedWorkspaceResults({projectId:'p',runs:[{...run,status:'running'}]}),'run:equation','p'),null);
});

test('JSON analysis stays available and only a completed publication replaces its generated source',()=>{
  const analysis={id:'analysis',project_id:'p',kind:'generated',state:'completed',completed_at:'2026-09-11T01:00:00Z',result:{reported_result:{value:1/3},artifacts:[{path:'work/result.json'}]}},publication={id:'published',project_id:'p',kind:'published_simulation',state:'validating',result:{source_job_id:'analysis',representation:'scalar_field'}};
  assert.deepEqual(completedWorkspaceResults({projectId:'p',jobs:[analysis,publication]}).map(row=>row.id),['analysis']);
  assert.deepEqual(completedWorkspaceResults({projectId:'p',jobs:[analysis,{...publication,state:'completed'}]}).map(row=>row.id),['published']);
});

test('saved run deep links open measured findings by default and preserve an explicit supported destination',()=>{
  assert.equal(linkedRunPanel(undefined),'findings');assert.equal(linkedRunPanel('unknown'),'findings');
  for(const panel of ['findings','world','evidence','laboratory'])assert.equal(linkedRunPanel(panel),panel);
});

test('reconnection does not reopen a consumed run link after navigation, but a real reload or new destination does',()=>{
  const opened=unhandledRunLink('equation',undefined,null);assert.equal(opened.panel,'findings');
  assert.equal(unhandledRunLink('equation',undefined,opened.key),null);
  assert.equal(unhandledRunLink('equation','evidence',opened.key).panel,'evidence');
  assert.equal(unhandledRunLink('other',undefined,opened.key).id,'other');
  assert.equal(unhandledRunLink('equation',undefined,null).id,'equation');
});
