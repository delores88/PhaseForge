import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {inside} from './common.mjs';

const MAX_FILE_BYTES=64*1024**2,MAX_TOTAL_BYTES=512*1024**2,MAX_RECORDS=100000;
const digest=value=>crypto.createHash('sha256').update(value).digest('hex');
const alphabet='ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
const order=(a,b)=>a<b?-1:a>b?1:0;

// A source entry alone does not prove inclusion: count only source-bearing v3
// mapping segments. Never resolve source paths or read anything from the build env.
function sourceReferences(map){
  if(typeof map.mappings!=='string')throw Error('Source map has no v3 mappings string');
  const counts=map.sources.map(()=>0);
  let source=0,line=0,column=0,name=0,generated=0,index=0;
  while(index<map.mappings.length){
    const separator=map.mappings[index];
    if(separator===';'||separator===','){if(separator===';')generated=0;index++;continue;}
    const fields=[];
    while(index<map.mappings.length&&!';,'.includes(map.mappings[index])){
      let value=0,factor=1,continuation;
      do{
        const digit=alphabet.indexOf(map.mappings[index++]??'!');
        if(digit<0||factor>2**45)throw Error('Invalid or oversized source-map VLQ');
        value+=(digit&31)*factor;factor*=32;continuation=digit&32;
      }while(continuation);
      fields.push(value%2?-Math.floor(value/2):Math.floor(value/2));
      if(fields.length>5)throw Error('Invalid source-map segment width');
    }
    if(![1,4,5].includes(fields.length))throw Error('Invalid source-map segment width');
    generated+=fields[0];
    if(generated<0)throw Error('Invalid generated source-map column');
    if(fields.length===1)continue;
    source+=fields[1];line+=fields[2];column+=fields[3];
    if(source<0||source>=counts.length||line<0||column<0)throw Error('Invalid source-map source location');
    if(fields.length===5){name+=fields[4];if(name<0||name>=(map.names?.length??0))throw Error('Invalid source-map name reference');}
    counts[source]++;
  }
  return counts;
}

function modulesFromMap(map,depth=0){
  if(depth>16||!map||map.version!==3)throw Error('Unsupported source-map version or nesting');
  if(Array.isArray(map.sections)){
    if(map.sections.length>MAX_RECORDS)throw Error('Source-map section budget exceeded');
    const records=[];
    for(const section of map.sections){
      if(section.url!==undefined)throw Error('External source-map sections are unsupported');
      if(!Number.isInteger(section.offset?.line)||section.offset.line<0||!Number.isInteger(section.offset?.column)||section.offset.column<0)throw Error('Invalid source-map section offset');
      records.push(...modulesFromMap(section.map,depth+1));
      if(records.length>MAX_RECORDS)throw Error('Source-map module budget exceeded');
    }
    return records;
  }
  if(!Array.isArray(map.sources)||map.sources.length>MAX_RECORDS||map.sources.some(value=>typeof value!=='string'||value.length>16384))throw Error('Invalid source-map sources');
  if(map.sourceRoot!==undefined&&typeof map.sourceRoot!=='string')throw Error('Invalid source-map sourceRoot');
  if(map.sourcesContent!==undefined&&(!Array.isArray(map.sourcesContent)||map.sourcesContent.some(value=>value!==null&&typeof value!=='string')))throw Error('Invalid source-map sourcesContent');
  const counts=sourceReferences(map);
  return map.sources.map((source,index)=>{
    const content=map.sourcesContent?.[index];
    return {source,source_root:map.sourceRoot??null,sha256:typeof content==='string'?digest(content):null,bytes:typeof content==='string'?Buffer.byteLength(content):null,mapped_segments:counts[index]};
  });
}

