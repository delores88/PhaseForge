import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {execFileSync,spawnSync} from 'node:child_process';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {machine} from './release/common.mjs';
import {auditableDependencies,auditableSection} from './release/auditable.mjs';
import {stageSeeds} from './release/stage-seeds.mjs';
import {stageOpenmmSources} from './release/openmm-sources.mjs';
import {verifyMsvcNotices} from './release/msvc-notices.mjs';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const arch=process.arch,os=process.platform==='win32'?'win':process.platform==='darwin'?'mac':process.platform;
if(!['win','linux','mac'].includes(os)||!['x64','arm64'].includes(arch)||(os==='mac'&&arch!=='arm64'))throw Error('Build natively on Windows/Linux x64 or ARM64, or Apple Silicon macOS. Cross-labelled native binaries are not accepted.');
if(os==='win'&&arch!=='x64')throw Error('This Windows scientific release bundles amd64 Python wheels and supports Windows x64 only.');
const binary=path.join(root,'backend','target','release',os==='win'?'phaseforge-backend.exe':'phaseforge-backend');
const identity=machine(binary);assert.equal(identity.architecture,arch,'Native backend architecture differs from its stage target');assert.equal(identity.format,({win:'pe',linux:'elf',mac:'macho'})[os]);
const version=JSON.parse(fs.readFileSync(path.join(root,'desktop/package.json'),'utf8')).version;
const actual=execFileSync(binary,['--version'],{encoding:'utf8'}).trim();
if(actual!==`PhaseForge ${version}`)throw Error(`Rebuild the native engine: expected ${version}, got ${actual}`);
if(!fs.existsSync(path.join(root,'frontend/out/index.html')))throw Error('Build the frontend before packaging.');
const dest=path.join(root,'desktop','runtime',os,arch);fs.mkdirSync(dest,{recursive:true});fs.copyFileSync(binary,path.join(dest,path.basename(binary)));
if(os==='win'){
  const sourceRoot=process.env.PHASEFORGE_RUNTIME_SEED_ROOT;
  if(!sourceRoot)throw Error('Set PHASEFORGE_RUNTIME_SEED_ROOT to the verified build_seeds.py output before staging Windows. The scientific runtime must be bundled.');
  await stageSeeds({sourceRoot,destination:dest,manifestRoot:path.join(root,'tools/runtime-seeds')});
  await stageOpenmmSources();
  await verifyMsvcNotices();
}
if(os==='mac'){
  // Bind compiler output before signing, then preserve these staged signed bytes
  // with mac.signIgnore. Changing resource metadata after the outer seal is invalid.
  const staged=path.join(dest,path.basename(binary)),before=fs.readFileSync(binary),metadata=auditableDependencies(before),section=auditableSection(before),hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
  const expectedIgnore='/Contents/Resources/runtime/phaseforge-backend$';
  assert.ok([].concat(JSON.parse(fs.readFileSync(path.join(root,'desktop/package.json'),'utf8')).build.mac.signIgnore||[]).includes(expectedIgnore),'mac.signIgnore must preserve the already signed backend exactly');
  const observe=args=>{const result=spawnSync('/usr/bin/codesign',args,{encoding:'utf8',timeout:30000,maxBuffer:1024*1024});if(result.error)throw result.error;return {executable:'/usr/bin/codesign',args,status:result.status,signal:result.signal,stdout:result.stdout,stderr:result.stderr};};
  const sourceSignature=observe(['--display','--verbose=4',binary]);
  const signing=observe(['--force','--sign','-','--timestamp=none',staged]);assert.equal(signing.status,0,'Staged ad-hoc signing failed');
  const verification=observe(['--verify','--strict','--verbose=2',staged]),display=observe(['--display','--verbose=4',staged]);
  assert.equal(verification.status,0,'Staged backend code integrity verification failed');assert.equal(display.status,0);assert.match(display.stdout+display.stderr,/Signature=adhoc/);
  const after=fs.readFileSync(staged);assert.ok(section.equals(auditableSection(after)),'Signing changed compiler dependency section bytes');assert.deepEqual(auditableDependencies(after),metadata);assert.equal(hash(fs.readFileSync(binary)),hash(before),'Signing must not alter original compiler output');
  fs.writeFileSync(path.join(dest,'backend-signing.json'),JSON.stringify({schema:'phaseforge.backend-signing.v1',source_commit:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),platform:'darwin',architecture:arch,compiler_output_sha256:hash(before),staged_signed_sha256:hash(after),compiler_dependency_section_sha256:hash(section),compiler_dependency_section_unchanged:true,source_signature:sourceSignature,signing,verification,display,scope:'Only the staged backend copy is explicitly ad-hoc signed before materials are bound. Original compiler output is unchanged and may already contain linker-generated ad-hoc signing. No Developer ID identity, timestamp service or notarization is used.'},null,2)+'\n');
}
fs.writeFileSync(path.join(dest,'build.json'),JSON.stringify({version,platform:process.platform,architecture:arch,sourceCommit:execFileSync('git',['rev-parse','HEAD'],{cwd:root,encoding:'utf8'}).trim(),sourceDirty:!!execFileSync('git',['status','--porcelain'],{cwd:root,encoding:'utf8'}).trim(),builtAt:new Date().toISOString()},null,2)+'\n');
console.log(`Staged ${actual}, ${os}/${arch}`);
