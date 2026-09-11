const {test}=require('node:test');const assert=require('node:assert/strict');const fs=require('node:fs');const os=require('node:os');const path=require('node:path');
const {startServer,assetPath}=require('../server.cjs');
test('desktop path resolver prevents traversal and malformed escapes',()=>{const root=path.resolve('fixture');for(const p of ['/../secrets','/%2e%2e/secrets','/a\\secrets','/%00','/%zz'])assert.equal(assetPath(root,p),null);assert.equal(assetPath(root,'/'),path.join(root,'index.html'));});
test('static desktop server serves exported UI with runtime config and rejects foreign hosts/origins',async()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-ui-'));fs.writeFileSync(path.join(root,'index.html'),'<html><head></head><body>Research</body></html>');
  const publicAssets=['brand/mark.svg','brand/app-icon.png','icon.ico','icon.png','phaseforge.svg','provenance.json','_next/static-other/icon.png'];
  const versionedAssets=['_next/static/chunks/a12b34c56d.js','_next/static/chunks/e98f76d54c.css','_next/static/build-123/_buildManifest.js'];
  for(const name of [...publicAssets,...versionedAssets]){const file=path.join(root,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,'original asset');}
  const {server,url}=await startServer(root);
  try{
    let r=await fetch(url);assert.equal(r.status,200);assert.match(await r.text(),/__PHASEFORGE_DESKTOP__/);assert.match(r.headers.get('content-security-policy'),/object-src 'none'/);assert.equal(r.headers.get('cache-control'),'no-store');
    for(const name of publicAssets){r=await fetch(url+'/'+name);assert.equal(r.status,200);assert.equal(r.headers.get('cache-control'),'no-store',name);await r.text();}
    for(const name of versionedAssets){r=await fetch(url+'/'+name);assert.equal(r.status,200);assert.equal(r.headers.get('cache-control'),'public, max-age=3600',name);await r.text();}
    fs.writeFileSync(path.join(root,'brand/app-icon.png'),'upgraded branding');r=await fetch(url+'/brand/app-icon.png');assert.equal(await r.text(),'upgraded branding');assert.equal(r.headers.get('cache-control'),'no-store');
    r=await fetch(url+'/icon.ico',{method:'HEAD'});assert.equal(r.headers.get('cache-control'),'no-store');assert.equal(await r.text(),'');
    r=await fetch(url+'/api/health',{headers:{Origin:'https://example.com'}});assert.equal(r.status,403);const code=await new Promise((resolve,reject)=>require('node:http').get(url,{headers:{Host:'evil.example'}},response=>{response.resume();resolve(response.statusCode);}).on('error',reject));assert.equal(code,403);r=await fetch(url+'/missing');assert.equal(r.status,404);
  }finally{server.closeAllConnections();await new Promise(resolve=>server.close(resolve));fs.rmSync(root,{recursive:true,force:true});}
});
test('interrupted backend response rejects instead of hanging the renderer',async()=>{
  const http=require('node:http');const upstream=http.createServer((_req,res)=>{res.writeHead(200,{'Content-Type':'application/json','Content-Length':'1000'});res.write('{"result":');setTimeout(()=>res.destroy(),20);});
  await new Promise(r=>upstream.listen(0,'127.0.0.1',r));const {server,url}=await startServer(process.cwd(),upstream.address().port);
  try{const r=await fetch(url+'/api/health');await assert.rejects(r.text());}finally{server.closeAllConnections();upstream.closeAllConnections();await Promise.all([new Promise(r=>server.close(r)),new Promise(r=>upstream.close(r))]);}
});