function mapReference(bytes){
  // Next emits a final comment. Inspect only the bounded footer, not JS strings.
  const footer=bytes.subarray(Math.max(0,bytes.length-8192)).toString('utf8');
  return footer.match(/(?:^|\n)[\t ]*\/\/[#@][\t ]*sourceMappingURL=([^\s]+)[\t ]*\s*$/)?.[1]
    ??footer.match(/\/\*[#@][\t ]*sourceMappingURL=([^\s]+)[\t ]*\*\/[\t ]*\s*$/)?.[1]
    ??null;
}

/** Evidence from the actual exported JS/maps, invariant under payload relocation. */
export async function frontendModuleEvidence(uiRoot){
  const root=fs.realpathSync(uiRoot),files=[],pending=[root],bundles=[],gaps=[];
  let examined=0,totalBytes=0;
  while(pending.length){
    const directory=pending.pop();
    for(const entry of fs.readdirSync(directory,{withFileTypes:true})){
      if(++examined>MAX_RECORDS)throw Error('Frontend evidence file budget exceeded');
      const file=inside(root,path.join(directory,entry.name));
      if(entry.isSymbolicLink())throw Error('Linked frontend evidence files are refused');
      if(entry.isDirectory())pending.push(file);
      else if(entry.isFile()&&entry.name.endsWith('.js'))files.push(file);
    }
  }
  function read(file){
    const size=fs.statSync(file).size;
    if(size>MAX_FILE_BYTES||(totalBytes+=size)>MAX_TOTAL_BYTES)throw Error('Frontend evidence byte budget exceeded');
    return fs.readFileSync(file);
  }
  const relative=file=>path.relative(root,file).split(path.sep).join('/');
  for(const file of files.sort((a,b)=>order(relative(a),relative(b)))){
    const bytes=read(file),bundle={path:relative(file),sha256:digest(bytes),bytes:bytes.length,map:null,modules:[]};
    bundles.push(bundle);
    const reference=mapReference(bytes);
    if(!reference){gaps.push({bundle:bundle.path,reason:'No emitted sourceMappingURL; generated runtime/bootstrap code may be unmapped.'});continue;}
    if(/[\\?#\x00-\x20]/.test(reference)||reference.includes(':')||reference.startsWith('//'))throw Error('Nonlocal or ambiguous frontend source-map reference');
    const mapFile=inside(root,reference.startsWith('/')?path.join(root,reference.slice(1)):path.resolve(path.dirname(file),reference));
    if(!mapFile.endsWith('.map'))throw Error('Frontend source-map reference is not a .map file');
    if(!fs.existsSync(mapFile)){gaps.push({bundle:bundle.path,reason:'Referenced source map is absent from the shipped UI.'});continue;}
    const mapBytes=read(mapFile);
    bundle.map={path:relative(mapFile),sha256:digest(mapBytes),bytes:mapBytes.length};
    try{
      bundle.modules=modulesFromMap(JSON.parse(mapBytes.toString('utf8'))).sort((a,b)=>order(a.source,b.source)||order(a.source_root??'',b.source_root??'')||order(a.sha256??'',b.sha256??''));
    }catch(error){gaps.push({bundle:bundle.path,reason:`Map cannot provide module evidence: ${error.message}`});continue;}
    if(!bundle.modules.some(module=>module.mapped_segments>0))gaps.push({bundle:bundle.path,reason:'Source map has no source-bearing mapping segments.'});
    if(bundle.modules.some(module=>module.sha256===null))gaps.push({bundle:bundle.path,reason:'Some source records lack sourcesContent; their source hashes are unavailable.'});
  }
  const records=bundles.flatMap(bundle=>bundle.modules),mapped=records.filter(module=>module.mapped_segments>0);
  const mappedBundles=bundles.filter(bundle=>bundle.modules.some(module=>module.mapped_segments>0)).length;
  return {
    schema:'phaseforge.frontend-modules.v1',
    scope:'Actual shipped JavaScript and referenced source-map bytes, with UTF-8 sourcesContent hashes and source-bearing mapping counts. Source paths are map identities, never filesystem inputs. Mapped source content may include lines removed during compilation; this is not a complete dependency/version inventory or proof of execution. Unmapped wrappers, inline HTML scripts, CSS and WASM are outside module coverage.',
    bundles,
    summary:{bundles:bundles.length,mapped_bundles:mappedBundles,unmapped_bundles:bundles.length-mappedBundles,source_records:records.length,mapped_source_records:mapped.length,unique_mapped_sources:new Set(mapped.map(module=>JSON.stringify([module.source_root,module.source,module.sha256]))).size,missing_sources_content:records.filter(module=>module.sha256===null).length},
    gaps,
  };
}
