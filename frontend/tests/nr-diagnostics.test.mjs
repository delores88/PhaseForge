import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {loadNRDiagnostics,normalizeNRIndex,normalizeNRFrame,nrPinnedJson,nrCellAt,nrTransform} from '../src/lib/nr-diagnostics.mjs';

const units={length:'L',time:'L/c',si_mapping:null};
const channels={chi:{label:'chi',unit:'1'},lapse:{label:'lapse',unit:'1'},hamiltonian:{label:'Hamiltonian',unit:'1/L^2'}};
const pin=path=>({path,bytes:2,sha256:'a'.repeat(64)});
function block(level,xlo,xhi,z){return {logical:[level,0,0,level],native_logical_level:level,geometry:[xlo,xhi,0,1,0,z*4],shape:[2,2],
  x:[xlo+(xhi-xlo)/4,xhi-(xhi-xlo)/4],y:[.25,.75],z,x_edges:[xlo,(xlo+xhi)/2,xhi],y_edges:[0,.5,1],
  channels:{chi:[[.2,.3],[.4,.5]],lapse:[[.6,.7],[.8,.9]],hamiltonian:[[1,2],[3,4]]}};}
function fixture(){
  const frames=[0,.5126953125].map((time,index)=>({index,time,scientific_time:time,time_unit:'L/c',cycle:index*35,
    data:pin(`slices/frame-${index}.json`),block_count:2,cell_count:8,source_metric:pin(`native/metric-${index}.bin`),source_constraints:pin(`native/con-${index}.bin`)}));
  const index={schema:'phaseforge.nr-amr-diagnostic-slices.v1',units:structuredClone(units),time_unit:'L/c',interpolation:'none',grid_location:'cell_center',axis_order:['y','x'],frame_count:2,initial_time:0,final_time:.5126953125,
    frames,channels:structuredClone(channels),primary_channel:'chi',channel_ranges:{chi:{min:.2,max:.5},lapse:{min:.6,max:.9},hamiltonian:{min:1,max:4}},execution:{complete:false,termination_reason:'deadline',time_target:1},physical_boost:{status:'unknown'},horizons:[]};
  const data=frames.map(entry=>({schema:'phaseforge.nr-amr-diagnostic-slice.v1',time:entry.time,scientific_time:entry.time,cycle:entry.cycle,
    coordinate_unit:'L',time_unit:'L/c',interpolation:'none',grid_location:'cell_center',blocks:[block(3,-1,0,.25),block(4,0,.5,.125)],cell_count:8,
    source_metric:entry.source_metric,source_constraints:entry.source_constraints,plane:{actual_z_min:.125,actual_z_max:.25,actual_z_varies_by_block:true},reduction_mask:{field:'chi',operator:'>=',value:.25}}));
  return {index,data};
}
test('native AMR coordinates and time are retained, with per-cell inspection and reversible pan/zoom',()=>{
  const f=fixture(),index=normalizeNRIndex(f.index),frame=normalizeNRFrame(f.data[1],index.frames[1],index);
  assert.equal(frame.time,.5126953125);assert.deepEqual(frame.blocks.map(b=>b.z),[.25,.125]);assert.deepEqual(frame.blocks.map(b=>b.native_logical_level),[3,4]);
  assert.deepEqual(frame.reductionMask,f.data[1].reduction_mask);
  const fine=nrCellAt(frame,.125,.25);assert.equal(fine.level,4);assert.equal(fine.z,.125);assert.equal(fine.values.hamiltonian,1);
  assert.equal(nrCellAt(frame,2,2),null);
  const t=nrTransform(frame.bounds,800,440,{x:.1,y:.4,zoom:2});const point=t.toData(...t.toScreen(.125,.25));
  assert.ok(Math.abs(point[0]-.125)<1e-12&&Math.abs(point[1]-.25)<1e-12);
});
test('changed times, invented SI, unsupported channel units and interpolated frames are refused',()=>{
  for(const alter of [i=>{i.frames[1].time=0;},i=>{i.units.si_mapping={length_m:1};},i=>{i.channels.hamiltonian.unit='s^-2';},i=>{i.interpolation='linear';}]){
    const f=fixture();alter(f.index);assert.throws(()=>normalizeNRIndex(f.index));
  }
});
test('ragged/nonfinite/duplicate AMR cells and altered source identity fail closed',()=>{
  for(const alter of [d=>{d.blocks[0].channels.chi[0][0]=NaN;},d=>{d.blocks[0].channels.lapse[0]=[1];},d=>{d.blocks[1].logical=d.blocks[0].logical;},d=>{d.blocks[0].x[0]=99;},d=>{d.source_metric={...d.source_metric,sha256:'b'.repeat(64)};}]){
    const f=fixture();alter(f.data[0]);assert.throws(()=>normalizeNRFrame(f.data[0],f.index.frames[0],normalizeNRIndex(f.index)));
  }
});
test('SHA and raw-byte pinning precede JSON decoding',async()=>{
  const bytes=Buffer.from('{"measurement":1}\n'),descriptor={bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};
  assert.deepEqual(await nrPinnedJson(bytes,descriptor),{measurement:1});
  await assert.rejects(()=>nrPinnedJson(Buffer.from('{"measurement":2}\n'),descriptor),/changed/);
  await assert.rejects(()=>nrPinnedJson(bytes,{...descriptor,bytes:1}),/byte count/);
});
test('complete loader binds the exact job/result/index/data chain and rejects unrelated jobs',async()=>{
  const f=fixture(),files=new Map(),artifacts=[];
  const store=(path,value)=>{const bytes=Buffer.from(JSON.stringify(value));const descriptor={path,bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};files.set(`diagnostics/${path}`,bytes);artifacts.push(descriptor);return descriptor;};
  f.index.frames.forEach((entry,n)=>{entry.data=store(entry.data.path,f.data[n]);});
  const index=store('slices/index.json',f.index);
  const result={schema:'phaseforge.nr-black-hole-result.v1',job_id:'job-a',engine_identity:{engine_id:'athenak_two_punctures_serial'},fulfillment:{requested_0_999c_collision:false},diagnostic_slices:index,artifacts:[...artifacts]};
  const saved=store('result.json',result),job={id:'job-a',input:{engine:'athenak_two_punctures_serial'},result:{diagnostics:{...saved,path:'diagnostics/result.json'}}};
  const loaded=await loadNRDiagnostics(job,async path=>files.get(path));assert.equal((await loaded.loadFrame(1)).time,.5126953125);
  await assert.rejects(()=>loadNRDiagnostics({...job,id:'job-b'},async path=>files.get(path)),/another execution/);
  files.set('diagnostics/slices/frame-0.json',Buffer.from('{}'));
  await assert.rejects(()=>loaded.loadFrame(0),/byte count|changed/);
});
test('existing uniform gauge slices retain code units and cannot imply a black-hole scene',()=>{
  const f=fixture(),index={...f.index,schema:'phaseforge.nr-diagnostic-slices.v1',primary_channel:'alpha',channels:{alpha:{unit:'1'},gxx:{unit:'1'},Kxx:{unit:'1/L'},hamiltonian:{unit:'1/L^2'}}};
  const measurements={schema:'phaseforge.nr-gauge-measurements.v1',series:index.frames.map(f=>({time:f.time,channels_3d:Object.fromEntries(Object.keys(index.channels).map(k=>[k,{min:0,max:2}]))}))};
  index.frames=index.frames.map(f=>({...f,shape:[2,2],plane:{coordinate:.375},source_block:{logical:[0,0,0,0],geometry:[0,1,0,1,0,1]}}));
  const normalized=normalizeNRIndex(index,{gauge:true,measurements});
  const frame=normalizeNRFrame({schema:'phaseforge.nr-diagnostic-slice-data.v1',shape:[2,2],x:[.25,.75],y:[.25,.75],z:.375,
    channels:Object.fromEntries(Object.keys(index.channels).map(k=>[k,[[1,1],[1,1]]]))},index.frames[0],normalized);
  assert.equal(frame.blocks.length,1);assert.equal(frame.blocks[0].z,.375);assert.equal(normalized.gauge,true);
});
