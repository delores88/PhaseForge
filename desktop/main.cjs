const {app,BrowserWindow,Menu,Tray,nativeImage,dialog,shell}=require('electron');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const path=require('node:path');
const {startServer}=require('./server.cjs');
const {installWorkbenchPermissions}=require('./permissions.cjs');
let window,backend,ui,tray,quitting=false,restarts=0;
const VERSION=require('./package.json').version;
const delay=ms=>new Promise(r=>setTimeout(r,ms));

async function health(){try{const r=await fetch('http://127.0.0.1:7331/api/health',{signal:AbortSignal.timeout(1500)});return r.ok?await r.json():null;}catch{return null;}}
function runtimePath(){return app.isPackaged?path.join(process.resourcesPath,'runtime',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend'):path.resolve(__dirname,'../backend/target/release',process.platform==='win32'?'phaseforge-backend.exe':'phaseforge-backend');}
function launchBackend(){
  const binary=runtimePath();if(!fs.existsSync(binary))throw new Error(`The local research engine is missing. Reinstall PhaseForge. (${binary})`);
  const logs=path.join(app.getPath('userData'),'logs');fs.mkdirSync(logs,{recursive:true});
  const output=fs.openSync(path.join(logs,'engine.log'),'a');
  backend=spawn(binary,[],{cwd:app.isPackaged?process.resourcesPath:path.resolve(__dirname,'..'),windowsHide:true,stdio:['ignore',output,output],env:{...process.env,PHASEFORGE_PORT:'7331',PHASEFORGE_BIND:'127.0.0.1'}});
  fs.closeSync(output);
  backend.once('error',error=>{dialog.showErrorBox('PhaseForge engine',error.message);});
  backend.once('exit',()=>{backend=null;if(!quitting){if(restarts++<3)setTimeout(()=>{if(!quitting)launchBackend();},2000);else{showWindow();dialog.showErrorBox('Research engine stopped','The local engine stopped after three recovery attempts. Your saved projects and checkpoints remain available. Quit and reopen PhaseForge to retry, or inspect the engine log.');}}});
}
async function ensureBackend(){
  const existing=await health();
  if(existing){if(existing.name!=='PhaseForge'||existing.version!==VERSION)throw new Error(`An older or different research engine occupies port 7331 (${existing.version||'unknown version'}). Close that app, then reopen PhaseForge. Your data has not been changed.`);return;}
  launchBackend();
  for(let i=0;i<90;i++){if(await health())return;await delay(500);}
  throw new Error('The research engine did not become ready. Inspect the engine log in the application data folder.');
}
function showWindow(){if(window){window.show();window.focus();}}
function makeWindow(url){
  window=new BrowserWindow({width:1540,height:980,minWidth:800,minHeight:600,title:'PhaseForge',backgroundColor:'#101417',autoHideMenuBar:true,show:false,webPreferences:{nodeIntegration:false,contextIsolation:true,sandbox:true,webSecurity:true}});
  window.webContents.setWindowOpenHandler(({url:target})=>{if(/^https:\/\//.test(target))shell.openExternal(target);return {action:'deny'};});
  window.webContents.on('will-navigate',(event,target)=>{if(new URL(target).origin!==url){event.preventDefault();if(/^https:\/\//.test(target))shell.openExternal(target);}});
  installWorkbenchPermissions(window.webContents,url);
  window.on('close',event=>{if(!quitting&&tray){event.preventDefault();window.hide();}});
  window.once('ready-to-show',()=>window.show());window.loadURL(url);
  const png=app.isPackaged?path.join(process.resourcesPath,'ui','icon.png'):path.resolve(__dirname,'../frontend/public/icon.png');
  const icon=nativeImage.createFromPath(png);
  if(!icon.isEmpty()){
    tray=new Tray(icon.resize({width:24,height:24}));tray.setToolTip('PhaseForge · research continues in the background');
    tray.setContextMenu(Menu.buildFromTemplate([{label:'Open PhaseForge',click:showWindow},{label:'Research continues when the window is closed',enabled:false},{type:'separator'},{label:'Quit PhaseForge',click:()=>{quitting=true;app.quit();}}]));tray.on('double-click',showWindow);
  }
}
if(!app.requestSingleInstanceLock())app.quit();
else{
  app.on('second-instance',showWindow);app.on('activate',showWindow);
  // Stable origin preserves the researcher's model, theme and workspace preferences.
  app.whenReady().then(async()=>{Menu.setApplicationMenu(null);await ensureBackend();ui=await startServer(app.isPackaged?path.join(process.resourcesPath,'ui'):path.resolve(__dirname,'../frontend/out'),7331,7332);makeWindow(ui.url);}).catch(error=>{dialog.showErrorBox('PhaseForge could not start',error.message);quitting=true;app.quit();});
  app.on('before-quit',()=>{quitting=true;backend?.kill();ui?.server.close();tray?.destroy();});
  app.on('window-all-closed',()=>{if(!tray)app.quit();});
}
