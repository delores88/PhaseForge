/** Exercises actual distributed packages on an isolated GitHub-hosted desktop. */
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {_electron} from 'playwright-core';
import {ROOT,command,copyEvidenceTree,hostedWorkspace,inside,inventory,machine,readJSON,releaseTarget,sha256,sleep,writeJSON} from './common.mjs';
import {startInertListeners} from './legacy-listeners.mjs';
import {installMacDmg,payloadLayout,removeMacApp} from './native-platform.mjs';
import {createLaboratoryAcceptance} from './native-laboratory.mjs';
import {observeManagedSqlite} from './sqlite-runtime.mjs';
import {observeManagedOpenSSL} from './openssl-runtime.mjs';

const version=readJSON(path.join(ROOT,'desktop/package.json')).version;
const {platform,architecture,folder:targetFolder}=releaseTarget();
const output=path.join(ROOT,'.local/marketplace',targetFolder);
const python=process.env.PHASEFORGE_ACCEPTANCE_PYTHON||'python';
const probe=(mode,args=[])=>JSON.parse(command(python,[path.join(ROOT,'scripts/release/process_probe.py'),mode,...args]));
const uiPort=7332;
let legacyListeners;
async function listening(port){return new Promise(resolve=>{const socket=net.connect({host:'127.0.0.1',port});socket.setTimeout(1000);socket.once('connect',()=>{socket.destroy();resolve(true);});socket.once('error',()=>resolve(false));socket.once('timeout',()=>{socket.destroy();resolve(false);});});}
async function requireClosed(ports=[uiPort]){for(const port of ports)if(await listening(port))throw Error(`Port ${port} is occupied; existing applications are never changed by acceptance`);}
async function waitClosed(backendPort){const ports=[uiPort,backendPort];for(let i=0;i<30;i++){if(!(await Promise.all(ports.map(listening))).some(Boolean))return;await sleep(500);}throw Error('An owned native service remained listening after normal Quit');}
function ownedFile(file){return inside(output,file,{existing:true});}
async function call(page,route,body){return page.evaluate(async({route,body})=>{
  const response=await fetch(route,{method:body===undefined?'GET':'POST',headers:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});
  const result=await response.json();if(!response.ok)throw Error(`Acceptance API ${response.status}: ${JSON.stringify(result)}`);return result;
},{route,body});}
async function finishRun(page,run){
  const deadline=Date.now()+45000;
  while(['queued','running'].includes(run.status)&&Date.now()<deadline){await sleep(150);run=await call(page,`/api/runs/${run.id}`);}
  assert.equal(run.status,'completed',JSON.stringify(run.error));return run;
}
async function offlineExperiment(page){
  const project=await call(page,'/api/projects',{name:'Release acceptance fixture',question:'Deterministic offline numerical-engine and persistence check'});
  const input=readJSON(path.join(ROOT,'scripts/release/offline-experiment.json'));
  const saved=(await call(page,`/api/projects/${project.id}/manifests`,{manifest:input,auto_run:false})).manifest;
  const run=await finishRun(page,await call(page,`/api/manifests/${saved.id}/run`,{}));
  assert.ok(Math.abs(run.result.metrics.final_x-1)<1e-8,'Installed RK4 result is incorrect');
  assert.ok(run.result.visualization.frames.length>=90,'Installed engine did not retain its actual numerical frames');
  const next=await call(page,`/api/projects/${project.id}/experiments/next`,{request_id:crypto.randomUUID(),source_run_id:run.id,operation:'finer_steps',run:true});
  const finer=await finishRun(page,next.run);assert.ok(Math.abs(finer.result.metrics.final_x-1)<1e-8);
  const usage=await call(page,'/api/usage');assert.equal(usage.totals.total_tokens,0,'A release fixture must never call a paid model');
  const result={project,manifest_id:saved.id,run_id:run.id,refined_run_id:finer.id,final_x:run.result.metrics.final_x,retained_frames:run.result.visualization.frames.length,provider_tokens:0};
  writeJSON(path.join(output,'offline-experiment-result.json'),result);return result;
}

