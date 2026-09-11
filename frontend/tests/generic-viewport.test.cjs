const test=require('node:test');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');

// Execute the actual JSX component with React. A build alone does not detect
// missing imports in a footer rendered only when a saved run contains frames.
// Next's existing SWC/webpack dependencies avoid adding a second transpiler.
const frontend=path.resolve(__dirname,'..');
const work=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-viewport-regression-'));
const webpack=require(path.join(frontend,'node_modules/next/dist/compiled/webpack/webpack')).webpack;
fs.writeFileSync(path.join(work,'loader.cjs'),`module.exports=function(source){if(this.resourcePath.endsWith('.css'))return 'export default {}';return require(${JSON.stringify(path.join(frontend,'node_modules/next/dist/build/swc'))}).transformSync(source,{filename:this.resourcePath,jsc:{parser:{syntax:'ecmascript',jsx:true},transform:{react:{runtime:'automatic'}}},module:{type:'es6'}}).code;};`);
fs.writeFileSync(path.join(work,'entry.jsx'),`import React from 'react';import{renderToStaticMarkup}from'react-dom/server';import GenericViewport from ${JSON.stringify(path.join(frontend,'src/components/workspace/GenericViewport.jsx'))};export const render=run=>renderToStaticMarkup(<GenericViewport run={run}/>);`);
const component=new Promise((resolve,reject)=>webpack({mode:'development',context:frontend,target:'node',entry:path.join(work,'entry.jsx'),output:{path:work,filename:'component.cjs',library:{type:'commonjs2'}},resolve:{extensions:['.js','.jsx','.mjs'],modules:[path.join(frontend,'node_modules')],alias:{'@':path.join(frontend,'src')}},module:{rules:[{test:/\.(jsx|css)$/,use:path.join(work,'loader.cjs')}]},optimization:{minimize:false},devtool:false},(error,stats)=>{if(error||stats.hasErrors())reject(error||Error(stats.toString({all:false,errors:true})));else resolve(require(path.join(work,'component.cjs')));}));

test('GenericViewport renders retained frames and its simulation timeline',async()=>{
  const {render}=await component;
  const run={id:'retained-state-regression',status:'completed',result:{visualization:{frames:[
    {time:0,entities:[{id:'sample',position:[0,0,0]}]},
    {time:100.00000000011343,entities:[{id:'sample',position:[1,2,3]}]},
  ]}}};
  const html=render(run);
  assert.match(html,/2 recorded frames/);
  assert.match(html,/aria-label="Simulation time"[^>]*min="0"[^>]*max="10000"[^>]*step="1"[^>]*value="0"/);
  assert.match(html,/aria-label="Next recorded frame"/);
  assert.doesNotMatch(html,/Build a world around your question/);
});

test('GenericViewport without retained frames keeps its empty state',async()=>{
  const {render}=await component;
  const html=render({id:'empty-state-regression',status:'completed'});
  assert.match(html,/Build a world around your question/);
  assert.doesNotMatch(html,/aria-label="Simulation time"/);
});
