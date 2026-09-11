import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash,webcrypto} from 'node:crypto';
import {registeredArtifacts,outputArtifacts,previewableArtifact,verifyArtifactImage} from '../src/lib/laboratory-artifacts.mjs';
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
