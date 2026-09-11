const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),{EventEmitter}=require('node:events');
const {installRendererDiagnostics}=require('../renderer-diagnostics.cjs');
test('native renderer diagnostics retain modern and legacy exceptions, redact tokens and survive rotation',()=>{
  const directory=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-renderer-log-')),contents=new EventEmitter();contents.getOSProcessId=()=>42;
  const installed=installRendererDiagnostics(contents,directory),read=()=>fs.readFileSync(installed.filename,'utf8').trim().split('\n').map(JSON.parse);
  contents.emit('console-message',{level:'error',message:'TypeError: sample.map is not a function\n at Fixture (app.js:4:2) Bearer private-token',sourceId:'app.js?token=private',lineNumber:4});
  contents.emit('console-message',{},3,'Legacy error sk-abcdefghijklmnopqrst',5,'legacy.js');
  contents.emit('console-message',{level:'info',message:'not retained'});
  contents.emit('render-process-gone',{}, {reason:'crashed',exitCode:5});
  let rows=read();assert.equal(rows.length,4);assert.match(rows[1].message,/TypeError: sample.map/);assert.match(rows[1].message,/app.js:4:2/);assert.doesNotMatch(JSON.stringify(rows),/private-token|token=private|sk-abcdefghijklmnopqrst/);assert.equal(rows[2].source,'legacy.js');assert.equal(rows[3].reason,'crashed');
  fs.appendFileSync(installed.filename,'x'.repeat(1024*1024));contents.emit('console-message',{level:'warning',message:'after rotation'});assert.equal(read()[0].message,'after rotation');assert.ok(fs.existsSync(path.join(directory,'renderer.previous.log')));
  fs.appendFileSync(installed.filename,'x'.repeat(1024*1024));contents.emit('console-message',{level:'error',message:'second rotation'});assert.equal(read()[0].message,'second rotation');
  installed.dispose();assert.equal(contents.listenerCount('console-message'),0);
  // Remove only this test's exact unique temporary directory.
  fs.rmSync(directory,{recursive:true,force:true});
});
test('logging failures do not throw into the desktop shell',()=>{
  const file=path.join(os.tmpdir(),`phaseforge-renderer-blocked-${process.pid}.txt`);fs.writeFileSync(file,'fixture');const contents=new EventEmitter();assert.doesNotThrow(()=>{const installed=installRendererDiagnostics(contents,file);contents.emit('console-message',{level:'error',message:'still usable'});installed.dispose();});fs.unlinkSync(file);
});
