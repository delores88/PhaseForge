import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import {installCameraNavigation} from '../src/lib/viewer-camera.mjs';

function rig(span){
  const camera=new THREE.PerspectiveCamera(40,1,.01,1000);camera.position.set(span,span,span);
  const listeners=new Map(),element={style:{},setAttribute(){},addEventListener:(key,value)=>listeners.set(key,value),removeEventListener:key=>listeners.delete(key),focus(){}};
  const controls=new THREE.EventDispatcher();controls.target=new THREE.Vector3();controls.update=()=>{camera.lookAt(controls.target);camera.updateMatrixWorld();controls.dispatchEvent({type:'change'});};
  controls.update();const nav=installCameraNavigation(THREE,{camera,controls,element,span});return {camera,controls,nav,listeners};
}
test('orbit, pan, zoom and roll preserve meaningful camera geometry across scene scales',()=>{
  for(const span of [1e-10,1,1e12]){
    const {camera,controls,nav}=rig(span),distance=camera.position.distanceTo(controls.target);
    nav.command({orbit:[.3,.2]});assert.ok(Math.abs(camera.position.distanceTo(controls.target)/distance-1)<1e-10);
    const before=camera.position.clone().sub(controls.target);nav.command({pan:[.1,-.1]});assert.ok(camera.position.clone().sub(controls.target).distanceTo(before)/span<1e-10);
    nav.command({zoom:-.5});assert.ok(camera.position.distanceTo(controls.target)<distance);
    nav.command({roll:.3});assert.ok(Math.abs(camera.up.length()-1)<1e-10);assert.ok(camera.near>0&&camera.far/camera.near<1e8);nav.dispose();
  }
});
test('focused keyboard controls consume only recognized camera gestures and remove listeners on disposal',()=>{
  const {nav,listeners,controls}=rig(4);let prevented=0;listeners.get('keydown')({key:'ArrowRight',shiftKey:true,preventDefault(){prevented++;},stopPropagation(){}});
  assert.equal(prevented,1);assert.ok(controls.target.length()>0);listeners.get('keydown')({key:'a',preventDefault(){prevented++;}});assert.equal(prevented,1);nav.dispose();assert.equal(listeners.size,0);
});

test('camera shortcuts do not intercept modified browser commands or an input descendant',()=>{
  const {nav,listeners,camera}=rig(4),before=camera.position.clone();let prevented=0;
  for(const event of [{key:'-',ctrlKey:true},{key:'ArrowLeft',altKey:true},{key:'Home',target:{tagName:'INPUT'}}])listeners.get('keydown')({...event,preventDefault(){prevented++;},stopPropagation(){}});
  assert.equal(prevented,0);assert.deepEqual(camera.position,before);nav.dispose();
});
