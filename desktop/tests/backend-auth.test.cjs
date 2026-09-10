const {test}=require('node:test');
const assert=require('node:assert/strict');
const http=require('node:http');
const {createHmac}=require('node:crypto');
const {createBackendSecret,readBackendEndpoint,verifyBackendReady}=require('../backend-auth.cjs');
const {EventEmitter}=require('node:events');
const {PassThrough}=require('node:stream');
const {startServer}=require('../server.cjs');

async function fixture(handler){const server=http.createServer(handler);await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));return {server,port:server.address().port,close:()=>{server.closeAllConnections();return new Promise(resolve=>server.close(resolve));}};}
test('readiness proves the per-launch secret without transmitting it and rejects replay',async()=>{
  const secret=createBackendSecret();let retained,requests=[];
  const service=await fixture((req,res)=>{requests.push({url:req.url,headers:req.headers});const nonce=new URL(req.url,'http://localhost').searchParams.get('nonce');const body=retained||{algorithm:'HMAC-SHA256',version:'alpha',nonce,proof:createHmac('sha256',Buffer.from(secret,'hex')).update(nonce).digest('hex')};retained=body;res.setHeader('Content-Type','application/json');res.end(JSON.stringify(body));});
  try{
    assert.equal(await verifyBackendReady(secret,'alpha',{port:service.port}),true);
    assert.equal(await verifyBackendReady(secret,'alpha',{port:service.port}),false);
    assert.equal(JSON.stringify(requests).includes(secret),false);
    assert.notEqual(requests[0].url,requests[1].url);
  }finally{await service.close();}
});

function endpointChild(){const child=new EventEmitter();child.stdout=new PassThrough();child.exitCode=null;child.killed=false;return child;}
const announcement=value=>'PHASEFORGE_DESKTOP_ENDPOINT '+JSON.stringify(value)+'\n';
test('endpoint discovery parses split private stdout records without using a development port',async()=>{
  const child=endpointChild(),pending=readBackendEndpoint(child,'alpha');
  const line=announcement({address:'127.0.0.1',port:49152,version:'alpha'});
  child.stdout.write('ordinary startup log\n'+line.slice(0,19));child.stdout.write(line.slice(19));
  assert.deepEqual(await pending,{address:'127.0.0.1',port:49152,version:'alpha'});child.stdout.end();
});
test('endpoint discovery rejects malformed, foreign, oversized and stale-child announcements',async()=>{
  const good={address:'127.0.0.1',port:49152,version:'alpha'};
  for(const line of [announcement({...good,address:'evil.example'}),announcement({...good,address:'::1'}),announcement({...good,port:0}),announcement({...good,port:'49152'}),announcement({...good,version:'old'}),announcement({...good,extra:true}),'PHASEFORGE_DESKTOP_ENDPOINT {bad}\n','x'.repeat(4097),announcement(good)+announcement(good)]){
    const child=endpointChild(),pending=readBackendEndpoint(child,'alpha');child.stdout.write(line);await assert.rejects(pending);child.stdout.end();
  }
  const child=endpointChild(),pending=readBackendEndpoint(child,'alpha');child.exitCode=1;child.emit('exit',1);
  child.stdout.write(announcement(good));await assert.rejects(pending,/exited/);child.stdout.end();
});
test('endpoint discovery has a finite startup timeout and proof refuses absent or invalid ports',async()=>{
  const child=endpointChild();await assert.rejects(readBackendEndpoint(child,'alpha',{timeoutMs:20}),/in time/);child.stdout.end();
  let calls=0;for(const port of [undefined,0,-1,65536,'7331'])assert.equal(await verifyBackendReady(createBackendSecret(),'alpha',{port,fetcher:()=>{calls++;}}),false);
  assert.equal(calls,0);
});
test('a public health impostor, wrong secret, version and malformed proof cannot prove readiness',async()=>{
  const secret=createBackendSecret();let response={name:'PhaseForge',version:'alpha',status:'ok'};
  const service=await fixture((req,res)=>{const nonce=new URL(req.url,'http://localhost').searchParams.get('nonce');res.setHeader('Content-Type','application/json');res.end(JSON.stringify(typeof response==='function'?response(nonce):response));});
  try{
    assert.equal(await verifyBackendReady(secret,'alpha',{port:service.port}),false);
    for(const kind of ['secret','version','malformed']){
      response=nonce=>({algorithm:'HMAC-SHA256',nonce,version:kind==='version'?'old':'alpha',proof:kind==='malformed'?'bad':createHmac('sha256',Buffer.from(kind==='secret'?createBackendSecret():secret,'hex')).update(nonce).digest('hex')});
      assert.equal(await verifyBackendReady(secret,'alpha',{port:service.port}),false);
    }
  }finally{await service.close();}
});
test('readiness never follows an untrusted listener redirect',async()=>{
  let hits=0;const other=await fixture((_req,res)=>{hits++;res.end('{}');});
  const first=await fixture((_req,res)=>{res.writeHead(302,{Location:`http://127.0.0.1:${other.port}/collect`});res.end();});
  try{assert.equal(await verifyBackendReady(createBackendSecret(),'alpha',{port:first.port}),false);assert.equal(hits,0);}finally{await first.close();await other.close();}
});
test('unverified readiness responses are bounded even without Content-Length',async()=>{
  const secret=createBackendSecret();
  const service=await fixture((req,res)=>{const nonce=new URL(req.url,'http://localhost').searchParams.get('nonce');res.writeHead(200,{'Content-Type':'application/json'});res.write(JSON.stringify({algorithm:'HMAC-SHA256',version:'alpha',nonce,proof:createHmac('sha256',Buffer.from(secret,'hex')).update(nonce).digest('hex'),padding:'x'.repeat(8192)}));res.end();});
  try{assert.equal(await verifyBackendReady(secret,'alpha',{port:service.port}),false);}finally{await service.close();}
});
test('desktop proxy withholds HTTP mutations and WebSocket traffic until its owned engine is verified',async()=>{
  let hits=0,ready=false;const upstream=await fixture((_req,res)=>{hits++;res.end('{}');});
  const {server,url}=await startServer(process.cwd(),upstream.port,0,{isBackendReady:()=>ready});
  try{
    assert.equal((await fetch(url+'/api/chat/cancel-all',{method:'POST'})).status,503);
    await new Promise(resolve=>{const req=http.get(url+'/api/events/ws',{headers:{Origin:url,Connection:'Upgrade',Upgrade:'websocket','Sec-WebSocket-Key':'dGhlIHNhbXBsZSBub25jZQ==','Sec-WebSocket-Version':'13'}});req.on('error',resolve);req.on('response',res=>{res.resume();resolve();});});
    assert.equal(hits,0);
    ready=true;assert.equal((await fetch(url+'/api/health')).status,200);assert.equal(hits,1);
    assert.equal((await fetch(url+'/api/chat/cancel-all',{method:'POST',headers:{'Sec-Fetch-Site':'cross-site'}})).status,403);
    assert.equal(hits,1,'Proxy must not invent a trusted Origin for a cross-site request');
    ready=false;assert.equal((await fetch(url+'/api/health')).status,503);assert.equal(hits,1);
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await upstream.close();}
});

