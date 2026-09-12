import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {ROOT,readJSON,sha256} from './common.mjs';
import {verifySourceFile} from './openmm-sources.mjs';

export const MSVC_RESOURCE='tools/third-party/msvc-runtime';
export const msvcNoticeRoot=path.join(ROOT,MSVC_RESOURCE);
export const MSVC_NOTICES=Object.freeze({
  'NOTICE.txt':{bytes:1427,sha256:'cb5bb1a8326c26f1f70322402811b3cd0f6e7c93a0ea088dbf3684ec2b6947f9'},
  'Visual-Studio-2022-Community-License.txt':{bytes:19043,sha256:'a38d10afc3ef8fb2e64af972a9c64d998ef964de8101b9f7a59703f72fa3a5ab'},
  'provenance.json':{bytes:2933,sha256:'48bf66e85522ea70cdfe043d220fddc4523d8a87b76611a336e2da43db36e862'},
  'END-USER-TERMS.txt':{bytes:4185,sha256:'ee84514c1e240e9976a811b885ad5bf194f2d4523a0402e9fdef6cf7564d9bc1'},
});

export async function verifyMsvcNotices(directory=msvcNoticeRoot){
  assert.deepEqual(fs.readdirSync(directory).sort(),Object.keys(MSVC_NOTICES).sort(),'Microsoft runtime notices have missing or unexpected members');
  const files=[];
  for(const [name,pin] of Object.entries(MSVC_NOTICES)){
    await verifySourceFile(path.join(directory,name),pin);
    files.push({path:`${MSVC_RESOURCE}/${name}`,...pin});
  }
  const provenance=readJSON(path.join(directory,'provenance.json'));
  assert.equal(provenance.schema,'phaseforge.msvc-runtime-provenance.v1');
  const frozen=path.join(ROOT,'tools/runtime-seeds/science-v5.manifest.json'),manifest=readJSON(frozen);
  const alias=manifest.transformations.native_aliases['MSVCP140.dll'];
  assert.equal(manifest.kind,'science-v5');
  assert.equal(alias.archive_member,provenance.origin.member);
  assert.equal(alias.sha256,provenance.component.sha256);
  assert.equal(manifest.files['MSVCP140.dll'],provenance.component.sha256);
  assert.equal(manifest.file_bytes['MSVCP140.dll'],provenance.component.bytes);
  assert.equal(provenance.origin.destination,'runtime/runtime-seeds/science-v5/MSVCP140.dll');
  assert.ok(manifest.sources.some(source=>source.url===provenance.origin.url&&source.sha256===provenance.origin.sha256),'Microsoft runtime origin must be an exact frozen upstream archive');
  for(const [name,pin] of Object.entries(provenance.files))assert.deepEqual(pin,MSVC_NOTICES[name]);
  return {schema:'phaseforge.msvc-runtime-delivery.v1',resource_directory:MSVC_RESOURCE,outside_asar:true,
    seed_kind:manifest.kind,seed_manifest_sha256:await sha256(frozen),component:provenance.component,
    origin:provenance.origin,files,
    scope:'Exact accompanying Microsoft notices and license text, bound to the byte-identical OpenMM dependency alias in the frozen science seed. This verification does not certify user license entitlement or replace the recorded redistribution terms.'};
}
