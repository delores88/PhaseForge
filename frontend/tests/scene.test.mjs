import test from 'node:test';
import assert from 'node:assert/strict';
import * as THREE from 'three';
import {buildSceneGraph} from '../src/lib/scene-geometry.mjs';
import {normalizeScene,sceneSourceFromEvidence,sceneFromEvidence,SCENE_LIMITS,SCENE_PRESETS,exampleScene,helixPoints,fibonacciSphere,potentialFlowVelocity,potentialStreamlines} from '../src/lib/scene.mjs';

test('camera parent updates retain scene identity across fresh Studio manifest wrappers',()=>{
  const scene=exampleScene('dna');
  const initial=sceneSourceFromEvidence(null,{visualization:{scene}});
  for(let i=0;i<30;i++)assert.strictEqual(sceneSourceFromEvidence(null,{visualization:{scene}}),initial);
  const replacement=exampleScene('flow');
  assert.notStrictEqual(sceneSourceFromEvidence(null,{visualization:{scene:replacement}}),initial);
  assert.equal(sceneSourceFromEvidence({manifest_id:'old'},{id:'new',visualization:{scene}}),null);
});

test('scene graph releases instance attributes and shared resources exactly once',()=>{
  const graph=buildSceneGraph(THREE,normalizeScene(exampleScene('dna')),{quality:'low'});
  let instances=0,releasedInstances=0,releasedGeometry=0;
  graph.root.traverse(object=>{if(object.isInstancedMesh){instances++;object.addEventListener('dispose',()=>releasedInstances++);}});
  for(const geometry of graph.resources.geometries)geometry.addEventListener('dispose',()=>releasedGeometry++);
  assert.ok(instances>0);const geometryCount=graph.resources.geometries.size;
  graph.dispose();graph.dispose();
  assert.equal(releasedInstances,instances);assert.equal(releasedGeometry,geometryCount);
  assert.equal(graph.root.children.length,0);assert.equal(graph.pickables.length,0);
});

test('scene input stays immutable and unsupported code/URLs never reach rendering data',()=>{
  const input={nodes:[{id:'atom',type:'atom',script:'throw Error()',url:'file:///private',parameters:{radius:NaN,expression:'alert(1)'}},{id:'unsafe',type:'javascript'}]};
  const scene=normalizeScene(input);
  assert.equal(scene.nodes.length,1);assert.equal(scene.nodes[0].parameters.radius,1);
  assert.equal(scene.nodes[0].script,undefined);assert.equal(scene.nodes[0].parameters.expression,undefined);
  assert.ok(Number.isNaN(input.nodes[0].parameters.radius));assert.ok(scene.warnings.length);
});

test('geometry limits, invalid coordinates and duplicate IDs are bounded before allocation',()=>{
  const input={nodes:Array.from({length:100},(_,i)=>({id:`a-${i}`,type:'molecule',parameters:{atoms:Array.from({length:5000},(_,j)=>({id:String(j),element:'C',position:[j,0,0]}))}}))};
  const scene=normalizeScene(input);
  assert.equal(scene.nodes.length,SCENE_LIMITS.nodes);
  assert.ok(scene.nodes.reduce((sum,n)=>sum+(n.parameters.atoms?.length||0),0)<=SCENE_LIMITS.atoms);
  const duplicate=normalizeScene({nodes:[{id:'x',type:'sphere'},{id:'x',type:'sphere'}]});assert.equal(duplicate.nodes.length,1);
});

test('malformed mesh triangles are dropped without reconnecting other vertices',()=>{
  const scene=normalizeScene({nodes:[{id:'surface',type:'mesh',parameters:{vertices:[[0,0,0],[1,0,0],[0,1,0],[0,0,1]],indices:[0,99,1,0,2,3]}}]});
  assert.deepEqual(scene.nodes[0].parameters.indices,[0,2,3]);
  const invalid=normalizeScene({nodes:[{id:'surface',type:'mesh',parameters:{vertices:[[0,0,0],[NaN,0,0],[0,1,0]],indices:[0,1,2]}}]});
  assert.equal(invalid.nodes.length,0);
});

test('atomic coordinate scales and negative world positions are retained',()=>{
  const scene=normalizeScene({nodes:[{id:'atom',type:'atom',position:[-1e-9,0,2e-10],parameters:{radius:1.4e-10}}]});
  assert.equal(scene.nodes[0].parameters.radius,1.4e-10);assert.equal(scene.nodes[0].parameters.thickness,1.4e-10*.08);assert.equal(scene.nodes[0].position[0],-1e-9);
});

test('a historical run cannot be relabeled with the current manifest geometry',()=>{
  const manifest={id:'new',visualization:{scene:exampleScene('dna')}};
  assert.equal(sceneFromEvidence({manifest_id:'old'},manifest),null);
  assert.equal(sceneFromEvidence({manifest_id:'new'},manifest).title,'Helical architecture');
  assert.equal(sceneFromEvidence({manifest_id:'old',result:{visualization:{scene:exampleScene('flow')}}},manifest).title,'Potential flow · sphere');
});

test('helix and membrane generation preserve specified radii and extent',()=>{
  for(const p of fibonacciSphere(87,1.7))assert.ok(Math.abs(Math.hypot(...p)-1.7)<1e-12);
  const points=helixPoints({turns:3.25,length:7,radius:1.3,count:73});
  assert.equal(points[0][1],-3.5);assert.equal(points.at(-1)[1],3.5);
  for(const [x,,z] of points)assert.ok(Math.abs(Math.hypot(x,z)-1.3)<1e-12);
});

