const {test}=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
const {EventEmitter}=require('node:events');
const {PassThrough,Writable}=require('node:stream');
const {readBackendEndpoint}=require('../backend-auth.cjs');

const source=fs.readFileSync(path.resolve(__dirname,'../main.cjs'),'utf8');
const diagnosticsSource=fs.readFileSync(path.resolve(__dirname,'../renderer-diagnostics.cjs'),'utf8');
const flush=()=>new Promise(resolve=>setImmediate(resolve));
function deferred(){let resolve;const promise=new Promise(done=>{resolve=done;});return {promise,resolve};}

// Exercise the actual desktop entrypoint with controlled OS/process boundaries.
// No executable is launched and no researcher files or credentials are touched.
function desktop({announce=true,proof=async()=>true,uiStart,appReady=Promise.resolve(),platform=process.platform}={}){
  const children=[],windows=[],errors=[],timers=[],servers=[],proofPorts=[],builtMenus=[],applicationMenus=[],files=new Map();
  let gate,targetGetter,proxyStarts=0;
  const app=new EventEmitter();
  Object.assign(app,{isPackaged:false,getPath:()=>'/phaseforge-fixture',setAppUserModelId(){},setAboutPanelOptions(){},requestSingleInstanceLock:()=>true,whenReady:()=>appReady,quit(){this.emit('before-quit');},showAboutPanel(){}});
  class Window extends EventEmitter{
    constructor(options){super();this.options=options;windows.push(this);this.webContents=new EventEmitter();Object.assign(this.webContents,{setWindowOpenHandler(){},getOSProcessId:()=>7000+windows.length,reloads:0,devToolsToggles:0,reload(){this.reloads++;},toggleDevTools(){this.devToolsToggles++;}});}
    show(){} focus(){} loadURL(){} setAppDetails(){} setThumbnailToolTip(){}
  }
  const modules={
    electron:{app,BrowserWindow:Window,Menu:{
      buildFromTemplate(template){const menu={template:Array.from(template,item=>({...item}))};builtMenus.push(menu);return menu;},
      setApplicationMenu(menu){applicationMenus.push(menu);},
    },nativeImage:{createFromPath:()=>({isEmpty:()=>true})},dialog:{showErrorBox:(...args)=>errors.push(args)},shell:{}},
    'node:child_process':{spawn(_binary,_args,options){
      const child=new EventEmitter();
      Object.assign(child,{options,stdout:new PassThrough(),port:48000+children.length,exitCode:null,killed:false,
        announce(){this.stdout.write(`PHASEFORGE_DESKTOP_ENDPOINT ${JSON.stringify({address:'127.0.0.1',port:this.port,version:'fixture'})}\n`);},
        kill(){if(this.killed)return;this.killed=true;this.exitCode=0;this.emit('exit',0);this.stdout.end();},
        crash(){this.exitCode=1;this.emit('exit',1);this.stdout.end();}});
      children.push(child);if(announce)queueMicrotask(()=>child.announce());return child;
    }},
    'node:fs':{existsSync:()=>true,mkdirSync(){},openSync:()=>1,closeSync(){},createWriteStream:()=>new Writable({write(_chunk,_encoding,done){done();}}),
      statSync:filename=>({size:Buffer.byteLength(files.get(filename)||'')}),
      appendFileSync:(filename,text)=>files.set(filename,(files.get(filename)||'')+text),
      renameSync:(from,to)=>{files.set(to,files.get(from));files.delete(from);},
    },
    'node:path':path,
    './server.cjs':{async startServer(_root,_backend,_port,options){
      proxyStarts++;gate=options.isBackendReady;targetGetter=_backend;
      if(uiStart)await uiStart;
      const server={closed:false,connectionsClosed:false,close(){this.closed=true;},closeAllConnections(){this.connectionsClosed=true;}};
      servers.push(server);return {server,url:'http://127.0.0.1:7332'};
    }},
    './permissions.cjs':{installWorkbenchPermissions(){}},
    './backend-auth.cjs':{createBackendSecret:()=>'',readBackendEndpoint,verifyBackendReady:(secret,version,options)=>{proofPorts.push(options.port);return proof(secret,version,options);}},
    './package.json':{version:'fixture'},
  };
  // Run the production diagnostics helper against this fixture's in-memory
  // filesystem. Its real event listeners/logging participate in lifecycle tests;
  // neither renderer observability nor the dependency allow-list is bypassed.
  const diagnosticsModule={exports:{}};
  vm.runInNewContext(diagnosticsSource,{module:diagnosticsModule,Date,require:name=>{
    assert.ok(['node:fs','node:path'].includes(name),`Unexpected diagnostics dependency ${name}`);
    return modules[name];
  }},{filename:'desktop/renderer-diagnostics.cjs'});
  modules['./renderer-diagnostics.cjs']=diagnosticsModule.exports;
  vm.runInNewContext(source,{
    require:name=>{assert.ok(modules[name],`Unexpected dependency ${name}`);return modules[name];},
    __dirname:path.resolve(__dirname,'..'),
    process:{platform,env:{},execPath:'/phaseforge-fixture/electron'},
    setTimeout:(callback,ms)=>{timers.push({callback,ms});},Date,
  },{filename:'desktop/main.cjs'});
  return {app,children,windows,errors,timers,servers,proofPorts,builtMenus,applicationMenus,diagnosticsRecords:()=>[...files.values()].flatMap(text=>text.trim().split('\n').filter(Boolean).map(JSON.parse)),get ready(){return gate?.()??false;},get target(){return targetGetter?.()??null;},get proxyStarts(){return proxyStarts;}};
}

