import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {inside,machine,releaseTarget} from './common.mjs';
import {inspectMacApp,mountedVolume,payloadLayout,removeMacApp,validateDmgRoot} from './native-platform.mjs';

function macho(cpu=0x0100000c,little=true){
  const bytes=Buffer.alloc(32);Buffer.from(little?'cffaedfe':'feedfacf','hex').copy(bytes);
  little?bytes.writeUInt32LE(cpu,4):bytes.writeUInt32BE(cpu,4);return bytes;
}
function fixture(context){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-native-platform-'));
  context.after(()=>fs.rmSync(inside(os.tmpdir(),root,{existing:true}),{recursive:true}));return root;
}
function app(root){
  const bundle=path.join(root,'PhaseForge.app');
  for(const name of ['Contents/MacOS/PhaseForge','Contents/Resources/runtime/phaseforge-backend']){const file=path.join(bundle,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,macho());}
  return bundle;
}
function symlink(context,target,file){
  try{fs.symlinkSync(target,file,'dir');return true;}
  catch(error){if(process.platform==='win32'&&['EPERM','EACCES'].includes(error.code)){context.skip('This Windows host cannot create symlinks; native Linux/macOS jobs must exercise this case');return false;}throw error;}
}

test('release admission keeps Windows/Linux x64 and admits only Apple Silicon macOS',()=>{
  assert.deepEqual(releaseTarget('win32','x64'),{platform:'windows',architecture:'x64',folder:'windows-x64'});
  assert.deepEqual(releaseTarget('linux','x64'),{platform:'linux',architecture:'x64',folder:'linux-x64'});
  assert.deepEqual(releaseTarget('darwin','arm64'),{platform:'macos',architecture:'arm64',folder:'macos-arm64'});
  for(const pair of [['darwin','x64'],['darwin','universal'],['darwin','arm'],['linux','arm64'],['freebsd','x64']])assert.throws(()=>releaseTarget(...pair),/admits/);
});
test('reads thin Mach-O CPU identities and exposes universal binaries for rejection',context=>{
  const root=fixture(context),file=path.join(root,'binary');
  fs.writeFileSync(file,macho());assert.deepEqual(machine(file),{format:'macho',architecture:'arm64',bits:64});
  fs.writeFileSync(file,macho(0x01000007,false));assert.equal(machine(file).architecture,'x64');
  for(const magic of ['cafebabe','bebafeca','cafebabf','bfbafeca']){fs.writeFileSync(file,Buffer.from(magic,'hex'));assert.equal(machine(file).architecture,'universal');}
  fs.writeFileSync(file,macho().subarray(0,12));assert.throws(()=>machine(file),/Truncated Mach-O/);
});
test('payload root preserves the complete app bundle and case-sensitive Resources location',()=>{
  const root=path.resolve('/Applications/PhaseForge.app');
  assert.deepEqual(payloadLayout(path.join(root,'Contents/Resources'),'darwin'),{root,resourcesRelative:path.join('Contents','Resources')});
  assert.throws(()=>payloadLayout(path.join(root,'Resources'),'darwin'));
  for(const system of ['win32','linux'])assert.deepEqual(payloadLayout(path.join(root,'resources'),system),{root,resourcesRelative:'resources'});
});
test('DMG receipt rejects another mountpoint, multiple mounted volumes and forged devices',()=>{
  const mount=path.resolve('/tmp/phaseforge/dmg-mount');
  assert.equal(mountedVolume([{'dev-entry':'/dev/disk4'},{'mount-point':mount,'dev-entry':'/dev/disk4s1'}],mount).device,'/dev/disk4s1');
  assert.throws(()=>mountedVolume([{'mount-point':'/Volumes/other','dev-entry':'/dev/disk4s1'}],mount),/outside/);
  assert.throws(()=>mountedVolume([{'mount-point':mount,'dev-entry':'/dev/disk4s1'},{'mount-point':'/Volumes/other','dev-entry':'/dev/disk5'}],mount),/exactly one/);
  assert.throws(()=>mountedVolume([{'mount-point':mount,'dev-entry':'/dev/disk4; touch unsafe'}],mount));
});
test('DMG root admits the sole app and expected Applications shortcut only',context=>{
  const root=fixture(context),bundle=app(root);
  if(!symlink(context,'/Applications',path.join(root,'Applications')))return;
  assert.equal(validateDmgRoot(root),bundle);
  fs.mkdirSync(path.join(root,'Another.app'));assert.throws(()=>validateDmgRoot(root),/one application/);
});
test('DMG root rejects a redirected Applications shortcut',context=>{
  const root=fixture(context);app(root);
  if(!symlink(context,'/tmp/other',path.join(root,'Applications')))return;
  assert.throws(()=>validateDmgRoot(root),/shortcut destination/);
});
test('all embedded Mach-O files must be thin arm64, including framework binaries',async context=>{
  const root=fixture(context),bundle=app(root),framework=path.join(bundle,'Contents/Frameworks/test.dylib');fs.mkdirSync(path.dirname(framework));
  fs.writeFileSync(framework,macho());const result=await inspectMacApp(bundle);assert.equal(result.native.length,3);assert.equal(result.inventory.files.length,3);
  fs.writeFileSync(framework,macho(0x01000007));await assert.rejects(inspectMacApp(bundle),/Apple Silicon/);
  fs.writeFileSync(framework,Buffer.from('cafebabe','hex'));await assert.rejects(inspectMacApp(bundle),/Universal/);
});
test('application inspection admits internal framework links and rejects escaping links',async context=>{
  const root=fixture(context),bundle=app(root),framework=path.join(bundle,'Contents/Frameworks/Test.framework');fs.mkdirSync(path.join(framework,'Versions/A'),{recursive:true});
  if(!symlink(context,'A',path.join(framework,'Versions/Current')))return;
  assert.ok((await inspectMacApp(bundle)).inventory.files.some(file=>file.path.endsWith('Versions/Current')));
  fs.mkdirSync(path.join(root,'outside'));fs.symlinkSync('../../outside',path.join(bundle,'Contents/escape'),'dir');await assert.rejects(inspectMacApp(bundle),/escapes/);
});
test('app removal preserves sibling research fixtures and refuses an outside app',context=>{
  const root=fixture(context),install=path.join(root,'install');fs.mkdirSync(install);const bundle=app(install),data=path.join(root,'fixture-data');fs.mkdirSync(data);fs.writeFileSync(path.join(data,'sentinel'),'preserve');
  assert.throws(()=>removeMacApp(install,app(root)),/boundary/);
  removeMacApp(install,bundle);assert.ok(!fs.existsSync(bundle));assert.equal(fs.readFileSync(path.join(data,'sentinel'),'utf8'),'preserve');
});
