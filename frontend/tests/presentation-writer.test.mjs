import test from 'node:test';
import assert from 'node:assert/strict';
import {PresentationWriter,presentationPatch} from '../src/lib/presentation-writer.mjs';

test('a concurrent agent color edit survives a stale camera autosave',async()=>{
  let server={revision:3,settings:{color:'#0088ff',camera:{position:[1,2,3]}}};const calls=[],states=[];
  const writer=new PresentationWriter({revision:2,settings:{color:'#ffffff',camera:{position:[1,2,3]}},uuid:()=> 'operation-1',onState:value=>states.push(value),send:async body=>{
    calls.push(body);if(body.base_revision!==server.revision)throw Object.assign(new Error('Conflict'),{status:409,body:{error:{current:server}}});
    server={revision:server.revision+1,settings:{...server.settings,...body.patch}};return server;
  }});
  writer.queue({camera:{position:[4,5,6]}});await writer.flush();
  assert.equal(server.settings.color,'#0088ff');assert.deepEqual(server.settings.camera.position,[4,5,6]);assert.equal(calls.length,2);assert.equal(calls[0].operation_id,calls[1].operation_id);assert.deepEqual(Object.keys(calls[0].patch),['camera']);assert.equal(states.at(-1).color,'#0088ff');
});
test('newer pending UI edits survive acknowledgement of an earlier request',async()=>{
  let release;const wait=new Promise(resolve=>release=resolve),calls=[],states=[];let number=0;
  const writer=new PresentationWriter({settings:{color:'white',opacity:1},uuid:()=>`op-${++number}`,onState:value=>states.push(value),send:async body=>{calls.push(body);if(calls.length===1)await wait;return {revision:calls.length,settings:{color:body.patch.color||'blue',opacity:body.patch.opacity??1}};}});
  writer.queue({color:'blue'});const running=writer.flush();writer.queue({opacity:.5});release();await running;
  assert.equal(calls.length,2);assert.equal(states[0].opacity,.5);assert.deepEqual(calls[1].patch,{opacity:.5});
});
test('uncertain failure retries the same operation identity without resending unrelated fields',async()=>{
  let fail=true;const calls=[];
  const writer=new PresentationWriter({uuid:()=> 'stable-id',send:async body=>{calls.push(body);if(fail)throw new Error('Connection lost');return {revision:1,settings:{color:'blue'}};}});
  writer.queue({color:'blue'});await writer.flush();fail=false;await writer.retry();assert.equal(calls.length,2);assert.equal(calls[0].operation_id,calls[1].operation_id);
  assert.deepEqual(presentationPatch({color:'blue',camera:{position:[1,2,3]}},{color:'blue',camera:{position:[3,2,1]}}),{camera:{position:[3,2,1]}});
});
