import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import zlib from 'node:zlib';
import net from 'node:net';
import {dsse} from '@sigstore/core';
import {auditableDependencies} from './auditable.mjs';
import {inside,inventory,machine,hostedWorkspace,copyEvidenceTree} from './common.mjs';
import {startInertListeners} from './legacy-listeners.mjs';
import {cryptoRuntimeIdentity} from './runtime-identity.mjs';

const dependencies={packages:[{name:'phaseforge-backend',version:'0.8.0-alpha.1',root:true,dependencies:[1]},{name:'rusqlite',version:'0.40.2',source:'crates.io',kind:'runtime'}]};
function pe(value=dependencies){
  const compressed=zlib.deflateSync(Buffer.from(JSON.stringify(value)));
  const bytes=Buffer.alloc(512+compressed.length+16);bytes.write('MZ');bytes.writeUInt32LE(128,0x3c);bytes.write('PE\0\0',128,'binary');bytes.writeUInt16LE(0x8664,132);bytes.writeUInt16LE(1,134);
  bytes.write('.dep-v0',152);bytes.writeUInt32LE(compressed.length+16,168);bytes.writeUInt32LE(512,172);compressed.copy(bytes,512);return bytes;
}
function elf(value=dependencies){
  const compressed=zlib.deflateSync(Buffer.from(JSON.stringify(value))),names=Buffer.from('\0.shstrtab\0.dep-v0\0');
  const bytes=Buffer.alloc(512+compressed.length);Buffer.from([127,69,76,70,2,1]).copy(bytes);bytes.writeUInt16LE(62,18);bytes.writeBigUInt64LE(64n,40);bytes.writeUInt16LE(64,58);bytes.writeUInt16LE(3,60);bytes.writeUInt16LE(1,62);
  bytes.writeUInt32LE(1,128);bytes.writeBigUInt64LE(256n,152);bytes.writeBigUInt64LE(BigInt(names.length),160);names.copy(bytes,256);
  bytes.writeUInt32LE(11,192);bytes.writeBigUInt64LE(512n,216);bytes.writeBigUInt64LE(BigInt(compressed.length),224);compressed.copy(bytes,512);return bytes;
}
test('reads actual PE section metadata with linker alignment padding',()=>assert.deepEqual(auditableDependencies(pe()),dependencies));
test('reads actual ELF named section metadata',()=>assert.deepEqual(auditableDependencies(elf()),dependencies));
test('rejects a native backend without compiler metadata',()=>{const bytes=pe();bytes.write('.text\0\0\0',152);assert.throws(()=>auditableDependencies(bytes),/Missing compiler/);});
test('rejects section offsets outside the binary instead of accepting declared lock contents',()=>{const bytes=pe();bytes.writeUInt32LE(0xfffffff0,172);assert.throws(()=>auditableDependencies(bytes),/bounds/);});
test('rejects malformed package entries and excessive compressed metadata',()=>{
  assert.throws(()=>auditableDependencies(pe({packages:[{name:'bad',version:5}]})),/Malformed/);
  const bytes=pe();bytes.writeUInt32LE(17*1024**2,168);assert.throws(()=>auditableDependencies(bytes),/budget/);
});
test('rejects truncated ELF section tables and wrong byte order',()=>{
  assert.throws(()=>auditableDependencies(elf().subarray(0,170)),/bounds/);
  const bytes=elf();bytes[5]=2;assert.throws(()=>auditableDependencies(bytes),/little-endian/);
});
test('DSSE Unicode payload types use UTF-8 byte length (GHSA-jfc7-64v2-mr8c)',()=>{
  const payload=Buffer.from('研究'),type='application/雪';
  const expected=Buffer.concat([Buffer.from(`DSSEv1 ${Buffer.byteLength(type)} ${type} ${payload.length} `),payload]);
  assert.deepEqual(dsse.preAuthEncoding(type,payload),expected);
  assert.notDeepEqual(dsse.preAuthEncoding(type,payload),Buffer.from(`DSSEv1 ${type.length} ${type} ${payload.length} ${payload.toString()}`));
});
test('release destinations reject traversal and boundary replacement',()=>{
  const folder=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-release-test-'));
  try{assert.throws(()=>inside(folder,folder));assert.throws(()=>inside(folder,path.join(folder,'..','escape')));assert.equal(inside(folder,path.join(folder,'new/file')),path.join(folder,'new/file'));}
  finally{fs.rmSync(inside(os.tmpdir(),folder,{existing:true}),{recursive:true});}
});
test('payload inventory binds exact bytes and distinguishes native executable architecture',async()=>{
  const folder=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-release-test-'));
  try{const win=path.join(folder,'engine.exe'),linux=path.join(folder,'engine');fs.writeFileSync(win,pe());fs.writeFileSync(linux,elf());assert.equal(machine(win).architecture,'x64');assert.equal(machine(linux).architecture,'x64');const first=await inventory(folder);fs.appendFileSync(win,'changed');const second=await inventory(folder);assert.notEqual(first.files.find(f=>f.path==='engine.exe').sha256,second.files.find(f=>f.path==='engine.exe').sha256);}
  finally{fs.rmSync(inside(os.tmpdir(),folder,{existing:true}),{recursive:true});}
});
test('native acceptance refuses ordinary developer environments before installation',()=>{
  const original=process.env.PHASEFORGE_RELEASE_ACCEPTANCE;delete process.env.PHASEFORGE_RELEASE_ACCEPTANCE;
  try{assert.throws(()=>hostedWorkspace(),/restricted/);}finally{if(original!==undefined)process.env.PHASEFORGE_RELEASE_ACCEPTANCE=original;}
});
test('copied payload preserves relative links and still rejects escaping links',async context=>{
  const folder=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-release-links-'));
  try{
    const source=path.join(folder,'source'),target=path.join(folder,'copy');
    fs.mkdirSync(path.join(source,'assets'),{recursive:true});fs.writeFileSync(path.join(source,'assets/icon'),'actual payload bytes');
    try{fs.symlinkSync('assets',path.join(source,'icons'),'dir');}
    catch(error){if(process.platform==='win32'&&['EPERM','EACCES'].includes(error.code)){context.skip('Windows host does not permit symlink creation; Linux release job must exercise this regression');return;}throw error;}
    copyEvidenceTree(source,target);
    assert.equal(fs.readlinkSync(path.join(target,'icons')),'assets');
    assert.equal(fs.readFileSync(path.join(target,'icons/icon'),'utf8'),'actual payload bytes');
    assert.ok((await inventory(target)).files.some(file=>file.path==='icons'&&file.link==='assets'));
    fs.symlinkSync('../source',path.join(target,'escape'),'dir');
    await assert.rejects(()=>inventory(target),/escapes root/);
  }finally{fs.rmSync(inside(os.tmpdir(),folder,{existing:true}),{recursive:true});}
});
test('payload inventory resolves a symlinked ancestor without admitting links outside the real payload',async context=>{
  const folder=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-release-alias-'));
  try{
    const physical=path.join(folder,'physical'),payload=path.join(physical,'payload'),alias=path.join(folder,'alias');
    fs.mkdirSync(path.join(payload,'assets'),{recursive:true});fs.writeFileSync(path.join(payload,'assets/icon'),'original bytes');
    fs.mkdirSync(path.join(physical,'outside'));fs.writeFileSync(path.join(physical,'outside/file'),'outside payload');
    try{fs.symlinkSync('physical',alias,'dir');}
    catch(error){if(process.platform==='win32'&&['EPERM','EACCES'].includes(error.code)){context.skip('Windows host does not permit symlink creation; native Linux/macOS jobs exercise this regression');return;}throw error;}
    fs.symlinkSync('assets',path.join(payload,'icons'),'dir');
    const canonical=await inventory(payload),aliased=await inventory(path.join(alias,'payload'));
    assert.deepEqual(aliased,canonical);
    assert.ok(aliased.files.some(file=>file.path==='icons'&&file.link==='assets'));
    fs.symlinkSync('../outside',path.join(payload,'escape'),'dir');
    await assert.rejects(()=>inventory(path.join(alias,'payload')),/Payload link escapes root: escape/);
  }finally{fs.rmSync(inside(os.tmpdir(),folder,{existing:true}),{recursive:true});}
});