async function observeBrand(page,assetPath){
  const expected=readJSON(path.join(ROOT,'frontend/public/brand/provenance.json')).files.find(row=>row.path===`frontend/public${assetPath}`);
  assert.ok(expected,'The observed logo must have original artwork provenance');
  assert.equal(expected.source,`02_SVG/${path.basename(assetPath)}`);
  assert.equal(expected.transformation,'none; exact original bytes');
  const observed=await page.evaluate(async assetPath=>{
    const sidebar=document.querySelector('.primarySidebar'),brand=document.querySelector('.primaryBrand');
    const bounds=element=>{const {x,y,width,height,right,bottom}=element.getBoundingClientRect();return {x,y,width,height,right,bottom};};
    const visible=[...brand.querySelectorAll('img')].filter(image=>{
      const style=getComputedStyle(image),rect=image.getBoundingClientRect();
      return rect.width>0&&rect.height>0&&style.display!=='none'&&style.visibility==='visible'&&Number(style.opacity)>0;
    });
    if(visible.length!==1)throw Error(`Expected one visible original logo, found ${visible.length}`);
    const image=visible[0],url=new URL(image.currentSrc);
    if(url.origin!==location.origin||url.pathname!==assetPath)throw Error('Displayed branding uses an unexpected artwork URL');
    if(!image.complete||!image.naturalWidth||!image.naturalHeight)throw Error('Displayed original artwork has not loaded');
    const response=await fetch(url.href);if(!response.ok)throw Error('Displayed artwork could not be read');
    const bytes=await response.arrayBuffer();if(bytes.byteLength>2_000_000)throw Error('Unexpectedly large logo');
    const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),byte=>byte.toString(16).padStart(2,'0')).join('');
    const canvas=document.createElement('canvas');canvas.width=256;canvas.height=Math.max(1,Math.round(256*image.naturalHeight/image.naturalWidth));
    if(canvas.height>1024)throw Error('Unexpected logo aspect ratio');
    const context=canvas.getContext('2d',{willReadFrequently:true});context.drawImage(image,0,0,canvas.width,canvas.height);
    const pixels=context.getImageData(0,0,canvas.width,canvas.height).data;let bluePixels=0;
    for(let i=0;i<pixels.length;i+=4)if(pixels[i+3]>32&&pixels[i+2]>pixels[i]+30&&Math.max(...pixels.subarray(i,i+3))-Math.min(...pixels.subarray(i,i+3))>40)bluePixels++;
    const ancestors=[];
    for(let element=image;element;element=element.parentElement){const style=getComputedStyle(element);ancestors.push({tag:element.tagName,filter:style.filter,opacity:style.opacity,mix_blend_mode:style.mixBlendMode});}
    return {theme:document.documentElement.dataset.theme,viewport:{width:innerWidth,height:innerHeight},sidebar:bounds(sidebar),image:{url:url.pathname,bytes:bytes.byteLength,sha256:digest,bounds:bounds(image),natural_width:image.naturalWidth,natural_height:image.naturalHeight,blue_pixels:bluePixels,raster_width:canvas.width,raster_height:canvas.height},ancestors,app_background:getComputedStyle(document.querySelector('.appFrame')).backgroundColor};
  },assetPath);
  assert.equal(observed.theme,'dark','Marketplace screenshots must show the actual dark UI');
  assert.equal(observed.image.sha256,expected.sha256);assert.equal(observed.image.bytes,expected.bytes);
  assert.ok(observed.image.blue_pixels>16,'The loaded logo must contain the original blue artwork, not a monochrome symbol');
  assert.ok(observed.ancestors.every(style=>style.filter==='none'&&Number(style.opacity)===1&&style.mix_blend_mode==='normal'),'CSS must not recolor or fade the original artwork');
  const background=observed.app_background.match(/^rgb\((\d+), (\d+), (\d+)\)$/);
  assert.ok(background&&background.slice(1).every(value=>Number(value)<80),'The rendered application background must be dark');
  const rect=observed.image.bounds;
  assert.ok(rect.x>=observed.sidebar.x&&rect.right<=observed.sidebar.right&&rect.y>=0&&rect.bottom<=observed.viewport.height,'The full artwork must fit visibly within the native sidebar');
  return observed;
}

