import {buildSceneGraph} from './scene-geometry.mjs';
import {PALETTE} from './scene.mjs';
import {releaseRenderer} from './viewer-resources.mjs';
import {installCameraNavigation} from './viewer-camera.mjs';

const radiusFor=(entity,fallback)=>Number.isFinite(entity?.radius)&&entity.radius>0?entity.radius:fallback;
const interpolatePosition=(a,b,alpha)=>b?a.position.map((v,i)=>v+(b.position[i]-v)*alpha):a.position;

export async function createSceneViewer(host,config) {
  const {sceneSpec,frames,meta,quality,sampleFrames,options,onTime,onStop,onStats,onSelect,onError,isCancelled}=config;
  const [THREE,{OrbitControls}]=await Promise.all([import('three'),import('three/examples/jsm/controls/OrbitControls.js')]);
  if(isCancelled())return null;
  const scene=new THREE.Scene();scene.background=new THREE.Color('#0a1019');
  const renderer=new THREE.WebGLRenderer({antialias:quality!=='low',alpha:false,powerPreference:'high-performance'});
  renderer.outputColorSpace=THREE.SRGBColorSpace;renderer.toneMapping=THREE.ACESFilmicToneMapping;renderer.toneMappingExposure=1.18;renderer.localClippingEnabled=true;
  let pixelRatio=Math.min(window.devicePixelRatio||1,quality==='high'?2:quality==='low'?1:1.5);renderer.setPixelRatio(pixelRatio);host.replaceChildren(renderer.domElement);
  let graph=null,raf=0,observer=null,controls=null,navigation=null,disposed=false;
  const extraGeometry=new Set(),extraMaterial=new Set(),extraInstances=new Set(),listeners=[];
  const addGeometry=g=>(extraGeometry.add(g),g),addMaterial=m=>(extraMaterial.add(m),m);
  const listen=(type,handler)=>{renderer.domElement.addEventListener(type,handler);listeners.push([type,handler]);};
  const dispose=()=>{if(disposed)return;disposed=true;cancelAnimationFrame(raf);observer?.disconnect();navigation?.dispose();controls?.dispose();for(const [type,handler] of listeners)renderer.domElement.removeEventListener(type,handler);graph?.dispose();extraInstances.forEach(object=>object.dispose());extraGeometry.forEach(g=>g.dispose());extraMaterial.forEach(m=>m.dispose());releaseRenderer(renderer);};
  try {
    const world=new THREE.Group();scene.add(world);
    if(sceneSpec?.nodes.length){graph=buildSceneGraph(THREE,sceneSpec,{quality});world.add(graph.root);}
    const frameMaps=frames.map(f=>new Map(f.entities.map(e=>[e.id,e])));
    const bound=new THREE.Box3();
    if(graph){
      if(!frames.length)bound.setFromObject(graph.root);
      else for(const node of sceneSpec.nodes.filter(n=>!n.entity_id))bound.union(new THREE.Box3().setFromObject(graph.objects.get(node.id)));
    }
    for(const frame of frames)for(const e of frame.entities){const p=new THREE.Vector3(...e.position),r=radiusFor(e,0);bound.expandByPoint(p.clone().addScalar(r));bound.expandByPoint(p.clone().addScalar(-r));}
    // Include the swept geometry, not just the recorded centers, in camera fitting.
    if(graph)for(const node of sceneSpec.nodes.filter(n=>n.entity_id)){
      const localBox=new THREE.Box3().setFromObject(graph.objects.get(node.id));
      for(const map of frameMaps){const entity=map.get(node.entity_id);if(entity)bound.union(localBox.clone().translate(new THREE.Vector3(...entity.position)));}
    }
    if(bound.isEmpty())bound.set(new THREE.Vector3(-1,-1,-1),new THREE.Vector3(1,1,1));
    const center=bound.getCenter(new THREE.Vector3()),size=bound.getSize(new THREE.Vector3()),span=Math.max(size.x,size.y,size.z,1e-15);world.position.copy(center).negate();
    const camera=new THREE.PerspectiveCamera(38,1,Math.max(span/100000,1e-22),span*5000);
    controls=new OrbitControls(camera,renderer.domElement);controls.enableDamping=true;controls.dampingFactor=.085;controls.screenSpacePanning=true;controls.minDistance=span*.002;controls.maxDistance=span*250;
    scene.add(new THREE.HemisphereLight('#d9e8f6','#253243',1.8));
    for(const [color,power,position] of [['#e8f3ff',3,[2,3,3]],['#65bdcc',1.8,[-3,1,-2]],['#b5a0d7',.7,[1,-1,1]]]){const light=new THREE.DirectionalLight(color,power);light.position.set(...position.map(n=>n*span));scene.add(light);}
    const ground=new THREE.GridHelper(span*2,24,'#334358','#1d2c3c');ground.position.y=bound.min.y-center.y-span*.05;ground.material.transparent=true;ground.material.opacity=.55;extraGeometry.add(ground.geometry);extraMaterial.add(ground.material);scene.add(ground);
    const clipping=new THREE.Plane(new THREE.Vector3(-1,0,0),span);
    const mapped=new Set((sceneSpec?.nodes||[]).map(n=>n.entity_id).filter(Boolean)),looseIDs=meta.ids.filter(id=>!mapped.has(id)),idIndex=new Map(meta.ids.map((id,i)=>[id,i]));
    const markers=new THREE.InstancedMesh(addGeometry(new THREE.SphereGeometry(1,quality==='low'?10:20,quality==='low'?8:14)),addMaterial(new THREE.MeshStandardMaterial({roughness:.4,metalness:.15})),Math.max(1,looseIDs.length));markers.count=0;markers.frustumCulled=false;markers.instanceMatrix.setUsage(THREE.DynamicDrawUsage);world.add(markers);
    extraInstances.add(markers);
    const matrix=new THREE.Matrix4(),color=new THREE.Color(),fallbackRadius=Math.min(span*.03,Math.max(span*.004,Number(config.pointSize)||span*.012));
    const traces=[];
    for(const id of meta.ids.slice(0,Math.min(96,Math.max(1,Math.floor(120000/Math.max(frames.length,1)))))){
      const points=[];for(let i=1;i<frames.length;i++){const a=frameMaps[i-1].get(id),b=frameMaps[i].get(id);points.push(...(a&&b?[...a.position,...b.position]:[0,0,0,0,0,0]));}
      const g=addGeometry(new THREE.BufferGeometry());g.setAttribute('position',new THREE.Float32BufferAttribute(points,3));g.setDrawRange(0,0);
      const line=new THREE.LineSegments(g,addMaterial(new THREE.LineBasicMaterial({color:PALETTE[(idIndex.get(id)||0)%PALETTE.length],transparent:true,opacity:.4})));line.frustumCulled=false;world.add(line);traces.push(line);
    }
    let maxSpeed=0;for(const f of frames)for(const e of f.entities.slice(0,64))if(e.velocity)maxSpeed=Math.max(maxSpeed,Math.hypot(...e.velocity));
    const vectorScale=maxSpeed?span*.15/maxSpeed:0;
    const arrows=meta.ids.slice(0,64).map((id,i)=>{const a=new THREE.ArrowHelper(new THREE.Vector3(1,0,0),new THREE.Vector3(),1,PALETTE[i%PALETTE.length]);world.add(a);a.traverse(o=>{if(o.geometry)extraGeometry.add(o.geometry);if(o.material)extraMaterial.add(o.material);});return a;});
    const selectionBox=new THREE.Box3Helper(new THREE.Box3(),'#a2efe4');selectionBox.visible=false;scene.add(selectionBox);extraGeometry.add(selectionBox.geometry);extraMaterial.add(selectionBox.material);
    let cursor=meta.start,last=performance.now(),renderClock=last,lastInteraction=last,interacting=false,uiTime=last,statsTime=last,statsCount=0,poseStamp='',visibleIDs=[],lowFrames=0;
    const cameraState=()=>({position:camera.position.clone().add(center).toArray(),target:controls.target.clone().add(center).toArray(),up:camera.up.toArray(),selected_node_id:options().selected||null,time:cursor,units:sceneSpec?.units||'model units'});
    const cameraKey=`phaseforge.scene.camera.${config.runId||sceneSpec?.title||'scene'}`;
    const reportCamera=()=>{const value=cameraState();try{sessionStorage.setItem(cameraKey,JSON.stringify(value));}catch{}options().onCameraChange?.(value);};
    let savedCamera=null;try{savedCamera=JSON.parse(sessionStorage.getItem(cameraKey)||'null');}catch{}
    controls.addEventListener('start',()=>{interacting=true;lastInteraction=performance.now();});
    controls.addEventListener('change',()=>{lastInteraction=performance.now();});
    controls.addEventListener('end',()=>{interacting=false;lastInteraction=performance.now();reportCamera();});
    function fit(which='perspective',targetId){
      let focus=new THREE.Vector3(),extent=span;const object=graph?.objects.get(targetId);
      if(object){const box=new THREE.Box3().setFromObject(object);focus=box.getCenter(new THREE.Vector3());const d=box.getSize(new THREE.Vector3());extent=Math.max(d.x,d.y,d.z,span*.02);}
      else if(targetId){const entity=sampleFrames(frames,cursor).left?.entities.find(e=>e.id===targetId);if(entity){focus.set(...entity.position).sub(center);extent=Math.max(radiusFor(entity,fallbackRadius)*6,span*.06);}}
      controls.target.copy(focus);controls.enableRotate=true;camera.up.set(0,1,0);
      const distance=extent/(2*Math.tan(THREE.MathUtils.degToRad(camera.fov)/2))*1.36/Math.min(camera.aspect,1);
      const direction=which==='xy'?new THREE.Vector3(0,0,1):which==='xz'?new THREE.Vector3(0,1,.0001):which==='yz'?new THREE.Vector3(1,0,0):new THREE.Vector3(.72,.36,1).normalize();
      camera.position.copy(focus).addScaledVector(direction,distance);controls.update();reportCamera();
    }
    navigation=installCameraNavigation(THREE,{camera,controls,element:renderer.domElement,span,onChange:reportCamera,fit:()=>fit()});
    const resize=()=>{const w=Math.max(1,host.clientWidth),h=Math.max(1,host.clientHeight);renderer.setSize(w,h,false);camera.aspect=w/h;camera.updateProjectionMatrix();};
    observer=new ResizeObserver(resize);observer.observe(host);resize();fit(config.view);
    if(sceneSpec?.camera){camera.position.set(...sceneSpec.camera.position).sub(center);controls.target.set(...sceneSpec.camera.target).sub(center);controls.update();}
    const ray=new THREE.Raycaster(),pointer=new THREE.Vector2();let down=null;
    listen('pointerdown',e=>{down=[e.clientX,e.clientY];});
    listen('pointerup',e=>{if(!down||Math.hypot(e.clientX-down[0],e.clientY-down[1])>5)return;down=null;const r=renderer.domElement.getBoundingClientRect();pointer.set((e.clientX-r.left)/r.width*2-1,-(e.clientY-r.top)/r.height*2+1);ray.setFromCamera(pointer,camera);const hits=ray.intersectObjects([markers,...(graph?.pickables||[])],false);const hit=hits.find(h=>{let visible=true;for(let o=h.object;o;o=o.parent)visible=visible&&o.visible;return visible&&(options().cut>=1||h.point.x<=clipping.constant);});if(hit){const id=hit.object===markers?visibleIDs[hit.instanceId]:hit.object.userData.sceneNodeId;if(id)onSelect(id,{world_position:hit.point.clone().add(center).toArray(),atom:hit.object.userData.atomRecords?.[hit.instanceId]||null});}});
    listen('webglcontextlost',e=>{e.preventDefault();cancelAnimationFrame(raf);onStop();onError('Graphics memory was interrupted. Restore the viewer with lower detail to recover.');});
    const apply=t=>{
      const opts=options(),stamp=`${t}|${opts.interpolate}|${opts.selected}|${opts.trails}|${opts.vectors}|${opts.grid}|${opts.cut}`;if(stamp===poseStamp)return;poseStamp=stamp;
      ground.visible=opts.grid;clipping.constant=span*(opts.cut-.5)*1.5;renderer.clippingPlanes=opts.cut<1?[clipping]:[];
      const sample=sampleFrames(frames,t),left=sample.left,rightMap=frameMaps[Math.min(sample.index+(sample.right!==sample.left?1:0),frames.length-1)];
      if(left){
        const loose=left.entities.filter(e=>!mapped.has(e.id));visibleIDs=loose.map(e=>e.id);markers.count=loose.length;
        loose.forEach((e,i)=>{const p=opts.interpolate?interpolatePosition(e,rightMap?.get(e.id),sample.alpha):e.position,r=radiusFor(e,fallbackRadius);matrix.makeScale(r,r,r);matrix.setPosition(...p);markers.setMatrixAt(i,matrix);markers.setColorAt(i,color.set(opts.selected===e.id?'#e5e9aa':PALETTE[(idIndex.get(e.id)||0)%PALETTE.length]));});markers.instanceMatrix.needsUpdate=true;if(markers.instanceColor)markers.instanceColor.needsUpdate=true;
        for(const node of sceneSpec?.nodes||[]){if(!node.entity_id)continue;const object=graph.objects.get(node.id),entity=frameMaps[sample.index].get(node.entity_id);object.visible=!!entity;if(entity){const p=opts.interpolate?interpolatePosition(entity,rightMap?.get(entity.id),sample.alpha):entity.position;object.position.set(...p.map((v,i)=>v+node.position[i]));}}
        graph?.updateBonds();
      }
      traces.forEach(line=>{line.visible=opts.trails;line.geometry.setDrawRange(0,Math.max(0,sample.index)*2);});
      arrows.forEach((arrow,i)=>{const e=frameMaps[sample.index]?.get(meta.ids[i]),length=e?.velocity?Math.hypot(...e.velocity):0;arrow.visible=!!(opts.vectors&&length>0);if(arrow.visible){arrow.position.set(...e.position);arrow.setDirection(new THREE.Vector3(...e.velocity).normalize());arrow.setLength(length*vectorScale,span*.014,span*.007);}});
      world.updateMatrixWorld(true);const target=graph?.objects.get(opts.selected);selectionBox.visible=!!target&&target.visible;if(target)selectionBox.box.setFromObject(target);
    };
    function capture(){renderer.render(scene,camera);const a=document.createElement('a');a.download=`phaseforge-${config.runId||'scene'}.png`;a.href=renderer.domElement.toDataURL('image/png');a.click();}
    function command(value){if(value.view)fit(value.view,value.node_id);if(Array.isArray(value.position)&&value.position.length===3&&value.position.every(Number.isFinite))camera.position.set(...value.position).sub(center);if(Array.isArray(value.target)&&value.target.length===3&&value.target.every(Number.isFinite))controls.target.set(...value.target).sub(center);if(Array.isArray(value.up)&&value.up.length===3&&value.up.every(Number.isFinite))camera.up.set(...value.up).normalize();if(value.node_id)onSelect(value.node_id);navigation.command(value);controls.update();reportCamera();}
    const api={dispose,seek:t=>{cursor=Math.max(meta.start,Math.min(meta.end,t));onTime(cursor);apply(cursor);reportCamera();},view:fit,fit,capture,cameraState,command,navigate:navigation.command,setCamera:command,vectorScale};
    apply(cursor);if(savedCamera)command(savedCamera);if(config.cameraCommand)command(config.cameraCommand);
    const draw=now=>{
      if(disposed)return;
      raf=requestAnimationFrame(draw);
      if(document.hidden){last=now;renderClock=now;return;}
      // Rendering never follows an uncapped high-refresh display. Playback uses
      // real elapsed time; lowering idle graphics work cannot slow numerical time.
      const active=options().playing||interacting||now-lastInteraction<700,interval=1000/(active?60:15),elapsed=now-renderClock;
      if(elapsed<interval)return;
      renderClock=now-elapsed%interval;
      const dt=Math.max(0,(now-last)/1000);last=now;
      if(options().playing&&meta.end>meta.start){cursor+=dt*(meta.end-meta.start)/Math.max(.1,Number(options().playbackDurationSeconds)||30)*options().speed;if(cursor>=meta.end){cursor=meta.end;onStop();}}
      apply(cursor);controls.update();renderer.render(scene,camera);statsCount++;
      if(now-uiTime>100){onTime(cursor);uiTime=now;}
      if(now-statsTime>1800){const fps=Math.round(statsCount*1000/(now-statsTime));lowFrames=active&&fps<24?lowFrames+1:0;if(lowFrames>=2&&pixelRatio>1){pixelRatio=1;renderer.setPixelRatio(pixelRatio);resize();lowFrames=0;}onStats({fps,idle:!active,triangles:renderer.info.render.triangles,drawCalls:renderer.info.render.calls,pixelRatio});statsCount=0;statsTime=now;}
    };raf=requestAnimationFrame(draw);return api;
  }catch(error){dispose();throw error;}
}
