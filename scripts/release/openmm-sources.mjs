import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {ROOT,readJSON,sha256,writeJSON} from './common.mjs';

export const OPENMM_SOURCE=Object.freeze({
  name:'openmm-8.5.2-36a30cbca54e727b216b606f3c011b67201eb8b4.zip',
  bytes:23152521,
  sha256:'c6c604a769d6ceced546cb2d0d8af36a784a0506d51495023170d9fe3c6ccf1f',
  commit:'36a30cbca54e727b216b606f3c011b67201eb8b4',
  url:'https://codeload.github.com/openmm/openmm/zip/36a30cbca54e727b216b606f3c011b67201eb8b4',
});
export const OPENMM_NOTICES=Object.freeze(['GPL.txt','LGPL.txt','Licenses.txt','XTC-NOTICES.txt','provenance.json','README.md','REBUILD.md']);
export const OPENMM_RESOURCE='third-party-sources/openmm';
export const stagedOpenmmSources=path.join(ROOT,'desktop/runtime',OPENMM_RESOURCE);
export const openmmNoticeRoot=path.join(ROOT,'tools/third-party/openmm-8.5.2');

function unlinked(file){
  let current=path.resolve(file);
  while(true){
    if(fs.existsSync(current)){
      const stat=fs.lstatSync(current);
      if(stat.isSymbolicLink()||(stat.isFile()&&stat.nlink!==1))throw Error('Linked source-delivery paths are refused');
    }
    const parent=path.dirname(current);if(parent===current)break;current=parent;
  }
}
export async function verifySourceFile(file,pin){
  unlinked(file);const stat=fs.lstatSync(file);
  if(!stat.isFile()||stat.size!==pin.bytes||await sha256(file)!==pin.sha256)throw Error(`Source-delivery file differs: ${path.basename(file)}`);
  return {bytes:pin.bytes,sha256:pin.sha256};
}
async function noticeRows(noticeRoot){
  unlinked(noticeRoot);
  const observed=fs.readdirSync(noticeRoot).sort();
  if(JSON.stringify(observed)!==JSON.stringify([...OPENMM_NOTICES].sort()))throw Error('OpenMM notice directory has missing or unexpected files');
  const provenance=readJSON(path.join(noticeRoot,'provenance.json'));
  if(provenance.schema!=='phaseforge.openmm-notices.v1'||provenance.commit!==OPENMM_SOURCE.commit||provenance.source.sha256!==OPENMM_SOURCE.sha256||provenance.source.bytes!==OPENMM_SOURCE.bytes)throw Error('OpenMM source and notice provenance differ');
  const rows=[];
  for(const name of OPENMM_NOTICES){
    const file=path.join(noticeRoot,name);unlinked(file);const stat=fs.lstatSync(file);
    if(!stat.isFile()||stat.size<1||stat.size>1024*1024)throw Error('Invalid OpenMM notice member');
    const pin=provenance.notice_files.find(row=>row.path===name);
    if(pin)await verifySourceFile(file,pin);
    rows.push({path:name,bytes:stat.size,sha256:await sha256(file)});
  }
  if(provenance.notice_files.length!==4)throw Error('Expected all four pinned OpenMM notice texts');
  return rows;
}
async function cachedArchive(cacheRoot,allowDownload){
  unlinked(cacheRoot);fs.mkdirSync(cacheRoot,{recursive:true});
  const target=path.join(cacheRoot,OPENMM_SOURCE.name);
  if(!fs.existsSync(target)){
    if(!allowDownload)throw Error('Pinned OpenMM corresponding source ZIP is absent from the offline cache');
    const response=await fetch(OPENMM_SOURCE.url,{redirect:'error',signal:AbortSignal.timeout(60000)});
    if(!response.ok||response.url!==OPENMM_SOURCE.url||!response.body)throw Error('Failed to download the exact upstream OpenMM source');
    const temporary=path.join(cacheRoot,`.openmm-download-${crypto.randomUUID()}.tmp`);
    const fd=fs.openSync(temporary,'wx');let bytes=0;
    try{
      for await(const chunk of response.body){bytes+=chunk.length;if(bytes>OPENMM_SOURCE.bytes)throw Error('OpenMM source download exceeds its exact pinned byte bound');fs.writeFileSync(fd,chunk);}
      fs.fsyncSync(fd);
    }finally{fs.closeSync(fd);}
    await verifySourceFile(temporary,OPENMM_SOURCE);
    if(fs.existsSync(target))await verifySourceFile(target,OPENMM_SOURCE);
    else fs.renameSync(temporary,target);
  }
  await verifySourceFile(target,OPENMM_SOURCE);return target;
}
function indexFor(rows){return {
  schema:'phaseforge.openmm-source-delivery.v1',resource_directory:OPENMM_RESOURCE,
  archive:{path:OPENMM_SOURCE.name,bytes:OPENMM_SOURCE.bytes,sha256:OPENMM_SOURCE.sha256,upstream_commit:OPENMM_SOURCE.commit,upstream_url:OPENMM_SOURCE.url},
  notices:rows,outside_asar:true,source_executed:false,source_rebuilt:false,
  scope:'Exact corresponding OpenMM source archive and accompanying notices delivered as ordinary installer resources. Archive SHA verification does not imply a recursive secret scan, vulnerability verdict, source rebuild, or complete application-source compliance review.',
  inner_source_secret_scan:'not performed by this packaging verification',
};}
export async function verifyOpenmmSourceDelivery(directory,{noticeRoot=openmmNoticeRoot}={}){
  unlinked(directory);const rows=await noticeRows(noticeRoot),expected=indexFor(rows);
  const actualNames=fs.readdirSync(directory).sort(),expectedNames=[OPENMM_SOURCE.name,...OPENMM_NOTICES,'delivery.json'].sort();
  if(JSON.stringify(actualNames)!==JSON.stringify(expectedNames))throw Error('OpenMM source delivery has missing or unexpected members');
  await verifySourceFile(path.join(directory,OPENMM_SOURCE.name),OPENMM_SOURCE);
  for(const row of rows)await verifySourceFile(path.join(directory,row.path),row);
  const index=path.join(directory,'delivery.json');unlinked(index);
  if(fs.statSync(index).size>64*1024||JSON.stringify(readJSON(index))!==JSON.stringify(expected))throw Error('OpenMM source delivery receipt differs from the source inputs');
  return {...expected,archive:{...expected.archive,path:`${OPENMM_RESOURCE}/${OPENMM_SOURCE.name}`},notices:rows.map(row=>({...row,path:`${OPENMM_RESOURCE}/${row.path}`})),receipt:{path:`${OPENMM_RESOURCE}/delivery.json`,sha256:await sha256(index)}};
}
export async function stageOpenmmSources({cacheRoot=process.env.PHASEFORGE_OPENMM_SOURCE_CACHE||path.join(ROOT,'.local/redistribution-sources'),destination=stagedOpenmmSources,noticeRoot=openmmNoticeRoot,allowDownload=true}={}){
  cacheRoot=path.resolve(cacheRoot);destination=path.resolve(destination);noticeRoot=path.resolve(noticeRoot);
  unlinked(destination);const archive=await cachedArchive(cacheRoot,allowDownload),rows=await noticeRows(noticeRoot);
  const parent=path.dirname(destination);fs.mkdirSync(parent,{recursive:true});unlinked(parent);
  const temporary=path.join(parent,`.openmm-unpublished-${crypto.randomUUID()}`);fs.mkdirSync(temporary);
  fs.copyFileSync(archive,path.join(temporary,OPENMM_SOURCE.name),fs.constants.COPYFILE_EXCL);
  for(const row of rows)fs.copyFileSync(path.join(noticeRoot,row.path),path.join(temporary,row.path),fs.constants.COPYFILE_EXCL);
  writeJSON(path.join(temporary,'delivery.json'),indexFor(rows));
  await verifyOpenmmSourceDelivery(temporary,{noticeRoot});
  await verifySourceFile(archive,OPENMM_SOURCE);
  if(fs.existsSync(destination)){
    const retained=path.join(parent,`.openmm-previous-${crypto.randomUUID()}`);
    fs.renameSync(destination,retained); // Preserve the previous staged resource, including failures.
  }
  fs.renameSync(temporary,destination);
  return verifyOpenmmSourceDelivery(destination,{noticeRoot});
}
