/** Inventory observations from the exact installed app; source locks have a separate scope. */
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {createRequire} from 'node:module';
import {ROOT,inside,inventory,readJSON,releaseTarget,sha256,writeJSON} from './common.mjs';
import {auditableDependencies,auditableSection} from './auditable.mjs';
import {cryptoRuntimeIdentity} from './runtime-identity.mjs';
import {frontendModuleEvidence} from './frontend-map.mjs';
import {OPENMM_RESOURCE,verifyOpenmmSourceDelivery} from './openmm-sources.mjs';
import {MSVC_RESOURCE,verifyMsvcNotices} from './msvc-notices.mjs';
import {sqliteComponentEvidence} from './sqlite-runtime.mjs';

const {platform,architecture,folder:targetFolder}=releaseTarget();
const folder=path.join(ROOT,'.local/marketplace',targetFolder),payload=path.join(folder,'payload');
const resourcesRelative=process.platform==='darwin'?'Contents/Resources':'resources',resources=path.join(payload,resourcesRelative);
const installed=readJSON(path.join(folder,'installed-payload.json'));
const observed=readJSON(path.join(folder,'launch-1/runtime.json'));
assert.equal(installed.source_commit,process.env.GITHUB_SHA);
assert.equal(installed.resources_relative,resourcesRelative);assert.equal(installed.backend.machine.architecture,architecture);
const backend=path.join(resources,'runtime',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');
assert.equal(await sha256(backend),installed.backend.sha256);
const dependencies=auditableDependencies(fs.readFileSync(backend));
const bound=readJSON(path.join(resources,'runtime/backend.dependencies.json'));
assert.equal(bound.source_commit,installed.source_commit);
assert.equal(bound.binary_sha256,installed.backend.sha256);assert.deepEqual(bound.dependencies,dependencies);
if(platform==='macos'){
  const signing=readJSON(path.join(resources,'runtime/backend-signing.json'));
  assert.equal(signing.source_commit,installed.source_commit);assert.equal(signing.platform,'darwin');assert.equal(signing.architecture,architecture);
  assert.equal(signing.staged_signed_sha256,installed.backend.sha256);assert.equal(signing.verification.status,0);assert.equal(signing.compiler_dependency_section_unchanged,true);
  assert.equal(signing.compiler_dependency_section_sha256,crypto.createHash('sha256').update(auditableSection(fs.readFileSync(backend))).digest('hex'));
  writeJSON(path.join(folder,'backend-signing.json'),signing);
}
const bundles=readJSON(path.join(resources,'runtime/frontend-bundles.json'));
assert.equal(bundles.source_commit,installed.source_commit);
assert.deepEqual(bundles.files,(await inventory(path.join(resources,'ui'))).files);
const frontendModules=readJSON(path.join(resources,'runtime/frontend-modules.json'));
assert.equal(frontendModules.source_commit,installed.source_commit);
assert.deepEqual(frontendModules,{source_commit:installed.source_commit,...await frontendModuleEvidence(path.join(resources,'ui'))},'Installed frontend module mapping differs from its staged evidence');
writeJSON(path.join(folder,'frontend-modules.json'),frontendModules);

const require=createRequire(path.join(ROOT,'desktop/package.json'));
const asar=require('@electron/asar');
const archive=path.join(resources,'app.asar'),expanded=path.join(folder,'expanded-asar');
assert.ok(!fs.existsSync(expanded),'ASAR evidence must be new');fs.mkdirSync(expanded);
let total=0,count=0;
for(const name of asar.listPackage(archive)){
  const relative=name.replace(/^[/\\]/,'').replaceAll('\\','/');
  const target=inside(expanded,path.resolve(expanded,relative));
  const entry=asar.statFile(archive,relative,false);
  if(entry.link)throw Error('Unexpected linked ASAR input');
  if(entry.files)continue;
  if(!Number.isSafeInteger(entry.size)||entry.size<0)throw Error('Invalid ASAR size');
  total+=entry.size;count++;
  if(total>64*1024**2||count>10000)throw Error('Packaged application ASAR exceeds its reviewed extraction budget');
  const bytes=asar.extractFile(archive,relative);assert.equal(bytes.length,entry.size);
  fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,bytes);
}
writeJSON(path.join(folder,'asar-inventory.json'),{archive_sha256:await sha256(archive),source_commit:installed.source_commit,...await inventory(expanded)});
const versions=observed.runtime.versions,sqlite=observed.health.sqlite;
for(const name of ['electron','chrome','node','v8'])assert.ok(typeof versions[name]==='string'&&versions[name].length,'Missing observed runtime version');
assert.ok(sqlite?.version&&sqlite.source_id,'Missing database runtime observation');
const common=[{name:'phaseforge:evidence:source-commit',value:installed.source_commit},{name:'phaseforge:evidence:backend-sha256',value:installed.backend.sha256},{name:'phaseforge:evidence:observations',value:'launch-1/runtime.json; installed-payload.json; NATIVE_ACCEPTANCE.json'}];
function component(name,version,purl,cpe,properties=[]){return {type:'library','bom-ref':purl,name,version,purl,...(cpe?{cpe}:{}),properties:[...common,...properties]};}
const components=[
  component('electron',versions.electron,`pkg:github/electron/electron@v${versions.electron}`,`cpe:2.3:a:electronjs:electron:${versions.electron}:*:*:*:*:*:*:*`),
  component('chromium',versions.chrome,`pkg:generic/chromium@${versions.chrome}`,`cpe:2.3:a:chromium:chromium:${versions.chrome}:*:*:*:*:*:*:*`,[{name:'phaseforge:evidence:scope',value:'Observed Electron Chromium version. Electron may backport fixes without changing Chromium version; raw matches require exact upstream patch review.'}]),
  component('node',versions.node,`pkg:generic/nodejs@${versions.node}`,`cpe:2.3:a:nodejs:node.js:${versions.node}:*:*:*:*:*:*:*`),
  component('v8',versions.v8,`pkg:generic/v8@${encodeURIComponent(versions.v8)}`,null),
  component('sqlite-backend',sqlite.version,`pkg:generic/sqlite@${sqlite.version}?consumer=phaseforge-backend`,`cpe:2.3:a:sqlite:sqlite:${sqlite.version}:*:*:*:*:*:*:*`,[{name:'sqlite:source-id',value:sqlite.source_id},{name:'phaseforge:evidence:scope',value:'Actual SELECT sqlite_version(), sqlite_source_id() from the packaged backend database connection.'}]),
];
if(platform==='windows'){
  const sourceMaterials=readJSON(path.join(resources,'runtime/third-party-source-materials.json'));
  assert.equal(sourceMaterials.source_commit,installed.source_commit);assert.equal(sourceMaterials.backend_sha256,installed.backend.sha256);
  const observedSources=await verifyOpenmmSourceDelivery(path.join(resources,OPENMM_RESOURCE));
  assert.deepEqual(sourceMaterials,{source_commit:installed.source_commit,backend_sha256:installed.backend.sha256,...observedSources});
  writeJSON(path.join(folder,'third-party-source-materials.json'),sourceMaterials);
  const msvcMaterials=readJSON(path.join(resources,'runtime/msvc-runtime-materials.json'));
  assert.deepEqual(msvcMaterials,{source_commit:installed.source_commit,backend_sha256:installed.backend.sha256,...await verifyMsvcNotices(path.join(resources,MSVC_RESOURCE))});
  writeJSON(path.join(folder,'msvc-runtime-materials.json'),msvcMaterials);
  const seedMaterials=readJSON(path.join(resources,'runtime/runtime-seed-materials.json'));
  assert.equal(seedMaterials.source_commit,installed.source_commit);assert.equal(seedMaterials.backend_sha256,installed.backend.sha256);
  const rebuiltPath=path.join(resources,'runtime/runtime-seed-build.json'),rebuilt=readJSON(rebuiltPath);
  assert.equal(rebuilt.source_commit,installed.source_commit);assert.equal(await sha256(rebuiltPath),seedMaterials.archive_rebuild_receipt.sha256);
  writeJSON(path.join(folder,'runtime-seed-build.json'),rebuilt);
  const managed=readJSON(path.join(folder,'managed-runtime-materials.json'));
  assert.equal(managed.source_commit,installed.source_commit);assert.equal(managed.delivery,'bundled_immutable_seeds');assert.equal(managed.integrity_valid,true);
  const sqliteExecution=readJSON(path.join(folder,'managed-sqlite-runtime.json'));
  assert.equal(sqliteExecution.schema,'phaseforge.managed-sqlite-execution.v1');assert.equal(sqliteExecution.passed,true);assert.equal(sqliteExecution.source_commit,installed.source_commit);
  assert.deepEqual(Object.keys(sqliteExecution.runtimes).sort(),['python-numpy-v3','science-v4']);
  for(const kind of ['science-v4','python-numpy-v3']){
    const seed=path.join(resources,'runtime/runtime-seeds',kind),manifestPath=path.join(seed,'phaseforge-runtime-seed.json');
    const manifest=readJSON(manifestPath),hash=await sha256(manifestPath);
    assert.equal(hash,await sha256(path.join(ROOT,'tools/runtime-seeds',`${kind}.manifest.json`)));
    assert.equal(hash,seedMaterials.seeds[kind].manifest_sha256);
    assert.equal(hash,rebuilt.seeds[kind].manifest_sha256);assert.deepEqual(manifest.sources,rebuilt.seeds[kind].sources);
    assert.equal(hash,managed.runtimes[kind].frozen_source_manifest.sha256);
    assert.equal(managed.runtimes[kind].copy_comparison.valid,true);
    const rows=(await inventory(seed)).files.filter(row=>row.path!=='phaseforge-runtime-seed.json');
    assert.equal(rows.length,Object.keys(manifest.files).length);
    for(const row of rows){assert.equal(row.sha256,manifest.files[row.path]);assert.equal(row.bytes,manifest.file_bytes[row.path]);}
    assert.equal(sqliteExecution.runtimes[kind].seed_manifest_sha256,hash);
    const sqliteEvidence=sqliteComponentEvidence(kind,sqliteExecution.runtimes[kind],managed.runtimes[kind].native_identity,manifest);
    components.push({...component(sqliteEvidence.name,sqliteEvidence.version,sqliteEvidence.purl,sqliteEvidence.cpe,sqliteEvidence.properties),hashes:sqliteEvidence.hashes});
    const versions={python:manifest.python,numpy:manifest.numpy,...(kind==='science-v4'?{openmm:manifest.openmm,pillow:manifest.pillow}:{})};
    for(const [name,version] of Object.entries(versions)){
      assert.match(version,/^\d+\.\d+\.\d+$/);
      const purl=name==='python'?`pkg:generic/cpython@${version}?consumer=${kind}`:`pkg:pypi/${name}@${version}?consumer=${kind}`;
      components.push(component(name,version,purl,name==='python'?`cpe:2.3:a:python:python:${version}:*:*:*:*:*:*:*`:null,[
        {name:'phaseforge:evidence:scope',value:'Pinned archive identity and exact bundled/native file hashes, independently compared to the managed app-data copy. Package file presence is not proof every module was loaded.'},
        {name:'phaseforge:evidence:seed-kind',value:kind},{name:'phaseforge:evidence:seed-manifest-sha256',value:hash},
        {name:'phaseforge:evidence:seed-materials',value:'runtime-seed-materials.json; managed-runtime-materials.json; laboratory acceptance receipts'},
      ]));
    }
  }
  writeJSON(path.join(folder,'runtime-seed-materials.json'),seedMaterials);
}
const cryptoIdentity=cryptoRuntimeIdentity(versions);
if(cryptoIdentity.status==='observed_version')components.push(component('openssl',cryptoIdentity.version,`pkg:generic/openssl@${cryptoIdentity.version}`,`cpe:2.3:a:openssl:openssl:${cryptoIdentity.version}:*:*:*:*:*:*:*`));
writeJSON(path.join(folder,'runtime-identity-gaps.json'),{source_commit:installed.source_commit,crypto:cryptoIdentity,gaps:cryptoIdentity.status==='unresolved'?[cryptoIdentity.gap]:[],scope:'Raw process.versions remain unchanged in the launch runtime receipt. This file does not assert a BoringSSL revision or waive missing coverage.'});
if(versions.sqlite)components.push(component('sqlite-node',versions.sqlite,`pkg:generic/sqlite@${versions.sqlite}?consumer=electron-node`,`cpe:2.3:a:sqlite:sqlite:${versions.sqlite}:*:*:*:*:*:*:*`,[{name:'phaseforge:evidence:scope',value:'Separate SQLite version observed in Electron process.versions.sqlite.'}]));
writeJSON(path.join(folder,'runtime-observations.cdx.json'),{bomFormat:'CycloneDX',specVersion:'1.6',version:1,metadata:{timestamp:new Date().toISOString(),component:{type:'application',name:'PhaseForge',version:installed.version},properties:[{name:'phaseforge:scope',value:'Supplemental runtime observations, not a complete payload SBOM. Raw Syft payload/ASAR inventories and compiled .dep-v0 dependencies are retained separately.'},{name:'phaseforge:crypto-identity-status',value:cryptoIdentity.status},{name:'phaseforge:crypto-identity-evidence',value:'runtime-identity-gaps.json; launch-1/runtime.json'}]},components});
const v8Base=versions.v8.split('-')[0];assert.match(v8Base,/^\d+(\.\d+)+$/);
writeJSON(path.join(folder,'runtime-screening-aliases.cdx.json'),{bomFormat:'CycloneDX',specVersion:'1.6',version:1,metadata:{properties:[{name:'phaseforge:scope',value:'Security screening aliases only. Google Chrome is not installed or bundled as a separate browser. These aliases broaden vulnerability candidate matching for vendored Chromium/V8; OS applicability and exact Electron backports require review.'}]},components:[
  component('chromium-chrome-security-screening',versions.chrome,`pkg:generic/phaseforge-chromium-screening@${versions.chrome}`,`cpe:2.3:a:google:chrome:${versions.chrome}:*:*:*:*:*:*:*`,[{name:'phaseforge:alias:observed-component',value:`Electron Chromium ${versions.chrome}`},{name:'phaseforge:alias:scope',value:'Conservative Google Chrome CVE screening alias for embedded Chromium. A match does not establish application/OS applicability; no matches are discarded automatically.'}]),
  component('v8-security-screening',v8Base,`pkg:generic/phaseforge-v8-screening@${v8Base}`,`cpe:2.3:a:google:v8:${v8Base}:*:*:*:*:*:*:*`,[{name:'phaseforge:alias:observed-version',value:versions.v8},{name:'phaseforge:alias:scope',value:'Numeric upstream V8 version before the Electron build suffix, derived from actual process.versions.v8.'},{name:'phaseforge:alias:vendor-reference',value:'https://nvd.nist.gov/vuln/detail/cve-2024-3914'}]),
]});
writeJSON(path.join(folder,'compiled-dependencies.json'),bound);
console.log(`Verified installed backend, ${bundles.files.length} frontend files and ${count} ASAR members; recorded actual runtime versions`);
