import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {command,inside,inventory,machine,writeJSON} from './common.mjs';

export function payloadLayout(resourcesPath,platform=process.platform){
  const resources=path.resolve(resourcesPath);
  if(platform==='darwin'){
    assert.equal(path.basename(resources),'Resources');assert.equal(path.basename(path.dirname(resources)),'Contents');
    const root=path.dirname(path.dirname(resources));assert.equal(path.basename(root),'PhaseForge.app');
    return {root,resourcesRelative:path.join('Contents','Resources')};
  }
  assert.ok(['win32','linux'].includes(platform));
  return {root:path.dirname(resources),resourcesRelative:path.basename(resources)};
}

export function mountedVolume(entities,expected){
  assert.ok(Array.isArray(entities)&&entities.length<=32,'Unexpected DMG mount receipt');
  const volumes=entities.filter(entry=>Object.hasOwn(entry,'mount-point'));
  assert.equal(volumes.length,1,'DMG must mount exactly one volume');
  assert.equal(path.resolve(volumes[0]['mount-point']),path.resolve(expected),'DMG mounted outside its isolated mountpoint');
  assert.match(volumes[0]['dev-entry'],/^\/dev\/disk\d+(?:s\d+)*$/);
  return {mount_point:volumes[0]['mount-point'],device:volumes[0]['dev-entry']};
}

export function validateDmgRoot(directory){
  const entries=fs.readdirSync(directory,{withFileTypes:true});
  const apps=entries.filter(entry=>entry.name.endsWith('.app'));
  assert.equal(apps.length,1,'DMG must contain one application');
  assert.equal(apps[0].name,'PhaseForge.app');assert.ok(apps[0].isDirectory()&&!apps[0].isSymbolicLink(),'Application root must be a real directory');
  // Pinned dmg-builder 1.2.5 / dmgbuild 75c8a6c writes the configured
  // electron-builder templates/background.tiff as this regular root file.
  const allowed=new Set(['PhaseForge.app','Applications','.background','.background.tiff','.DS_Store','.VolumeIcon.icns','.fseventsd','.Trashes']);
  const unexpected=entries.filter(entry=>!allowed.has(entry.name)).map(entry=>({name:entry.name,type:entry.isSymbolicLink()?'symlink':entry.isDirectory()?'directory':entry.isFile()?'file':'other'}));
  assert.equal(unexpected.length,0,`Unexpected DMG root entries: ${JSON.stringify(unexpected)}`);
  const background=entries.find(entry=>entry.name==='.background.tiff');
  if(background){assert.ok(background.isFile()&&!background.isSymbolicLink(),'DMG background must be a regular file');assert.ok(fs.statSync(path.join(directory,background.name)).size<=16*1024**2,'DMG background exceeds its metadata budget');}
  const applications=path.join(directory,'Applications');
  assert.ok(fs.lstatSync(applications).isSymbolicLink(),'DMG must contain its expected Applications shortcut');
  assert.equal(fs.readlinkSync(applications),'/Applications','Unexpected Applications shortcut destination');
  return path.join(directory,'PhaseForge.app');
}

export async function inspectMacApp(bundle){
  const boundary=fs.realpathSync(bundle),pending=[boundary],native=[];let entries=0;
  while(pending.length)for(const entry of fs.readdirSync(pending.pop(),{withFileTypes:true})){
    if(++entries>100000)throw Error('Application tree exceeds its entry budget');
    const file=path.join(entry.parentPath||entry.path,entry.name);
    if(entry.isSymbolicLink()){
      assert.ok(!path.isAbsolute(fs.readlinkSync(file)),'Application framework links must remain relative');
      assert.ok(fs.realpathSync(file).startsWith(boundary+path.sep),'Application link escapes its bundle');
    }else if(entry.isDirectory())pending.push(file);
    else if(entry.isFile()){
      const descriptor=fs.openSync(file,'r'),bytes=Buffer.alloc(4);try{fs.readSync(descriptor,bytes,0,4,0);}finally{fs.closeSync(descriptor);}
      if(['cffaedfe','feedfacf','cefaedfe','feedface','cafebabe','bebafeca','cafebabf','bfbafeca'].includes(bytes.toString('hex'))){
        const identity=machine(file);assert.equal(identity.format,'macho','Universal Mach-O files are not admitted');assert.equal(identity.architecture,'arm64','Every packaged Mach-O file must target Apple Silicon');assert.equal(identity.bits,64);
        native.push({path:path.relative(boundary,file).replaceAll(path.sep,'/'),...identity});
      }
    }else throw Error('Unsupported application filesystem entry');
  }
  for(const relative of ['Contents/MacOS/PhaseForge','Contents/Resources/runtime/phaseforge-backend'])assert.ok(native.some(row=>row.path===relative),'Required native executable missing from the application');
  return {native,inventory:await inventory(bundle)};
}

