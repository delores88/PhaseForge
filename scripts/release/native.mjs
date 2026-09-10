/** Exercises actual distributed packages on an isolated GitHub-hosted desktop. */
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import {_electron} from 'playwright-core';
import {ROOT,command,copyEvidenceTree,hostedWorkspace,inside,inventory,machine,readJSON,sha256,sleep,writeJSON} from './common.mjs';
import {startInertListeners} from './legacy-listeners.mjs';

const version=readJSON(path.join(ROOT,'desktop/package.json')).version;
const platform=process.platform==='win32'?'windows':'linux';
const output=path.join(ROOT,'.local/marketplace',`${platform}-x64`);
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

async function session(executable,workspace,index,fixture){
  await requireClosed();const folder=path.join(output,`launch-${index}`);fs.mkdirSync(folder,{recursive:true});
  const environment={...process.env,PHASEFORGE_DATA_DIR:workspace,PHASEFORGE_DISABLE_GPU:'1'};
  for(const key of ['OPENAI_API_KEY','ANTHROPIC_API_KEY','GH_TOKEN','GITHUB_TOKEN','ACTIONS_ID_TOKEN_REQUEST_TOKEN','ACTIONS_ID_TOKEN_REQUEST_URL','NODE_OPTIONS','ELECTRON_RUN_AS_NODE'])delete environment[key];
  for(const key of Object.keys(environment))if(/(?:TOKEN|SECRET|API_KEY|PASSWORD|CREDENTIAL)/i.test(key))delete environment[key];
  const known=new Map(),owners=new Map(),errors=[],requests=[],began=Date.now();let electronApp,appPID;
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
    writeJSON(identities,[...known.values()]);return [...sample.values()];
  };
  try{
    electronApp=await _electron.launch({executablePath:executable,args:[`--user-data-dir=${path.join(workspace,'electron-profile')}`],env:environment,cwd:path.dirname(executable),chromiumSandbox:true,timeout:90000});
    if(electronApp.process().exitCode===null)trackOwner(electronApp.process().pid);
    const runtime=await electronApp.evaluate(({app})=>({pid:process.pid,execPath:process.execPath,resourcesPath:process.resourcesPath,versions:process.versions,appVersion:app.getVersion(),packaged:app.isPackaged,appPath:app.getAppPath(),userData:app.getPath('userData')}));
    appPID=runtime.pid;trackOwner(appPID);remember();assert.equal(runtime.packaged,true);assert.equal(runtime.appVersion,version);
    const page=await electronApp.firstWindow({timeout:90000});
    page.on('pageerror',error=>errors.push(error.message));
    page.on('request',request=>{const url=new URL(request.url());if(['http:','https:'].includes(url.protocol)&&url.hostname!=='127.0.0.1')requests.push(url.origin);});
    await page.waitForLoadState('domcontentloaded');await page.getByRole('link',{name:'PhaseForge · Alpha research workbench',exact:true}).waitFor({state:'visible',timeout:30000});
    const health=await call(page,'/api/health');assert.equal(health.status,'ok');assert.equal(health.version,version);assert.equal(health.local_only,true);
    assert.match(health.sqlite?.version||'',/^\d+\.\d+\.\d+$/,'Actual backend SQLite version is required');
    assert.ok(Number.isInteger(health.sqlite.version_number));assert.match(health.sqlite.source_id,/^[0-9-]+ [0-9:]+ [a-f0-9]+$/);
    assert.equal(health.sqlite.linkage,'bundled');
    assert.equal(health.sqlite.version,'3.53.2','This ALPHA requires its reviewed SQLite baseline');assert.equal(health.sqlite.version_number,3053002);
    assert.equal(runtime.versions.electron,readJSON(path.join(ROOT,'desktop/package.json')).devDependencies.electron,'Observed Electron differs from the locked package input');
    assert.equal(health.endpoint?.address,'127.0.0.1');
    const backendPort=health.endpoint.port;assert.ok(Number.isInteger(backendPort)&&backendPort>0&&backendPort<=65535);
    assert.ok(![3000,7331,uiPort].includes(backendPort),'Packaged backend must use its own OS-assigned port');
    const metadata=readJSON(path.join(runtime.resourcesPath,'runtime/build.json'));
    assert.equal(metadata.sourceCommit,process.env.GITHUB_SHA);assert.equal(metadata.sourceDirty,false);
    const backend=path.join(runtime.resourcesPath,'runtime',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');
    assert.equal(machine(runtime.execPath).architecture,'x64');assert.equal(machine(backend).architecture,'x64');
    const samePath=(a,b)=>process.platform==='win32'?path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase():path.resolve(a)===path.resolve(b);
    const children=remember();assert.ok(children.some(item=>samePath(item.exe,backend)),'Packaged backend must belong to the launched application');
    const readySeconds=(Date.now()-began)/1000;
    if(index===1){
      fixture=await offlineExperiment(page);
      const payload=path.join(output,'payload');assert.ok(!fs.existsSync(payload),'Payload copy must be new');
      copyEvidenceTree(path.dirname(runtime.resourcesPath),payload);
      const copiedBackend=path.join(payload,'resources/runtime',path.basename(backend));
      assert.equal(await sha256(copiedBackend),await sha256(backend));
      writeJSON(path.join(output,'installed-payload.json'),{source_commit:process.env.GITHUB_SHA,version,runtime,backend:{sha256:await sha256(backend),machine:machine(backend)},...await inventory(payload)});
    }else{
      const projects=await call(page,'/api/projects');const rows=Array.isArray(projects)?projects:projects.projects;
      assert.ok(rows.some(row=>row.id===fixture.project.id),'Reopened installed app lost its fixture project');
      const retained=await call(page,`/api/runs/${fixture.run_id}`);assert.equal(retained.status,'completed');assert.equal(retained.result.metrics.final_x,fixture.final_x);
    }
    await page.goto(`http://127.0.0.1:7332/?project=${fixture.project.id}&run=${fixture.run_id}`,{waitUntil:'domcontentloaded'});
    await page.getByRole('button',{name:'Results',exact:true}).click({timeout:30000});
    await page.getByText('What we can say now',{exact:true}).waitFor({timeout:30000});
    writeJSON(path.join(folder,'findings-report.json'),await call(page,`/api/runs/${fixture.run_id}/findings`));
    let peakRSS=0;
    for(let i=0;i<6;i++){const records=remember();peakRSS=Math.max(peakRSS,records.reduce((sum,item)=>sum+item.rss_bytes,0));assert.equal((await call(page,'/api/health')).status,'ok');await sleep(2000);}
    assert.deepEqual(requests,[],'Native fixture unexpectedly requested an external website');
    assert.deepEqual(errors,[],'Packaged page reported an uncaught JavaScript error');
    await page.screenshot({path:path.join(folder,'window.png')});
    writeJSON(path.join(folder,'runtime.json'),{runtime,health,build:metadata,page_errors:errors,external_origins:requests,ready_seconds:readySeconds,peak_sampled_tree_rss_bytes:peakRSS});
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
  const suffix=process.platform==='win32'?'-setup.exe':'.AppImage';
  const artifacts=fs.readdirSync(path.join(ROOT,'desktop/dist')).filter(name=>name.endsWith(suffix));
  assert.equal(artifacts.length,1,'Exactly one admitted installer must be produced per native job');
  const artifact=path.join(ROOT,'desktop/dist',artifacts[0]);
  const root=inside(temporary,path.join(temporary,`phaseforge-acceptance-${process.env.GITHUB_RUN_ID}-${platform}`));
  assert.ok(!fs.existsSync(root),'Acceptance installation directory must be new');fs.mkdirSync(root);
  const install=inside(root,path.join(root,'install')),workspace=inside(root,path.join(root,'fixture-data'));
  fs.mkdirSync(workspace);fs.writeFileSync(path.join(workspace,'preserve.txt'),'PhaseForge disposable installation fixture\n');
  legacyListeners=await startInertListeners([3000,7331].flatMap(port=>['127.0.0.1','::1'].map(address=>({address,port}))));
  let executable;
  if(process.platform==='win32'){
    assert.ok(!install.includes(' '),'NSIS /D acceptance path must not require shell quoting');
    command(artifact,['/S',`/D=${install}`],{timeout:180000});executable=path.join(install,'PhaseForge.exe');
  }else{
    fs.mkdirSync(install);executable=path.join(install,path.basename(artifact));fs.copyFileSync(artifact,executable);fs.chmodSync(executable,0o755);
    assert.equal(machine(executable).architecture,'x64');
  }
  assert.ok(fs.statSync(executable).isFile(),'The actual installer did not create the application');
  const host=probe('host'),fixture=await session(executable,workspace,1,null);await session(executable,workspace,2,fixture);
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
  }else fs.unlinkSync(executable);
  assert.equal(await sha256(database),before.database_sha256);assert.equal(await sha256(path.join(workspace,'preserve.txt')),before.sentinel_sha256);await requireClosed();
  const legacyPorts=legacyListeners.receipts();
  writeJSON(path.join(output,'occupied-legacy-ports.json'),legacyPorts);
  assert.ok(legacyPorts.every(row=>row.connections===0&&row.received_bytes===0),'The packaged application probed an occupied legacy port');
  const receipt={schema:'phaseforge.native-acceptance.v1',passed:true,source_commit:process.env.GITHUB_SHA,version,platform,architecture:'x64',host,artifact:{name:path.basename(artifact),bytes:fs.statSync(artifact).size,sha256:await sha256(artifact)},fresh_install:true,same_workspace_restart:true,offline_numerical_engine:true,normal_quit:true,uninstall:true,fixture_data_preserved:true,closed_database_backup_restore:true,fixture,retained_fixture:before,observed_at:new Date().toISOString(),limits:['No bundled local LLM or optional Blender/CAD engine was exercised.','Runner RAM and sampled RSS are recorded; they do not establish a minimum-RAM certification.','No prior-version upgrade/downgrade test.','AppImage installation means copying the distributed portable executable to a new application directory; removal deletes that executable.']};
  receipt.occupied_legacy_ports=legacyPorts;
  writeJSON(path.join(output,'NATIVE_ACCEPTANCE.json'),receipt);console.log(JSON.stringify({passed:true,platform,version,artifact:receipt.artifact.name}));
}
main().catch(error=>{writeJSON(path.join(output,'NATIVE_FAILURE.json'),{passed:false,error:String(error.stack||error),source_commit:process.env.GITHUB_SHA,recorded_at:new Date().toISOString()});console.error(error);process.exitCode=1;}).finally(async()=>{if(legacyListeners){writeJSON(path.join(output,'occupied-legacy-ports.json'),legacyListeners.receipts());await legacyListeners.close();}});
