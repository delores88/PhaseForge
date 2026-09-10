const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const MIME={'.html':'text/html; charset=utf-8','.js':'text/javascript; charset=utf-8','.css':'text/css; charset=utf-8','.json':'application/json','.svg':'image/svg+xml','.png':'image/png','.ico':'image/x-icon','.woff2':'font/woff2'};

function assetPath(root,pathname){
  let decoded;try{decoded=decodeURIComponent(pathname);}catch{return null;}
  if(decoded.includes('\0')||decoded.includes('\\'))return null;
  const file=path.resolve(root,`.${decoded.endsWith('/')?`${decoded}index.html`:decoded}`);
  const rel=path.relative(root,file);
  if(rel.startsWith('..')||path.isAbsolute(rel))return null;
  return file;
}
function startServer(root,backendPort=7331,port=0){
  root=path.resolve(root);
  return new Promise((resolve,reject)=>{
    const server=http.createServer((req,res)=>{
      const host=`127.0.0.1:${server.address().port}`;
      if(req.headers.host!==host){res.writeHead(403).end();return;}
      const url=new URL(req.url,`http://${host}`);
      if(url.pathname.startsWith('/api/')){
        if(req.headers.origin && req.headers.origin!==`http://${host}`){res.writeHead(403).end();return;}
        const proxy=http.request({host:'127.0.0.1',port:backendPort,path:url.pathname+url.search,method:req.method,headers:{...req.headers,host:`127.0.0.1:${backendPort}`,origin:'http://127.0.0.1:3000'}},upstream=>{
          const headers={...upstream.headers};delete headers['access-control-allow-origin'];res.writeHead(upstream.statusCode,headers);
          upstream.on('aborted',()=>res.destroy());upstream.on('error',()=>res.destroy());res.on('close',()=>upstream.destroy());upstream.pipe(res);
        });
        proxy.on('error',()=>{if(!res.headersSent)res.writeHead(503,{'Content-Type':'application/json'});res.end(JSON.stringify({error:{message:'Local engine is unavailable. It will retry automatically after a crash; reopen PhaseForge if it remains offline. Your saved work remains available.'}}));});
        req.on('aborted',()=>proxy.destroy());req.pipe(proxy);return;
      }
      if(!['GET','HEAD'].includes(req.method)){res.writeHead(405).end();return;}
      let file=assetPath(root,url.pathname);
      if(!file){res.writeHead(400).end();return;}
      try{if(fs.statSync(file).isDirectory())file=path.join(file,'index.html');}catch{}
      fs.readFile(file,(err,body)=>{
        if(err){res.writeHead(404).end('Not found');return;}
        const ext=path.extname(file);const headers={'Content-Type':MIME[ext]||'application/octet-stream','X-Content-Type-Options':'nosniff','Referrer-Policy':'no-referrer','Cache-Control':ext==='.html'?'no-store':'public, max-age=3600'};
        if(ext==='.html'){
          // Fixed application configuration, never user content. The renderer has
          // no Node privileges; requests use this loopback origin's API proxy.
          body=Buffer.from(body.toString().replace('<head>','<head><script>window.__PHASEFORGE_DESKTOP__=true;</script>'));
          headers['Content-Security-Policy']="default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self' ws://127.0.0.1:*; worker-src 'self' blob:; object-src 'none'; base-uri 'self'; frame-ancestors 'none'";
        }
        res.writeHead(200,headers);res.end(req.method==='HEAD'?undefined:body);
      });
    });
    server.on('upgrade',(req,socket,head)=>{
      const host=`127.0.0.1:${server.address().port}`;
      if(req.headers.host!==host||req.url!=='/api/events/ws'||req.headers.origin!==`http://${host}`){socket.destroy();return;}
      const proxy=http.request({host:'127.0.0.1',port:backendPort,path:req.url,headers:{...req.headers,host:`127.0.0.1:${backendPort}`,origin:'http://127.0.0.1:3000'}});
      proxy.on('upgrade',(response,upstream,uphead)=>{
        socket.write(`HTTP/1.1 ${response.statusCode} Switching Protocols\r\n${Object.entries(response.headers).map(([k,v])=>`${k}: ${v}`).join('\r\n')}\r\n\r\n`);
        if(uphead.length)socket.write(uphead);if(head.length)upstream.write(head);socket.pipe(upstream).pipe(socket);
        upstream.on('error',()=>socket.destroy());socket.on('error',()=>upstream.destroy());socket.on('close',()=>upstream.destroy());
      });proxy.on('error',()=>socket.destroy());proxy.end();
    });
    server.once('error',reject);server.listen(port,'127.0.0.1',()=>resolve({server,url:`http://127.0.0.1:${server.address().port}`}));
  });
}
module.exports={startServer,assetPath};
