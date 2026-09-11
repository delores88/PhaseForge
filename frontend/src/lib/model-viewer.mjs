import {illustrationCoordinates,illustrationNodeId} from './illustration-coordinates.mjs';
import {MODEL_LIMITS,readModelAsset} from './model-assets.mjs';
import {enforceSafeModelMaterials} from './model-assets-shaders.mjs';
import {createMolecularSurface,createMolecularBackbone,prepareMolecularCoordinates,DISPLAY_RADII_ANGSTROM} from './molecular-surface.mjs';
import {ELEMENT_COLORS} from './scene.mjs';
import {releaseRenderer,disposeObjectTree as disposeTree} from './viewer-resources.mjs';
import {installCameraNavigation} from './viewer-camera.mjs';
import {advanceRenderClock} from './viewer-cadence.mjs';

async function loadModel(THREE,config) {
  if(config.structure){
    if(config.representation==='ribbon')return createMolecularBackbone(THREE,config.structure);
    if(config.representation==='surface'||!config.representation){
      return createMolecularSurface(THREE,config.structure,{resolution:config.quality==='high'?128:config.quality==='low'?64:104,signal:config.signal,onProgress:config.onProgress,yieldControl:()=>new Promise(resolve=>setTimeout(resolve,0))});
    }
    const {atoms,unitScale,units}=prepareMolecularCoordinates(config.structure),group=new THREE.Group();
    if(atoms.length>16000)throw new Error('Atom-by-atom display supports up to 16,000 atoms. Choose Molecular surface or a smaller assembly.');
    const geometry=new THREE.SphereGeometry(1,atoms.length>5000?8:16,atoms.length>5000?6:12),material=new THREE.MeshPhysicalMaterial({color:'#ffffff',roughness:.48,metalness:0,clearcoat:.12}),mesh=new THREE.InstancedMesh(geometry,material,atoms.length);
    const matrix=new THREE.Matrix4(),color=new THREE.Color(),byId=new Map(atoms.map(a=>[a.id,a]));
    atoms.forEach((atom,i)=>{const radius=(DISPLAY_RADII_ANGSTROM[atom.element]||1.7)*unitScale*(config.representation==='space_filling'?1:.28);matrix.makeScale(radius,radius,radius);matrix.setPosition(...atom.position);mesh.setMatrixAt(i,matrix);mesh.setColorAt(i,color.set(ELEMENT_COLORS[atom.element]||'#8baeba'));});
    mesh.instanceMatrix.needsUpdate=true;mesh.instanceColor.needsUpdate=true;mesh.userData.atomRecords=atoms;mesh.name=config.structure.name||'Atomic coordinates';group.add(mesh);
    if(config.representation==='ball_and_stick'){
      const bonds=(config.structure.bonds||[]).filter(b=>byId.has(String(b.atom_a??b.from))&&byId.has(String(b.atom_b??b.to))).slice(0,40000),cylinder=new THREE.CylinderGeometry(1,1,1,6),bondMesh=new THREE.InstancedMesh(cylinder,new THREE.MeshStandardMaterial({color:'#9faeb9',roughness:.7}),bonds.length),up=new THREE.Vector3(0,1,0),q=new THREE.Quaternion();
      bonds.forEach((b,i)=>{const a=new THREE.Vector3(...byId.get(String(b.atom_a??b.from)).position),z=new THREE.Vector3(...byId.get(String(b.atom_b??b.to)).position),direction=z.clone().sub(a),length=direction.length(),r=Math.min(.12*unitScale,length*.08);q.setFromUnitVectors(up,direction.normalize());matrix.compose(a.add(z).multiplyScalar(.5),q,new THREE.Vector3(r,length,r));bondMesh.setMatrixAt(i,matrix);});bondMesh.instanceMatrix.needsUpdate=true;group.add(bondMesh);
    }
    return {root:group,atoms,meta:{atomCount:atoms.length,units,unitScale,description:'Element display geometry at the supplied atomic coordinates. Atomic radii and visible bonds are representations, not a force-field calculation.'}};
  }
  config.onProgress?.(.05);const asset=await readModelAsset(config);config.onProgress?.(.3);if(config.signal.aborted)throw new DOMException('Cancelled','AbortError');
  if(asset.format==='stl'){
    const {STLLoader}=await import('three/examples/jsm/loaders/STLLoader.js'),geometry=new STLLoader().parse(asset.buffer);geometry.computeVertexNormals();
    const root=new THREE.Mesh(geometry,new THREE.MeshPhysicalMaterial({color:'#84aaa8',vertexColors:!!geometry.attributes.color,roughness:.55,metalness:.03,clearcoat:.15}));root.name=asset.name;return {root,atoms:[],meta:{...asset,buffer:undefined,json:undefined,description:'Imported STL surface. Units and scientific interpretation follow the source asset.'}};
  }
  const {GLTFLoader}=await import('three/examples/jsm/loaders/GLTFLoader.js'),manager=new THREE.LoadingManager();
  manager.setURLModifier(url=>{if(!url.startsWith('blob:'))throw new Error('External model resources are disabled. Use an embedded GLB.');return url;});
  const gltf=await new GLTFLoader(manager).parseAsync(asset.buffer,'');
  if(config.signal.aborted){disposeTree(gltf.scene);throw new DOMException('Cancelled','AbortError');}
  return {root:gltf.scene,atoms:[],meta:{...asset,buffer:undefined,json:undefined,description:'Imported self-contained GLB geometry and materials. Rendering does not establish the source model’s scientific validity.'}};
}