test('occupied-port fixtures detect even TCP-only probes and close their owned listener',async()=>{
  const listeners=await startInertListeners([{address:'127.0.0.1',port:0}]);
  const port=listeners.receipts()[0].port;
  try{
    assert.equal(listeners.receipts()[0].connections,0);
    await new Promise((resolve,reject)=>{const socket=net.connect({host:'127.0.0.1',port});socket.on('error',reject);socket.once('connect',()=>socket.end());socket.once('close',resolve);});
    assert.equal(listeners.receipts()[0].connections,1);assert.equal(listeners.receipts()[0].received_bytes,0);
  }finally{await listeners.close();}
  const closed=await new Promise(resolve=>{const socket=net.connect({host:'127.0.0.1',port});socket.once('error',()=>resolve(true));socket.once('connect',()=>{socket.destroy();resolve(false);});});
  assert.equal(closed,true);
});
test('Electron OpenSSL placeholders remain explicit identity gaps without fabricated libraries',()=>{
  for(const value of ['0.0.0','0.0.0-electron','unknown','',undefined]){
    const result=cryptoRuntimeIdentity({openssl:value});assert.equal(result.status,'unresolved');assert.equal(result.component,null);assert.ok(result.gap.includes('BoringSSL'));
  }
  const actual=cryptoRuntimeIdentity({openssl:'3.5.4'});assert.equal(actual.status,'observed_version');assert.equal(actual.version,'3.5.4');
});
