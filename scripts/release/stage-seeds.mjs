import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {inside,sha256} from './common.mjs';

const manifestName='phaseforge-runtime-seed.json';
function unlinked(file){
  let current=path.resolve(file);
  while(true){const info=fs.lstatSync(current);if(info.isSymbolicLink()||(info.isFile()&&info.nlink!==1))throw Error(`Linked runtime seed path refused: ${current}`);const parent=path.dirname(current);if(parent===current)break;current=parent;}
}
export async function verifySeed(directory,frozen){
  unlinked(directory);unlinked(frozen);
  const manifestPath=path.join(directory,manifestName);unlinked(manifestPath);
  if(await sha256(manifestPath)!==await sha256(frozen))throw Error('Runtime seed manifest differs from frozen source');
  const manifest=JSON.parse(fs.readFileSync(frozen,'utf8')),observed=new Set();let bytes=0;
  const pending=[directory];
  while(pending.length){for(const entry of fs.readdirSync(pending.pop(),{withFileTypes:true})){
    const file=path.join(entry.parentPath||entry.path,entry.name);unlinked(file);
    if(entry.isDirectory()){pending.push(file);continue;}
    if(!entry.isFile())throw Error('Runtime seed contains a non-regular file');
    const relative=path.relative(directory,file).replaceAll(path.sep,'/');if(relative===manifestName)continue;
    const size=fs.statSync(file).size;bytes+=size;observed.add(relative);
    if(observed.size>10000||bytes>512*1024**2)throw Error('Runtime seed exceeds bounded inventory');
    if(manifest.files[relative]!==await sha256(file)||manifest.file_bytes[relative]!==size)throw Error(`Runtime seed file differs: ${relative}`);
  }}
  if(observed.size!==Object.keys(manifest.files).length)throw Error('Runtime seed files are missing');
  return {manifest_sha256:await sha256(frozen),files:observed.size,bytes};
}

export async function stageSeeds({sourceRoot,destination,manifestRoot}){
  const boundary=path.resolve(destination);fs.mkdirSync(boundary,{recursive:true});unlinked(boundary);
  const target=inside(boundary,path.join(boundary,'runtime-seeds'));
  const staging=inside(boundary,path.join(boundary,`.runtime-seeds-${crypto.randomUUID()}`));
  const backup=inside(boundary,path.join(boundary,`.runtime-seeds-backup-${crypto.randomUUID()}`));
  const seeds={};
  // Verify all source inputs before creating or replacing any staged runtime.
  for(const kind of ['science-v2','python-numpy-v2'])seeds[kind]=await verifySeed(path.resolve(sourceRoot,kind),path.join(manifestRoot,`${kind}.manifest.json`));
  fs.mkdirSync(staging);
  try{
    for(const kind of Object.keys(seeds)){
      fs.cpSync(path.resolve(sourceRoot,kind),path.join(staging,kind),{recursive:true,errorOnExist:true,force:false,dereference:false});
      await verifySeed(path.join(staging,kind),path.join(manifestRoot,`${kind}.manifest.json`));
    }
    if(fs.existsSync(target)){unlinked(target);fs.renameSync(target,backup);}
    try{fs.renameSync(staging,target);}catch(error){if(fs.existsSync(backup))fs.renameSync(backup,target);throw error;}
    if(fs.existsSync(backup))fs.rmSync(inside(boundary,backup),{recursive:true});
    return seeds;
  }finally{if(fs.existsSync(staging))fs.rmSync(inside(boundary,staging),{recursive:true});}
}
