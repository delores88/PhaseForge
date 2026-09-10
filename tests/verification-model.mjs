// Execute actual pure protocol validation and API helper code with local fetch doubles.
import fs from 'node:fs';import assert from 'node:assert/strict';
const load=async p=>import('data:text/javascript;base64,'+Buffer.from(fs.readFileSync(new URL(p,import.meta.url),'utf8')).toString('base64'));
const v=await load('../frontend/src/lib/verification.js'),{api}=await load('../frontend/src/lib/api.js');let n=0;
const test=(name,fn)=>{fn();n++;console.log('PASS',name);};
const study={id:'s',recipe:{seed:13,absolute_tolerance:1e-6,relative_tolerance:.001,objectives:[{metric:'state'}],descriptors:[{metric:'state',minimum:0,maximum:2}]}};
const trial={id:'t',metrics:{state:.5}},p=v.initialVerification(study,trial);
test('valid defaults accepted without claiming they are science',()=>assert.equal(v.validateVerification(p,study),''));
test('new seed not exploration seed',()=>assert.notEqual(p.holdout_seed,study.recipe.seed));
test('fixed descriptor scale',()=>assert.equal(p.metrics[0].scale,2));
test('controls are not manufactured',()=>assert.deepEqual(p.controls,[]));
test('no fabricated reference corpus',()=>assert.deepEqual(p.reference_ids,[]));
test('same holdout seed rejected',()=>assert(v.validateVerification({...p,holdout_seed:13},study)));
for(const [key,value] of [['holdout_replicates',1],['holdout_replicates',17],['perturb_fraction',0],['perturb_fraction',.26],['wall_seconds',4],['wall_seconds',3601],['max_rhs_evaluations',99],['max_rhs_evaluations',20000001],['duplicate_distance',-1],['solver_absolute_tolerance',0]])test('reject invalid '+key+'='+value,()=>assert(v.validateVerification({...p,[key]:value},study)));
test('missing metric rejected',()=>assert(v.validateVerification({...p,metrics:[]},study)));
test('zero feature scale rejected',()=>assert(v.validateVerification({...p,metrics:[{...p.metrics[0],scale:0}]},study)));
test('infinite tolerance rejected',()=>assert(v.validateVerification({...p,metrics:[{...p.metrics[0],absolute_tolerance:Infinity}]},study)));
test('partial control rejected',()=>assert(v.validateVerification({...p,controls:[{run_id:'r',metric:'state',minimum:0,maximum:1,rationale:''}]},study)));
test('null external measurement never turns into zero',()=>assert.throws(()=>v.externalMeasurements('{"state":null}',p.metrics)));
test('strings are not reference measurements',()=>assert.throws(()=>v.externalMeasurements('{"state":"1"}',p.metrics)));
test('extraneous metric rejected',()=>assert.throws(()=>v.externalMeasurements('{"state":1,"extra":2}',p.metrics)));
test('valid external measurement parsed',()=>assert.equal(v.externalMeasurements('{"state":0}',p.metrics).state,0));
test('running state cannot be reviewed',()=>assert.equal(v.terminalVerification('running'),false));
test('task counts retain true progress',()=>assert.deepEqual(v.taskProgress({completed_tasks:2,total_tasks:8,stage:'Adaptive replay'}),{completed:2,total:8,determinate:true,label:'Adaptive replay'}));
let calls=[],status=200,body={};globalThis.fetch=async(url,options={})=>{calls.push({url,options});return{ok:status===200,status,statusText:'fixture',json:async()=>body};};
for(const [fn,args,url,verb] of [
 ['verificationList',['s'],'/api/verification/dossiers?study_id=s','GET'],['verification',['d'],'/api/verification/dossiers/d','GET'],
 ['createVerification',[p],'/api/verification/dossiers','POST'],['startVerification',['d'],'/api/verification/dossiers/d/start','POST'],
 ['cancelVerification',['d'],'/api/verification/dossiers/d/cancel','POST'],['probeVerification',[],'/api/verification/probe','POST'],
 ['verificationCatalog',['p'],'/api/verification/catalog?project_id=p','GET'],['addVerificationReference',[{}],'/api/verification/catalog','POST'],
 ['verificationSearch',['d','careful query',true],'/api/verification/dossiers/d/search','POST'],
 ['saveVerificationReview',['d',{}],'/api/verification/dossiers/d/review','POST'],['verificationAgentReview',['d'],'/api/verification/dossiers/d/agent-review','POST']]){
 await api[fn](...args);test(fn+' route',()=>{assert(calls.at(-1).url.endsWith(url));assert.equal(calls.at(-1).options.method||'GET',verb);});
}
await api.verificationSearch('d','query',false);test('consent propagated not invented',()=>assert.equal(JSON.parse(calls.at(-1).options.body).allow_public_query,false));
const c=new AbortController();await api.verification('d',c.signal);test('poll is cancellable',()=>assert.equal(calls.at(-1).options.signal,c.signal));
await api.evidenceWindow('run',{series:'a & b',start:0,end:1,max_points:100},c.signal);test('window query encoded',()=>{const url=new URL(calls.at(-1).url);assert.equal(url.searchParams.get('series'),'a & b');assert.equal(url.searchParams.get('start'),'0');assert.equal(calls.at(-1).options.signal,c.signal);});
status=422;body={error:{message:'Unsupported independent engine'}};await assert.rejects(api.startVerification('d'),/Unsupported independent engine/);n++;
console.log(`PASS ${n} verification helper/API checks; fetch doubles, no native server.`);
