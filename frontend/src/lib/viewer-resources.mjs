// Explicit context release prevents detached viewers from occupying the browser's
// limited WebGL context slots until an unpredictable garbage-collection cycle.
const releasedRenderers=new WeakSet();
export function releaseRenderer(renderer) {
  if(!renderer||releasedRenderers.has(renderer))return;
  releasedRenderers.add(renderer);
  try {renderer.dispose();}
  finally {try {renderer.forceContextLoss?.();} finally {renderer.domElement?.remove();}}
}

export function disposeObjectTree(root) {
  const geometries=new Set(),materials=new Set(),textures=new Set(),skeletons=new Set();
  root?.traverse(object=>{
    // Three.js releases instanceMatrix/instanceColor on the object's dispose
    // event, independently of the shared geometry's disposal.
    if(object.isInstancedMesh)object.dispose();
    object.shadow?.dispose?.();
    if(object.skeleton)skeletons.add(object.skeleton);
    if(object.geometry)geometries.add(object.geometry);
    for(const material of object.material?(Array.isArray(object.material)?object.material:[object.material]):[]){
      materials.add(material);for(const value of Object.values(material))if(value?.isTexture)textures.add(value);
    }
  });
  skeletons.forEach(skeleton=>skeleton.dispose());
  geometries.forEach(geometry=>geometry.dispose());materials.forEach(material=>material.dispose());
  textures.forEach(texture=>{texture.source?.data?.close?.();texture.dispose();});
}
