// Blender normalizes authored coordinates, then glTF exports Z-up as Y-up.
const vector=value=>Array.isArray(value)&&value.length===3&&value.every(Number.isFinite);
export function illustrationCoordinates(mapping){
  const center=mapping?.source_center,scale=mapping?.display_scale;
  if(!vector(center)||!Number.isFinite(scale)||scale<=0)return {toSource:v=>[...v],toAsset:v=>[...v],directionToSource:v=>[...v],directionToAsset:v=>[...v]};
  const directionToSource=([x,y,z])=>[x,-z,y],directionToAsset=([x,y,z])=>[x,z,-y];
  return {directionToSource,directionToAsset,toSource:value=>directionToSource(value).map((v,i)=>v/scale+center[i]),toAsset:value=>directionToAsset(value.map((v,i)=>(v-center[i])*scale))};
}
export function illustrationNodeId(object){for(let node=object;node;node=node.parent)if(node.userData?.phaseforge_node_id!=null)return String(node.userData.phaseforge_node_id);return null;}
