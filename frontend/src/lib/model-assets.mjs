/** Strict, data-only asset intake. Validate before invoking any Three.js loader. */
export const MODEL_LIMITS=Object.freeze({bytes:128*1024*1024,triangles:2_000_000,vertices:6_000_000,nodes:12000,images:16,texturePixels:32_000_000,textureSide:8192});
const fail=message=>{throw new Error(message);};
const finiteVector=v=>Array.isArray(v)&&v.every(n=>typeof n==='number'&&Number.isFinite(n)&&Math.abs(n)<=1e15);

function textureDimensions(bytes,mime) {
  const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  if(mime==='image/png'){
    if(bytes.length<24||view.getUint32(0)!==0x89504e47)fail('Invalid embedded PNG.');
    return [view.getUint32(16),view.getUint32(20)];
  }
  if(mime==='image/jpeg'){
    if(bytes.length<4||bytes[0]!==255||bytes[1]!==216)fail('Invalid embedded JPEG.');
    let offset=2;
    while(offset+4<bytes.length){if(bytes[offset]!==255){offset++;continue;}const marker=bytes[offset+1];if(marker===216||marker===217){offset+=2;continue;}const length=view.getUint16(offset+2);if(length<2||offset+2+length>bytes.length)break;if([192,193,194,195,197,198,199,201,202,203,205,206,207].includes(marker)&&length>=7)return [view.getUint16(offset+7),view.getUint16(offset+5)];offset+=2+length;}
    fail('Embedded JPEG dimensions could not be checked.');
  }
  fail('Embedded textures must be PNG or JPEG. Export other textures as embedded PNG/JPEG first.');
}

export function inspectGLB(buffer) {
  if(!(buffer instanceof ArrayBuffer)||buffer.byteLength<20||buffer.byteLength>MODEL_LIMITS.bytes)fail('GLB must be a complete file of at most 128 MiB.');
  const view=new DataView(buffer);
  if(view.getUint32(0,true)!==0x46546c67||view.getUint32(4,true)!==2||view.getUint32(8,true)!==buffer.byteLength)fail('Only complete GLB version 2 files are supported.');
  let offset=12,json=null,binary=null;
  while(offset+8<=buffer.byteLength){const length=view.getUint32(offset,true),type=view.getUint32(offset+4,true);offset+=8;if(length%4!==0||offset+length>buffer.byteLength)fail('The GLB contains an invalid chunk.');if(type===0x4e4f534a){if(json)fail('GLB contains multiple metadata chunks.');try{json=JSON.parse(new TextDecoder().decode(new Uint8Array(buffer,offset,length)).trim());}catch{fail('GLB metadata is not valid JSON.');}}else if(type===0x004e4942){if(binary)fail('GLB contains multiple binary buffers.');binary=new Uint8Array(buffer,offset,length);}offset+=length;}
  if(!json||offset!==buffer.byteLength||json.asset?.version!=='2.0')fail('GLB metadata is missing or unsupported.');
  const buffers=json.buffers||[],views=json.bufferViews||[],nodes=json.nodes||[],accessors=json.accessors||[];
  if(buffers.length>1||buffers.some(b=>b.uri!==undefined))fail('External or URI-based buffers are not allowed. Import a self-contained GLB.');
  if(buffers.length&&(!binary||buffers[0].byteLength>binary.byteLength))fail('GLB binary data are incomplete.');
  if(nodes.length>MODEL_LIMITS.nodes)fail('The model exceeds 12,000 scene nodes.');
  const unsupported=new Set(['KHR_draco_mesh_compression','EXT_meshopt_compression','KHR_texture_basisu']);
  if((json.extensionsUsed||[]).some(name=>unsupported.has(name)))fail('Compressed geometry or Basis textures require an uncompressed GLB export.');
  for(const node of nodes){for(const [key,length] of [['translation',3],['rotation',4],['scale',3],['matrix',16]])if(node[key]&&(!finiteVector(node[key])||node[key].length!==length))fail('A node contains invalid transform coordinates.');for(const child of node.children||[])if(!Number.isInteger(child)||child<0||child>=nodes.length)fail('A scene node references an unknown child.');}
  const visitedNodes=new Uint8Array(nodes.length);
  for(let start=0;start<nodes.length;start++){
    const stack=[[start,false,0]];
    while(stack.length){const [index,leaving,depth]=stack.pop();if(leaving){visitedNodes[index]=2;continue;}if(visitedNodes[index]===1)fail('Cyclic scene-node hierarchies are not supported.');if(visitedNodes[index]===2)continue;if(depth>256)fail('The scene hierarchy is too deeply nested.');visitedNodes[index]=1;stack.push([index,true,depth]);for(const child of nodes[index].children||[])stack.push([child,false,depth+1]);}
  }
  for(const v of views){if(v.buffer!==0||!Number.isInteger(v.byteLength)||v.byteLength<0||!Number.isInteger(v.byteOffset||0)||(v.byteOffset||0)<0||(v.byteOffset||0)+v.byteLength>(binary?.byteLength||0))fail('A buffer view points outside the embedded asset.');}
  let accessorItems=0,accessorBytes=0,triangles=0,vertices=0;
  for(const a of accessors){if(!Number.isInteger(a.count)||a.count<0||a.count>MODEL_LIMITS.vertices)fail('A geometry accessor exceeds the allocation budget.');const components=({SCALAR:1,VEC2:2,VEC3:3,VEC4:4,MAT2:4,MAT3:9,MAT4:16})[a.type],componentBytes=({5120:1,5121:1,5122:2,5123:2,5125:4,5126:4})[a.componentType];if(!components||!componentBytes)fail('Unsupported geometry accessor format.');accessorItems+=a.count;accessorBytes+=a.count*components*componentBytes;if(accessorItems>MODEL_LIMITS.vertices*12||accessorBytes>MODEL_LIMITS.bytes*3)fail('The model accessor allocation is too large.');}
  for(const mesh of json.meshes||[])for(const p of mesh.primitives||[]){const n=accessors[p.attributes?.POSITION]?.count||0,indices=accessors[p.indices]?.count||n;vertices+=n;triangles+=Math.ceil(indices/3);}
  if(vertices>MODEL_LIMITS.vertices||triangles>MODEL_LIMITS.triangles)fail('The model exceeds the 2-million-triangle / 6-million-vertex viewer budget.');
  const images=json.images||[];if(images.length>MODEL_LIMITS.images)fail('The model exceeds 16 embedded textures.');let pixels=0;
  for(const image of images){if(image.uri!==undefined||!Number.isInteger(image.bufferView)||!views[image.bufferView])fail('Textures must be embedded buffer views; external URLs and data URIs are not loaded.');const v=views[image.bufferView],bytes=binary.subarray(v.byteOffset||0,(v.byteOffset||0)+v.byteLength),[w,h]=textureDimensions(bytes,image.mimeType);if(!w||!h||w>MODEL_LIMITS.textureSide||h>MODEL_LIMITS.textureSide||(pixels+=w*h)>MODEL_LIMITS.texturePixels)fail('Embedded texture dimensions exceed the viewer memory budget.');}
  // Unknown extensions can carry URLs even when normal glTF buffers are embedded.
  const pending=[json];let visited=0;
  while(pending.length){const value=pending.pop();if(++visited>2_000_000)fail('Model metadata is too complex.');if(!value||typeof value!=='object')continue;for(const [key,child] of Object.entries(value)){if(key==='uri'||key==='url')fail('Asset URLs are not loaded. Use a fully embedded GLB export.');if(child&&typeof child==='object')pending.push(child);}}
  return {format:'glb',json,triangles,vertices,bytes:buffer.byteLength,textures:images.length};
}