test('analytic potential flow has correct far-field and equatorial velocity and avoids the body',()=>{
  const equator=potentialFlowVelocity([0,2,0]);assert.equal(equator[0],1.0625);assert.ok(Math.abs(equator[1])+Math.abs(equator[2])<1e-12);
  assert.ok(Math.abs(potentialFlowVelocity([100,0,0])[0]-1)<.00001);
  const lines=potentialStreamlines(36);
  for(const points of lines){assert.ok(points.length>20);assert.ok(points.at(-1)[0]>3.8);for(const p of points){assert.ok(p.every(Number.isFinite));assert.ok(Math.hypot(...p)>=1);}}
});

for(const preset of SCENE_PRESETS)test(`${preset.id} scene creates finite indexed geometry with deterministic disposal`,()=>{
  const scene=exampleScene(preset.id),graph=buildSceneGraph(THREE,scene);
  const bounds=new THREE.Box3().setFromObject(graph.root);assert.ok(!bounds.isEmpty());assert.ok([...bounds.min.toArray(),...bounds.max.toArray()].every(Number.isFinite));
  assert.equal(graph.objects.size,scene.nodes.length);assert.ok(graph.pickables.length>0);
  let vertices=0,renderedTriangles=0;
  graph.root.traverse(o=>{if(!o.geometry)return;const p=o.geometry.attributes.position;if(p){vertices+=p.count;for(const value of p.array)assert.ok(Number.isFinite(value));}if(o.isMesh)renderedTriangles+=(o.geometry.index?.count||p.count)/3*(o.isInstancedMesh?o.count:1);});
  assert.ok(vertices>100);assert.ok(renderedTriangles<2_000_000,`Triangle budget exceeded: ${renderedTriangles}`);
  let disposed=0;for(const g of graph.resources.geometries)g.addEventListener('dispose',()=>disposed++);
  const expected=graph.resources.geometries.size;graph.dispose();assert.equal(disposed,expected);assert.equal(graph.root.children.length,0);
});

test('cross-object bonds update as their numerical-bound objects move',()=>{
  const scene=normalizeScene({nodes:[{id:'a',type:'atom'},{id:'b',type:'atom',position:[2,0,0]}],bonds:[{from:'a',to:'b'}]});
  const graph=buildSceneGraph(THREE,scene);graph.objects.get('b').position.set(4,0,0);graph.updateBonds();
  const bond=graph.root.children.at(-1).children[0],matrix=new THREE.Matrix4();bond.getMatrixAt(0,matrix);
  assert.equal(new THREE.Vector3().setFromMatrixPosition(matrix).x,2);assert.equal(new THREE.Vector3().setFromMatrixScale(matrix).y,4);graph.dispose();
});

test('molecular atoms, multiple bonds and envelopes follow supplied radius across coordinate scales',()=>{
  for(const unit of [1e-10,1,1e8]){
    const atoms=[{id:'a',element:'C',position:[-.1*unit,0,0]},{id:'b',element:'N',position:[0,0,0]},{id:'c',element:'O',position:[.1*unit,.1*unit,0]}];
    const original=structuredClone(atoms),radius=.045*unit;
    const scene=normalizeScene({nodes:[{id:'molecule',type:'molecule',parameters:{atoms,bonds:[{from:'a',to:'b',order:2},{from:'b',to:'c',order:3}],radius,thickness:.09*unit,representation:'surface'}}]});
    const graph=buildSceneGraph(THREE,scene),[atomMesh,bondMesh,envelope]=graph.objects.get('molecule').children,matrix=new THREE.Matrix4();
    assert.equal(atomMesh.count,atoms.length);assert.equal(bondMesh.count,10);assert.equal(envelope.count,atoms.length);
    for(let i=0;i<atoms.length;i++){
      atomMesh.getMatrixAt(i,matrix);const position=new THREE.Vector3().setFromMatrixPosition(matrix),scale=new THREE.Vector3().setFromMatrixScale(matrix);
      assert.ok(Math.abs(scale.x/radius-1)<1e-6,`atom radius lost at unit ${unit}`);
      position.toArray().forEach((v,k)=>assert.ok(Math.abs(v-atoms[i].position[k])<unit*1e-7));
      envelope.getMatrixAt(i,matrix);assert.ok(Math.abs(new THREE.Vector3().setFromMatrixScale(matrix).x/(radius*2.24)-1)<1e-6);
    }
    for(let i=0;i<bondMesh.count;i++){
      bondMesh.getMatrixAt(i,matrix);const scale=new THREE.Vector3().setFromMatrixScale(matrix),position=new THREE.Vector3().setFromMatrixPosition(matrix);
      assert.ok(scale.x/radius<=.181,`multiple bond too thick at unit ${unit}`);
      assert.ok(Math.abs(position.z)/radius<.81,`multiple-bond offset lost its units at ${unit}`);
    }
    assert.deepEqual(atomMesh.userData.atomRecords,original);assert.deepEqual(atoms,original);graph.dispose();
  }
});

test('omitted molecular radii derive from bond or coordinate spacing rather than fixed world units',()=>{
  for(const unit of [1e-10,1,1e8])for(const includeBonds of [false,true]){
    const atoms=[0,1,2].map(i=>({id:String(i),element:'C',position:[i*unit,0,0]})),bonds=includeBonds?[{from:'0',to:'1'},{from:'1',to:'2'}]:[];
    const scene=normalizeScene({nodes:[{id:'molecule',type:'molecule',parameters:{atoms,bonds,radius:null,thickness:null}}]}),parameters=scene.nodes[0].parameters;
    assert.ok(Math.abs(parameters.radius/unit-.24)<1e-12);
    assert.ok(Math.abs(parameters.thickness/unit-.0576)<1e-12);
    assert.deepEqual(parameters.atoms,atoms);assert.equal(parameters.bonds.length,bonds.length);
  }
  assert.equal(exampleScene('molecular').nodes.find(n=>n.type==='molecule').parameters.radius,.29);
});
