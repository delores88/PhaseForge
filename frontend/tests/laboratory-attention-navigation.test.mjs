import test from 'node:test';
import assert from 'node:assert/strict';
import {attentionDestination,attentionArtifactLinks,boundedEvidenceText} from '../src/lib/laboratory-attention-navigation.mjs';
const make=(ids=[])=>({id:'attempt',project_id:'project',state:'paused',result:{attention:{schema:'phaseforge.agent-attention.v1',status:'needs_direction',actions:[{id:'revise_request',label:'Revise'},{id:'inspect_evidence',label:'Evidence'},{id:'inspect_capabilities',label:'Coverage'}],evidence_job_ids:ids}}});
test('directions require the current project and an actual server-listed action',()=>{
  assert.throws(()=>attentionDestination(make(),{id:'inspect_evidence'},'other'),/own project/);
  assert.throws(()=>attentionDestination(make(),{id:'resume'},'project'),/no longer available/);
  const resolved=make();resolved.result.attention.status='resolved';assert.throws(()=>attentionDestination(resolved,{id:'revise_request'},'project'),/no longer available/);
});
test('evidence uses explicit source identities instead of a currently selected unrelated result',()=>{
  assert.deepEqual(attentionDestination(make(['evidence-old','evidence-new','evidence-old']),{id:'inspect_evidence'},'project'),{type:'evidence',sourceId:'attempt',projectId:'project',jobIds:['evidence-old','evidence-new','attempt'],hasReferencedEvidence:true});
  assert.deepEqual(attentionDestination(make(),{id:'inspect_evidence'},'project').jobIds,['attempt']);
  assert.throws(()=>attentionDestination(make(['../other']),{id:'inspect_evidence'},'project'),/invalid/);
});
test('revise and coverage only return navigation; the original request is untouched',()=>{
  const job=make();job.input={content:'original objective'};const before=JSON.stringify(job);
  assert.deepEqual(attentionDestination(job,{id:'revise_request'},'project'),{type:'chat'});
  assert.deepEqual(attentionDestination(job,{id:'inspect_capabilities'},'project'),{type:'capabilities'});
  assert.equal(JSON.stringify(job),before);
});
test('artifact links preserve explicit pins and reject unsafe relative paths',()=>{
  const sha='a'.repeat(64),job={id:'attempt',state:'completed',result:{artifacts:[{path:'work/data.json',bytes:3,sha256:sha},{path:'../private',bytes:1,sha256:sha}],diagnostics:{path:'diagnostics/result.json',sha256:sha},process_receipt:'../../outside',process_receipt_sha256:sha}};
  assert.deepEqual(attentionArtifactLinks(job).map(item=>item.path),['work/data.json','diagnostics/result.json']);
});
test('large evidence previews are explicitly bounded without modifying originals',()=>{
  const source={value:'x'.repeat(70000)},preview=boundedEvidenceText(source);
  assert.equal(preview.text.length,65536);assert.equal(preview.truncated,true);assert.equal(source.value.length,70000);
  assert.equal(boundedEvidenceText({ok:true}).truncated,false);
});