test('desktop attaches real renderer diagnostics without changing window isolation or engine lifecycle',async()=>{
  const d=desktop();await flush();
  const window=d.windows[0],contents=window.webContents;
  assert.deepEqual({...window.options.webPreferences},{nodeIntegration:false,contextIsolation:true,sandbox:true,webSecurity:true});
  assert.equal(d.diagnosticsRecords()[0].kind,'renderer-attached');
  for(const event of ['console-message','render-process-gone','did-fail-load','preload-error'])assert.equal(contents.listenerCount(event),1);
  contents.emit('console-message',{level:'error',message:'ReferenceError: missingTimeline is not defined Bearer test-token',sourceId:'app.js',lineNumber:17});
  contents.emit('render-process-gone',{}, {reason:'crashed',exitCode:5});
  const rows=d.diagnosticsRecords();assert.equal(rows.length,3);assert.match(rows[1].message,/ReferenceError: missingTimeline/);assert.doesNotMatch(rows[1].message,/test-token/);assert.equal(rows[2].reason,'crashed');
  assert.equal(d.children.length,1);assert.equal(d.children[0].killed,false);assert.equal(d.ready,true);assert.deepEqual(d.errors,[]);d.app.quit();
});

test('interface recovery shortcuts reload only the renderer and leave backend jobs running',async()=>{
  const d=desktop();await flush();const contents=d.windows[0].webContents;let prevented=0;
  const send=input=>contents.emit('before-input-event',{preventDefault(){prevented++;}},{type:'keyDown',control:true,alt:false,meta:false,shift:false,...input});
  send({key:'r'});assert.equal(contents.reloads,1);assert.equal(prevented,1);
  send({key:'I',shift:true});assert.equal(contents.devToolsToggles,1);assert.equal(prevented,2);
  send({key:'r',alt:true});send({key:'r',meta:true});send({key:'r',control:false});send({key:'r',type:'keyUp'});
  assert.equal(contents.reloads,1);assert.equal(prevented,2);
  assert.equal(d.children.length,1);assert.equal(d.children[0].killed,false);assert.equal(d.ready,true);assert.equal(d.servers[0].closed,false);assert.equal(d.proxyStarts,1);d.app.quit();
});

test('macOS installs the standard application, editing, viewing and window menus before opening the workbench',async()=>{
  const d=desktop({platform:'darwin'});await flush();
  assert.equal(d.builtMenus.length,1);
  assert.deepEqual(d.builtMenus[0].template,[{role:'appMenu'},{role:'editMenu'},{role:'viewMenu'},{role:'windowMenu'}]);
  assert.equal(d.applicationMenus.length,1);assert.equal(d.applicationMenus[0],d.builtMenus[0]);
  assert.equal(d.windows.length,1);assert.equal(d.ready,true);assert.deepEqual(d.errors,[]);d.app.quit();
});

