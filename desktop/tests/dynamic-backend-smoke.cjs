// Explicit integration smoke; requires a built backend, never launches Electron.
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const net=require('node:net');
const http=require('node:http');
const {spawn,execFileSync}=require('node:child_process');
const {createBackendSecret,readBackendEndpoint,verifyBackendReady}=require('../backend-auth.cjs');
const {startServer}=require('../server.cjs');

async function deadline(promise,ms){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error('Owned integration process timed out')),ms);})]);}finally{clearTimeout(timer);}}
async function stop(child){
  if(!child||child.exitCode!==null||child.signalCode!==null)return;
  const ended=new Promise(resolve=>child.once('close',resolve));let failure;
  try{
    if(process.platform==='win32')execFileSync('taskkill',['/PID',String(child.pid),'/T','/F'],{windowsHide:true,timeout:15000,stdio:'pipe'});
    else process.kill(-child.pid,'SIGKILL');
  }catch(error){failure=error;}
  try{await deadline(ended,10000);}catch(error){child.kill();throw failure||error;}
}
async function websocket(url,expected){
  await deadline(new Promise((resolve,reject)=>{
    const request=http.get(url+'/api/events/ws',{headers:{Origin:url,Connection:'Upgrade',Upgrade:'websocket','Sec-WebSocket-Version':'13','Sec-WebSocket-Key':'dGhlIHNhbXBsZSBub25jZQ=='}});
    request.on('error',()=>expected?reject(new Error('Verified WebSocket was rejected')):resolve());
    request.on('response',response=>{response.resume();expected?reject(new Error('Verified WebSocket did not upgrade')):resolve();});
    request.on('upgrade',(_response,socket)=>{socket.destroy();expected?resolve():reject(new Error('Unverified WebSocket upgraded'));});
  }),6000);
}
async function main(){
  const binary=path.resolve(process.argv[2]||'');assert.ok(fs.statSync(binary).isFile(),'Pass the compiled backend path');
  const version=require('../package.json').version;
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-dynamic-backend-'));
  const relative=path.relative(fs.realpathSync(os.tmpdir()),fs.realpathSync(root));assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative));
  const sentinels=[],children=[],connections=new Set();let hits=0,selected=null,ui;
  try{
    const occupied=[];
    for(const port of [3000,7331]){
      const server=net.createServer(socket=>{hits++;socket.destroy();});
      try{await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(port,'127.0.0.1',resolve);});sentinels.push(server);occupied.push({port,owned:true});}
      catch(error){if(error.code!=='EADDRINUSE')throw error;occupied.push({port,owned:false});}
    }
    ui=await startServer(root,()=>selected,0);
    ui.server.on('connection',socket=>{connections.add(socket);socket.on('close',()=>connections.delete(socket));});
    const config=path.join(root,'config.toml');
    fs.writeFileSync(config,`bind_address="127.0.0.1"\nport=0\ndata_directory=${JSON.stringify(path.join(root,'data'))}\ngpu_enabled=false\nallowed_frontend_origins=${JSON.stringify([ui.url])}\n`);
    const launch=async()=>{
      const secret=createBackendSecret();
      const child=spawn(binary,['--config',config],{cwd:root,windowsHide:true,detached:process.platform!=='win32',stdio:['ignore','pipe','pipe'],env:{...process.env,PHASEFORGE_PORT:'0',PHASEFORGE_BIND:'127.0.0.1',PHASEFORGE_DATA_DIR:path.join(root,'data'),PHASEFORGE_DISABLE_GPU:'1',PHASEFORGE_DESKTOP_SECRET:secret}});
      children.push(child);child.on('error',()=>{});child.stderr.resume();
      const endpoint=await readBackendEndpoint(child,version);child.stdout.resume();
      assert.ok(![3000,7331].includes(endpoint.port));
      assert.equal(await verifyBackendReady(secret,version,{port:endpoint.port}),true);
      assert.equal(child.exitCode,null);assert.equal(child.killed,false);
      selected=endpoint;return {child,secret,endpoint};
    };
    assert.equal((await fetch(ui.url+'/api/health')).status,503);await websocket(ui.url,false);
    const first=await launch();
    const health=await (await fetch(ui.url+'/api/health',{headers:{Origin:ui.url}})).json();
    assert.deepEqual(health.endpoint,{address:'127.0.0.1',port:first.endpoint.port});assert.equal(health.version,version);
    assert.equal((await fetch(ui.url+'/api/chat/cancel-all',{method:'POST',headers:{Origin:ui.url}})).status,200);
    assert.equal((await fetch(ui.url+'/api/chat/cancel-all',{method:'POST',headers:{'Sec-Fetch-Site':'cross-site'}})).status,403);
    await websocket(ui.url,true);
    selected=null;await stop(first.child);
    assert.equal((await fetch(ui.url+'/api/health')).status,503);await websocket(ui.url,false);
    const second=await launch();assert.notEqual(second.endpoint,first.endpoint);
    assert.equal(await verifyBackendReady(first.secret,version,{port:second.endpoint.port}),false);
    assert.equal((await (await fetch(ui.url+'/api/health')).json()).endpoint.port,second.endpoint.port);
    await websocket(ui.url,true);assert.equal(hits,0,'The app contacted a development-port sentinel');
    console.log(JSON.stringify({passed:true,ports:children.length,development_ports:occupied,sentinel_connections:hits,protocol:'owned stdout plus per-launch HMAC',provider_calls:0}));
  }finally{
    selected=null;for(const child of children)await stop(child);
    for(const socket of connections)socket.destroy();
    if(ui){ui.server.closeAllConnections();await new Promise(resolve=>ui.server.close(resolve));}
    await Promise.all(sentinels.map(server=>new Promise(resolve=>server.close(resolve))));
    const boundary=path.relative(fs.realpathSync(os.tmpdir()),fs.realpathSync(root));assert.ok(boundary&&!boundary.startsWith('..')&&!path.isAbsolute(boundary));fs.rmSync(root,{recursive:true});
  }
}
main().catch(error=>{console.error(error.stack||String(error));process.exitCode=1;});
