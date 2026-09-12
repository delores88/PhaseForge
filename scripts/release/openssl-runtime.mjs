/** Read-only OpenSSL execution checks for the exact managed runtime copies. */
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {ROOT,command,readJSON,sha256} from './common.mjs';
import {verifySeed} from './stage-seeds.mjs';

export const OPENSSL_VERSION='3.0.22';
export const OPENSSL_LIBRARIES=Object.freeze(['libcrypto-3.dll','libssl-3.dll']);
export const MANAGED_OPENSSL_LAYOUT=Object.freeze({'science-v5':'environments/science-v5','python-numpy-v4':'environments/python-numpy-v4/runtime'});
const samePath=(left,right)=>typeof left==='string'&&typeof right==='string'&&path.resolve(left).toLowerCase()===path.resolve(right).toLowerCase();
const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
const HASH_INPUT='phaseforge-openssl-compatibility';
const HASH_EXPECTED=hash(HASH_INPUT);
const SHA=/^[a-f0-9]{64}$/;

// Minimal fixed-shape X.509 fixture encoder. A fresh private key exists only
// in memory and the temporary handshake directory; no key bytes are committed.
function der(tag,...parts){
  const content=Buffer.concat(parts),size=content.length,bytes=[];
  for(let value=size;value;value=Math.floor(value/256)) bytes.unshift(value&255);
  const length=size<128?Buffer.from([size]):Buffer.from([128|bytes.length,...bytes]);
  return Buffer.concat([Buffer.from([tag]),length,content]);
}
const sequence=(...parts)=>der(0x30,...parts);
const octets=bytes=>der(0x04,bytes);
const bool=value=>der(0x01,Buffer.from([value?255:0]));
const bits=(bytes,unused=0)=>der(0x03,Buffer.from([unused]),bytes);
function integer(bytes){
  let value=Buffer.from(bytes);
  while(value.length>1&&value[0]===0)value=value.subarray(1);
  if(!value.length)value=Buffer.from([0]);
  if(value[0]&128)value=Buffer.concat([Buffer.from([0]),value]);
  return der(0x02,value);
}
function oid(text){
  const arcs=text.split('.').map(BigInt),bytes=[];
  for(const arc of [40n*arcs[0]+arcs[1],...arcs.slice(2)]){
    const encoded=[Number(arc&127n)];
    for(let value=arc>>7n;value;value>>=7n)encoded.unshift(Number(value&127n)|128);
    bytes.push(...encoded);
  }
  return der(0x06,Buffer.from(bytes));
}
const extension=(id,value,critical=false)=>sequence(oid(id),...(critical?[bool(true)]:[]),octets(value));
const pem=(label,bytes)=>`-----BEGIN ${label}-----\n${bytes.toString('base64').match(/.{1,64}/g).join('\n')}\n-----END ${label}-----\n`;
export function createLocalTlsFixture(){
  const {privateKey,publicKey}=crypto.generateKeyPairSync('rsa',{modulusLength:2048,publicExponent:65537});
  const spki=publicKey.export({type:'spki',format:'der'}),rsaPublic=publicKey.export({type:'pkcs1',format:'der'});
  // SHA-1 identifies the key (SKI); the certificate signature uses SHA-256.
  const keyId=crypto.createHash('sha1').update(rsaPublic).digest();
  const algorithm=sequence(oid('1.2.840.113549.1.1.11'),der(0x05));
  const name=sequence(der(0x31,sequence(oid('2.5.4.3'),der(0x0c,Buffer.from('localhost')))));
  const validity=sequence(der(0x17,Buffer.from('200101000000Z')),der(0x18,Buffer.from('20991231235959Z')));
  const extensions=sequence(
    extension('2.5.29.19',sequence(bool(true)),true),
    extension('2.5.29.15',bits(Buffer.from([0xa6]),1),true),
    extension('2.5.29.37',sequence(oid('1.3.6.1.5.5.7.3.1'))),
    extension('2.5.29.17',sequence(der(0x82,Buffer.from('localhost')))),
    extension('2.5.29.14',octets(keyId)),extension('2.5.29.35',sequence(der(0x80,keyId))));
  const serial=crypto.randomBytes(16);if(serial.every(byte=>byte===0))serial[15]=1;
  const tbs=sequence(der(0xa0,integer(Buffer.from([2]))),integer(serial),algorithm,name,validity,name,spki,der(0xa3,extensions));
  const signature=crypto.sign('RSA-SHA256',tbs,privateKey),certificate=pem('CERTIFICATE',sequence(tbs,algorithm,bits(signature)));
  const parsed=new crypto.X509Certificate(certificate);
  assert.ok(parsed.verify(publicKey)&&parsed.checkPrivateKey(privateKey));assert.equal(parsed.checkHost('localhost'),'localhost');assert.equal(parsed.ca,true);
  return {certificate,key:privateKey.export({type:'pkcs8',format:'pem'}),certificateSha256:hash(parsed.raw)};
}

