import test from 'node:test';
import assert from 'node:assert/strict';
import {selectStudioRenderInput,loadSavedRenderRequest,SAVED_RENDER_INPUT_LIMIT} from '../src/lib/studio-render-input.mjs';

const projectId='project-a';
const scene={title:'Source-derived scene',nodes:[{id:'env',type:'protein',position:[17,-42,3],parameters:{points:[[4,5,6],[7,8,9]]}}]};
const manifest={project_id:projectId,visualization:{scene:{title:'Unrelated toy scene',nodes:[{id:'ball',position:[0,0,0]}]}}};
const camera={position:[222,333,444],target:[17,-42,3],focal_length_mm:70};
const bindings=[{node_id:'env',structure_id:'real-structure'}];
const job={id:'render-a',project_id:projectId,type:'renders',settings:{scene,bindings,camera}};
const choose=(extra={})=>selectStudioRenderInput({projectId,manifest,...extra});

test('selected saved render retains its exact scene, bindings and source camera instead of the current manifest',()=>{
  const selected=choose({job,structure:{id:'different-structure',project_id:projectId}});
  assert.deepEqual(selected.input,{scene,bindings,camera});assert.equal(selected.scene,scene);
  assert.equal(selected.input.scene.nodes[0].position[0],17);assert.notEqual(selected.scene,manifest.visualization.scene);
});
test('older render settings recover the original request only from the matching saved input',()=>{
  const old={...job,settings:{bindings,camera}};
  assert.equal(choose({job:old}).input,null);
  assert.match(choose({job:old,jobSource:{jobId:old.id}}).error,/Loading/);
  const request={project_id:projectId,scene,bindings,camera};
  assert.deepEqual(choose({job:old,jobSource:{jobId:old.id,request}}).input,{scene,bindings,camera});
  for(const jobSource of [{jobId:'other',request},{jobId:old.id,error:'HTTP 404'},{jobId:old.id,request:{...request,project_id:'foreign'}},{jobId:old.id,request:{project_id:projectId}}]) {
    assert.equal(choose({job:old,jobSource}).input,null);
  }
});
test('foreign jobs, missing binding sources and ambiguous render inputs never fall back',()=>{
  for(const invalid of [{...job,project_id:'other'}, {...job,settings:{scene,bindings:[{node_id:'env'}]}}, {...job,settings:{scene,bindings:[{node_id:'absent',structure_id:'source'}]}}, {...job,settings:{scene,structure_id:'mixed'}}])assert.equal(choose({job:invalid}).input,null);
  assert.deepEqual(choose({job:{...job,settings:{structure_id:'original',camera}}}).input,{structure_id:'original',camera});
});
test('a directly selected structure wins over stale design bindings',()=>{
  const design={project_id:projectId,design:{kind:'scene',scene,asset_bindings:[{asset_id:'missing',node_id:'env'}]}};
  assert.deepEqual(choose({design,structure:{id:'selected',project_id:projectId}}).input,{structure_id:'selected'});
  assert.equal(choose({design,structure:{id:'selected',project_id:'other'}}).input,null);
});
test('active design resolves only project-owned source bindings',()=>{
  const design={project_id:projectId,design:{kind:'scene',scene,asset_bindings:[{asset_id:'asset-a',node_id:'env'}]}};
  const assets=[{id:'asset-a',project_id:projectId,molecule_id:'real-structure'}];
  assert.deepEqual(choose({design,assets}).input,{scene,bindings});
  assert.equal(choose({design,assets:[]}).input,null);
  assert.equal(choose({design,assets:[{...assets[0],project_id:'other'}]}).input,null);
});
test('local models, imported imagery/data, fabrication jobs and CAD designs cannot render an unrelated scene',()=>{
  for(const selection of [{localFile:{name:'part.stl'}},{assetModel:{name:'model.glb'}},{assetImage:{name:'image.png'}},{visibleAsset:{mime_type:'text/csv'}},{visibleAsset:{molecule_id:'loading'}},
    {job:{id:'fab',project_id:projectId,type:'fabrications'}},...['cad','pcb'].map(kind=>({design:{project_id:projectId,design:{kind,scene}}}))]) {
    const selected=choose(selection);assert.equal(selected.input,null);assert.equal(selected.scene,null);assert.ok(selected.error);
  }
  assert.equal(choose().scene,manifest.visualization.scene);
});
test('saved input loader returns the original request, bounds streamed bytes and forwards cancellation',async()=>{
  const request={project_id:projectId,scene,bindings,camera};const controller=new AbortController();
  assert.deepEqual(await loadSavedRenderRequest('/local/input.json',{signal:controller.signal,fetcher:async(url,options)=>{
    assert.equal(url,'/local/input.json');assert.equal(options.signal,controller.signal);return new Response(JSON.stringify({request,bound_structures:{ignored:'retained only on disk'}}));
  }}),request);
  await assert.rejects(loadSavedRenderRequest('/local/input.json',{fetcher:async()=>new Response('{}',{headers:{'content-length':String(SAVED_RENDER_INPUT_LIMIT+1)}})}),/32 MiB/);
  let cancelled=false;
  const stream=new ReadableStream({start(c){c.enqueue(new Uint8Array(SAVED_RENDER_INPUT_LIMIT));c.enqueue(new Uint8Array(1));},cancel(){cancelled=true;}});
  await assert.rejects(loadSavedRenderRequest('/local/input.json',{fetcher:async()=>new Response(stream)}),/32 MiB/);assert.equal(cancelled,true);
  await assert.rejects(loadSavedRenderRequest('/local/input.json',{fetcher:async()=>new Response('{}',{status:404})}),/unavailable/);
});
