import test from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import path from 'node:path';
import {createSecureContext} from 'node:tls';
import {OPENSSL_VERSION,OPENSSL_LIBRARIES,MANAGED_OPENSSL_LAYOUT,OPENSSL_PROBE,createLocalTlsFixture,validateOpenSSLObservation,opensslComponentEvidence} from './openssl-runtime.mjs';

const directory=path.resolve('fixture/science-v5'),certificateSha256='9'.repeat(64);
const expected={directory,executable:path.join(directory,'python.exe'),certificateSha256};
const digest=crypto.createHash('sha256').update('phaseforge-openssl-compatibility').digest('hex');
function observation(){return {
  openssl_version:'OpenSSL 3.0.22 20 Aug 2026',openssl_version_info:[3,0,0,22,0],executable:expected.executable,
  pointer_bits:64,isolated:1,dont_write_bytecode:true,ssl_module_path:path.join(directory,'_ssl.pyd'),hashlib_module_path:path.join(directory,'_hashlib.pyd'),
  loaded_libraries:Object.fromEntries(OPENSSL_LIBRARIES.map(name=>[name,path.join(directory,name)])),
  sha256_digest:digest,openssl_sha256_digest:digest,
  tls:{passed:true,version:'TLSv1.3',server_version:'TLSv1.3',cipher:['TLS_AES_256_GCM_SHA384','TLSv1.3',256],
    client_received:'phaseforge-server-probe',server_received:'phaseforge-client-probe',certificate_sha256:certificateSha256,check_hostname:true,verify_mode:2},
};}
function evidenceFixture(){
  const archive='openssl-bin-3.0.22.zip',archiveHash='a'.repeat(64);
  const replacements={},files={},sizes={},libraries={},rows=[];
  OPENSSL_LIBRARIES.forEach((name,index)=>{
    const sha256=String(index+1).repeat(64),bytes=100+index,member=`cpython-bin-deps-openssl-bin-3.0.22/amd64/${name}`;
    const original={archive_name:'python-3.13.15-embed-amd64.zip',archive_sha256:'b'.repeat(64),archive_member:name,sha256:'c'.repeat(64),bytes:90,version:'3.0.21'};
    replacements[name]={component:'OpenSSL',version:OPENSSL_VERSION,original,replacement:{archive_name:archive,archive_sha256:archiveHash,archive_member:member,sha256,bytes,version:OPENSSL_VERSION}};
    files[name]=sha256;sizes[name]=bytes;libraries[name]={sha256,bytes};
    rows.push({path:name,sha256,bytes,origin:{archive,archive_sha256:archiveHash,member,component:'OpenSSL',component_version:OPENSSL_VERSION,replaces:structuredClone(original)}});
  });
  return {observed:{...observation(),passed:true,version:OPENSSL_VERSION,libraries,seed_manifest_sha256:'d'.repeat(64)},
    identity:{all_native_archive_members_verified:true,files:rows},
    manifest:{kind:'science-v5',openssl:OPENSSL_VERSION,files,file_bytes:sizes,sources:[{name:archive,sha256:archiveHash}],transformations:{native_replacements:replacements}}};
}