export const OPENSSL_PROBE=String.raw`
import ctypes,hashlib,json,ssl,sys,_ssl,_hashlib
from ctypes import wintypes
certificate,key=sys.argv[1:3]
client_context=ssl.SSLContext(ssl.PROTOCOL_TLS_CLIENT)
client_context.load_verify_locations(cafile=certificate)
client_context.minimum_version=client_context.maximum_version=ssl.TLSVersion.TLSv1_3
server_context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
server_context.load_cert_chain(certificate,key)
server_context.minimum_version=server_context.maximum_version=ssl.TLSVersion.TLSv1_3
client_in,client_out,server_in,server_out=[ssl.MemoryBIO() for _ in range(4)]
client=client_context.wrap_bio(client_in,client_out,server_hostname='localhost')
server=server_context.wrap_bio(server_in,server_out,server_side=True)
def transfer():
    for outgoing,incoming in ((client_out,server_in),(server_out,client_in)):
        while outgoing.pending: incoming.write(outgoing.read())
done=[False,False]
for attempt in range(100):
    for index,connection in enumerate((client,server)):
        if not done[index]:
            try:
                connection.do_handshake()
                done[index]=True
            except ssl.SSLWantReadError: pass
    transfer()
    if all(done): break
if not all(done): raise RuntimeError('Bounded in-memory TLS handshake did not complete')
client.write(b'phaseforge-client-probe')
transfer()
server_received=server.read(4096).decode('ascii')
server.write(b'phaseforge-server-probe')
transfer()
client_received=client.read(4096).decode('ascii')
kernel=ctypes.WinDLL('kernel32',use_last_error=True)
kernel.GetModuleHandleW.argtypes=[wintypes.LPCWSTR]
kernel.GetModuleHandleW.restype=wintypes.HMODULE
kernel.GetModuleFileNameW.argtypes=[wintypes.HMODULE,wintypes.LPWSTR,wintypes.DWORD]
kernel.GetModuleFileNameW.restype=wintypes.DWORD
libraries={}
for name in ('libcrypto-3.dll','libssl-3.dll'):
    handle=kernel.GetModuleHandleW(name)
    if not handle: raise ctypes.WinError(ctypes.get_last_error())
    buffer=ctypes.create_unicode_buffer(32768)
    count=kernel.GetModuleFileNameW(handle,buffer,len(buffer))
    if not 0<count<len(buffer): raise RuntimeError('OpenSSL loaded path is unavailable')
    libraries[name]=buffer.value
payload=b'phaseforge-openssl-compatibility'
print(json.dumps({'openssl_version':ssl.OPENSSL_VERSION,'openssl_version_info':list(ssl.OPENSSL_VERSION_INFO),
    'executable':sys.executable,'pointer_bits':ctypes.sizeof(ctypes.c_void_p)*8,'isolated':sys.flags.isolated,
    'dont_write_bytecode':sys.dont_write_bytecode,'ssl_module_path':_ssl.__file__,'hashlib_module_path':_hashlib.__file__,
    'loaded_libraries':libraries,'sha256_digest':hashlib.sha256(payload).hexdigest(),
    'openssl_sha256_digest':_hashlib.openssl_sha256(payload).hexdigest(),
    'tls':{'passed':True,'version':client.version(),'server_version':server.version(),'cipher':list(client.cipher()),
        'client_received':client_received,'server_received':server_received,
        'certificate_sha256':hashlib.sha256(client.getpeercert(binary_form=True)).hexdigest(),
        'check_hostname':client_context.check_hostname,'verify_mode':int(client_context.verify_mode),
        'transport':'MemoryBIO; no sockets or external network'}}))
`;

