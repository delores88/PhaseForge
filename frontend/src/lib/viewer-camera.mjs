/** Shared scale-aware camera operations. Coordinates are in the caller's displayed world. */
export function installCameraNavigation(THREE,{camera,controls,element,span=1,onChange,fit}){
  element.tabIndex=0;element.setAttribute('aria-label','3D view. Drag to orbit, Shift drag to pan, scroll to zoom. Arrow keys orbit; Shift arrows pan; plus and minus zoom; Home fits the scene.');
  element.style.touchAction='none';element.style.overscrollBehavior='contain';
  controls.enableDamping=true;controls.dampingFactor=.09;controls.screenSpacePanning=true;controls.zoomToCursor=true;
  controls.rotateSpeed=.65;controls.panSpeed=.8;controls.zoomSpeed=.7;
  controls.minDistance=Math.max(span*1e-5,1e-20);controls.maxDistance=span*250;
  // Excessive near/far ratios lose depth precision and can even black out Eevee
  // light clusters. Keep clipping tied to the current distance as users zoom.
  const updateClipping=()=>{const distance=camera.position.distanceTo(controls.target);camera.near=Math.max(span*1e-8,distance/10000,1e-24);camera.far=Math.max(span*100,distance*10);camera.updateProjectionMatrix();};
  controls.addEventListener('change',updateClipping);updateClipping();
  const changed=()=>{controls.update();onChange?.();};
  function command(value){
    const offset=camera.position.clone().sub(controls.target),distance=Math.max(offset.length(),span*1e-6);
    if(value.zoom){offset.multiplyScalar(Math.exp(value.zoom));offset.clampLength(controls.minDistance,controls.maxDistance);camera.position.copy(controls.target).add(offset);}
    if(value.pan){const right=new THREE.Vector3().setFromMatrixColumn(camera.matrix,0),up=new THREE.Vector3().setFromMatrixColumn(camera.matrix,1),move=right.multiplyScalar(value.pan[0]*distance).addScaledVector(up,value.pan[1]*distance);camera.position.add(move);controls.target.add(move);}
    if(value.orbit){const align=new THREE.Quaternion().setFromUnitVectors(camera.up,new THREE.Vector3(0,1,0)),spherical=new THREE.Spherical().setFromVector3(offset.applyQuaternion(align));spherical.theta+=value.orbit[0];spherical.phi+=value.orbit[1];spherical.makeSafe();camera.position.copy(controls.target).add(new THREE.Vector3().setFromSpherical(spherical).applyQuaternion(align.invert()));}
    if(value.roll){camera.up.applyAxisAngle(camera.getWorldDirection(new THREE.Vector3()),value.roll).normalize();}
    if(value.home)fit?.();changed();
  }
  const keydown=event=>{const actions={ArrowLeft:{orbit:[.12,0]},ArrowRight:{orbit:[-.12,0]},ArrowUp:{orbit:[0,-.12]},ArrowDown:{orbit:[0,.12]},'+':{zoom:-.12},'=':{zoom:-.12},'-':{zoom:.12},Home:{home:true}};let action=actions[event.key];if(event.shiftKey&&event.key.startsWith('Arrow'))action={pan:[event.key==='ArrowLeft'?-.08:event.key==='ArrowRight'?.08:0,event.key==='ArrowUp'?.08:event.key==='ArrowDown'?-.08:0]};if(action){event.preventDefault();event.stopPropagation();command(action);}};
  const wheel=event=>event.stopPropagation(),context=event=>event.preventDefault(),focus=()=>element.focus({preventScroll:true});
  element.addEventListener('keydown',keydown);element.addEventListener('wheel',wheel,{passive:true});element.addEventListener('contextmenu',context);element.addEventListener('pointerdown',focus);
  return {command,dispose(){controls.removeEventListener('change',updateClipping);element.removeEventListener('keydown',keydown);element.removeEventListener('wheel',wheel);element.removeEventListener('contextmenu',context);element.removeEventListener('pointerdown',focus);}};
}