async function brandingScreenshots(electronApp,page,folder){
  const nativeWindow=await electronApp.browserWindow(page);
  await nativeWindow.evaluate(window=>{window.setContentSize(1008,700);window.setPosition(0,0);});
  await page.waitForFunction(()=>innerWidth===1008&&document.querySelector('.primarySidebar')?.getBoundingClientRect().width===190,{},{timeout:10000});
  await page.getByRole('button',{name:'Switch to light mode',exact:true}).waitFor({state:'visible'});
  await page.locator('.primaryBrandImage.brandDark').waitFor({state:'visible'});
  const expanded=await observeBrand(page,'/brand/phaseforge-horizontal-dark.svg');
  writeJSON(path.join(folder,'branding-expanded-observation.json'),{source_commit:process.env.GITHUB_SHA,observed:expanded});
  await page.screenshot({path:path.join(folder,'window.png'),scale:'css'});
  assert.equal(expanded.viewport.width,1008);assert.equal(expanded.sidebar.width,190);
  assert.ok(expanded.image.bounds.width>=160,'Expanded navigation must retain the full readable wordmark');
  await page.getByRole('button',{name:'Collapse navigation',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('.appFrame')?.classList.contains('appFrame--compact')&&document.querySelector('.primarySidebar')?.getBoundingClientRect().width===64);
  await page.locator('.primaryBrandSymbol.brandDark').waitFor({state:'visible'});
  const compact=await observeBrand(page,'/brand/phaseforge-symbol-dark.svg');
  writeJSON(path.join(folder,'branding-compact-observation.json'),{source_commit:process.env.GITHUB_SHA,observed:compact});
  await page.screenshot({path:path.join(folder,'window-compact.png'),scale:'css'});
  assert.equal(compact.sidebar.width,64);assert.ok(compact.image.bounds.width>=48&&compact.image.bounds.height>=38,'Compact navigation must show the readable original color symbol');
  await page.getByRole('button',{name:'Expand navigation',exact:true}).click();
  await page.waitForFunction(()=>!document.querySelector('.appFrame')?.classList.contains('appFrame--compact')&&document.querySelector('.primarySidebar')?.getBoundingClientRect().width===190);
  await nativeWindow.dispose();
  const receipt={passed:true,source_commit:process.env.GITHUB_SHA,expanded,compact,screenshot_theme:'dark',scope:'Actual native BrowserWindow resized to 1008 content pixels; loaded original SVG byte identities, blue raster pixels, CSS treatment and visible bounds observed in its renderer. Compact navigation is exercised through its visible control.'};
  writeJSON(path.join(folder,'branding.json'),receipt);return receipt;
}

async function session(executable,workspace,index,fixture){
  await requireClosed();const folder=path.join(output,`launch-${index}`);fs.mkdirSync(folder,{recursive:true});
  const freshProfile=!fs.existsSync(path.join(workspace,'electron-profile'));
  if(index===1||index===3)assert.ok(freshProfile,'Default-theme acceptance requires a genuinely fresh Electron profile');
  const environment={...process.env,PHASEFORGE_DATA_DIR:workspace,PHASEFORGE_DISABLE_GPU:'1'};
  for(const key of ['OPENAI_API_KEY','ANTHROPIC_API_KEY','GH_TOKEN','GITHUB_TOKEN','ACTIONS_ID_TOKEN_REQUEST_TOKEN','ACTIONS_ID_TOKEN_REQUEST_URL','NODE_OPTIONS','ELECTRON_RUN_AS_NODE'])delete environment[key];
  for(const key of Object.keys(environment))if(/(?:TOKEN|SECRET|API_KEY|PASSWORD|CREDENTIAL)/i.test(key))delete environment[key];
  const known=new Map(),owners=new Map(),errors=[],requests=[],began=Date.now();let electronApp,appPID,peakRSS=0,peakSample=null;
  const identities=path.join(folder,'owned-processes.json');
  const trackOwner=pid=>{
    const records=probe('snapshot',['--pid',String(pid)]),owner=records.find(record=>record.pid===pid);
    if(owner)owners.set(pid,owner.created);
    for(const record of records)known.set(`${record.pid}:${record.created}`,record);
    writeJSON(identities,[...known.values()]);
  };
  const remember=()=>{
    const sample=new Map();
    for(const [pid,created] of owners){
      for(const record of probe('snapshot',['--pid',String(pid),'--created',String(created)])){
        const key=`${record.pid}:${record.created}`;known.set(key,record);sample.set(key,record);
      }
    }
    const rows=[...sample.values()],rss=rows.reduce((sum,item)=>sum+item.rss_bytes,0);
    if(rss>peakRSS){peakRSS=rss;peakSample={at:new Date().toISOString(),rss_bytes:rss,processes:rows};writeJSON(path.join(folder,'peak-process-sample.json'),peakSample);}
    writeJSON(identities,[...known.values()]);return rows;
  };
  try{
    electronApp=await _electron.launch({executablePath:executable,args:[`--user-data-dir=${path.join(workspace,'electron-profile')}`],env:environment,cwd:path.dirname(executable),chromiumSandbox:true,timeout:90000});
    if(electronApp.process().exitCode===null)trackOwner(electronApp.process().pid);
    const runtime=await electronApp.evaluate(({app})=>({pid:process.pid,architecture:process.arch,platform:process.platform,execPath:process.execPath,resourcesPath:process.resourcesPath,versions:process.versions,appVersion:app.getVersion(),packaged:app.isPackaged,appPath:app.getAppPath(),userData:app.getPath('userData')}));
    appPID=runtime.pid;trackOwner(appPID);remember();assert.equal(runtime.packaged,true);assert.equal(runtime.appVersion,version);
    assert.equal(runtime.architecture,architecture);assert.equal(runtime.platform,process.platform);
    const page=await electronApp.firstWindow({timeout:90000});
    page.on('pageerror',error=>errors.push(error.message));
    page.on('request',request=>{const url=new URL(request.url());if(['http:','https:'].includes(url.protocol)&&url.hostname!=='127.0.0.1')requests.push(url.origin);});
    await page.waitForLoadState('domcontentloaded');await page.locator('a.primaryBrand').waitFor({state:'visible',timeout:30000});
    const appearance=()=>page.evaluate(()=>({theme:document.documentElement.dataset.theme,stored_theme:localStorage.getItem('phaseforge.theme'),prefers_light:matchMedia('(prefers-color-scheme: light)').matches}));
    const initialAppearance=await appearance();
    if(freshProfile){assert.equal(initialAppearance.stored_theme,null,'A fresh profile must have no seeded theme preference');assert.equal(initialAppearance.theme,'dark','The untouched fresh installation must default to dark mode');}
    else{assert.equal(index,2);assert.equal(initialAppearance.stored_theme,'light','The explicitly saved light preference must survive normal Quit');assert.equal(initialAppearance.theme,'light','Reopening must honor the explicitly saved light preference');}
    writeJSON(path.join(folder,'initial-appearance.json'),{fresh_profile:freshProfile,...initialAppearance});
    if(index===1){
      await page.emulateMedia({colorScheme:'light'});await page.reload({waitUntil:'domcontentloaded'});
      await page.locator('a.primaryBrand').waitFor({state:'visible'});
      const underLightPreference=await appearance();
      assert.equal(underLightPreference.prefers_light,true);assert.equal(underLightPreference.stored_theme,null);assert.equal(underLightPreference.theme,'dark','An OS light preference must not replace the fresh dark default');
      writeJSON(path.join(folder,'default-under-light-preference.json'),{passed:true,...underLightPreference,scope:'The actual packaged entry page reloaded with Playwright renderer prefers-color-scheme: light emulation; no theme storage or DOM override, and no native operating-system setting change.'});
      await page.emulateMedia({colorScheme:null});
    }else if(index===2){
      await page.getByRole('button',{name:'Switch to dark mode',exact:true}).click();
      await page.waitForFunction(()=>document.documentElement.dataset.theme==='dark'&&localStorage.getItem('phaseforge.theme')==='dark');
    }
    const health=await call(page,'/api/health');assert.equal(health.status,'ok');assert.equal(health.version,version);assert.equal(health.local_only,true);
    assert.match(health.sqlite?.version||'',/^\d+\.\d+\.\d+$/,'Actual backend SQLite version is required');
    assert.ok(Number.isInteger(health.sqlite.version_number));assert.match(health.sqlite.source_id,/^[0-9-]+ [0-9:]+ [a-f0-9]+$/);
    assert.equal(health.sqlite.linkage,'bundled');
    assert.equal(health.sqlite.version,'3.53.2','This candidate requires its reviewed SQLite baseline');assert.equal(health.sqlite.version_number,3053002);
    assert.equal(runtime.versions.electron,readJSON(path.join(ROOT,'desktop/package.json')).devDependencies.electron,'Observed Electron differs from the locked package input');
    assert.equal(health.endpoint?.address,'127.0.0.1');
    const backendPort=health.endpoint.port;assert.ok(Number.isInteger(backendPort)&&backendPort>0&&backendPort<=65535);
    assert.ok(![3000,7331,uiPort].includes(backendPort),'Packaged backend must use its own OS-assigned port');
    const metadata=readJSON(path.join(runtime.resourcesPath,'runtime/build.json'));
    assert.equal(metadata.sourceCommit,process.env.GITHUB_SHA);assert.equal(metadata.sourceDirty,false);
    const backend=path.join(runtime.resourcesPath,'runtime',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');
    assert.equal(machine(runtime.execPath).architecture,architecture);assert.equal(machine(backend).architecture,architecture);
    const samePath=(a,b)=>process.platform==='win32'?path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase():path.resolve(a)===path.resolve(b);
    const children=remember();assert.ok(children.some(item=>samePath(item.exe,backend)),'Packaged backend must belong to the launched application');
    const readySeconds=(Date.now()-began)/1000;
    if(index===1){
      fixture=await offlineExperiment(page);
      const payload=path.join(output,'payload');assert.ok(!fs.existsSync(payload),'Payload copy must be new');
      const layout=payloadLayout(runtime.resourcesPath);
      copyEvidenceTree(layout.root,payload);
      const copiedBackend=path.join(payload,layout.resourcesRelative,'runtime',path.basename(backend));
      assert.equal(await sha256(copiedBackend),await sha256(backend));
      writeJSON(path.join(output,'installed-payload.json'),{source_commit:process.env.GITHUB_SHA,version,runtime,resources_relative:layout.resourcesRelative.replaceAll(path.sep,'/'),backend:{sha256:await sha256(backend),machine:machine(backend)},...await inventory(payload)});
    }else{
      const projects=await call(page,'/api/projects');const rows=Array.isArray(projects)?projects:projects.projects;
      assert.ok(rows.some(row=>row.id===fixture.project.id),'Reopened installed app lost its fixture project');
      const retained=await call(page,`/api/runs/${fixture.run_id}`);assert.equal(retained.status,'completed');assert.equal(retained.result.metrics.final_x,fixture.final_x);
    }
    const laboratory=platform==='windows'?createLaboratoryAcceptance({page,call,remember,observedProcesses:()=>[...known.values()],workspace,output,projectId:fixture.project.id,sourceCommit:process.env.GITHUB_SHA,version,backendSha256:await sha256(backend)}):null;
    if(laboratory){if(index===1)await laboratory.firstLaunch();else await laboratory.reopen(index);}
    await page.goto(`http://127.0.0.1:7332/?project=${fixture.project.id}&run=${fixture.run_id}`,{waitUntil:'domcontentloaded'});
    await page.getByRole('button',{name:'Results',exact:true}).click({timeout:30000});
    await page.getByText('What we can say now',{exact:true}).waitFor({timeout:30000});
    writeJSON(path.join(folder,'findings-report.json'),await call(page,`/api/runs/${fixture.run_id}/findings`));
    for(let i=0;i<6;i++){const records=remember();peakRSS=Math.max(peakRSS,records.reduce((sum,item)=>sum+item.rss_bytes,0));assert.equal((await call(page,'/api/health')).status,'ok');await sleep(2000);}
    assert.deepEqual(requests,[],'Native fixture unexpectedly requested an external website');
    assert.deepEqual(errors,[],'Packaged page reported an uncaught JavaScript error');
    const branding=await brandingScreenshots(electronApp,page,folder);
    assert.deepEqual(requests,[]);assert.deepEqual(errors,[]);
    writeJSON(path.join(folder,'runtime.json'),{runtime,health,build:metadata,initial_appearance:{fresh_profile:freshProfile,...initialAppearance},branding,page_errors:errors,external_origins:requests,ready_seconds:readySeconds,peak_sampled_tree_rss_bytes:peakRSS});
    if(index===1){
      await page.getByRole('button',{name:'Switch to light mode',exact:true}).click();
      await page.waitForFunction(()=>document.documentElement.dataset.theme==='light'&&localStorage.getItem('phaseforge.theme')==='light');
    }
    writeJSON(path.join(folder,'saved-theme-before-quit.json'),await appearance());
    assert.deepEqual(requests,[]);assert.deepEqual(errors,[]);
    if(laboratory&&index===1)await laboratory.beforeQuit();
    remember();const closed=electronApp.waitForEvent('close',{timeout:30000});
    const launcher=electronApp.process();
    const nativeExit=launcher.exitCode!==null||launcher.signalCode!==null?Promise.resolve({code:launcher.exitCode,signal:launcher.signalCode}):new Promise(resolve=>launcher.once('exit',(code,signal)=>resolve({code,signal})));
    await electronApp.close();await closed;const exit=await nativeExit;assert.equal(exit.code,0,'Normal application Quit returned a failure code');await waitClosed(backendPort);
    let survivors=[];
    for(let i=0;i<20;i++){survivors=probe('alive',['--identities',identities]);if(!survivors.length)break;await sleep(500);}
    assert.deepEqual(survivors,[],'An owned process survived normal app Quit');
    const result={passed:true,source_commit:process.env.GITHUB_SHA,version,ready_seconds:readySeconds,normal_quit:true,owned_processes_stopped:true,closed_backend_endpoint:health.endpoint,observed_roots:[...owners].map(([pid,created])=>({pid,created})),peak_sampled_tree_rss_bytes:peakRSS,provider_tokens:0,instrumentation:'Playwright launches the actual installed executable with local debugger transport; normal Electron app.quit lifecycle, sandbox enabled. Live launcher and Electron root identities plus observed descendants are tracked; PID reuse is rejected. This is not an uninstrumented launch certification.'};
    writeJSON(path.join(folder,'result.json'),result);return fixture;
  }finally{
    if(electronApp)await electronApp.close().catch(()=>{});
    if(fs.existsSync(identities)&&probe('alive',['--identities',identities]).length)probe('cleanup',['--identities',identities]);
    const engineLog=path.join(workspace,'electron-profile/logs/engine.log');
    if(fs.existsSync(engineLog)){
      const bytes=fs.statSync(engineLog).size,descriptor=fs.openSync(engineLog,'r'),tail=Buffer.alloc(Math.min(bytes,65536));
      try{fs.readSync(descriptor,tail,0,tail.length,Math.max(0,bytes-tail.length));}finally{fs.closeSync(descriptor);}
      writeJSON(path.join(folder,'engine-log.json'),{total_bytes:bytes,tail_utf8:tail.toString('utf8'),scope:'Bounded engine log from the isolated credential-free fixture'});
    }
  }
}

async function main(){
  const temporary=hostedWorkspace();await requireClosed();
  if(fs.existsSync(output)&&fs.readdirSync(output).some(name=>name.startsWith('launch-')||name==='payload'))throw Error('Native acceptance evidence must not overwrite a prior run');
  fs.mkdirSync(output,{recursive:true});
  const suffix=process.platform==='win32'?'-setup.exe':process.platform==='darwin'?'.dmg':'.AppImage';
  const artifacts=fs.readdirSync(path.join(ROOT,'desktop/dist')).filter(name=>name.endsWith(suffix));
  assert.equal(artifacts.length,1,'Exactly one admitted installer must be produced per native job');
  const artifact=path.join(ROOT,'desktop/dist',artifacts[0]);
  const root=inside(temporary,path.join(temporary,`phaseforge-acceptance-${process.env.GITHUB_RUN_ID}-${platform}`));
  assert.ok(!fs.existsSync(root),'Acceptance installation directory must be new');fs.mkdirSync(root);
  const install=inside(root,path.join(root,'install')),workspace=inside(root,path.join(root,'fixture-data'));
  fs.mkdirSync(workspace);fs.writeFileSync(path.join(workspace,'preserve.txt'),'PhaseForge disposable installation fixture\n');
  legacyListeners=await startInertListeners([3000,7331].flatMap(port=>['127.0.0.1','::1'].map(address=>({address,port}))));
  let executable,macInstallation;
  if(process.platform==='win32'){
    assert.ok(!install.includes(' '),'NSIS /D acceptance path must not require shell quoting');
    command(artifact,['/S','/ACCEPT_MSVC_TERMS=MSVC-2026-09-11',`/D=${install}`],{timeout:180000});executable=path.join(install,'PhaseForge.exe');
  }else if(process.platform==='darwin'){
    macInstallation=await installMacDmg(artifact,install,inside(root,path.join(root,'dmg-mount')),path.join(output,'macos-install.json'),python);
    executable=macInstallation.executable;
  }else{
    fs.mkdirSync(install);executable=path.join(install,path.basename(artifact));fs.copyFileSync(artifact,executable);fs.chmodSync(executable,0o755);
    assert.equal(machine(executable).architecture,'x64');
  }
  assert.ok(fs.statSync(executable).isFile(),'The actual installer did not create the application');
  const host=probe('host');
  if(process.platform==='darwin'){
    assert.equal(host.machine,'arm64');host.macos_version=command('/usr/bin/sw_vers',['-productVersion']);host.macos_build=command('/usr/bin/sw_vers',['-buildVersion']);
  }
  const fixture=await session(executable,workspace,1,null);await session(executable,workspace,2,fixture);
  if(platform==='windows'){
    const observed=readJSON(path.join(output,'installed-payload.json'));
    writeJSON(path.join(output,'managed-sqlite-runtime.json'),await observeManagedSqlite({workspace,sourceCommit:process.env.GITHUB_SHA}));
    writeJSON(path.join(output,'managed-openssl-runtime.json'),await observeManagedOpenSSL({workspace,sourceCommit:process.env.GITHUB_SHA}));
    const args=['-B',path.join(ROOT,'scripts/release/managed_runtime.py'),'--workspace',workspace,'--seed-root',path.join(observed.runtime.resourcesPath,'runtime/runtime-seeds'),'--source-commit',process.env.GITHUB_SHA,'--output',path.join(output,'managed-runtime-materials.json')];
    args.push('--archive-root',fs.realpathSync.native(path.join(process.env.RUNNER_TEMP,'phaseforge-seed-archives')));
    if(process.env.PHASEFORGE_SCIENCE_BOOTSTRAP_PYTHON)args.push('--bootstrap-python',process.env.PHASEFORGE_SCIENCE_BOOTSTRAP_PYTHON);
    command(python,args,{timeout:180000,maxBuffer:1024*1024});
    assert.equal(readJSON(path.join(output,'managed-runtime-materials.json')).integrity_valid,true);
  }
  const backup=inside(root,path.join(root,'closed-data-backup')),restored=inside(root,path.join(root,'restored-data'));
  // All SQLite connections are closed before this file-level backup. Include any WAL/SHM files.
  copyEvidenceTree(workspace,backup,{filter:source=>path.relative(workspace,source).split(path.sep)[0]!=='electron-profile'});
  copyEvidenceTree(backup,restored);
  const originalDatabase=path.join(workspace,'phaseforge.sqlite3');
  assert.equal(await sha256(path.join(restored,'phaseforge.sqlite3')),await sha256(originalDatabase));
  await session(executable,restored,3,fixture);
  const database=originalDatabase;assert.ok(fs.existsSync(database));
  const before={database_sha256:await sha256(database),sentinel_sha256:await sha256(path.join(workspace,'preserve.txt'))};
  if(process.platform==='win32'){
    const uninstaller=path.join(install,'Uninstall PhaseForge.exe');assert.ok(fs.existsSync(uninstaller));
    command(uninstaller,['/S'],{timeout:180000});
    for(let i=0;i<60&&fs.existsSync(executable);i++)await sleep(500);
    assert.equal(fs.existsSync(executable),false,'NSIS uninstall left the application executable');
  }else if(process.platform==='darwin')removeMacApp(install,macInstallation.bundle);
  else fs.unlinkSync(executable);
  assert.equal(await sha256(database),before.database_sha256);assert.equal(await sha256(path.join(workspace,'preserve.txt')),before.sentinel_sha256);await requireClosed();
  const legacyPorts=legacyListeners.receipts();
  writeJSON(path.join(output,'occupied-legacy-ports.json'),legacyPorts);
  assert.ok(legacyPorts.every(row=>row.connections===0&&row.received_bytes===0),'The packaged application probed an occupied legacy port');
  const receipt={schema:'phaseforge.native-acceptance.v1',passed:true,source_commit:process.env.GITHUB_SHA,version,platform,architecture,host,artifact:{name:path.basename(artifact),bytes:fs.statSync(artifact).size,sha256:await sha256(artifact)},fresh_install:true,same_workspace_restart:true,offline_numerical_engine:true,normal_quit:true,uninstall:true,fixture_data_preserved:true,closed_database_backup_restore:true,fixture,retained_fixture:before,observed_at:new Date().toISOString(),limits:['No bundled local LLM or optional Blender/CAD engine was exercised.','Runner RAM and sampled RSS are recorded; they do not establish a minimum-RAM certification.','No prior-version upgrade/downgrade test.',process.platform==='win32'?'Windows installation means running the exact NSIS installer into a fresh per-user directory; normal NSIS uninstall removes the application while fixture data and retained scientific outputs are checked separately.':process.platform==='darwin'?'macOS installation means copying PhaseForge.app from the read-only DMG into an isolated application directory; removal deletes only that app bundle. Direct instrumented launch does not certify quarantined Finder/Gatekeeper download handling.':'AppImage installation means copying the distributed portable executable to a new application directory; removal deletes that executable.']};
  if(macInstallation)receipt.macos_installation=macInstallation.receipt;
  receipt.occupied_legacy_ports=legacyPorts;
  receipt.dark_mode_original_color_branding=true;
  receipt.explicit_light_preference_survives_restart=true;
  receipt.branding_evidence='launch-*/initial-appearance.json, branding.json, window.png and window-compact.png';
  if(platform==='windows'){
    const laboratory=readJSON(path.join(output,'LABORATORY_ACCEPTANCE.json'));
    receipt.microsoft_runtime_agreement={method:'Explicit versioned unattended-install argument supplied by the release developer',argument:'/ACCEPT_MSVC_TERMS=MSVC-2026-09-11',terms_sha256:await sha256(path.join(ROOT,'tools/third-party/msvc-runtime/END-USER-TERMS.txt')),interactive_agreement_page_exercised:false};
    assert.equal(laboratory.passed,true);assert.equal(laboratory.source_commit,process.env.GITHUB_SHA);
    receipt.laboratory_evidence={path:'LABORATORY_ACCEPTANCE.json',sha256:await sha256(path.join(output,'LABORATORY_ACCEPTANCE.json'))};
    receipt.managed_runtime_evidence={path:'managed-runtime-materials.json',sha256:await sha256(path.join(output,'managed-runtime-materials.json')),delivery:'bundled_immutable_seeds'};
    receipt.managed_sqlite_evidence={path:'managed-sqlite-runtime.json',sha256:await sha256(path.join(output,'managed-sqlite-runtime.json'))};
    receipt.managed_openssl_evidence={path:'managed-openssl-runtime.json',sha256:await sha256(path.join(output,'managed-openssl-runtime.json'))};
  }
  writeJSON(path.join(output,'NATIVE_ACCEPTANCE.json'),receipt);console.log(JSON.stringify({passed:true,platform,version,artifact:receipt.artifact.name}));
}
main().catch(error=>{writeJSON(path.join(output,'NATIVE_FAILURE.json'),{passed:false,error:String(error.stack||error),source_commit:process.env.GITHUB_SHA,recorded_at:new Date().toISOString()});console.error(error);process.exitCode=1;}).finally(async()=>{if(legacyListeners){writeJSON(path.join(output,'occupied-legacy-ports.json'),legacyListeners.receipts());await legacyListeners.close();}});
