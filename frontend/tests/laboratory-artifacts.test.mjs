import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash,webcrypto} from 'node:crypto';
import {registeredArtifacts,outputArtifacts,previewableArtifact,verifyArtifactImage,numericalArtifact,verifyArtifactJson,boundedArtifactBytes,JSON_PREVIEW_BYTES} from '../src/lib/laboratory-artifacts.mjs';
import {laboratoryViewable} from '../src/lib/laboratory-plot.mjs';
import {selectedLaboratoryContext} from '../src/lib/workbenchRequests.mjs';
const digest='a'.repeat(64),file=path=>({path,bytes:10,sha256:digest});
test('completed generated output is discoverable, favors registered declared results and retains source files',()=>{
  const job={id:'generated-a',project_id:'p',kind:'generated',state:'completed',result:{artifacts:[file('work/imports/source.png'),file('work/labeled.png'),file('work/labeled.svg'),file('work/caption.txt'),file('work/source.py')],reported_result:{artifacts:['labeled.png','labeled.svg','caption.txt','missing.png','../escape.png']}}};
  assert.ok(laboratoryViewable(job));assert.ok(!laboratoryViewable({...job,state:'running'}));assert.equal(selectedLaboratoryContext('p','laboratory',job.id,[job]),job);assert.equal(selectedLaboratoryContext('another','laboratory',job.id,[job]),null);
  assert.deepEqual(outputArtifacts(job).map(x=>x.path),['work/labeled.png','work/labeled.svg','work/caption.txt']);assert.equal(registeredArtifacts(job).length,5);assert.deepEqual(outputArtifacts(job).filter(previewableArtifact).map(x=>x.path),['work/labeled.png']);assert.ok(!previewableArtifact(file('work/labeled.svg')));
  const malformed={...job,result:{artifacts:[file('../bad.png'),file('work\\bad.png'),{...file('work/unpinned.png'),sha256:''},file('work/good.png'),file('work/good.png')]}};assert.deepEqual(registeredArtifacts(malformed).map(x=>x.path),['work/good.png']);
});
test('image presentation checks the raw completed hash and size before publishing the preview',async()=>{
  const bytes=Buffer.from('a bounded image fixture'),artifact={path:'work/figure.png',bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')},data=bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength);
  assert.equal(await verifyArtifactImage(data,artifact,webcrypto.subtle),'image/png');await assert.rejects(verifyArtifactImage(data,{...artifact,sha256:digest},webcrypto.subtle),/receipt/);await assert.rejects(verifyArtifactImage(data,{...artifact,bytes:1},webcrypto.subtle),/size/);await assert.rejects(verifyArtifactImage(data,{...artifact,path:'figure.svg'},webcrypto.subtle),/format/);
});

const pinnedJson=text=>{const bytes=new TextEncoder().encode(text);return {bytes,artifact:{path:'work/result.json',bytes:bytes.byteLength,sha256:createHash('sha256').update(bytes).digest('hex')}};};
test('numerical JSON preview favors retained result.json and preserves exact values as escaped display text',async()=>{
  const text='{"fine_value":0.33349609375,"analytic_value":0.3333333333333333,"label":"<script>never execute</script>"}',{bytes,artifact}=pinnedJson(text);
  const job={id:'analysis',state:'completed',result:{artifacts:[file('work/input.json'),file('work/other.json'),artifact]}};assert.equal(numericalArtifact(job),artifact);
  const preview=await verifyArtifactJson(bytes,artifact,webcrypto.subtle);assert.deepEqual(JSON.parse(preview.text),JSON.parse(text));assert.equal(preview.truncated,false);
  await assert.rejects(verifyArtifactJson(bytes,{...artifact,sha256:digest},webcrypto.subtle),/receipt/);
  await assert.rejects(verifyArtifactJson(bytes,{...artifact,bytes:1},webcrypto.subtle),/size/);
});

test('canonical registered numerical result remains available when the producer advertises images and scene JSON but omits itself',()=>{
  const canonical=file('work/result.json'),scene=file('work/scene.json'),image=file('work/figure.png');
  const job={id:'mixed',state:'completed',result:{artifacts:[scene,image,canonical],reported_result:{artifacts:['scene.json','figure.png'],invented_metric:123}}};
  assert.equal(numericalArtifact(job),canonical);assert.deepEqual(outputArtifacts(job).map(item=>item.path),['work/result.json','work/scene.json','work/figure.png']);
  const unregistered={...job,result:{...job.result,artifacts:[scene,image]}};
  assert.equal(outputArtifacts(unregistered).some(item=>item.path==='work/result.json'),false);
  assert.equal(numericalArtifact(unregistered),scene,'reported_result never fabricates a numerical file');
  const imported={...job,result:{artifacts:[file('work/imports/result.json'),canonical],reported_result:{artifacts:['imports/result.json']}}};
  assert.equal(numericalArtifact(imported),canonical,'source input with same basename cannot displace canonical output');
});
test('JSON preview refuses invalid or unbounded values while large valid results have explicit display truncation',async()=>{
  for(const text of ['{"value":1e999}','[invalid]', '['.repeat(34)+'0'+']'.repeat(34)]){const {bytes,artifact}=pinnedJson(text);await assert.rejects(verifyArtifactJson(bytes,artifact,webcrypto.subtle));}
  const large=pinnedJson(JSON.stringify({retained:'x'.repeat(70000)}));const preview=await verifyArtifactJson(large.bytes,large.artifact,webcrypto.subtle);assert.equal(preview.truncated,true);assert.equal(preview.text.length,65536);
  const oversized=pinnedJson(JSON.stringify({retained:'x'.repeat(JSON_PREVIEW_BYTES)}));await assert.rejects(verifyArtifactJson(oversized.bytes,oversized.artifact,webcrypto.subtle),/size/);
  const precise=pinnedJson('{"exact_integer":12345678901234567890123456789,"decimal":0.123456789012345678901}');assert.equal((await verifyArtifactJson(precise.bytes,precise.artifact,webcrypto.subtle)).text,new TextDecoder().decode(precise.bytes));
});
test('bounded JSON body reader cancels an oversized stream with absent Content-Length',async()=>{
  let cancelled=false;const stream=new ReadableStream({start(controller){controller.enqueue(new Uint8Array(8));controller.enqueue(new Uint8Array(8));},cancel(){cancelled=true;}});
  await assert.rejects(boundedArtifactBytes(new Response(stream),12),/preview limit/);assert.equal(cancelled,true);
  const bytes=await boundedArtifactBytes(new Response('123'),12);assert.equal(new TextDecoder().decode(bytes),'123');
});
