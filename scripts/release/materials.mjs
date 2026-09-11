import fs from 'node:fs';
import path from 'node:path';
import {ROOT,inventory,machine,readJSON,releaseTarget,sha256,sourceIdentity,writeJSON} from './common.mjs';
import {auditableDependencies} from './auditable.mjs';
import {frontendModuleEvidence} from './frontend-map.mjs';

const source=sourceIdentity(),platform=process.platform==='win32'?'win':process.platform==='darwin'?'mac':'linux';
releaseTarget();
const runtime=path.join(ROOT,'desktop/runtime',platform,process.arch),backend=path.join(runtime,process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');
const metadata=readJSON(path.join(runtime,'build.json'));
if(metadata.sourceCommit!==source.commit||metadata.sourceDirty)throw Error('Staged runtime is not the exact clean source');
if(metadata.platform!==process.platform||metadata.architecture!==process.arch||machine(backend).architecture!==process.arch)throw Error('Staged backend target differs from the actual host');
const backendHash=await sha256(backend),dependencies=auditableDependencies(fs.readFileSync(backend));
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
