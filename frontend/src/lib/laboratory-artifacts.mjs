import {artifactPath} from './laboratory-plot.mjs';
export function registeredArtifacts(job){
  if(job?.state!=='completed'||!Array.isArray(job.result?.artifacts))return [];
  const seen=new Set();
  return job.result.artifacts.filter(item=>{
    if(!item||typeof item.path!=='string'||!/^[a-f0-9]{64}$/.test(item.sha256||'')||!Number.isSafeInteger(item.bytes)||item.bytes<0||seen.has(item.path))return false;
    try{artifactPath(job.id,item.path);}catch{return false;}seen.add(item.path);return true;
  });
}
export function outputArtifacts(job){
  const all=registeredArtifacts(job),reported=job.result?.reported_result?.artifacts;
  if(!Array.isArray(reported)||!reported.length)return all.filter(item=>!/(^|\/)(imports|input|source)(\/|\.)|\.(py|log)$/.test(item.path));
  return reported.flatMap(name=>{if(typeof name!=='string')return[];const found=all.find(item=>item.path===name||item.path===`work/${name}`);return found?[found]:[];});
}
export const previewableArtifact=item=>/\.(png|jpe?g|webp)$/i.test(item?.path||'');
export async function verifyArtifactImage(bytes,artifact,subtle=globalThis.crypto?.subtle){
  if(!previewableArtifact(artifact)||bytes.byteLength!==artifact.bytes||bytes.byteLength>32*1024*1024||!subtle)throw Error('The image does not match its registered file size or format.');
  const hash=Array.from(new Uint8Array(await subtle.digest('SHA-256',bytes)),byte=>byte.toString(16).padStart(2,'0')).join('');
  if(hash!==artifact.sha256)throw Error('The image does not match its completed output receipt.');
  return /\.png$/i.test(artifact.path)?'image/png':/\.webp$/i.test(artifact.path)?'image/webp':'image/jpeg';
}