export function inspectSTL(buffer) {
  if(!(buffer instanceof ArrayBuffer)||buffer.byteLength>MODEL_LIMITS.bytes)fail('STL files are limited to 128 MiB.');
  const view=new DataView(buffer);let triangles=0,binary=false;
  if(buffer.byteLength>=84){const count=view.getUint32(80,true);if(84+count*50===buffer.byteLength){triangles=count;binary=true;}}
  if(!binary){const source=new TextDecoder().decode(buffer);if(!/^\s*solid\b/i.test(source))fail('The file is neither binary nor ASCII STL.');triangles=(source.match(/\bfacet\s+normal\b/gi)||[]).length;let vertices=0;for(const match of source.matchAll(/\bvertex\s+([^\r\n]+)/gi)){const values=match[1].trim().split(/\s+/).map(Number);if(!finiteVector(values)||values.length!==3)fail('STL contains invalid vertex coordinates.');vertices++;}if(vertices!==triangles*3)fail('STL contains incomplete triangle records.');}
  if(!triangles||triangles>MODEL_LIMITS.triangles)fail('STL requires 1 to 2 million valid triangles.');
  if(binary)for(let triangle=0;triangle<triangles;triangle++)for(let i=0;i<12;i++){const value=view.getFloat32(84+triangle*50+i*4,true);if(!Number.isFinite(value)||Math.abs(value)>1e15)fail('STL contains invalid vertex or normal coordinates.');}
  return {format:'stl',triangles,vertices:triangles*3,bytes:buffer.byteLength};
}

export async function readModelAsset({url,file,signal}) {
  let buffer,name;
  if(file){if(file.size>MODEL_LIMITS.bytes)fail('Model files are limited to 128 MiB.');buffer=await file.arrayBuffer();name=file.name;}
  else if(url){
    const target=new URL(url,window.location.href);
    if(!['http:','https:'].includes(target.protocol)||(target.origin!==window.location.origin&&!['127.0.0.1','localhost','[::1]'].includes(target.hostname)))fail('Open a local GLB/STL file or a render from this workbench. Remote asset URLs are not loaded.');
    const response=await fetch(target.href,{signal});if(!response.ok)fail(`Model download failed (${response.status}).`);
    if(Number(response.headers.get('content-length'))>MODEL_LIMITS.bytes)fail('The model exceeds 128 MiB.');
    const reader=response.body?.getReader();if(!reader)fail('The model response could not be streamed.');
    const chunks=[];let length=0;
    try{while(true){const {done,value}=await reader.read();if(done)break;length+=value.byteLength;if(length>MODEL_LIMITS.bytes)fail('The model exceeds 128 MiB.');chunks.push(value);}}catch(error){await reader.cancel().catch(()=>{});throw error;}
    const joined=new Uint8Array(length);let at=0;for(const chunk of chunks){joined.set(chunk,at);at+=chunk.byteLength;}buffer=joined.buffer;name=target.pathname.split('/').at(-1);
  }else fail('Choose a model file or a completed render.');
  if(signal?.aborted)throw new DOMException('Cancelled','AbortError');
  const glb=buffer.byteLength>=4&&new DataView(buffer).getUint32(0,true)===0x46546c67;
  const inspection=glb?inspectGLB(buffer):inspectSTL(buffer);
  return {buffer,name,...inspection};
}
