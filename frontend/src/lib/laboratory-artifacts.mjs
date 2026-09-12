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
  const declared=reported.flatMap(name=>{if(typeof name!=='string')return[];const found=all.find(item=>item.path===name||item.path===`work/${name}`);return found?[found]:[];});
  // A result cannot list itself in every producer's nested artifact list.
  // Offer its canonical registered bytes, never a synthesized reported_result.
  const canonical=all.find(item=>item.path==='work/result.json')||all.find(item=>item.path==='result.json');
  return [...new Map([...(canonical?[canonical]:[]),...declared].map(item=>[item.path,item])).values()];
}
export const previewableArtifact=item=>/\.(png|jpe?g|webp)$/i.test(item?.path||'');
export const JSON_PREVIEW_BYTES=1024*1024;
export const jsonArtifact=item=>/\.json$/i.test(item?.path||'');
export function numericalArtifact(job){
  const outputs=outputArtifacts(job).filter(jsonArtifact);
  return outputs.find(item=>item.path==='work/result.json')||outputs.find(item=>item.path==='result.json')||outputs.find(item=>/(^|\/)result\.json$/i.test(item.path))||outputs[0]||null;
}
/** Refuse oversized bodies while reading, even when Content-Length is missing. */
export async function boundedArtifactBytes(response,limit=JSON_PREVIEW_BYTES){
  if(Number(response.headers?.get('Content-Length'))>limit)throw Error('This result exceeds the JSON preview limit. Download the original file below.');
  if(!response.body?.getReader){const bytes=await response.arrayBuffer();if(bytes.byteLength>limit)throw Error('This result exceeds the JSON preview limit. Download the original file below.');return bytes;}
  const reader=response.body.getReader(),parts=[];let size=0;
  try{for(;;){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>limit){await reader.cancel();throw Error('This result exceeds the JSON preview limit. Download the original file below.');}parts.push(value);}}
  finally{reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const part of parts){bytes.set(part,offset);offset+=part.byteLength;}return bytes.buffer;
}
export async function verifyArtifactJson(bytes,artifact,subtle=globalThis.crypto?.subtle){
  if(!jsonArtifact(artifact)||!Number.isSafeInteger(artifact.bytes)||bytes.byteLength!==artifact.bytes||bytes.byteLength>JSON_PREVIEW_BYTES||!subtle||!/^[a-f0-9]{64}$/.test(artifact.sha256||''))throw Error('The JSON does not match its registered file size or format.');
  const hash=Array.from(new Uint8Array(await subtle.digest('SHA-256',bytes)),byte=>byte.toString(16).padStart(2,'0')).join('');
  if(hash!==artifact.sha256)throw Error('The JSON does not match its completed output receipt.');
  const text=new TextDecoder('utf-8',{fatal:true}).decode(bytes).replace(/^\uFEFF/,''),value=JSON.parse(text),stack=[[value,0]];let nodes=0;
  while(stack.length){const [entry,depth]=stack.pop();if(++nodes>50000||depth>32)throw Error('This result is too complex for an inline preview. Download the original file below.');if(typeof entry==='number'&&!Number.isFinite(entry))throw Error('The JSON contains a non-finite numerical value.');if(entry&&typeof entry==='object')for(const child of Object.values(entry)){if(nodes+stack.length>=50000)throw Error('This result is too complex for an inline preview. Download the original file below.');stack.push([child,depth+1]);}}
  // Show the original numeric tokens: JSON.stringify would round large Python
  // integers through JavaScript's Number representation before displaying them.
  const limit=65536;return {text:text.slice(0,limit),truncated:text.length>limit};
}
export async function verifyArtifactImage(bytes,artifact,subtle=globalThis.crypto?.subtle){
  if(!previewableArtifact(artifact)||bytes.byteLength!==artifact.bytes||bytes.byteLength>32*1024*1024||!subtle)throw Error('The image does not match its registered file size or format.');
  const hash=Array.from(new Uint8Array(await subtle.digest('SHA-256',bytes)),byte=>byte.toString(16).padStart(2,'0')).join('');
  if(hash!==artifact.sha256)throw Error('The image does not match its completed output receipt.');
  return /\.png$/i.test(artifact.path)?'image/png':/\.webp$/i.test(artifact.path)?'image/webp':'image/jpeg';
}
