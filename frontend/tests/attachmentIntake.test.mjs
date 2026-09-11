import test from 'node:test';
import assert from 'node:assert/strict';
import {readAttachmentBatch} from '../src/lib/attachmentIntake.mjs';
function file(name,bytes){if(typeof bytes==='string')bytes=new TextEncoder().encode(bytes);return{name,size:bytes.length,type:'text/plain',arrayBuffer:async()=>bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength)};}
test('UTF-8 source bytes including BOM and non-ASCII labels survive intake',async()=>{
  const original='\uFEFFtime,Δx\r\n0,1\r\n';const [result]=await readAttachmentBatch([file('measurements.csv',original)]);
  assert.equal(result.content,original);assert.equal(result.size_bytes,new TextEncoder().encode(original).length);
});
test('a bad batch rejects as a whole and leaves existing data untouched',async()=>{
  const existing=[{name:'kept.txt',size_bytes:3,content:'old'}];
  await assert.rejects(readAttachmentBatch([file('valid.txt','new'),file('broken.txt',new Uint8Array([255]))],existing),/UTF-8/);
  assert.deepEqual(existing,[{name:'kept.txt',size_bytes:3,content:'old'}]);
});
test('file paths, binary data, executable types and server-size bounds are explicit',async()=>{
  for(const name of ['../secret.csv','C:\\secret.csv','script.py'])await assert.rejects(readAttachmentBatch([file(name,'abc')]));
  await assert.rejects(readAttachmentBatch([file('binary.txt','a\0b')]),/NUL/);
  await assert.rejects(readAttachmentBatch([file('large.csv','x'.repeat(1024*1024+1))]),/CSV 1 MiB/);
  await assert.rejects(readAttachmentBatch(Array.from({length:9},(_,i)=>file(`${i}.txt`,'x'))),/eight/);
});
