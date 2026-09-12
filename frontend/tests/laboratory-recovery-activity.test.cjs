const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const frontend=path.resolve(__dirname,'..'),work=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-recovery-activity-'));
const webpack=require(path.join(frontend,'node_modules/next/dist/compiled/webpack/webpack')).webpack;
fs.writeFileSync(path.join(work,'loader.cjs'),`module.exports=function(source){if(this.resourcePath.endsWith('.css'))return 'export default {}';return require(${JSON.stringify(path.join(frontend,'node_modules/next/dist/build/swc'))}).transformSync(source,{filename:this.resourcePath,jsc:{parser:{syntax:'ecmascript',jsx:true},transform:{react:{runtime:'automatic'}}},module:{type:'es6'}}).code;};`);
fs.writeFileSync(path.join(work,'entry.jsx'),`import React from 'react';import{renderToStaticMarkup}from'react-dom/server';import Activity from ${JSON.stringify(path.join(frontend,'src/components/workspace/LaboratoryActivity.jsx'))};export const render=job=>renderToStaticMarkup(<Activity jobs={[job]} onControl={()=>{}} onSelect={()=>{}} onAttentionAction={()=>{}}/>);`);
const component=new Promise((resolve,reject)=>webpack({mode:'development',context:frontend,target:'node',entry:path.join(work,'entry.jsx'),output:{path:work,filename:'component.cjs',library:{type:'commonjs2'}},resolve:{extensions:['.js','.jsx','.mjs'],modules:[path.join(frontend,'node_modules')],alias:{'@':path.join(frontend,'src')}},module:{rules:[{test:/\.mjs$/,resolve:{fullySpecified:false}},{test:/\.(jsx|css)$/,use:path.join(work,'loader.cjs')}]},optimization:{minimize:false},devtool:false},(error,stats)=>{if(error||stats.hasErrors())reject(error||Error(stats.toString({all:false,errors:true})));else resolve(require(path.join(work,'component.cjs')));}));
const base={id:'saved-nr',kind:'solver',title:'Retained numerical result',state:'completed',created_at:'2026-09-12T00:00:00Z',completed_at:'2026-09-12T00:01:00Z',updated_at:'2026-09-12T01:00:00Z',seen_at:'2026-09-12T00:02:00Z',input:{engine:'athenak_two_punctures_cuda'},events:[],recovery:{operation_id:'op-1',status:'started',phase:'verifying'}};
test('actual completed result stays openable during recovery and shows a separate enabled cancel control',async()=>{
  const {render}=await component,html=render(base);
  assert.match(html,/Verifying saved files/);assert.match(html,/1 recovering/);assert.match(html,/>Completed</);assert.match(html,/Open results/);
  assert.match(html,/<button type="button" disabled="">[\s\S]*?Recovering…<\/button>/);
  assert.match(html,/<button type="button"><svg[^]*?Cancel recovery<\/button>/);
  assert.doesNotMatch(html,/Time limit reached|New result|>Resume</);
});
test('actual recovery cancellation waits for drain and terminal failure preserves usable results with its exact error',async()=>{
  const {render}=await component,stopping=render({...base,recovery:{...base.recovery,status:'cancel_requested'}});
  assert.match(stopping,/Stopping recovery…/);assert.doesNotMatch(stopping,/Cancel recovery<\/button>/);assert.match(stopping,/Open results/);
  const failed=render({...base,recovery:{...base.recovery,status:'failed',phase:'finished',error:'Exact retained artifact hash changed'}});
  assert.match(failed,/Recovery failed/);assert.match(failed,/Exact retained artifact hash changed/);assert.match(failed,/Recover retained output/);assert.match(failed,/Open results/);assert.doesNotMatch(failed,/Cancel recovery<\/button>/);
});
test('actual paused capability gap has useful request/evidence navigation and no paid Resume or unused budget picker',async()=>{
  const {render}=await component;
  const job={...base,kind:'session',state:'paused',input:{model:'chosen-chat-model'},recovery:null,result:{attention:{schema:'phaseforge.agent-attention.v1',status:'needs_direction',kind:'capability_gap',reason:'This engine is not registered.',actions:[{id:'revise_request',label:'Revise request'},{id:'inspect_capabilities',label:'Inspect supported engines'},{id:'inspect_evidence',label:'Inspect saved evidence'}]}}};
  const html=render(job);assert.match(html,/Needs direction/);assert.match(html,/This engine is not registered/);assert.match(html,/Revise request<\/button>/);assert.match(html,/Inspect supported engines<\/button>/);assert.match(html,/Inspect saved evidence<\/button>/);
  assert.doesNotMatch(html,/>Resume<\/button>|Continuation time for|Recover retained output/);
});
