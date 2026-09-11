import {test} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {FieldStore,normalizeFieldIndex,recordedFieldNumber,fieldColor,fieldCell,validateFieldFrame} from '../src/lib/laboratory-field.mjs';
const index={representation:'scalar_field',shape:[2,3],lengths_um:[6,8],axis_order:['y','x'],grid_location:'cell_center',x_um:[1,3,5],y_um:[2,6],field_unit:'1',color_scale:{min:0,max:1,stops:[[0,[20,38,80]],[1,[255,190,60]]]},frames:[{step:0,time:0,view_path:'fields/view-0.json'},{step:1,time:.1,view_path:'fields/view-1.json'},{step:2,time:.2,view_path:'fields/view-2.json'}]};
test('cell selection preserves bottom-up y and exact endpoint time despite nominal binary roundoff',()=>{
  const normalized=normalizeFieldIndex(index);assert.deepEqual(fieldCell(normalized,0,0),{x:0,y:0,position:[1,2]});assert.deepEqual(fieldCell(normalized,1,1),{x:2,y:1,position:[5,6]});
  assert.equal(recordedFieldNumber(normalized,.099999),0);assert.equal(recordedFieldNumber(normalized,.1),1);assert.equal(recordedFieldNumber(normalized,normalized.end),2);
  assert.throws(()=>normalizeFieldIndex({...index,axis_order:['x','y']}),/invalid recorded field grid/);
});
test('fixed scale encodes actual values and presentation contrast changes legend identically',()=>{
  assert.deepEqual(fieldColor(0,index.color_scale),[20,38,80]);assert.deepEqual(fieldColor(1,index.color_scale),[255,190,60]);assert.deepEqual(fieldColor(.5,index.color_scale),[138,114,70]);
  assert.deepEqual(fieldColor(.5,index.color_scale,{colorLow:'#000000',colorHigh:'#ffffff'}),[128,128,128]);
  assert.deepEqual(fieldColor(.1,index.color_scale,{contrast:2}),[0,0,29]);
  assert.deepEqual(fieldColor(7,{min:7,max:7}),[138,114,70]);
});
test('bounded display cache verifies committed bytes and rejects inconsistent numerical grids',async()=>{
  const values=[[0,.25,.5],[.5,.75,1]],artifacts=index.frames.map(record=>new TextEncoder().encode(JSON.stringify({time:record.time,step:record.step,shape:index.shape,values,field_unit:'1'}))),signed={...index,frames:index.frames.map((f,i)=>({...f,view_sha256:createHash('sha256').update(artifacts[i]).digest('hex'),sha256:`field-${i}`}))};
  let calls=0;const store=new FieldStore({index:signed,maxFrames:2,loadBytes:async path=>{calls++;return artifacts[signed.frames.findIndex(f=>f.view_path===path)];}});
  assert.equal((await store.frame(0)).source_sha256,'field-0');await store.frame(0);assert.equal(calls,1);await store.frame(1);await store.frame(2);assert.equal(store.cache.size,2);await store.frame(0);assert.equal(calls,4);store.close();await assert.rejects(store.frame(1),/closed/);
  const bad=new FieldStore({index:signed,loadBytes:async()=>new TextEncoder().encode('{}')});await assert.rejects(bad.frame(0),/checksum/);
  assert.throws(()=>validateFieldFrame({time:0,step:0,shape:[2,3],field_unit:'1',values:[[0,1,2],[3,4,NaN]]},normalizeFieldIndex(index),index.frames[0]),/disagree/);
});
test('rapid scrubbing skips superseded queued fields and never loads two large arrays together',async()=>{
  let release,calls=[];const wait=new Promise(resolve=>{release=resolve;}),store=new FieldStore({index,loadBytes:async path=>{calls.push(path);if(path===index.frames[0].view_path)await wait;const record=index.frames.find(f=>f.view_path===path);return new TextEncoder().encode(JSON.stringify({time:record.time,step:record.step,shape:index.shape,field_unit:'1',values:[[0,0,0],[1,1,1]]}));}});
  const first=store.frame(0,{latestOnly:true});await new Promise(resolve=>setTimeout(resolve,0));
  const obsolete=store.frame(1,{latestOnly:true}),latest=store.frame(2,{latestOnly:true});
  assert.deepEqual(calls,[index.frames[0].view_path]);const rejected=assert.rejects(obsolete,/Superseded/);release();await first;await rejected;assert.equal((await latest).step,2);assert.deepEqual(calls,[index.frames[0].view_path,index.frames[2].view_path]);
});