export async function createScientificModelViewer(host,config) {
  const THREE=await import('three');let model=null,renderer=null,scene=null,controls=null,navigation=null,observer=null,composer=null,environment=null,raf=0,disposed=false;
  const passes=[],listeners=[],auxiliary=[],originalMaterials=new Map(),contrastUniform={value:1};
  const coordinates=illustrationCoordinates(config.coordinateMapping);
  const dispose=()=>{if(disposed)return;disposed=true;config.signal.removeEventListener('abort',dispose);cancelAnimationFrame(raf);observer?.disconnect();navigation?.dispose();controls?.dispose();for(const [type,handler] of listeners)renderer?.domElement.removeEventListener(type,handler);passes.forEach(p=>p.dispose?.());composer?.dispose();environment?.dispose();disposeTree(model?.root);for(const o of auxiliary)disposeTree(o);releaseRenderer(renderer);};
  if(config.signal.aborted)return null;
  config.signal.addEventListener('abort',dispose,{once:true});
  try{
    model=await loadModel(THREE,config);if(config.signal.aborted){disposeTree(model?.root);dispose();return null;}
    // Includes material arrays and line/point objects, before renderer creation.
    enforceSafeModelMaterials(model.root);
    config.onProgress?.(.94);
    let triangles=0,meshes=0;const pickables=[],sourceMaterials=new Set();
    model.root.traverse(object=>{
      if(object.isLight||object.isCamera){object.visible=false;return;}
      if(!object.isMesh)return;meshes++;const p=object.geometry?.attributes.position;if(!p)return;
      triangles+=(object.geometry.index?.count||p.count)/3*(object.isInstancedMesh?object.count:1);
      if(triangles>MODEL_LIMITS.triangles*2||meshes>MODEL_LIMITS.nodes)throw new Error('Expanded model geometry exceeds the interactive drawing budget.');
      for(let i=0;i<p.array.length;i++)if(!Number.isFinite(p.array[i])||Math.abs(p.array[i])>1e15)throw new Error('The model contains invalid or out-of-range vertex coordinates.');
      object.castShadow=true;object.receiveShadow=true;pickables.push(object);
      const isolated=(Array.isArray(object.material)?object.material:[object.material]).map(material=>{if(!material)return material;sourceMaterials.add(material);return material.clone();});object.material=Array.isArray(object.material)?isolated:isolated[0];
      for(const material of Array.isArray(object.material)?object.material:[object.material])if(material){originalMaterials.set(material,{roughness:material.roughness,metalness:material.metalness,color:material.color?.clone(),nodeId:illustrationNodeId(object)});material.envMapIntensity=.35;material.onBeforeCompile=shader=>{shader.uniforms.phaseforgeContrast=contrastUniform;shader.fragmentShader='uniform float phaseforgeContrast;\n'+shader.fragmentShader;shader.fragmentShader=shader.fragmentShader.replace('#include <colorspace_fragment>','#include <colorspace_fragment>\ngl_FragColor.rgb=clamp((gl_FragColor.rgb-0.5)*phaseforgeContrast+0.5,0.0,1.0);');};}
    });
    sourceMaterials.forEach(material=>material.dispose());
    const bounds=new THREE.Box3().setFromObject(model.root);if(bounds.isEmpty()||!meshes)throw new Error('The file contains no visible triangle surface.');
    const center=bounds.getCenter(new THREE.Vector3()),size=bounds.getSize(new THREE.Vector3()),span=Math.max(size.x,size.y,size.z,1e-15),normalization=4/span;
    renderer=new THREE.WebGLRenderer({antialias:true,powerPreference:'high-performance'});renderer.setPixelRatio(Math.min(window.devicePixelRatio||1,config.quality==='low'?1:1.6));renderer.outputColorSpace=THREE.SRGBColorSpace;renderer.toneMapping=THREE.ACESFilmicToneMapping;renderer.toneMappingExposure=1.15;renderer.shadowMap.enabled=config.quality!=='low';renderer.shadowMap.type=THREE.PCFSoftShadowMap;renderer.localClippingEnabled=true;host.replaceChildren(renderer.domElement);
    scene=new THREE.Scene();const world=new THREE.Group(),origin=new THREE.Group();origin.position.copy(center).negate();origin.add(model.root);world.add(origin);world.scale.setScalar(normalization);scene.add(world);
    const camera=new THREE.PerspectiveCamera(35,1,.01,500),{OrbitControls}=await import('three/examples/jsm/controls/OrbitControls.js');
    if(disposed)return null;
    controls=new OrbitControls(camera,renderer.domElement);controls.enableDamping=true;controls.dampingFactor=.075;controls.screenSpacePanning=true;controls.minDistance=.03;controls.maxDistance=200;
    const hemisphere=new THREE.HemisphereLight('#e6eff4','#293644',.4);scene.add(hemisphere);
    const key=new THREE.DirectionalLight('#f8f4eb',2.1);key.position.set(-4,7,6);key.castShadow=true;key.shadow.mapSize.set(2048,2048);Object.assign(key.shadow.camera,{left:-5,right:5,top:5,bottom:-5,near:.1,far:30});key.shadow.normalBias=.018;key.shadow.bias=-.00005;scene.add(key);auxiliary.push(key);
    const rim=new THREE.DirectionalLight('#b6daec',1.6);rim.position.set(4,3,-5);scene.add(rim);const fill=new THREE.DirectionalLight('#e6bba2',.35);fill.position.set(3,-1,3);scene.add(fill);
    const floor=new THREE.Mesh(new THREE.PlaneGeometry(200,200),new THREE.ShadowMaterial({opacity:.24}));floor.rotation.x=-Math.PI/2;floor.position.y=(bounds.min.y-center.y)*normalization-.025;floor.receiveShadow=true;scene.add(floor);auxiliary.push(floor);
    const {RoomEnvironment}=await import('three/examples/jsm/environments/RoomEnvironment.js');if(disposed)return null;const room=new RoomEnvironment(),pmrem=new THREE.PMREMGenerator(renderer);environment=pmrem.fromScene(room,.03);scene.environment=environment.texture;room.dispose();pmrem.dispose();
    let ao=null;
    if(config.quality!=='low'){
      const [{EffectComposer},{RenderPass},{SSAOPass},{OutputPass}]=await Promise.all([import('three/examples/jsm/postprocessing/EffectComposer.js'),import('three/examples/jsm/postprocessing/RenderPass.js'),import('three/examples/jsm/postprocessing/SSAOPass.js'),import('three/examples/jsm/postprocessing/OutputPass.js')]);
      if(disposed)return null;
      composer=new EffectComposer(renderer);const renderPass=new RenderPass(scene,camera),output=new OutputPass();ao=new SSAOPass(scene,camera,512,512,24);ao.kernelRadius=.28;ao.minDistance=.0005;ao.maxDistance=.025;passes.push(renderPass,ao,output);passes.forEach(p=>composer.addPass(p));
    }
    const clipped=new THREE.Plane(new THREE.Vector3(-1,0,0),3),box=new THREE.Box3Helper(new THREE.Box3(),'#d6e9aa');box.visible=false;scene.add(box);auxiliary.push(box);
    const selectedGeometry=new THREE.SphereGeometry(1,12,8),selectedMaterial=new THREE.MeshBasicMaterial({color:'#f4d58b',transparent:true,opacity:.75,depthTest:false}),markers=new THREE.InstancedMesh(selectedGeometry,selectedMaterial,Math.min(2000,Math.max(model.atoms?.length||1,1)));markers.count=0;markers.renderOrder=10;origin.add(markers);auxiliary.push(markers);
    let dirty=true,lastInteraction=performance.now(),lastTick=0,dragging=false,down=null,settingsStamp='',selectedObject=null,selectedPoint=null;
    const cameraKey=`phaseforge.model.camera.${config.url||config.structure?.id||config.file?.name||config.structure?.name||'model'}`;
    let savedCamera=null;try{savedCamera=JSON.parse(sessionStorage.getItem(cameraKey)||'null');}catch{}
    const cameraState=()=>({position:coordinates.toSource(camera.position.clone().divideScalar(normalization).add(center).toArray()),target:coordinates.toSource(controls.target.clone().divideScalar(normalization).add(center).toArray()),up:coordinates.directionToSource(camera.up.toArray()),focal_length_mm:camera.getFocalLength(),units:config.sourceUnits||model.meta.units||'source asset units'});
    const reportCamera=()=>{dirty=true;const value=cameraState();try{sessionStorage.setItem(cameraKey,JSON.stringify(value));}catch{}config.options().onCameraChange?.(value);};
    controls.addEventListener('start',()=>{dragging=true;lastInteraction=performance.now();});controls.addEventListener('change',()=>{dirty=true;lastInteraction=performance.now();});controls.addEventListener('end',()=>{dragging=false;reportCamera();});
    const resize=()=>{const width=Math.max(1,host.clientWidth),height=Math.max(1,host.clientHeight);renderer.setSize(width,height,false);composer?.setSize(width,height);camera.aspect=width/height;camera.updateProjectionMatrix();dirty=true;};observer=new ResizeObserver(resize);observer.observe(host);resize();
    function fit(view='perspective',selection=false){
      controls.target.set(0,0,0);let extent=4;if(selection&&selectedPoint){controls.target.copy(selectedPoint);extent=.65;}camera.up.set(0,1,0);const distance=extent/(2*Math.tan(THREE.MathUtils.degToRad(camera.fov)/2))*1.27/Math.min(camera.aspect,1),direction=view==='xy'?new THREE.Vector3(0,0,1):view==='xz'?new THREE.Vector3(0,1,.0001):view==='yz'?new THREE.Vector3(1,0,0):new THREE.Vector3(.65,.28,1).normalize();camera.position.copy(controls.target).addScaledVector(direction,distance);controls.update();reportCamera();
    }
    function setCamera(value){if(!value)return;for(const key of ['position','target','up'])if(value[key]&&(!Array.isArray(value[key])||value[key].length!==3||!value[key].every(Number.isFinite)))return;if(value.position)camera.position.set(...coordinates.toAsset(value.position)).sub(center).multiplyScalar(normalization);if(value.target)controls.target.set(...coordinates.toAsset(value.target)).sub(center).multiplyScalar(normalization);if(value.up)camera.up.set(...coordinates.directionToAsset(value.up)).normalize();if(Number.isFinite(value.focal_length_mm))camera.setFocalLength(Math.max(20,Math.min(120,value.focal_length_mm)));controls.update();reportCamera();}
    navigation=installCameraNavigation(THREE,{camera,controls,element:renderer.domElement,span:4,onChange:reportCamera,fit});
    fit();setCamera(config.options().displayPresentation?.camera||savedCamera);
    const listen=(type,handler)=>{renderer.domElement.addEventListener(type,handler);listeners.push([type,handler]);};
    const ray=new THREE.Raycaster(),pointer=new THREE.Vector2();
    listen('pointerdown',e=>{down=[e.clientX,e.clientY];});
    listen('pointerup',e=>{
      if(!down||Math.hypot(e.clientX-down[0],e.clientY-down[1])>5)return;down=null;const rect=renderer.domElement.getBoundingClientRect();pointer.set((e.clientX-rect.left)/rect.width*2-1,-(e.clientY-rect.top)/rect.height*2+1);ray.setFromCamera(pointer,camera);const hit=ray.intersectObjects(pickables,false).find(h=>h.object.visible&&(config.options().cut>=1||h.point.x<=clipped.constant));if(!hit)return;
      const worldPoint=hit.point.clone().divideScalar(normalization).add(center);let atom=hit.object.userData.atomRecords?.[hit.instanceId]||null;
      if(!atom&&model.atoms?.length){let best=Infinity;for(const a of model.atoms){const distance=worldPoint.distanceToSquared(new THREE.Vector3(...a.position));if(distance<best){atom=a;best=distance;}}}
      selectedPoint=hit.point.clone();selectedObject=atom?null:hit.object;world.updateMatrixWorld(true);box.visible=!!selectedObject;if(selectedObject)box.box.setFromObject(selectedObject);dirty=true;
      const inspection={name:hit.object.name||hit.object.parent?.name||'Model surface',mesh_uuid:hit.object.uuid,entity_id:illustrationNodeId(hit.object),atom,world_position:coordinates.toSource(worldPoint.toArray()),units:config.sourceUnits||model.meta.units||'source asset units',provenance:config.provenance||model.meta.description,camera:cameraState()};config.onSelection?.(inspection);config.options().onInspect?.(inspection);
    });
    listen('webglcontextlost',e=>{e.preventDefault();cancelAnimationFrame(raf);config.onError?.('Graphics memory was interrupted. Restore the model with Economy detail.');});
    function applySettings(){
      const opts=config.options(),display=opts.displayPresentation||{},stamp=JSON.stringify([opts.style,opts.cut,opts.ao,opts.selectedAtomIds,display]);if(stamp===settingsStamp)return;settingsStamp=stamp;dirty=true;
      const microscopy=opts.style==='microscopy';scene.background=new THREE.Color(display.background||(microscopy?'#101a22':'#d6dcdf'));hemisphere.intensity=microscopy?.4:.65;renderer.toneMappingExposure=Number.isFinite(display.exposure)?Math.max(.15,Math.min(3,display.exposure)):(microscopy?.9:1.03);contrastUniform.value=Number.isFinite(display.contrast)?Math.max(.5,Math.min(2,display.contrast)):1;floor.material.opacity=microscopy?.28:.2;
      const selectedNodes=new Set((display.selectedIds||[]).map(String)),hiddenNodes=new Set((display.hiddenIds||[]).map(String));
      for(const [material,base] of originalMaterials){if(base.color){material.color.copy(selectedNodes.has(base.nodeId)?new THREE.Color(display.highlightColor||'#ffd45a'):display.color?new THREE.Color(display.color):base.color);if(selectedNodes.size&&display.dimOthers&&!selectedNodes.has(base.nodeId))material.color.multiplyScalar(.22);}if(base.roughness!==undefined)material.roughness=microscopy?Math.max(.6,base.roughness):base.roughness;if(base.metalness!==undefined)material.metalness=microscopy?Math.min(.08,base.metalness):base.metalness;}
      let selectedBounds=null;world.updateMatrixWorld(true);for(const object of pickables){const id=illustrationNodeId(object);object.visible=!hiddenNodes.has(id);if(object.visible&&selectedNodes.has(id)){const bounds=new THREE.Box3().setFromObject(object);if(selectedBounds)selectedBounds.union(bounds);else selectedBounds=bounds;}}if(selectedBounds){box.box.copy(selectedBounds);box.visible=true;selectedPoint=selectedBounds.getCenter(new THREE.Vector3());}else if(!selectedObject)box.visible=false;
      if(ao)ao.enabled=opts.ao!==false;clipped.constant=(opts.cut-.5)*5;renderer.clippingPlanes=opts.cut<1?[clipped]:[];
      const selected=new Set((opts.selectedAtomIds||[]).map(String)),atoms=(model.atoms||[]).filter(a=>selected.has(a.id)).slice(0,2000),matrix=new THREE.Matrix4();markers.count=atoms.length;
      const unitScale=model.meta.unitScale||1;
      atoms.forEach((atom,i)=>{const radius=(DISPLAY_RADII_ANGSTROM[atom.element]||1.7)*unitScale*.42;matrix.makeScale(radius,radius,radius);matrix.setPosition(...atom.position);markers.setMatrixAt(i,matrix);});markers.instanceMatrix.needsUpdate=true;
    }
    const render=()=>{applySettings();controls.update();if(composer)composer.render();else renderer.render(scene,camera);dirty=false;};
    const draw=now=>{if(disposed)return;raf=requestAnimationFrame(draw);if(document.hidden)return;const interval=1000/((dragging||now-lastInteraction<700)?60:15),clock=advanceRenderClock(now,lastTick,interval);if(clock===null)return;lastTick=clock;applySettings();controls.update();if(dirty)render();};
    render();raf=requestAnimationFrame(draw);config.onProgress?.(1);config.onLoaded?.({...model.meta,triangles:Math.round(triangles),meshes});
    if(config.signal.aborted){dispose();return null;}
    return {dispose,fit,cameraState,setCamera,navigate:navigation.command,focusSelection:()=>fit('perspective',true),clearSelection:()=>{box.visible=false;selectedPoint=null;dirty=true;},captureFrame:()=>{render();return {dataUrl:renderer.domElement.toDataURL('image/png'),camera:cameraState(),provenance:config.provenance||model.meta.description};},capture:()=>{render();const link=document.createElement('a');link.download='phaseforge-model.png';link.href=renderer.domElement.toDataURL('image/png');link.click();}};
  }catch(error){dispose();throw error;}
}