test('OpenSSL observation verifies fixed branch, both imports, real digest and mutually exchanged TLS bytes',()=>{
  assert.equal(validateOpenSSLObservation(observation(),expected),true);
  assert.deepEqual(Object.keys(MANAGED_OPENSSL_LAYOUT),['science-v5','python-numpy-v4']);
  assert.match(OPENSSL_PROBE,/ssl\.MemoryBIO\(\)/);assert.match(OPENSSL_PROBE,/server_hostname='localhost'/);
  assert.doesNotMatch(OPENSSL_PROBE,/socket\(|create_connection|urllib|requests\./);
});

test('observation rejects wrong versions, host-loaded DLLs, missing pair and unverified or incomplete TLS',()=>{
  for(const [label,mutate] of [
    ['old-version',row=>row.openssl_version='OpenSSL 3.0.21'],
    ['different-branch',row=>row.openssl_version_info=[3,1,0,22,0]],
    ['wrong-python',row=>row.executable=path.resolve('host/python.exe')],
    ['wrong-ssl-extension',row=>row.ssl_module_path=path.resolve('host/_ssl.pyd')],
    ['wrong-hash-extension',row=>row.hashlib_module_path=path.resolve('host/_hashlib.pyd')],
    ['host-crypto',row=>row.loaded_libraries['libcrypto-3.dll']=path.resolve('host/libcrypto-3.dll')],
    ['host-ssl',row=>row.loaded_libraries['libssl-3.dll']=path.resolve('host/libssl-3.dll')],
    ['missing-pair',row=>delete row.loaded_libraries['libssl-3.dll']],
    ['wrong-digest',row=>row.openssl_sha256_digest='0'.repeat(64)],
    ['tls-incomplete',row=>row.tls.passed=false],
    ['wrong-tls',row=>row.tls.version='TLSv1.2'],
    ['unverified-peer',row=>row.tls.verify_mode=0],
    ['no-hostname-check',row=>row.tls.check_hostname=false],
    ['wrong-peer',row=>row.tls.certificate_sha256='0'.repeat(64)],
    ['missing-response',row=>row.tls.client_received=''],
    ['wrong-server-data',row=>row.tls.server_received='other'],
  ]){
    const value=observation();mutate(value);
    assert.throws(()=>validateOpenSSLObservation(value,expected),undefined,label);
  }
});

test('ephemeral localhost fixture has a valid matching key, strict certificate extensions and no committed private bytes',()=>{
  const first=createLocalTlsFixture(),second=createLocalTlsFixture();
  assert.notEqual(first.key,second.key);assert.notEqual(first.certificateSha256,second.certificateSha256);
  const certificate=new crypto.X509Certificate(first.certificate),key=crypto.createPrivateKey(first.key);
  assert.equal(certificate.verify(certificate.publicKey),true);assert.equal(certificate.checkPrivateKey(key),true);
  assert.equal(certificate.checkHost('localhost'),'localhost');assert.equal(certificate.checkHost('elsewhere.invalid'),undefined);
  assert.equal(certificate.publicKey.asymmetricKeyDetails.modulusLength,2048);
  assert.equal(certificate.ca,true);
  assert.equal(crypto.createHash('sha256').update(certificate.raw).digest('hex'),first.certificateSha256);
  assert.doesNotThrow(()=>createSecureContext({cert:first.certificate,key:first.key,ca:first.certificate,minVersion:'TLSv1.3'}));
});

test('OpenSSL scanner component binds both executed hashes to the exact replacement archive and original preimages',()=>{
  const {observed,identity,manifest}=evidenceFixture();
  const value=opensslComponentEvidence('science-v5',observed,identity,manifest);
  assert.equal(value.version,'3.0.22');assert.equal(value.cpe,'cpe:2.3:a:openssl:openssl:3.0.22:*:*:*:*:*:*:*');
  assert.deepEqual(value.hashes.map(row=>row.content),['1'.repeat(64),'2'.repeat(64)]);
  for(const name of OPENSSL_LIBRARIES) assert.ok(value.properties.some(row=>row.name===`phaseforge:evidence:${name}:member`));
});

test('component refuses changed DLLs, wrong or missing native provenance and mismatched manifest pins',()=>{
  for(const [label,mutate] of [
    ['unverified-archive',fixture=>fixture.identity.all_native_archive_members_verified=false],
    ['missing-pair',fixture=>fixture.identity.files.pop()],
    ['missing-observed-pair',fixture=>delete fixture.observed.libraries['libssl-3.dll']],
    ['changed-loaded-DLL',fixture=>fixture.observed.libraries['libcrypto-3.dll'].sha256='f'.repeat(64)],
    ['changed-SSL-DLL',fixture=>fixture.identity.files[1].sha256='f'.repeat(64)],
    ['wrong-byte-count',fixture=>fixture.identity.files[0].bytes++],
    ['missing-replacement',fixture=>delete fixture.manifest.transformations.native_replacements['libssl-3.dll']],
    ['wrong-package-origin',fixture=>fixture.identity.files[0].origin.component='CPython'],
    ['old-component-version',fixture=>fixture.identity.files[0].origin.component_version='3.0.21'],
    ['wrong-archive',fixture=>fixture.identity.files[0].origin.archive='python.zip'],
    ['wrong-archive-hash',fixture=>fixture.identity.files[1].origin.archive_sha256='f'.repeat(64)],
    ['wrong-member',fixture=>fixture.identity.files[0].origin.member='other.dll'],
    ['wrong-preimage',fixture=>fixture.identity.files[1].origin.replaces.sha256='f'.repeat(64)],
    ['wrong-source-pin',fixture=>fixture.manifest.sources[0].sha256='f'.repeat(64)],
    ['wrong-frozen-DLL',fixture=>fixture.manifest.files['libssl-3.dll']='f'.repeat(64)],
    ['wrong-runtime-kind',fixture=>fixture.manifest.kind='python-numpy-v4'],
  ]){
    const fixture=evidenceFixture();mutate(fixture);
    assert.throws(()=>opensslComponentEvidence('science-v5',fixture.observed,fixture.identity,fixture.manifest),undefined,label);
  }
});
