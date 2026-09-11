import fs from 'node:fs';
import path from 'node:path';
import {ROOT,inventory,machine,readJSON,releaseTarget,sha256,sourceIdentity,writeJSON} from './common.mjs';
import {auditableDependencies} from './auditable.mjs';
import {frontendModuleEvidence} from './frontend-map.mjs';
import {stagedOpenmmSources,verifyOpenmmSourceDelivery} from './openmm-sources.mjs';
import {verifyMsvcNotices} from './msvc-notices.mjs';

const source=sourceIdentity(),platform=process.platform==='win32'?'win':process.platform==='darwin'?'mac':'linux';
releaseTarget();
const runtime=path.join(ROOT,'desktop/runtime',platform,process.arch),backend=path.join(runtime,process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');
const metadata=readJSON(path.join(runtime,'build.json'));
if(metadata.sourceCommit!==source.commit||metadata.sourceDirty)throw Error('Staged runtime is not the exact clean source');
if(metadata.platform!==process.platform||metadata.architecture!==process.arch||machine(backend).architecture!==process.arch)throw Error('Staged backend target differs from the actual host');
const backendHash=await sha256(backend),dependencies=auditableDependencies(fs.readFileSync(backend));
if(process.platform==='win32'){
  writeJSON(path.join(runtime,'third-party-source-materials.json'),{source_commit:source.commit,backend_sha256:backendHash,...await verifyOpenmmSourceDelivery(stagedOpenmmSources)});
  writeJSON(path.join(runtime,'msvc-runtime-materials.json'),{source_commit:source.commit,backend_sha256:backendHash,...await verifyMsvcNotices()});
  const seeds={},rebuiltPath=path.join(process.env.PHASEFORGE_RUNTIME_SEED_ROOT||path.join(ROOT,'.local/release-seeds/ci'),'seed-build.json');
  const rebuilt=readJSON(rebuiltPath);
  if(rebuilt.source_commit!==source.commit||rebuilt.schema!=='phaseforge.runtime-seed-build.v1'||rebuilt.managed_executables_run!==false)throw Error('Missing exact-commit archive rebuild receipt');
  for(const kind of ['science-v3','python-numpy-v2']){
    const seed=path.join(runtime,'runtime-seeds',kind),manifestPath=path.join(seed,'phaseforge-runtime-seed.json');
    const frozen=path.join(ROOT,'tools/runtime-seeds',`${kind}.manifest.json`);
    if(await sha256(manifestPath)!==await sha256(frozen))throw Error(`Staged ${kind} differs from the frozen source manifest`);
    const manifest=readJSON(manifestPath),observed=await inventory(seed);
    const actual=Object.fromEntries(observed.files.filter(row=>row.path!=='phaseforge-runtime-seed.json').map(row=>[row.path,row]));
    if(Object.keys(actual).length!==Object.keys(manifest.files).length)throw Error(`Staged ${kind} file count differs from its manifest`);
    for(const [name,hash] of Object.entries(manifest.files)){
      const row=actual[name];if(!row||row.link||row.sha256!==hash||row.bytes!==manifest.file_bytes[name])throw Error(`Staged ${kind} file differs: ${name}`);
    }
    seeds[kind]={manifest_sha256:await sha256(manifestPath),files:Object.keys(actual).length,bytes:observed.bytes,native_files:manifest.native_files,sources:manifest.sources,transformations:manifest.transformations};
    if(rebuilt.seeds[kind].manifest_sha256!==seeds[kind].manifest_sha256||JSON.stringify(rebuilt.seeds[kind].sources)!==JSON.stringify(manifest.sources))throw Error('Archive rebuild and staged seed provenance differ');
  }
  fs.copyFileSync(rebuiltPath,path.join(runtime,'runtime-seed-build.json'));
  writeJSON(path.join(runtime,'runtime-seed-materials.json'),{schema:'phaseforge.runtime-seed-materials.v1',source_commit:source.commit,backend_sha256:backendHash,seeds,archive_rebuild_receipt:{path:'runtime-seed-build.json',sha256:await sha256(rebuiltPath)},scope:'Exact bundled seed files verified against committed manifests before packaging. Native and wheel members are retained; matching CPython source replaces its embedded standard-library bytecode. This is a byte inventory, not a complete dependency or vulnerability verdict.'});
}
if(process.platform==='darwin'){
  const signing=readJSON(path.join(runtime,'backend-signing.json'));
  if(signing.source_commit!==source.commit||signing.staged_signed_sha256!==backendHash||signing.compiler_dependency_section_unchanged!==true||signing.verification.status!==0)throw Error('Staged signed backend is not bound to its compiler-output receipt');
}
writeJSON(path.join(runtime,'backend.dependencies.json'),{schema:'phaseforge.compiled-dependencies.v1',source_commit:source.commit,binary_sha256:backendHash,origin:'Compiler-emitted cargo-auditable .dep-v0 section of this exact backend',dependencies});
fs.copyFileSync(path.join(ROOT,'backend/Cargo.lock'),path.join(runtime,'Cargo.lock'));
const inputs=path.join(runtime,'build-inputs');fs.mkdirSync(inputs,{recursive:true});
const files=['frontend/package.json','frontend/package-lock.json','desktop/package.json','desktop/package-lock.json'];
for(const file of files)fs.copyFileSync(path.join(ROOT,file),path.join(inputs,file.replaceAll('/','-')));
writeJSON(path.join(inputs,'SCOPE.json'),{source_commit:source.commit,scope:'Reviewed source/build inputs, including development dependencies. This is not a declaration that every locked package is loaded at runtime.',frontend_bundles:'Actual emitted files are independently hashed in frontend-bundles.json; the frontend is compiled into static JavaScript bundles.',backend_dependencies:'The adjacent backend.dependencies.json comes from compiler-emitted binary metadata; Cargo.lock retains the complete resolution, including non-runtime and other-target inputs.'});
writeJSON(path.join(runtime,'frontend-bundles.json'),{source_commit:source.commit,scope:'Actual packaged static frontend bytes',...await inventory(path.join(ROOT,'frontend/out'))});
writeJSON(path.join(runtime,'frontend-modules.json'),{source_commit:source.commit,...await frontendModuleEvidence(path.join(ROOT,'frontend/out'))});
console.log(`Bound ${dependencies.packages.length} compiler dependency records and actual frontend bundles to ${backendHash}`);
