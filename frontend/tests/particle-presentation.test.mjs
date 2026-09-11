import test from 'node:test';
import assert from 'node:assert/strict';
import {particleUnits,periodicParticles,particleRadius,retainedParticleBounds,normalizedParticlePosition} from '../src/lib/particle-presentation.mjs';
import {TrajectoryStore,interpolateRecordedPosition} from '../src/lib/laboratory-trajectory.mjs';

test('isolated bodies retain scaled units, negative coordinates and full-run framing',async()=>{
  const topology={boundary:'isolated',units:{position:'L0',mass:'M0',time:'T0'},entities:[{id:'heavy',mass:3,display_radius:.04,color:'#ff9900'}]};
  const source={index:{boundary:'isolated',time_unit:'T0',position_unit:'L0',chunks:[{bounds:{min:[-2,1,-4],max:[3,6,0]}},{bounds:{min:[-8,-3,-2],max:[1,12,5]}}]},chunk:()=>{throw Error('Framing must use committed bounds without rereading every chunk');}};
  assert.deepEqual(particleUnits(topology,source.index),{position:'L0',mass:'M0',time:'T0'});assert.equal(periodicParticles(topology,source.index),false);assert.equal(particleRadius(topology.entities[0]),.04);
  assert.deepEqual(await retainedParticleBounds(topology,source),{min:[-8,-3,-4],max:[3,12,5]});
  assert.deepEqual(interpolateRecordedPosition([-2,1,0],[4,-3,1],.5,null,false),[1,-1,.5]);
  assert.throws(()=>periodicParticles({...topology,box_nm:[1,1,1]},source.index),/isolated/);
  assert.equal(new Float32Array(normalizedParticlePosition([1000000.05,0,0],[1000000.025,0,0],80))[0],2);
});
test('OpenMM periodic cell and physical radius remain unchanged',async()=>{
  const topology={box_nm:[2,3,4],position_unit:'nm',entities:[{id:'ar',radius_nm:.17}]},source={index:{wrapping:'periodic [0,L)',time_unit:'ps'}};
  assert.equal(periodicParticles(topology,source.index),true);assert.deepEqual(await retainedParticleBounds(topology,source),{min:[0,0,0],max:[2,3,4]});assert.equal(particleRadius(topology.entities[0]),.17);assert.equal(particleUnits(topology,source.index).position,'nm');
});
test('legacy isolated framing reads serially and includes later translated states',async()=>{
  const reads=[],source={index:{boundary:'isolated',chunks:[{},{}]},chunk:async i=>{reads.push(i);return [{entities:[{position:i?[9,-4,7]:[-1,2,3]}]}];}};
  assert.deepEqual(await retainedParticleBounds({},source),{min:[-1,-4,3],max:[9,2,7]});assert.deepEqual(reads,[0,1]);
});
test('production byte loader refuses an altered numerical trajectory',async()=>{
  const bytes=new TextEncoder().encode(JSON.stringify({frames:[{time:0,entities:[{id:'body-0',position:[-2,3,4]}]}]})),sha=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
  const index={boundary:'isolated',time_unit:'T0',chunks:[{path:'trajectory/a.json',start_time:0,end_time:0,frame_count:1,sha256:sha}]};
  const good=new TrajectoryStore({index,loadBytes:async()=>bytes});assert.deepEqual((await good.sample(0)).left.entities[0].position,[-2,3,4]);
  const changed=bytes.slice();changed[changed.length-1]=32;
  await assert.rejects(new TrajectoryStore({index,loadBytes:async()=>changed}).sample(0),/digest/);
  await assert.rejects(new TrajectoryStore({index:{...index,chunks:[{...index.chunks[0],sha256:''}]},loadBytes:async()=>bytes}).sample(0),/digest/);
});
