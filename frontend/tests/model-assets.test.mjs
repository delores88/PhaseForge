import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import {inspectGLB,inspectSTL,MODEL_LIMITS} from '../src/lib/model-assets.mjs';
import {molecularDensity,createMolecularSurface,prepareMolecularCoordinates,molecularBackboneSegments,createMolecularBackbone} from '../src/lib/molecular-surface.mjs';

function glb(metadata,binary=new Uint8Array(36)){
  const encoded=new TextEncoder().encode(JSON.stringify(metadata)),jsonSize=Math.ceil(encoded.length/4)*4,binSize=Math.ceil(binary.length/4)*4,buffer=new ArrayBuffer(12+8+jsonSize+8+binSize),v=new DataView(buffer),bytes=new Uint8Array(buffer);
  v.setUint32(0,0x46546c67,true);v.setUint32(4,2,true);v.setUint32(8,buffer.byteLength,true);v.setUint32(12,jsonSize,true);v.setUint32(16,0x4e4f534a,true);bytes.fill(32,20,20+jsonSize);bytes.set(encoded,20);v.setUint32(20+jsonSize,binSize,true);v.setUint32(24+jsonSize,0x004e4942,true);bytes.set(binary,28+jsonSize);return buffer;
}
const metadata=()=>({asset:{version:'2.0'},buffers:[{byteLength:36}],bufferViews:[{buffer:0,byteLength:36}],accessors:[{bufferView:0,componentType:5126,type:'VEC3',count:3}],meshes:[{primitives:[{attributes:{POSITION:0}}]}],nodes:[{mesh:0}],scenes:[{nodes:[0]}],scene:0});

test('embedded GLB is admitted with its actual primitive budget',()=>{
  const result=inspectGLB(glb(metadata()));assert.equal(result.triangles,1);assert.equal(result.vertices,3);assert.equal(result.textures,0);
});
test('external resource references and cyclic scene graphs are rejected before any loader',()=>{
  for(const uri of ['https://example.com/model.bin','file:///private/model.bin','data:application/octet-stream;base64,AAAA']){const m=metadata();m.buffers[0].uri=uri;assert.throws(()=>inspectGLB(glb(m)),/External|URI/);}
  const m=metadata();m.nodes=[{children:[1]},{children:[0]}];assert.throws(()=>inspectGLB(glb(m)),/Cyclic/);
  const extra=metadata();extra.extensions={custom:{url:'https://example.com/asset'}};assert.throws(()=>inspectGLB(glb(extra)),/URLs/);
});
test('GLB accessor, chunk and decoded texture allocations are bounded',()=>{
  const m=metadata();m.accessors[0].count=MODEL_LIMITS.vertices+1;assert.throws(()=>inspectGLB(glb(m)),/allocation budget/);
  const truncated=glb(metadata()).slice(0,-4);assert.throws(()=>inspectGLB(truncated),/complete/);
  const image=new Uint8Array(36),view=new DataView(image.buffer);view.setUint32(0,0x89504e47);view.setUint32(16,40000);view.setUint32(20,40000);
  const texture=metadata();texture.images=[{bufferView:0,mimeType:'image/png'}];assert.throws(()=>inspectGLB(glb(texture,image)),/texture dimensions/);
});
test('binary and ASCII STL retain triangle counts and reject nonfinite coordinates',()=>{
  const binary=new ArrayBuffer(134),v=new DataView(binary);v.setUint32(80,1,true);v.setFloat32(108,1,true);v.setFloat32(124,1,true);assert.equal(inspectSTL(binary).triangles,1);
  v.setFloat32(108,Infinity,true);assert.throws(()=>inspectSTL(binary),/invalid vertex/);
  const ascii=new TextEncoder().encode('solid sample\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid').buffer;assert.equal(inspectSTL(ascii).triangles,1);
});

const structure=(scale=1,units='angstrom')=>({id:'coordinate-fixture',name:'Coordinate envelope fixture',units,atoms:[{id:1,element:'C',chain_id:'A',position:[-1.4*scale,0,0]},{id:2,element:'O',chain_id:'A',position:[0,0,0]},{id:3,element:'N',chain_id:'B',position:[1.4*scale,0,0]}],bonds:[{atom_a:1,atom_b:2},{atom_a:2,atom_b:3}]});
test('coordinate-derived surfaces preserve source atoms and interpolate a continuous envelope',async()=>{
  const input=structure(),saved=structuredClone(input),data=await molecularDensity(input,{resolution:48});
  assert.equal(data.atomCount,3);assert.deepEqual(input,saved);assert.equal(data.atoms[0].position[0],-1.4);
  const midpoint=Math.floor(data.resolution/2),index=midpoint*data.resolution**2+midpoint*data.resolution+midpoint;
  assert.ok(data.field[index]>.5);assert.equal(data.field[0],0);assert.ok(data.palette.every(Number.isFinite));
  const {root,meta}=await createMolecularSurface(THREE,input,{resolution:48});
  assert.equal(root.isInstancedMesh,undefined);assert.equal(root.userData.molecularSurface,true);assert.equal(meta.atomCount,3);assert.ok(root.geometry.attributes.position.count>300);
  for(const name of ['position','normal','color'])assert.ok(root.geometry.attributes[name].array.every(Number.isFinite));
  root.geometry.dispose();root.material.dispose();
});
test('molecular surfaces are invariant under angstrom-to-nanometer unit conversion',async()=>{
  const a=await molecularDensity(structure(),{resolution:40}),b=await molecularDensity(structure(.1,'nm'),{resolution:40});
  assert.ok(Math.abs(a.span*.1-b.span)<1e-12);assert.equal(a.field.length,b.field.length);
  let error=0;for(let i=0;i<a.field.length;i++)error=Math.max(error,Math.abs(a.field[i]-b.field[i]));assert.ok(error<1e-5,`Density differs after unit conversion: ${error}`);
});
test('invalid scientific coordinates and cancelled generation cannot produce a substitute surface',async()=>{
  const bad=structure();bad.atoms[0].position[0]=NaN;assert.throws(()=>prepareMolecularCoordinates(bad),/invalid coordinates/);
  const duplicate=structure();duplicate.atoms[1].id=1;assert.throws(()=>prepareMolecularCoordinates(duplicate),/unique/);
  const controller=new AbortController();controller.abort();await assert.rejects(()=>molecularDensity(structure(),{signal:controller.signal}),{name:'AbortError'});
});

test('backbone ribbons preserve chain gaps and reject absent backbone coordinates',()=>{
  const input={atoms:[0,3.8,20,23.8].map((x,id)=>({id,name:'CA',element:'C',chain_id:'A',position:[x,0,0]}))};
  assert.deepEqual(molecularBackboneSegments(input).segments.map(s=>s.map(a=>a.id)),[['0','1'],['2','3']]);
  const {root}=createMolecularBackbone(THREE,input);assert.equal(root.children.length,2);
  root.traverse(o=>{if(o.geometry){assert.ok(o.geometry.attributes.position.array.every(Number.isFinite));o.geometry.dispose();o.material.dispose();}});
  assert.throws(()=>createMolecularBackbone(THREE,structure()),/no connected/);
});

test('overlapping atoms cannot inflate a molecular display envelope',async()=>{
  const single={atoms:[{id:'a',element:'C',position:[0,0,0]}]},dense={atoms:Array.from({length:100},(_,id)=>({id,element:'C',position:[0,0,0]}))};
  const a=await molecularDensity(single,{resolution:32}),b=await molecularDensity(dense,{resolution:32});
  assert.deepEqual(a.field,b.field);
});
