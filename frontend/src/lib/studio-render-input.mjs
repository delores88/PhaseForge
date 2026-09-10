/** Resolve only the content currently selected in Studio. Never borrow a stale scene. */
const unavailable=(error,scene=null)=>({input:null,scene,error});
function sourceInput(request,label) {
  const scene=request?.scene,structureId=request?.structure_id,bindings=request?.bindings||[];
  if(!!scene===!!structureId)return unavailable(`${label} has no single recoverable scene or molecular source. Choose its original design or structure.`);
  if(!Array.isArray(bindings)||bindings.length>16)return unavailable(`${label} has invalid molecular source bindings.`);
  if(structureId&&bindings.length)return unavailable(`${label} mixes a standalone structure with scene bindings.`);
  if(scene&&(!Array.isArray(scene.nodes)||!scene.nodes.length))return unavailable(`${label} has no renderable scene geometry.`);
  if(bindings.some(binding=>!binding.structure_id||!binding.node_id||!scene?.nodes.some(node=>node.id===binding.node_id)))return unavailable(`${label} has a missing or incompatible molecular source binding.`,scene);
  const input=structureId?{structure_id:structureId}:{scene,bindings};
  if(request.camera!==undefined)input.camera=request.camera;
  return {input,scene:scene||null,error:''};
}

export function selectStudioRenderInput({projectId,job,jobType,jobSource,structure,design,assets=[],manifest,localFile,assetModel,assetImage,visibleAsset}) {
  if(!projectId)return unavailable('Select a research project first.');
  if(job) {
    if(job.project_id!==projectId)return unavailable('This saved job does not belong to the selected project.');
    if((job.type||jobType)!=='renders')return unavailable('The selected engineering artifact is not a Blender scene. Open a scene design or molecular structure to render.');
    if(job.settings?.scene||job.settings?.structure_id)return sourceInput(job.settings,'The selected render');
    if(jobSource?.jobId!==job.id)return unavailable('Loading the original source of this saved render…');
    if(jobSource.error)return unavailable(`Cannot recover this render's original input: ${jobSource.error} Choose its original design or structure.`);
    if(!jobSource.request)return unavailable('Loading the original source of this saved render…');
    if(jobSource.request?.project_id!==projectId)return unavailable('The saved render input does not belong to the selected project.');
    return sourceInput(jobSource.request,'The selected render');
  }
  if(localFile||assetModel||assetImage)return unavailable('The selected file is available for inspection. Open a scene design or molecular structure to render in Blender.');
  if(structure) {
    if(structure.project_id!==projectId||!structure.id)return unavailable('The selected molecular structure is missing or belongs to another project.');
    return sourceInput({structure_id:structure.id},'The selected structure');
  }
  if(visibleAsset)return unavailable(visibleAsset.molecule_id?'The selected molecular structure has not loaded. Reopen the source before rendering.':'The selected source contains data or imagery, not a Blender scene.');
  if(design) {
    if(design.project_id!==projectId)return unavailable('This design does not belong to the selected project.');
    if(design.design?.kind!=='scene')return unavailable('The selected CAD or PCB design uses Build engineering files. Open a scene design or molecular structure to render.');
    const bindings=design.design.asset_bindings||[],nodeBindings=bindings.filter(binding=>binding.node_id);
    const resolve=binding=>assets.find(asset=>asset.id===binding.asset_id&&asset.project_id===projectId)?.molecule_id;
    if(bindings.some(binding=>!resolve(binding)))return unavailable('A source bound to this design is missing from the project. Import it again before rendering.',design.design.scene);
    if(nodeBindings.length)return sourceInput({scene:design.design.scene,bindings:nodeBindings.map(binding=>({node_id:binding.node_id,structure_id:resolve(binding)}))},'The selected design');
    if(bindings.length===1)return sourceInput({structure_id:resolve(bindings[0])},'The selected design');
    if(bindings.length>1)return unavailable('This design has several standalone molecular sources. Select the structure to render.');
    return sourceInput({scene:design.design.scene},'The selected design');
  }
  if(manifest&&manifest.project_id!==projectId)return unavailable('The experiment scene does not belong to the selected project.');
  return sourceInput({scene:manifest?.visualization?.scene},'The experiment');
}

export const SAVED_RENDER_INPUT_LIMIT=32*1024*1024;
export async function loadSavedRenderRequest(url,{signal,fetcher=fetch}={}) {
  const response=await fetcher(url,{signal});
  if(!response.ok)throw Error(`Original input is unavailable (HTTP ${response.status}).`);
  if(Number(response.headers.get('content-length'))>SAVED_RENDER_INPUT_LIMIT)throw Error('Original input exceeds the 32 MiB inspection limit.');
  const reader=response.body?.getReader();if(!reader)throw Error('Original input cannot be read safely.');
  const decoder=new TextDecoder();const chunks=[];let bytes=0;
  try {
    while(true){const {done,value}=await reader.read();if(done)break;bytes+=value.byteLength;if(bytes>SAVED_RENDER_INPUT_LIMIT)throw Error('Original input exceeds the 32 MiB inspection limit.');chunks.push(decoder.decode(value,{stream:true}));}
    chunks.push(decoder.decode());
  } catch(error){await reader.cancel().catch(()=>{});throw error;}finally{reader.releaseLock();}
  const saved=JSON.parse(chunks.join(''));if(!saved.request||typeof saved.request!=='object')throw Error('Original input has no saved render request.');
  return saved.request;
}
