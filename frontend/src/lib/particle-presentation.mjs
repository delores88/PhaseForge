/** Presentation units and framing come from retained records, never a guessed physical scale. */
export function particleUnits(topology={},index={}) {
  return {position:topology.position_unit||index.position_unit||topology.units?.position||index.units?.position||'position units',time:index.time_unit||index.units?.time||topology.units?.time||'time units',mass:topology.units?.mass||index.units?.mass||'mass units'};
}
export function periodicParticles(topology={},index={}) {
  if(index.boundary==='isolated'||topology.boundary==='isolated') {
    if(index.wrapping?.startsWith('periodic')||topology.box_nm||topology.box)throw Error('An isolated trajectory cannot declare a periodic box or wrapping.');
    return false;
  }
  return !!(index.wrapping?.startsWith('periodic')||index.boundary==='periodic'||topology.boundary==='periodic');
}
export function particleRadius(entity,span=1) {
  const value=entity?.display_radius??entity?.radius_nm??entity?.radius??span*.016;
  if(!Number.isFinite(value)||value<=0)throw Error('A particle display radius must be finite and positive.');
  return value;
}
export function normalizedParticlePosition(position,center,scale) {
  // Subtract in float64 before values reach Three's float32 instance buffer.
  return position.map((value,axis)=>(value-center[axis])*scale);
}
export function mergeRecordedBounds(bounds) {
  if(!bounds.length)return null;
  const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];
  for(const value of bounds){if(!value||!Array.isArray(value.min)||!Array.isArray(value.max)||value.min.length!==3||value.max.length!==3||value.min.some((v,i)=>!Number.isFinite(v)||!Number.isFinite(value.max[i])||v>value.max[i]))throw Error('Retained trajectory bounds are invalid.');for(let axis=0;axis<3;axis++){min[axis]=Math.min(min[axis],value.min[axis]);max[axis]=Math.max(max[axis],value.max[axis]);}}
  return {min,max};
}
export function chunkBounds(index) {
  return index.chunks?.length&&index.chunks.every(chunk=>chunk.bounds)?mergeRecordedBounds(index.chunks.map(chunk=>chunk.bounds)):null;
}
export async function retainedParticleBounds(topology,source) {
  if(periodicParticles(topology,source.index)){const box=topology.box_nm||topology.box;if(!Array.isArray(box)||box.length!==3||!box.every(v=>Number.isFinite(v)&&v>0))throw Error('Periodic trajectories require a positive physical cell.');return {min:[0,0,0],max:[...box]};}
  const registered=chunkBounds(source.index);if(registered)return registered;
  // Compatibility with prior particle producers: serial reads preserve the
  // store's bounded cache. Framing covers every retained state, not just t=0.
  let bounds=null;
  for(let i=0;i<source.index.chunks.length;i++)for(const frame of await source.chunk(i))for(const entity of frame.entities)bounds=mergeRecordedBounds([...(bounds?[bounds]:[]),{min:entity.position,max:entity.position}]);
  return bounds||{min:[-1,-1,-1],max:[1,1,1]};
}