export function validateOpenSSLObservation(value,{executable,directory,certificateSha256}){
  assert.match(value.openssl_version,/^OpenSSL 3\.0\.22(?:\s|$)/,'Managed OpenSSL must be the exact fixed version');
  assert.deepEqual(value.openssl_version_info,[3,0,0,22,0],'Actual OpenSSL version tuple differs from the reviewed branch');
  assert.equal(value.pointer_bits,64);assert.equal(value.isolated,1);assert.equal(value.dont_write_bytecode,true);
  assert.ok(samePath(value.executable,executable),'Probe used another Python executable');
  assert.ok(samePath(value.ssl_module_path,path.join(directory,'_ssl.pyd')),'SSL extension resolved outside this runtime');
  assert.ok(samePath(value.hashlib_module_path,path.join(directory,'_hashlib.pyd')),'Hash extension resolved outside this runtime');
  assert.deepEqual(Object.keys(value.loaded_libraries||{}).sort(),[...OPENSSL_LIBRARIES].sort(),'Both loaded OpenSSL libraries must be identified');
  for(const name of OPENSSL_LIBRARIES) assert.ok(samePath(value.loaded_libraries[name],path.join(directory,name)),`${name} resolved outside this exact managed runtime`);
  assert.equal(value.sha256_digest,HASH_EXPECTED);assert.equal(value.openssl_sha256_digest,HASH_EXPECTED,'The OpenSSL-backed digest was not verified');
  assert.equal(value.tls?.passed,true);assert.equal(value.tls?.version,'TLSv1.3');assert.equal(value.tls?.server_version,'TLSv1.3');
  assert.ok(Array.isArray(value.tls?.cipher)&&value.tls.cipher.length===3&&value.tls.cipher[1]==='TLSv1.3'&&value.tls.cipher[2]>=128,'Negotiated cipher evidence is absent');
  assert.equal(value.tls.client_received,'phaseforge-server-probe');assert.equal(value.tls.server_received,'phaseforge-client-probe');
  assert.equal(value.tls.check_hostname,true);assert.equal(value.tls.verify_mode,2,'TLS peer certificate verification must remain enabled');
  assert.match(certificateSha256,SHA);assert.equal(value.tls.certificate_sha256,certificateSha256,'TLS peer differs from the generated localhost fixture');
  return true;
}

export function opensslComponentEvidence(kind,observed,identity,manifest){
  assert.ok(Object.hasOwn(MANAGED_OPENSSL_LAYOUT,kind));assert.equal(observed.passed,true);
  assert.equal(observed.version,OPENSSL_VERSION);assert.equal(manifest.openssl,OPENSSL_VERSION);assert.equal(manifest.kind,kind);
  validateOpenSSLObservation(observed,{executable:observed.executable,directory:path.dirname(observed.executable),certificateSha256:observed.tls?.certificate_sha256});
  assert.equal(identity.all_native_archive_members_verified,true,'Every native member requires archive provenance');
  const members=[];
  for(const name of OPENSSL_LIBRARIES){
    const matches=identity.files.filter(row=>row.path===name);assert.equal(matches.length,1,`Exactly one ${name} origin is required`);
    const row=matches[0],replacement=manifest.transformations?.native_replacements?.[name],executed=observed.libraries?.[name];
    assert.ok(replacement&&executed,`Missing frozen or executed ${name} identity`);
    assert.equal(replacement.component,'OpenSSL');assert.equal(replacement.version,OPENSSL_VERSION);
    assert.match(row.sha256,SHA);assert.equal(row.sha256,executed.sha256);assert.equal(row.bytes,executed.bytes);
    assert.equal(row.sha256,manifest.files[name]);assert.equal(row.bytes,manifest.file_bytes[name]);
    assert.equal(row.sha256,replacement.replacement.sha256);assert.equal(row.bytes,replacement.replacement.bytes);
    assert.equal(row.origin?.archive,replacement.replacement.archive_name);assert.equal(row.origin?.archive_sha256,replacement.replacement.archive_sha256);
    assert.equal(row.origin?.member,replacement.replacement.archive_member);
    assert.equal(row.origin?.component,'OpenSSL');assert.equal(row.origin?.component_version,OPENSSL_VERSION);
    assert.deepEqual(row.origin?.replaces,replacement.original);
    const source=manifest.sources.find(entry=>entry.name===row.origin.archive);
    assert.ok(source);assert.equal(source.sha256,row.origin.archive_sha256);
    members.push(row);
  }
  assert.equal(members[0].origin.archive,members[1].origin.archive,'The fixed OpenSSL pair must share its reviewed archive');
  assert.equal(members[0].origin.archive_sha256,members[1].origin.archive_sha256);
  return {name:`openssl-${kind}`,version:OPENSSL_VERSION,purl:`pkg:generic/openssl@${OPENSSL_VERSION}?consumer=${kind}`,
    cpe:`cpe:2.3:a:openssl:openssl:${OPENSSL_VERSION}:*:*:*:*:*:*:*`,hashes:members.map(row=>({alg:'SHA-256',content:row.sha256})),properties:[
      {name:'phaseforge:evidence:seed-kind',value:kind},{name:'openssl:observed-version',value:observed.openssl_version},
      ...members.flatMap(row=>[{name:`phaseforge:evidence:${row.path}:sha256`,value:row.sha256},{name:`phaseforge:evidence:${row.path}:archive`,value:row.origin.archive},
        {name:`phaseforge:evidence:${row.path}:archive-sha256`,value:row.origin.archive_sha256},{name:`phaseforge:evidence:${row.path}:member`,value:row.origin.member}]),
      {name:'phaseforge:evidence:scope',value:'Actual managed Python _ssl/_hashlib imports, verified local MemoryBIO TLS handshake and digest, exact loaded DLL paths and unchanged frozen pair hashes, independently verified replacement archive members. Original embedded OpenSSL members are recorded as replaced, not shipped.'},
      {name:'phaseforge:evidence:observations',value:'managed-openssl-runtime.json; managed-runtime-materials.json'}]};
}

