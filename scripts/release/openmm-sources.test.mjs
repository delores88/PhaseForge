import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import test from 'node:test';
import {createRequire} from 'node:module';
import {openmmNoticeRoot,stageOpenmmSources,verifyOpenmmSourceDelivery,verifySourceFile} from './openmm-sources.mjs';

function fixture(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-openmm-source-'));
  t.after(()=>{assert.ok(path.resolve(root).startsWith(path.resolve(os.tmpdir())+path.sep));assert.ok(path.basename(root).startsWith('phaseforge-openmm-source-'));fs.rmSync(root,{recursive:true});});
  return root;
}
test('source verifier accepts exact bytes and rejects same-size mutation and truncation',async t=>{
  const root=fixture(t),file=path.join(root,'source.zip'),data=Buffer.from('pinned corresponding source fixture');
  const pin={bytes:data.length,sha256:crypto.createHash('sha256').update(data).digest('hex')};fs.writeFileSync(file,data);
  assert.deepEqual(await verifySourceFile(file,pin),pin);
  const changed=Buffer.from(data);changed[0]^=1;fs.writeFileSync(file,changed);await assert.rejects(verifySourceFile(file,pin),/differs/);
  fs.writeFileSync(file,data.subarray(1));await assert.rejects(verifySourceFile(file,pin),/differs/);
});
test('source verifier refuses a hard-linked archive even when content is exact',async t=>{
  const root=fixture(t),file=path.join(root,'source.zip'),alias=path.join(root,'linked.zip'),data=Buffer.from('source');
  fs.writeFileSync(file,data);fs.linkSync(file,alias);
  await assert.rejects(verifySourceFile(alias,{bytes:data.length,sha256:crypto.createHash('sha256').update(data).digest('hex')}),/Linked/);
});
test('offline missing source fails without replacing existing staged resources',async t=>{
  const root=fixture(t),destination=path.join(root,'staged');fs.mkdirSync(destination);fs.writeFileSync(path.join(destination,'retained'),'previous attempt');
  await assert.rejects(stageOpenmmSources({cacheRoot:path.join(root,'cache'),destination,allowDownload:false}),/absent.*offline cache/);
  assert.equal(fs.readFileSync(path.join(destination,'retained'),'utf8'),'previous attempt');
});
test('delivery verification rejects missing source and a modified license before packaging',async t=>{
  const root=fixture(t),delivery=path.join(root,'delivery'),notices=path.join(root,'notices');fs.mkdirSync(delivery);fs.cpSync(openmmNoticeRoot,notices,{recursive:true});
  await assert.rejects(verifyOpenmmSourceDelivery(delivery,{noticeRoot:notices}),/missing or unexpected/);
  const file=path.join(notices,'LGPL.txt'),bytes=fs.readFileSync(file);bytes[0]^=1;fs.writeFileSync(file,bytes);
  await assert.rejects(verifyOpenmmSourceDelivery(delivery,{noticeRoot:notices}),/Source-delivery file differs/);
});
test('afterPack rejects a package whose corresponding source only exists elsewhere',async t=>{
  const root=fixture(t),require=createRequire(import.meta.url),afterPack=require('../../desktop/build/check-packed-sources.cjs');
  fs.mkdirSync(path.join(root,'resources'));fs.writeFileSync(path.join(root,'resources/app.asar'),'not credited as external source');
  await assert.rejects(afterPack({electronPlatformName:'win32',appOutDir:root}),/ENOENT/);
});
