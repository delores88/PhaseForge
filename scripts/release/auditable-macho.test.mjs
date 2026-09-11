import test from 'node:test';
import assert from 'node:assert/strict';
import zlib from 'node:zlib';
import {auditableDependencies,auditableSection} from './auditable.mjs';

const dependencies={packages:[{name:'phaseforge-backend',version:'0.8.0-alpha.1',root:true}]};
function macho(){
  const compressed=zlib.deflateSync(Buffer.from(JSON.stringify(dependencies))),bytes=Buffer.alloc(512+compressed.length);
  bytes.writeUInt32LE(0xfeedfacf,0);bytes.writeUInt32LE(0x0100000c,4);bytes.writeUInt32LE(2,12);
  bytes.writeUInt32LE(1,16);bytes.writeUInt32LE(152,20);
  bytes.writeUInt32LE(0x19,32);bytes.writeUInt32LE(152,36);bytes.write('__DATA',40);bytes.writeUInt32LE(1,96);
  bytes.write('.dep-v0',104);bytes.write('__DATA',120);bytes.writeBigUInt64LE(BigInt(compressed.length),144);bytes.writeUInt32LE(512,152);compressed.copy(bytes,512);
  return bytes;
}
test('extracts cargo-auditable metadata from an ARM64 Mach-O section',()=>assert.deepEqual(auditableDependencies(macho()),dependencies));
test('added code-signature load command and trailer preserve exact compiler section identity',()=>{
  const original=macho(),signed=Buffer.concat([original,Buffer.alloc(32,0x5a)]);
  signed.writeUInt32LE(2,16);signed.writeUInt32LE(168,20);
  signed.writeUInt32LE(0x1d,184);signed.writeUInt32LE(16,188);signed.writeUInt32LE(original.length,192);signed.writeUInt32LE(32,196);
  assert.notDeepEqual(signed,original);assert.deepEqual(auditableSection(signed),auditableSection(original));assert.deepEqual(auditableDependencies(signed),dependencies);
});
test('rejects truncated and oversized Mach-O command/section tables',()=>{
  for(const corrupt of [bytes=>bytes.subarray(0,100),bytes=>{bytes.writeUInt32LE(0xfffffff8,36);return bytes;},bytes=>{bytes.writeUInt32LE(4097,96);return bytes;},bytes=>{bytes.writeUInt32LE(2,16);return bytes;}])assert.throws(()=>auditableDependencies(corrupt(macho())),/bounds|segment/);
});
test('rejects Mach-O metadata outside file or beyond the decompression budget',()=>{
  const outside=macho();outside.writeUInt32LE(0xfffffff0,152);assert.throws(()=>auditableDependencies(outside),/bounds/);
  const oversized=macho();oversized.writeBigUInt64LE(17n*1024n*1024n,144);assert.throws(()=>auditableDependencies(oversized),/budget/);
  const missing=macho();missing.write('__text\0',104);assert.throws(()=>auditableDependencies(missing),/Missing compiler/);
});
test('rejects duplicate dependency sections rather than selecting an ambiguous payload',()=>{
  const bytes=macho();bytes.writeUInt32LE(232,20);bytes.writeUInt32LE(232,36);bytes.writeUInt32LE(2,96);bytes.copy(bytes,184,104,184);
  assert.throws(()=>auditableDependencies(bytes),/Missing compiler/);
});
