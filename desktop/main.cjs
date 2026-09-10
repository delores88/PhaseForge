const {app,BrowserWindow,Menu,Tray,nativeImage,dialog,shell}=require('electron');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const {startServer}=require('./server.cjs');
const {installWorkbenchPermissions}=require('./permissions.cjs');
const {createBackendSecret,readBackendEndpoint,verifyBackendReady}=require('./backend-auth.cjs');
let window,backend,backendEndpoint,ui,tray,quitting=false,restarts=0,backendReady=false,initialBackendReady=false;
const VERSION=require('./package.json').version;
const delay=ms=>new Promise(r=>setTimeout(r,ms));

function runtimePath(){return app.isPackaged?path.join(process.resourcesPath,'runtime',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend'):path.resolve(__dirname,'../backend/target/release',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');}
function engineReady(){return !!backendEndpoint&&backendReady&&backend?.exitCode===null&&!backend.killed;}
function engineTarget(){return engineReady()?backendEndpoint:null;}
async function launchBackend(){
  if(quitting)return false;
  backendReady=false;backendEndpoint=null;
  const binary=runtimePath();if(!fs.existsSync(binary))throw new Error(`The local research engine is missing. Reinstall PhaseForge. (${binary})`);
  const secret=createBackendSecret();
  const logs=path.join(app.getPath('userData'),'logs');fs.mkdirSync(logs,{recursive:true});
  const logPath=path.join(logs,'engine.log'),output=fs.openSync(logPath,'a');
  const child=spawn(binary,[],{cwd:app.isPackaged?process.resourcesPath:path.resolve(__dirname,'..'),windowsHide:true,stdio:['ignore','pipe',output],env:{...process.env,PHASEFORGE_PORT:'0',PHASEFORGE_BIND:'127.0.0.1',PHASEFORGE_DESKTOP_SECRET:secret}});
  backend=child;let launchError=null;
  fs.closeSync(output);
  const log=fs.createWriteStream(logPath,{flags:'a'});
  log.on('error',()=>{child.stdout.unpipe(log);child.stdout.resume();});
  child.stdout.pipe(log);
  child.once('error',error=>{launchError=error;if(backend===child)backendReady=false;});
  child.once('exit',()=>{if(backend!==child)return;backend=null;backendEndpoint=null;backendReady=false;if(!quitting&&initialBackendReady){if(restarts++<3)setTimeout(()=>{if(!quitting)launchBackend().catch(error=>{if(quitting)return;showWindow();dialog.showErrorBox('Research engine could not restart',error.message);});},2000);else{showWindow();dialog.showErrorBox('Research engine stopped','The local engine stopped after three recovery attempts. Your saved projects and checkpoints remain available. Quit and reopen PhaseForge to retry, or inspect the engine log.');}}});
  const deadline=Date.now()+45000;
  try{
  const endpoint=await readBackendEndpoint(child,VERSION,{timeoutMs:45000});
  while(Date.now()<deadline&&!quitting){
    if(launchError)throw new Error(`The research engine could not start: ${launchError.message}`);
    if(backend!==child||child.exitCode!==null||child.killed)throw new Error('The research engine exited before authenticated readiness. Inspect the engine log in the application data folder.');
    if(await verifyBackendReady(secret,VERSION,{port:endpoint.port})){if(!quitting&&backend===child&&child.exitCode===null&&!child.killed){backendEndpoint=endpoint;backendReady=true;initialBackendReady=true;return true;}}
    if(quitting)break;
    await delay(500);
  }
  throw new Error('The research engine did not prove readiness. Inspect the engine log in the application data folder.');
  }catch(error){if(backend===child)child.kill();if(quitting)return false;throw error;}
}
function showWindow(){if(window&&!quitting){window.show();window.focus();}}
function appIcon(){return app.isPackaged?path.join(process.resourcesPath,'ui',process.platform==='win32'?'icon.ico':'icon.png'):path.resolve(__dirname,'../frontend/public',process.platform==='win32'?'icon.ico':'icon.png');}
function makeWindow(url){
  window=new BrowserWindow({width:1540,height:980,minWidth:800,minHeight:600,title:'PhaseForge · Alpha research workbench',icon:appIcon(),backgroundColor:'#101417',autoHideMenuBar:true,show:false,webPreferences:{nodeIntegration:false,contextIsolation:true,sandbox:true,webSecurity:true}});
  if(process.platform==='win32'){
    window.setAppDetails({appId:'science.phaseforge.desktop',appIconPath:appIcon(),appIconIndex:0,relaunchDisplayName:'PhaseForge',relaunchCommand:`"${process.execPath}"`});
    window.setThumbnailToolTip('PhaseForge Alpha · Scientific research workbench');
  }
  window.webContents.setWindowOpenHandler(({url:target})=>{if(/^https:\/\//.test(target))shell.openExternal(target);return {action:'deny'};});
  window.webContents.on('will-navigate',(event,target)=>{if(new URL(target).origin!==url){event.preventDefault();if(/^https:\/\//.test(target))shell.openExternal(target);}});
  installWorkbenchPermissions(window.webContents,url);
  window.on('close',event=>{if(!quitting&&tray){event.preventDefault();window.hide();}});
  window.once('ready-to-show',()=>{if(!quitting)window.show();});window.loadURL(url);
  const png=app.isPackaged?path.join(process.resourcesPath,'ui','brand','favicon-32.png'):path.resolve(__dirname,'../frontend/public/brand/favicon-32.png');
  const icon=nativeImage.createFromPath(png);
  if(!icon.isEmpty()){
    tray=new Tray(icon.resize({width:24,height:24}));tray.setToolTip('PhaseForge · research continues in the background');
    tray.setContextMenu(Menu.buildFromTemplate([{label:'Open PhaseForge',click:showWindow},{label:'Research continues when the window is closed',enabled:false},{type:'separator'},{label:'About PhaseForge',click:()=>app.showAboutPanel()},{label:'Quit PhaseForge',click:()=>{quitting=true;app.quit();}}]));tray.on('double-click',showWindow);
  }
}
if(process.platform==='win32')app.setAppUserModelId('science.phaseforge.desktop');
app.setAboutPanelOptions({applicationName:'PhaseForge',applicationVersion:VERSION,version:VERSION,iconPath:app.isPackaged?path.join(process.resourcesPath,'ui','icon.png'):path.resolve(__dirname,'../frontend/public/icon.png'),credits:'Alpha research workbench. Free app; your selected OpenAI or Anthropic provider charges separately for API usage. More features are coming.',website:'https://github.com/delores88/PhaseForge'});
if(!app.requestSingleInstanceLock())app.quit();
else{
  app.on('second-instance',showWindow);app.on('activate',showWindow);
  // Stable origin preserves the researcher's model, theme and workspace preferences.
  app.whenReady().then(async()=>{
    if(quitting)return;
    Menu.setApplicationMenu(null);
    if(!await launchBackend()||quitting)return;
    const started=await startServer(app.isPackaged?path.join(process.resourcesPath,'ui'):path.resolve(__dirname,'../frontend/out'),engineTarget,7332,{isBackendReady:engineReady});
    if(quitting){started.server.closeAllConnections?.();started.server.close();return;}
    ui=started;makeWindow(ui.url);
  }).catch(error=>{if(quitting)return;dialog.showErrorBox('PhaseForge could not start',error.message);quitting=true;app.quit();});
  app.on('before-quit',()=>{quitting=true;backendReady=false;backendEndpoint=null;backend?.kill();ui?.server.closeAllConnections?.();ui?.server.close();tray?.destroy();});
  app.on('window-all-closed',()=>{if(!tray)app.quit();});
}