test('Windows and Linux retain the menu-free workbench without building macOS menus',async()=>{
  for(const platform of ['win32','linux']){
    const d=desktop({platform});await flush();
    assert.deepEqual(d.builtMenus,[]);assert.deepEqual(d.applicationMenus,[null]);
    assert.equal(d.windows.length,1);assert.equal(d.ready,true);assert.deepEqual(d.errors,[]);d.app.quit();
  }
});

test('desktop delegates backend port allocation to the OS and proves only the child-announced endpoint',async()=>{
  const d=desktop();await flush();
  assert.equal(d.children.length,1);assert.equal(d.children[0].options.env.PHASEFORGE_PORT,'0');
  assert.equal(d.children[0].options.stdio[1],'pipe');assert.deepEqual(d.proofPorts,[48000]);
  assert.equal(d.target.port,48000);assert.equal(d.windows.length,1);d.app.quit();
});

test('desktop does not open its proxy or window before its live child proves readiness',async()=>{
  const proof=deferred(),d=desktop({proof:()=>proof.promise});await flush();
  assert.equal(d.children.length,1);assert.equal(d.proxyStarts,0);assert.equal(d.windows.length,0);
  proof.resolve(true);await flush();assert.equal(d.ready,true);assert.equal(d.windows.length,1);
  d.app.quit();
});

test('backend exit between proof and window creation recovers with at most three retries',async()=>{
  const ui=deferred(),d=desktop({uiStart:ui.promise});await flush();
  assert.equal(d.proxyStarts,1);assert.equal(d.windows.length,0);
  d.children[0].crash();assert.equal(d.ready,false);assert.equal(d.timers.length,1);
  ui.resolve();await flush();assert.equal(d.windows.length,1);
  for(let attempt=0;attempt<3;attempt++){
    const timer=d.timers.shift();assert.equal(timer.ms,2000);timer.callback();await flush();
    assert.equal(d.children.length,attempt+2);assert.equal(d.ready,true);assert.equal(d.target.port,48001+attempt);
    d.children.at(-1).crash();assert.equal(d.ready,false);
  }
  assert.equal(d.timers.length,0);assert.equal(d.children.length,4);
  assert.equal(d.errors.length,1);assert.match(d.errors[0][1],/three recovery attempts/);
  d.app.quit();
});

test('quitting before application readiness never spawns an engine or opens UI',async()=>{
  const appReady=deferred(),d=desktop({appReady:appReady.promise});await flush();
  d.app.quit();appReady.resolve();await flush();
  assert.equal(d.children.length,0);assert.equal(d.proxyStarts,0);assert.equal(d.windows.length,0);assert.equal(d.errors.length,0);
});

test('quitting while the child announces its endpoint cancels startup and never contacts any listener',async()=>{
  const d=desktop({announce:false});await flush();
  d.app.quit();await flush();assert.equal(d.children[0].killed,true);
  assert.equal(d.proofPorts.length,0);assert.equal(d.proxyStarts,0);assert.equal(d.windows.length,0);assert.equal(d.errors.length,0);
});

test('quitting during proof cancels startup without a stale ready flag or error dialog',async()=>{
  const proof=deferred(),d=desktop({proof:()=>proof.promise});await flush();
  d.app.quit();proof.resolve(true);await flush();
  assert.equal(d.children[0].killed,true);assert.equal(d.ready,false);assert.equal(d.proxyStarts,0);assert.equal(d.windows.length,0);assert.equal(d.errors.length,0);assert.equal(d.timers.length,0);
});

test('quitting while the UI listener starts closes that late listener without creating a window',async()=>{
  const ui=deferred(),d=desktop({uiStart:ui.promise});await flush();
  d.app.quit();ui.resolve();await flush();
  assert.equal(d.children[0].killed,true);assert.equal(d.ready,false);assert.equal(d.windows.length,0);assert.equal(d.errors.length,0);
  assert.equal(d.servers[0].closed,true);assert.equal(d.servers[0].connectionsClosed,true);
});

test('a late proof from an exited initial child cannot authorize the proxy',async()=>{
  const proof=deferred(),d=desktop({proof:()=>proof.promise});await flush();
  d.children[0].crash();proof.resolve(true);await flush();
  const timer=d.timers.shift();assert.equal(timer.ms,500);timer.callback();await flush();
  assert.equal(d.proxyStarts,0);assert.equal(d.windows.length,0);assert.equal(d.ready,false);
  assert.equal(d.errors.length,1);assert.match(d.errors[0][1],/exited before authenticated readiness/);
});