export async function observeManagedOpenSSL({workspace,sourceCommit,execute=command}){
  assert.equal(process.platform,'win32','Managed OpenSSL execution acceptance requires Windows');
  const runtimes={};
  for(const [kind,relative] of Object.entries(MANAGED_OPENSSL_LAYOUT)){
    const directory=path.join(workspace,relative),frozen=path.join(ROOT,'tools/runtime-seeds',`${kind}.manifest.json`);
    await verifySeed(directory,frozen);
    const manifest=readJSON(frozen),executable=path.join(directory,'python.exe'),libraries={};
    assert.equal(manifest.openssl,OPENSSL_VERSION);
    for(const name of OPENSSL_LIBRARIES){
      const file=path.join(directory,name);assert.ok(fs.statSync(file).isFile());
      libraries[name]={sha256:await sha256(file),bytes:fs.statSync(file).size};
      assert.equal(libraries[name].sha256,manifest.files[name]);assert.equal(libraries[name].bytes,manifest.file_bytes[name]);
    }
    const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-openssl-fixture-'));
    let observed;
    try{
      const fixture=createLocalTlsFixture(),certificate=path.join(temporary,'localhost-cert.pem'),key=path.join(temporary,'localhost-key.pem');
      fs.writeFileSync(certificate,fixture.certificate,{flag:'wx',mode:0o600});fs.writeFileSync(key,fixture.key,{flag:'wx',mode:0o600});
      observed=JSON.parse(execute(executable,['-I','-B','-c',OPENSSL_PROBE,certificate,key],{timeout:15000,maxBuffer:64*1024}));
      validateOpenSSLObservation(observed,{executable,directory,certificateSha256:fixture.certificateSha256});
    }finally{
      assert.ok(path.dirname(temporary)===os.tmpdir()&&path.basename(temporary).startsWith('phaseforge-openssl-fixture-'));
      fs.rmSync(temporary,{recursive:true,force:true});
    }
    for(const name of OPENSSL_LIBRARIES) assert.equal(await sha256(path.join(directory,name)),libraries[name].sha256,'Loaded OpenSSL bytes changed during observation');
    await verifySeed(directory,frozen);
    runtimes[kind]={...observed,version:OPENSSL_VERSION,passed:true,libraries,seed_manifest_sha256:await sha256(frozen)};
  }
  return {schema:'phaseforge.managed-openssl-execution.v1',source_commit:sourceCommit,passed:true,runtimes,
    scope:'Actual bundled managed-copy Python _ssl/_hashlib imports, digest and verified local in-memory TLS handshake with an ephemeral localhost fixture. Both loaded DLL paths and unchanged frozen file pins verified. No sockets, provider calls, user data or exploit payload; this is not general security certification.'};
}
