import {interpolateRecordedPosition,playbackTimeStep} from './laboratory-trajectory.mjs';
import {installCameraNavigation} from './viewer-camera.mjs';
import {releaseRenderer,disposeObjectTree} from './viewer-resources.mjs';
import {particleUnits,periodicParticles,particleRadius,retainedParticleBounds,chunkBounds,normalizedParticlePosition} from './particle-presentation.mjs';
import {advanceRenderClock} from './viewer-cadence.mjs';

export async function createLaboratoryViewer(host,{topology,source,options,onTime,onStats,onSelect,onCamera,onLoading,onError,onStop,signal}){
  const [THREE,{OrbitControls}]=await Promise.all([import('three'),import('three/examples/jsm/controls/OrbitControls.js')]);
  if(signal.aborted)return null;
  const first=await source.sample(source.index.start);if(signal.aborted)return null;
  const retainedBounds=await retainedParticleBounds(topology,source);if(signal.aborted)return null;
  const scene=new THREE.Scene(),renderer=new THREE.WebGLRenderer({antialias:true,powerPreference:'high-performance'});
  renderer.outputColorSpace=THREE.SRGBColorSpace;renderer.toneMapping=THREE.ACESFilmicToneMapping;
  let pixelRatio=Math.min(devicePixelRatio||1,1.75);renderer.setPixelRatio(pixelRatio);host.replaceChildren(renderer.domElement);
  const box=topology.box_nm||topology.box,periodic=periodicParticles(topology,source.index),entities=topology.entities||[],units=particleUnits(topology,source.index);
  const bounds=new THREE.Box3(new THREE.Vector3(...retainedBounds.min),new THREE.Vector3(...retainedBounds.max));
  const center=bounds.getCenter(new THREE.Vector3()),size=bounds.getSize(new THREE.Vector3()),span=Math.max(size.x,size.y,size.z,1e-15),scale=4/span;
  const world=new THREE.Group();if(periodic){world.scale.setScalar(scale);world.position.copy(center).multiplyScalar(-scale);}scene.add(world);
  const camera=new THREE.PerspectiveCamera(38,1,.00001,10000),controls=new OrbitControls(camera,renderer.domElement);
  scene.add(new THREE.HemisphereLight('#cfddff','#182239',1.6));
  for(const [color,intensity,p] of [['#f0f5ff',3.4,[4,7,5]],['#63d5e9',2.5,[-5,2,-4]],['#ab84ef',1.2,[1,-3,2]]]){const light=new THREE.DirectionalLight(color,intensity);light.position.set(...p);scene.add(light);}
  const mesh=new THREE.InstancedMesh(new THREE.SphereGeometry(1,28,20),new THREE.MeshPhysicalMaterial({color:'#ffffff',roughness:.27,metalness:.12,clearcoat:.4,clearcoatRoughness:.3,transparent:true}),Math.max(entities.length,first.left.entities.length));
  let materialShader=null;
  mesh.material.onBeforeCompile=shader=>{shader.uniforms.displayContrast={value:Math.max(.5,Math.min(2,Number(options().contrast)||1))};shader.fragmentShader=`uniform float displayContrast;\n${shader.fragmentShader}`.replace('#include <colorspace_fragment>','#include <colorspace_fragment>\ngl_FragColor.rgb = clamp((gl_FragColor.rgb - 0.5) * displayContrast + 0.5, 0.0, 1.0);');materialShader=shader;};
  mesh.material.customProgramCacheKey=()=> 'phaseforge-laboratory-contrast-v1';
  mesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);mesh.frustumCulled=false;world.add(mesh);
  const outline=new THREE.LineSegments(new THREE.EdgesGeometry(new THREE.BoxGeometry(size.x,size.y,size.z)),new THREE.LineBasicMaterial({color:'#769cbe',transparent:true,opacity:.3}));outline.position.copy(center);world.add(outline);
  const grid=new THREE.GridHelper(8,24,'#344863','#1a283e');grid.position.y=-size.y*scale/2-.04;scene.add(grid);
  const halo=new THREE.Mesh(new THREE.SphereGeometry(1,28,18),new THREE.MeshBasicMaterial({color:'#ffdb84',wireframe:true,transparent:true,opacity:.65}));halo.visible=false;world.add(halo);
  const matrix=new THREE.Matrix4(),color=new THREE.Color(),byId=new Map(entities.map(e=>[String(e.id),e]));
  let current=first,cursor=first.time,disposed=false,raf=0,last=performance.now(),lastDraw=0,lastStats=last,framesRendered=0,totalFrameMs=0,frameTimes=[],lastUI=0,sampling=false,wantedTime=null,settingsStamp='',displayStamp='',visibleIds=[],down=null,lowPerformance=0,interactionUntil=0;
  const cameraState=()=>({position:camera.position.clone().divideScalar(scale).add(center).toArray(),target:controls.target.clone().divideScalar(scale).add(center).toArray(),up:camera.up.toArray(),fov:camera.fov,units:units.position});
  const reportCamera=()=>onCamera?.(cameraState());
  function fit(view='perspective',selection){
    const latest=!periodic&&chunkBounds(source.index),framing=latest?new THREE.Box3(new THREE.Vector3(...latest.min),new THREE.Vector3(...latest.max)):bounds,extentSize=framing.getSize(new THREE.Vector3());
    let target=framing.getCenter(new THREE.Vector3()),extent=Math.max(...extentSize.toArray(),span);const entity=current.left.entities.find(e=>String(e.id)===String(selection));
    if(entity){target.set(...entity.position);extent=Math.max(particleRadius(byId.get(String(entity.id)),span)*7,span*.12);}
    const radius=entity?extent*scale/2:extentSize.length()*scale/2+Math.max(...entities.map(e=>particleRadius(e,span)),0)*scale;
    const distance=radius/Math.sin(THREE.MathUtils.degToRad(camera.fov)/2)*1.1/Math.min(camera.aspect,1),direction=view==='front'?new THREE.Vector3(0,0,1):view==='top'?new THREE.Vector3(0,1,.0001):view==='side'?new THREE.Vector3(1,0,0):new THREE.Vector3(.8,.48,1).normalize();
    controls.target.copy(target.sub(center).multiplyScalar(scale));camera.up.set(0,1,0);camera.position.copy(controls.target).addScaledVector(direction,distance);controls.update();reportCamera();
  }
  function setCamera(value){if(!value)return;for(const key of ['position','target','up'])if(value[key]&&(!Array.isArray(value[key])||value[key].length!==3||!value[key].every(Number.isFinite)))return;
    if(value.position)camera.position.set(...value.position).sub(center).multiplyScalar(scale);if(value.target)controls.target.set(...value.target).sub(center).multiplyScalar(scale);if(value.up)camera.up.set(...value.up).normalize();if(Number.isFinite(value.fov)){camera.fov=Math.min(100,Math.max(5,value.fov));camera.updateProjectionMatrix();}controls.update();reportCamera();}
  const navigation=installCameraNavigation(THREE,{camera,controls,element:renderer.domElement,span:4,onChange:reportCamera,fit});
  controls.addEventListener('start',()=>{interactionUntil=performance.now()+1000;});controls.addEventListener('change',()=>{interactionUntil=performance.now()+700;});controls.addEventListener('end',reportCamera);
  const resize=()=>{const w=Math.max(1,host.clientWidth),h=Math.max(1,host.clientHeight);renderer.setSize(w,h,false);camera.aspect=w/h;camera.updateProjectionMatrix();};
  const resizeObserver=new ResizeObserver(resize);resizeObserver.observe(host);resize();fit();setCamera(options().camera);
  function apply(){
    const opts=options(),selected=new Set((opts.selectedIds||[]).map(String)),hidden=new Set((opts.hiddenIds||[]).map(String));
    const stamp=JSON.stringify([opts.color,opts.background,opts.exposure,opts.contrast,opts.opacity,opts.dimOthers,[...selected],[...hidden],opts.showBox,opts.showGrid,opts.interpolate]);
    if(stamp===settingsStamp&&displayStamp===`${current.frameIndex}:${opts.interpolate?current.alpha:0}`)return;
    settingsStamp=stamp;displayStamp=`${current.frameIndex}:${opts.interpolate?current.alpha:0}`;
    scene.background=new THREE.Color(opts.background||'#080f20');renderer.toneMappingExposure=Math.max(.15,Math.min(3,Number(opts.exposure)||1.1));
    if(materialShader)materialShader.uniforms.displayContrast.value=Math.max(.5,Math.min(2,Number(opts.contrast)||1));
    mesh.material.opacity=Math.max(.05,Math.min(1,Number(opts.opacity)||1));outline.visible=periodic&&opts.showBox!==false;grid.visible=opts.showGrid!==false;
    const right=new Map(current.right.entities.map(e=>[String(e.id),e]));visibleIds=[];halo.visible=false;
    for(const entity of current.left.entities){const id=String(entity.id);if(hidden.has(id))continue;const spec=byId.get(id),r=particleRadius(spec,span);
      const p=opts.interpolate?interpolateRecordedPosition(entity.position,right.get(id)?.position,current.alpha,box,periodic):entity.position;
      const renderedRadius=periodic?r:r*scale,renderedPosition=periodic?p:normalizedParticlePosition(p,center.toArray(),scale);
      matrix.makeScale(renderedRadius,renderedRadius,renderedRadius);matrix.setPosition(...renderedPosition);mesh.setMatrixAt(visibleIds.length,matrix);
      color.set(selected.has(id)?'#ffd275':opts.color||spec?.color||'#9f8ded');if(opts.dimOthers&&selected.size&&!selected.has(id))color.multiplyScalar(.18);mesh.setColorAt(visibleIds.length,color);visibleIds.push(id);
      if(selected.has(id)&&!halo.visible){halo.position.set(...renderedPosition);halo.scale.setScalar(renderedRadius*1.15);halo.visible=true;}
    }
    mesh.count=visibleIds.length;mesh.instanceMatrix.needsUpdate=true;if(mesh.instanceColor)mesh.instanceColor.needsUpdate=true;
  }
  async function sampleAt(time){wantedTime=time;if(sampling)return;sampling=true;let indicated=false;const loadingTimer=setTimeout(()=>{indicated=true;if(!disposed)onLoading?.(true);},90);
    try{while(wantedTime!==null&&!disposed){const requested=wantedTime;wantedTime=null;const sample=await source.sample(requested);if(disposed)return;current=sample;cursor=requested;apply();if(!options().playing)onTime?.({time:cursor,recordedTime:sample.left.time,frameIndex:sample.frameIndex,alpha:sample.alpha,entityCount:sample.left.entities.length});}}
    catch(error){if(!disposed&&error.name!=='AbortError'){onError?.(error.message);onStop?.();}}finally{sampling=false;clearTimeout(loadingTimer);if(indicated&&!disposed)onLoading?.(false);}
  }
  const raycaster=new THREE.Raycaster(),pointer=new THREE.Vector2();
  const pointerDown=e=>{down=[e.clientX,e.clientY];};
  const pointerUp=e=>{if(!down||Math.hypot(e.clientX-down[0],e.clientY-down[1])>5)return;down=null;const rect=renderer.domElement.getBoundingClientRect();pointer.set((e.clientX-rect.left)/rect.width*2-1,-(e.clientY-rect.top)/rect.height*2+1);raycaster.setFromCamera(pointer,camera);const hit=raycaster.intersectObject(mesh)[0];onSelect?.(hit?visibleIds[hit.instanceId]:null);};
  const contextLost=e=>{e.preventDefault();onStop?.();onError?.('The graphics context was interrupted. Reopen this view to recover; saved numerical data are intact.');};
  renderer.domElement.addEventListener('pointerdown',pointerDown);renderer.domElement.addEventListener('pointerup',pointerUp);renderer.domElement.addEventListener('webglcontextlost',contextLost);
  function capture(){apply();controls.update();renderer.render(scene,camera);return {dataUrl:renderer.domElement.toDataURL('image/png'),time:options().interpolate?cursor:current.left.time,requestedTime:cursor,timeUnit:source.index.time_unit,camera:cameraState(),presentation:{...options(),camera:cameraState()},frameIndex:current.frameIndex,interpolation:!!options().interpolate&&current.alpha>0,sourceFrames:[current.left.time,current.right.time],width:renderer.domElement.width,height:renderer.domElement.height};}
  const draw=now=>{if(disposed)return;raf=requestAnimationFrame(draw);if(document.hidden){last=now;return;}const opts=options(),active=opts.playing||now<interactionUntil,interval=1000/(active?60:15),clock=advanceRenderClock(now,lastDraw,interval);if(clock===null)return;
    lastDraw=clock;const elapsed=Math.min(.25,Math.max(0,(now-last)/1000));last=now;
    if(opts.playing&&!sampling){const next=Math.min(source.index.end,cursor+playbackTimeStep(elapsed,source.index.start,source.index.end,opts.playbackDurationSeconds||30,opts.speed||1));sampleAt(next);if(next>=source.index.end)onStop?.();}
    const start=performance.now();apply();controls.update();renderer.render(scene,camera);const frameMs=performance.now()-start;totalFrameMs+=frameMs;frameTimes.push(frameMs);framesRendered++;
    if(now-lastUI>250){onTime?.({time:cursor,recordedTime:current.left.time,frameIndex:current.frameIndex,alpha:current.alpha,entityCount:current.left.entities.length});lastUI=now;}
    if(now-lastStats>2000){const fps=framesRendered*1000/(now-lastStats);lowPerformance=active&&fps<28?lowPerformance+1:0;if(lowPerformance>=2&&pixelRatio>1){pixelRatio=Math.max(1,pixelRatio-.25);renderer.setPixelRatio(pixelRatio);resize();lowPerformance=0;}
      frameTimes.sort((a,b)=>a-b);onStats?.({fps:Math.round(fps),meanFrameMs:totalFrameMs/framesRendered,p95FrameMs:frameTimes[Math.floor(frameTimes.length*.95)]||0,pixelRatio,width:renderer.domElement.width,height:renderer.domElement.height,triangles:renderer.info.render.triangles,drawCalls:renderer.info.render.calls,geometries:renderer.info.memory.geometries,active});framesRendered=0;totalFrameMs=0;frameTimes=[];lastStats=now;}
  };
  function dispose(){if(disposed)return;disposed=true;signal.removeEventListener('abort',dispose);cancelAnimationFrame(raf);resizeObserver.disconnect();navigation.dispose();controls.dispose();renderer.domElement.removeEventListener('pointerdown',pointerDown);renderer.domElement.removeEventListener('pointerup',pointerUp);renderer.domElement.removeEventListener('webglcontextlost',contextLost);disposeObjectTree(scene);releaseRenderer(renderer);}
  signal.addEventListener('abort',dispose,{once:true});apply();raf=requestAnimationFrame(draw);
  return {dispose,capture,cameraState,setCamera,fit,navigate:navigation.command,seek:sampleAt,sample:()=>current,stats:()=>({width:renderer.domElement.width,height:renderer.domElement.height,pixelRatio}),refresh:()=>sampleAt(cursor)};
}
