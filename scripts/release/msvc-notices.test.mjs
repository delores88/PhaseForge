import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import {msvcNoticeRoot,verifyMsvcNotices} from './msvc-notices.mjs';

function fixture(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-msvc-notices-'));
  t.after(()=>{assert.ok(path.resolve(root).startsWith(path.resolve(os.tmpdir())+path.sep));assert.ok(path.basename(root).startsWith('phaseforge-msvc-notices-'));fs.rmSync(root,{recursive:true});});
  fs.cpSync(msvcNoticeRoot,path.join(root,'notices'),{recursive:true});
  return {root,notices:path.join(root,'notices')};
}
test('exact Microsoft notices bind the copied files to the frozen science-v4 DLL and upstream archive',async t=>{
  const {notices}=fixture(t),result=await verifyMsvcNotices(notices);
  assert.equal(result.seed_kind,'science-v4');assert.equal(result.files.length,4);assert.equal(result.outside_asar,true);
  assert.equal(result.origin.destination,'runtime/runtime-seeds/science-v4/MSVCP140.dll');
  assert.equal(result.component.version,'14.40.33810.0');assert.equal(result.component.bytes,575056);
});
test('missing Microsoft license and unregistered extra member both fail closed',async t=>{
  const {notices}=fixture(t),file=path.join(notices,'NOTICE.txt'),bytes=fs.readFileSync(file);
  fs.unlinkSync(file);await assert.rejects(verifyMsvcNotices(notices),/missing or unexpected/);
  fs.writeFileSync(file,bytes);fs.writeFileSync(path.join(notices,'unregistered.txt'),'extra');
  await assert.rejects(verifyMsvcNotices(notices),/missing or unexpected/);
});
test('same-size mutation and forged provenance cannot replace pinned Microsoft notices',async t=>{
  const {notices}=fixture(t),file=path.join(notices,'NOTICE.txt'),bytes=fs.readFileSync(file),changed=Buffer.from(bytes);changed[0]^=1;
  fs.writeFileSync(file,changed);await assert.rejects(verifyMsvcNotices(notices),/differs/);
  fs.writeFileSync(file,bytes);fs.appendFileSync(path.join(notices,'provenance.json'),' ');
  await assert.rejects(verifyMsvcNotices(notices),/differs/);
});
test('hard-linked Microsoft notice is refused despite matching bytes',async t=>{
  const {root,notices}=fixture(t);fs.linkSync(path.join(notices,'NOTICE.txt'),path.join(root,'linked.txt'));
  await assert.rejects(verifyMsvcNotices(notices),/Linked/);
});