test('proxy follows only the current verified endpoint and preserves actual UI or absent CLI Origin',async()=>{
  let release,arrived;const waiting=new Promise(resolve=>{arrived=resolve;});
  const first=await fixture((req,res)=>{
    if(req.url==='/api/pending'){release=()=>res.end('{"stale":true}');arrived();return;}
    res.end(JSON.stringify({engine:'first',origin:req.headers.origin??null,host:req.headers.host}));
  });
  const second=await fixture((req,res)=>res.end(JSON.stringify({engine:'second',origin:req.headers.origin??null,host:req.headers.host})));
  let selected={address:'127.0.0.1',port:first.port};
  const {server,url}=await startServer(process.cwd(),()=>selected);
  try{
    let response=await fetch(url+'/api/health');let body=await response.json();assert.equal(body.origin,null);assert.equal(body.engine,'first');
    response=await fetch(url+'/api/health',{headers:{Origin:url}});body=await response.json();assert.equal(body.origin,url);assert.equal(body.host,`127.0.0.1:${first.port}`);
    const pending=fetch(url+'/api/pending');await waiting;
    selected={address:'127.0.0.1',port:second.port};release();assert.equal((await pending).status,503);
    body=await (await fetch(url+'/api/health',{headers:{Origin:url}})).json();assert.equal(body.engine,'second');assert.equal(body.origin,url);assert.equal(body.host,`127.0.0.1:${second.port}`);
    selected=null;assert.equal((await fetch(url+'/api/health')).status,503);
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await first.close();await second.close();}
});

async function expectRejectedUpgrade(url,limitMs){
  await new Promise((resolve,reject)=>{
    const request=http.get(url+'/api/events/ws',{headers:{Origin:url,Connection:'Upgrade',Upgrade:'websocket','Sec-WebSocket-Key':'dGhlIHNhbXBsZSBub25jZQ==','Sec-WebSocket-Version':'13'}});
    const timeout=setTimeout(()=>{request.destroy();reject(new Error('Desktop left the WebSocket handshake pending'));},limitMs);
    request.once('error',()=>{clearTimeout(timeout);resolve();});
    request.once('response',response=>{clearTimeout(timeout);response.resume();resolve();});
    request.once('upgrade',(_response,socket)=>{clearTimeout(timeout);socket.destroy();reject(new Error('Unexpected accepted WebSocket upgrade'));});
  });
}

test('desktop promptly closes WebSocket upgrades rejected by the upstream',async()=>{
  let hits=0;const upstream=await fixture((_req,res)=>{hits++;res.writeHead(503).end('recovering');});
  const {server,url}=await startServer(process.cwd(),upstream.port,0,{isBackendReady:()=>true});
  try{await expectRejectedUpgrade(url,1000);assert.equal(hits,1);}
  finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await upstream.close();}
});

test('desktop bounds a silent upstream WebSocket handshake and releases its connection',async()=>{
  let hits=0,release;
  const closed=new Promise(resolve=>{release=resolve;});
  const upstream=await fixture(req=>{hits++;req.socket.once('close',release);});
  const {server,url}=await startServer(process.cwd(),upstream.port,0,{isBackendReady:()=>true});
  try{
    await expectRejectedUpgrade(url,6500);assert.equal(hits,1);
    await new Promise((resolve,reject)=>{
      const timeout=setTimeout(()=>reject(new Error('Upstream handshake connection was retained')),1000);
      closed.then(()=>{clearTimeout(timeout);resolve();});
    });
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));await upstream.close();}
});
