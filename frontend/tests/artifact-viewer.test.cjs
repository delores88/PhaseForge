const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const frontend=path.resolve(__dirname,'..'),work=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-artifact-viewer-'));
const webpack=require(path.join(frontend,'node_modules/next/dist/compiled/webpack/webpack')).webpack;
fs.writeFileSync(path.join(work,'loader.cjs'),`module.exports=function(source){if(this.resourcePath.endsWith('.css'))return 'export default {}';return require(${JSON.stringify(path.join(frontend,'node_modules/next/dist/build/swc'))}).transformSync(source,{filename:this.resourcePath,jsc:{parser:{syntax:'ecmascript',jsx:true},transform:{react:{runtime:'automatic'}}},module:{type:'es6'}}).code;};`);
fs.writeFileSync(path.join(work,'entry.jsx'),`import React from 'react';import{renderToStaticMarkup}from'react-dom/server';import Viewer from ${JSON.stringify(path.join(frontend,'src/components/workspace/LaboratoryArtifactViewer.jsx'))};export const render=job=>renderToStaticMarkup(<Viewer job={job}/>);`);
const component=new Promise((resolve,reject)=>webpack({mode:'development',context:frontend,target:'node',entry:path.join(work,'entry.jsx'),output:{path:work,filename:'component.cjs',library:{type:'commonjs2'}},resolve:{extensions:['.js','.jsx','.mjs'],modules:[path.join(frontend,'node_modules')],alias:{'@':path.join(frontend,'src')}},module:{rules:[{test:/\.mjs$/,resolve:{fullySpecified:false}},{test:/\.(jsx|css)$/,use:path.join(work,'loader.cjs')}]},optimization:{minimize:false},devtool:false},(error,stats)=>{if(error||stats.hasErrors())reject(error||Error(stats.toString({all:false,errors:true})));else resolve(require(path.join(work,'component.cjs')));}));
const job=path=>({id:'saved-analysis',kind:'generated',state:'completed',title:'Numerical integral',result:{artifacts:[{path,bytes:32,sha256:'a'.repeat(64)}]}});
test('actual analysis viewer with only JSON has no image canvas or inert camera controls, and retains its download',async()=>{
  const {render}=await component,html=render(job('work/result.json'));
  assert.doesNotMatch(html,/Image controls|Image canvas|Image magnification|Zoom image|Fullscreen/);
  assert.match(html,/Loading and verifying the saved result/);assert.match(html,/download="phaseforge-saved-analysis-result.json"/);assert.match(html,/All retained files &amp; provenance/);
});
test('actual image output retains its image inspector while nonvisual attachments render only their file access',async()=>{
  const {render}=await component;assert.match(render(job('work/figure.png')),/Image controls/);
  const html=render(job('work/dataset.npz'));assert.doesNotMatch(html,/Image controls|Image canvas/);assert.match(html,/Saved files are available below/);assert.match(html,/dataset.npz/);
});

test('mixed retained output exposes explicit image and numerical views even when canonical result is omitted from its own artifact list',async()=>{
  const {render}=await component,mixed=job('work/figure.png');mixed.result.artifacts.push({path:'work/result.json',bytes:25,sha256:'b'.repeat(64)});mixed.result.reported_result={artifacts:['figure.png']};
  const html=render(mixed);assert.match(html,/Saved output view/);assert.match(html,/>Numerical result<\/button>/);assert.match(html,/Image controls/);assert.match(html,/download="phaseforge-saved-analysis-result.json"/);
});