function observedCommand(executable,args){
  const result=spawnSync(executable,args,{encoding:'utf8',timeout:30000,maxBuffer:1024*1024,windowsHide:true});
  if(result.error)throw result.error;
  return {executable,args,status:result.status,signal:result.signal,stdout:result.stdout,stderr:result.stderr};
}

export function macSignatureObservations(bundle){
  assert.equal(process.platform,'darwin');
  const display=observedCommand('/usr/bin/codesign',['--display','--verbose=4',bundle]);
  const verify=observedCommand('/usr/bin/codesign',['--verify','--deep','--strict','--verbose=2',bundle]);
  const assessment=observedCommand('/usr/sbin/spctl',['--assess','--type','execute','--verbose=4',bundle]);
  const quarantine=observedCommand('/usr/bin/xattr',['-p','com.apple.quarantine',bundle]);
  const text=display.stdout+display.stderr;
  return {signature_observation:/Signature=adhoc/.test(text)?'ad-hoc':/not signed at all/.test(text)?'unsigned':display.status===0?'signature-present-see-raw-observation':'undetermined',display,verify,gatekeeper_assessment:assessment,quarantine_attribute:quarantine,scope:'Read-only observations. No signing identity, notarization, quarantine mutation or security-setting change. A direct instrumented executable launch does not certify Finder/Gatekeeper handling of a quarantined internet download.'};
}

export async function installMacDmg(artifact,install,mountpoint,evidence,python){
  assert.equal(process.platform,'darwin');assert.equal(process.arch,'arm64');
  assert.ok(!fs.existsSync(install)&&!fs.existsSync(mountpoint),'DMG installation destinations must be new');
  fs.mkdirSync(mountpoint);let mounted=false,receipt;
  const parse=xml=>JSON.parse(command(python,['-c','import json,plistlib,sys;print(json.dumps(plistlib.loads(sys.stdin.buffer.read())))'],{input:xml}));
  try{
    const verification=command('/usr/bin/hdiutil',['verify',artifact],{timeout:120000});
    const raw=command('/usr/bin/hdiutil',['attach','-readonly','-nobrowse','-mountpoint',mountpoint,'-plist',artifact],{timeout:120000});
    mounted=true;
    const volume=mountedVolume(parse(raw)['system-entities'],mountpoint);
    const rootEntries=fs.readdirSync(mountpoint,{withFileTypes:true});assert.ok(rootEntries.length<=128,'DMG root exceeds its entry budget');
    writeJSON(path.join(path.dirname(evidence),'macos-dmg-root.json'),{volume,entries:rootEntries.map(entry=>({name:entry.name,type:entry.isSymbolicLink()?'symlink':entry.isDirectory()?'directory':entry.isFile()?'file':'other'})),scope:'Actual read-only mounted DMG root, recorded before validation. This listing does not admit or execute any entry.'});
    const source=validateDmgRoot(mountpoint),inspection=await inspectMacApp(source),before=inspection.inventory;
    fs.mkdirSync(install);
    const bundle=inside(install,path.join(install,'PhaseForge.app'));
    command('/usr/bin/ditto',[source,bundle],{timeout:120000});
    assert.deepEqual(await inventory(bundle),before,'Copied application differs from the actual mounted DMG');
    receipt={method:'Verified DMG mounted read-only and without browsing; sole PhaseForge.app copied with ditto into a new isolated installation directory',volume,dmg_verification:verification,application_inventory_entries:before.files.length,application_bytes:before.bytes,native_files:inspection.native,copy_exact:true,bundle,signature:macSignatureObservations(bundle),scope:'Only the copied app is installed. The DMG Applications symlink is verified but never followed or copied into /Applications.'};
    writeJSON(evidence,receipt);
    return {executable:path.join(bundle,'Contents/MacOS/PhaseForge'),bundle,receipt};
  }finally{
    // Detach only the explicit mountpoint created by this invocation. A failed
    // attach can leave it mounted before the plist was returned.
    if(!mounted){
      const info=parse(command('/usr/bin/hdiutil',['info','-plist']));
      mounted=(info.images||[]).some(image=>(image['system-entities']||[]).some(entry=>entry['mount-point']===mountpoint));
    }
    if(mounted)command('/usr/bin/hdiutil',['detach',mountpoint],{timeout:60000});
    fs.rmdirSync(mountpoint);
    if(receipt){receipt.detached=true;writeJSON(evidence,receipt);}
  }
}

export function removeMacApp(install,bundle){
  assert.ok(fs.lstatSync(install).isDirectory()&&!fs.lstatSync(install).isSymbolicLink(),'Application installation boundary must be a real directory');
  const verified=inside(install,bundle,{existing:true});assert.equal(path.basename(verified),'PhaseForge.app');
  assert.equal(path.dirname(verified),path.resolve(install),'Removal is limited to the direct installed application');
  assert.ok(fs.lstatSync(verified).isDirectory());fs.rmSync(verified,{recursive:true});
  assert.equal(fs.existsSync(verified),false);
}
