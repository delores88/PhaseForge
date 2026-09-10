// Executes actual JS helpers and API calls with deterministic fetch doubles. Not a browser/backend test.
import fs from 'node:fs';import assert from 'node:assert/strict';
const read=p=>fs.readFileSync(new URL(p,import.meta.url),'utf8');
const load=async p=>import('data:text/javascript;base64,'+Buffer.from(read(p)).toString('base64'));
const m=await load('../frontend/src/lib/discovery.js');const {api}=await load('../frontend/src/lib/api.js');let passed=0;
const check=(name,f)=>{f();passed++;console.log('PASS',name);};
const manifest={id:'m',title:'Study',hypothesis:'test',model:{kind:'state_vector_ode',variables:[{name:'x',initial:1}]},constants:[{name:'k',value:2}],search:{variables:[]},observables:[{name:'state',unit:''}]};
const r=m.initialRecipe(manifest);r.parameters=[{name:'p',target:'initial:x',minimum:0,maximum:1}];
check('draft is not autonomous by default',()=>assert.equal(r.auto_review,false));check('valid study accepted',()=>assert.equal(m.validateRecipe(r),''));
check('empty ranges rejected',()=>assert(m.validateRecipe({...r,parameters:[]})));check('invalid bounds rejected',()=>assert(m.validateRecipe({...r,parameters:[{...r.parameters[0],maximum:-1}]})));
check('map needs descriptor',()=>assert(m.validateRecipe({...r,strategy:'map_elites'})));check('trial ceiling enforced',()=>assert(m.validateRecipe({...r,exploration_trials:513})));
check('nonfinite tolerance rejected',()=>assert(m.validateRecipe({...r,relative_tolerance:NaN})));check('zero is real',()=>assert.equal(m.fmt(0),'0'));check('missing is not zero',()=>assert.equal(m.fmt(null),'—'));
check('queued not export-ready',()=>assert.equal(m.terminalStudy('running'),false));check('target list retains source values',()=>assert.equal(m.targets(manifest)[1].value,1));
check('refinements excluded from exploration plot',()=>assert.equal(m.eligiblePoints({trials:[{phase:'half_step',eligible:true},{phase:'explore',eligible:true}]}).length,1));
let calls=[],status=200,body={},blob=new Blob(['zip']);globalThis.fetch=async(url,opts={})=>{calls.push({url,opts});return {ok:status===200,status,json:async()=>body,blob:async()=>blob,statusText:'fixture'};};
const c=new AbortController();
for(const [method,args,suffix,verb] of [['studies',['p',c.signal],'/api/discovery/studies?project_id=p','GET'],['study',['s',c.signal],'/api/discovery/studies/s','GET'],['createStudy',[r],'/api/discovery/studies','POST'],['startStudy',['s'],'/api/discovery/studies/s/start','POST'],['pauseStudy',['s'],'/api/discovery/studies/s/pause','POST'],['cancelStudy',['s'],'/api/discovery/studies/s/cancel','POST'],['reviewStudy',['s'],'/api/discovery/studies/s/review','POST'],['nextStudyProposal',['s'],'/api/discovery/studies/s/next-proposal','POST'],['forkStudy',['s'],'/api/discovery/studies/s/fork','POST'],['notebook',['p'],'/api/research/notebooks/p','GET'],['saveNotebook',['p',{revision:1}],'/api/research/notebooks/p','PUT'],['signals',['r',c.signal],'/api/runs/r/signals','GET']]){
 await api[method](...args);check(method+' route and verb',()=>{assert(calls.at(-1).url.endsWith(suffix));assert.equal(calls.at(-1).opts.method||'GET',verb);});
}
await api.literature('test public query');check('explicit metadata consent',()=>assert.deepEqual(JSON.parse(calls.at(-1).opts.body),{query:'test public query',allow_public_query:true}));
await api.study('s',c.signal);check('aborted signal preserved',()=>assert.equal(calls.at(-1).opts.signal,c.signal));
assert.equal(await api.exportStudy('s'),blob);passed++;
status=422;body={error:{message:'Pause before export'}};await assert.rejects(api.exportStudy('s'),/Pause before export/);passed++;
status=409;body={error:{message:'record changed'}};await assert.rejects(api.saveNotebook('p',{}),/record changed/);passed++;
console.log(`PASS ${passed} discovery view-model / API helper checks; fetch doubled, no live server.`);
