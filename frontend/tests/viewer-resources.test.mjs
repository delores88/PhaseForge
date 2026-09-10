import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import {releaseRenderer,disposeObjectTree} from '../src/lib/viewer-resources.mjs';

test('detached renderer loses its context once, even when renderer disposal fails',()=>{
  for(const fail of [false,true]){
    const calls=[],renderer={dispose(){calls.push('dispose');if(fail)throw Error('dispose failed');},forceContextLoss(){calls.push('context');},domElement:{remove(){calls.push('remove');}}};
    if(fail)assert.throws(()=>releaseRenderer(renderer),/dispose failed/);else releaseRenderer(renderer);
    releaseRenderer(renderer);assert.deepEqual(calls,['dispose','context','remove']);
  }
});

test('model cleanup releases shared geometry, textures, instance buffers and light shadows',()=>{
  const root=new THREE.Group(),geometry=new THREE.SphereGeometry(1,6,4),texture=new THREE.Texture(),material=new THREE.MeshStandardMaterial({map:texture});
  const instance=new THREE.InstancedMesh(geometry,material,2),mesh=new THREE.Mesh(geometry,material),light=new THREE.DirectionalLight();root.add(instance,mesh,light);
  const counts={geometry:0,material:0,texture:0,instance:0,shadow:0,bitmap:0};
  for(const [name,resource] of Object.entries({geometry,material,texture,instance}))resource.addEventListener('dispose',()=>counts[name]++);
  texture.source.data={close(){counts.bitmap++;}};light.shadow.map={dispose(){counts.shadow++;}};
  disposeObjectTree(root);assert.deepEqual(counts,{geometry:1,material:1,texture:1,instance:1,shadow:1,bitmap:1});
});
