import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';

export const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../..');
export const REPOSITORY='delores88/PhaseForge';
export const WORKFLOW=`${REPOSITORY}/.github/workflows/marketplace-candidates.yml`;
export const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
export function readJSON(file){return JSON.parse(fs.readFileSync(file,'utf8').replace(/^\uFEFF/,''));}
export function writeJSON(file,value){fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,JSON.stringify(value,null,2)+'\n');}
export function command(exe,args,options={}){return execFileSync(exe,args,{cwd:ROOT,encoding:'utf8',timeout:30000,windowsHide:true,maxBuffer:32*1024*1024,...options}).trim();}
export function inside(root,value,{existing=false}={}){
  const boundary=path.resolve(root),target=path.resolve(value),relative=path.relative(boundary,target);
  if(!relative||relative==='..'||relative.startsWith(`..${path.sep}`)||path.isAbsolute(relative))throw Error('Path must remain strictly within its declared boundary');
  let current=target;
  while(current!==boundary){if(fs.existsSync(current)&&fs.lstatSync(current).isSymbolicLink())throw Error('Linked release paths are refused');current=path.dirname(current);}
  if(existing&&!fs.existsSync(target))throw Error(`Required path is absent: ${target}`);
  return target;
}
export async function sha256(file){const hash=crypto.createHash('sha256');for await(const chunk of fs.createReadStream(file))hash.update(chunk);return hash.digest('hex');}
export function copyEvidenceTree(source,destination,{filter}={}){
  if(fs.existsSync(destination))throw Error('Evidence copies must use a new destination');
  // AppImages include relative links. Preserve their meaning in the copied tree;
  // inventory() independently rejects links that resolve outside that tree.
  fs.cpSync(source,destination,{recursive:true,dereference:false,verbatimSymlinks:true,...(filter?{filter}:{})});
}
export function sourceIdentity(expected=process.env.GITHUB_SHA){
  const commit=command('git',['rev-parse','HEAD']);
  if(!/^[a-f0-9]{40}$/.test(expected||'')||commit!==expected)throw Error('The checkout must match the full expected source commit');
  const dirty=command('git',['status','--porcelain','--untracked-files=normal']);
  if(dirty)throw Error('Release evidence requires an unchanged clean source checkout');
  return {repository:REPOSITORY,commit,dirty:false};
}
export function releaseTarget(platform=process.platform,architecture=process.arch){
  const name=({win32:'windows',linux:'linux',darwin:'macos'})[platform];
  if(!name||(platform==='darwin'?architecture!=='arm64':architecture!=='x64'))throw Error('Native helper requires Windows/Linux x64 or macOS Apple Silicon arm64; the current release workflow builds Windows only');
  return {platform:name,architecture,folder:`${name}-${architecture}`};
}
export function hostedWorkspace(){
  if(process.env.GITHUB_ACTIONS!=='true'||process.env.RUNNER_ENVIRONMENT!=='github-hosted'||process.env.PHASEFORGE_RELEASE_ACCEPTANCE!=='1')throw Error('Native installation acceptance is restricted to an explicitly enabled GitHub-hosted disposable job');
  if(path.resolve(process.env.GITHUB_WORKSPACE||'.')!==ROOT||fs.realpathSync(ROOT)!==ROOT)throw Error('Hosted workspace identity does not match this checkout');
  releaseTarget();
  if(process.platform==='darwin'&&command('/usr/bin/uname',['-m'])!=='arm64')throw Error('Native macOS acceptance requires an actual Apple Silicon host, not Rosetta');
  sourceIdentity();
  return hostedTemporaryDirectory(process.env.RUNNER_TEMP);
}
// The caller owns this disposable root. Normalize its spelling before deriving
// application paths; the Rust file-handle checks still reject unexpected aliases.
export function hostedTemporaryDirectory(temporary){
  if(!temporary||!path.isAbsolute(temporary)||!fs.statSync(temporary).isDirectory())throw Error('Missing hosted temporary directory');
  return fs.realpathSync.native(temporary);
}
export function machine(file){
  const descriptor=fs.openSync(file,'r');try{
    const header=Buffer.alloc(4096),read=fs.readSync(descriptor,header,0,header.length,0);
    const magic=header.subarray(0,4).toString('hex');
    if(['cafebabe','bebafeca','cafebabf','bfbafeca'].includes(magic))return {format:'macho-universal',architecture:'universal'};
    if(['cffaedfe','feedfacf','cefaedfe','feedface'].includes(magic)){
      const little=magic==='cffaedfe'||magic==='cefaedfe',bits=magic==='cffaedfe'||magic==='feedfacf'?64:32;
      if(read<(bits===64?32:28))throw Error('Truncated Mach-O header');
      const cpu=little?header.readUInt32LE(4):header.readUInt32BE(4);
      return {format:'macho',architecture:({0x0100000c:'arm64',0x01000007:'x64',12:'arm',7:'x86'})[cpu]||'unknown',bits};
    }
    if(header.subarray(0,2).toString()==='MZ'){
      const offset=header.readUInt32LE(0x3c),pe=Buffer.alloc(6);fs.readSync(descriptor,pe,0,6,offset);
      if(pe.subarray(0,4).toString('binary')!=='PE\0\0')throw Error('Invalid PE header');
      return {format:'pe',architecture:({0x8664:'x64',0xaa64:'arm64',0x14c:'x86'})[pe.readUInt16LE(4)]||'unknown'};
    }
    if(header.subarray(0,4).equals(Buffer.from([0x7f,0x45,0x4c,0x46]))){
      const architecture=({62:'x64',183:'arm64'})[header[5]===1?header.readUInt16LE(18):header.readUInt16BE(18)]||'unknown';
      return {format:'elf',architecture,bits:header[4]===2?64:32};
    }
    throw Error(`Unrecognized native executable: ${path.basename(file)}`);
  }finally{fs.closeSync(descriptor);}
}
export async function inventory(directory){
  // The containing path can itself be an OS alias (macOS /var -> /private/var).
  // Compare link destinations with the same canonical boundary we traverse.
  const root=fs.realpathSync(directory),records=[],pending=[root];let total=0;
  while(pending.length){for(const entry of fs.readdirSync(pending.pop(),{withFileTypes:true})){
    const file=path.join(entry.parentPath||entry.path,entry.name),relative=path.relative(root,file).replaceAll(path.sep,'/');
    if(entry.isSymbolicLink()){const target=fs.readlinkSync(file);if(!fs.realpathSync(file).startsWith(root+path.sep))throw Error(`Payload link escapes root: ${relative}`);records.push({path:relative,link:target});}
    else if(entry.isDirectory())pending.push(file);
    else if(entry.isFile()){const bytes=fs.statSync(file).size;total+=bytes;records.push({path:relative,bytes,sha256:await sha256(file)});}
    if(records.length>100000||total>4*1024**3)throw Error('Release inventory exceeded its file/byte budget');
  }}
  return {files:records.sort((a,b)=>a.path.localeCompare(b.path)),bytes:total};
}
