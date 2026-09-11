import test from 'node:test';
import assert from 'node:assert/strict';
import {TrajectoryStore,normalizeTrajectoryIndex,validateFrames,bracketFrames,interpolateRecordedPosition,playbackTimeStep} from '../src/lib/laboratory-trajectory.mjs';

const frame=(time,x=time)=>({time,entities:[{id:'ar-0000',position:[x,0,0]}]});
test('disk-backed seeking crosses chunk boundaries without dropping authoritative records',async()=>{
  const requested=[],chunks=Array.from({length:100},(_,i)=>({path:`trajectory/chunk-${i}.json`,start_time:i*10,end_time:i*10+9,frame_count:10}));
  const store=new TrajectoryStore({index:{chunks,time_unit:'ps',frame_count:1000},maxChunks:2,loadJson:async path=>{requested.push(path);const i=Number(path.match(/chunk-(\d+)/)[1]);return {frames:Array.from({length:10},(_,j)=>frame(i*10+j))};}});
  const boundary=await store.sample(9.5);assert.equal(boundary.left.time,9);assert.equal(boundary.right.time,10);assert.equal(boundary.alpha,.5);
  const distant=await store.sample(998);assert.equal(distant.left.time,998);assert.equal(distant.frameIndex,998);assert.equal(store.index.frame_count,1000);assert.equal(store.cache.size,2);assert.equal(requested.length,3);
  await store.sample(0);assert.equal(requested.length,4);assert.equal(store.cache.size,2);store.dispose();assert.equal(store.bytes,0);
});
test('seeking a live index can load newly calculated states beyond the former end',async()=>{
  const chunk=i=>({path:`${i}.json`,start_time:i*2,end_time:i*2+1,frame_count:2});
  const store=new TrajectoryStore({index:{chunks:[chunk(0)]},loadJson:async path=>[frame(Number(path[0])*2),frame(Number(path[0])*2+1)]});
  assert.equal((await store.sample(5)).left.time,1);store.updateIndex({chunks:[chunk(0),chunk(1),chunk(2)]});assert.equal((await store.sample(5)).left.time,5);assert.equal(store.index.frame_count,6);
});
test('periodic boundary interpolation takes the shortest image path without inventing a central crossing',()=>{
  const result=interpolateRecordedPosition([9.8,2,3],[.2,2,3],.25,[10,10,10],true);assert.ok(Math.abs(result[0]-9.9)<1e-12);
  assert.deepEqual(interpolateRecordedPosition([9.8,2,3],[.2,2,3],.5,[10,10,10],false),[5,2,3]);
});
test('scientific horizon and playback duration remain independent',()=>{
  assert.equal(playbackTimeStep(1,0,400,80,1),5);assert.equal(playbackTimeStep(1,0,400,80,.5),2.5);assert.equal(playbackTimeStep(1,0,800,80,1),10);assert.equal(playbackTimeStep(1,0,400,160,1),2.5);
});
test('invalid scientific records and escaping chunk paths are rejected',()=>{
  assert.throws(()=>normalizeTrajectoryIndex({chunks:[{path:'../secret',start_time:0,end_time:1}]}),/invalid chunk/);
  assert.throws(()=>validateFrames([frame(1),frame(0)]),/unordered/);assert.throws(()=>validateFrames([{time:0,entities:[{id:'a',position:[NaN,0,0]}]}]),/invalid entity/);
  assert.throws(()=>validateFrames([{time:0,entities:[{id:'a',position:[0,0,0]},{id:'a',position:[1,0,0]}]}]),/invalid entity/);
  assert.equal(bracketFrames([frame(0),frame(1)],2).left.time,1);
});
