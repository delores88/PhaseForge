/** Synthetic checker fixtures only: no app, runtime, model, installer or scanner. */
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {OPENMM_PARAMETERS,FIELD_PARAMETERS,pairReference,validateOpenMM,validateField,validateBoundary,verifyRestart,verifyRestoredStopped,acceptanceComplete,evidenceFiles,solverProcess,createLaboratoryAcceptance} from './native-laboratory.mjs';

function removeFixture(temporary){const resolved=path.resolve(temporary);assert.equal(path.dirname(resolved),path.resolve(os.tmpdir()));assert.match(path.basename(resolved),/^phaseforge-lab-(evidence|factory)-/);assert.equal(fs.realpathSync(resolved),resolved);fs.rmSync(resolved,{recursive:true,force:true});}

test('scalar LJ reference agrees with independent exact unswitched pair and periodic displacement',()=>{
  const model={box_nm:[10,10,10],cutoff_nm:3,switch_nm:2,sigma_nm:1,epsilon_kj_mol:2};
  assert.deepEqual(pairReference([[0,0,0],[1,0,0]],model),{energy:0,virial:48});
  assert.deepEqual(pairReference([[0,0,0],[9,0,0]],model),{energy:0,virial:48});
  assert.deepEqual(pairReference([[0,0,0],[4,0,0]],model),{energy:0,virial:0});
});
test('scalar LJ reference includes switching derivative at midpoint',()=>{
  const r=2.5,q6=1/r**6,u=8*(q6*q6-q6),raw=48*(2*q6*q6-q6);
  const result=pairReference([[0,0,0],[r,0,0]],{box_nm:[10,10,10],cutoff_nm:3,switch_nm:2,sigma_nm:1,epsilon_kj_mol:2});
  assert.ok(Math.abs(result.energy-u/2)<1e-14);assert.ok(Math.abs(result.virial-(raw/2+r*u*1.875))<1e-14);
});
test('scalar pair reference rejects non-finite and overlapping coordinates',()=>{
  const model={box_nm:[10,10,10],cutoff_nm:3,switch_nm:2,sigma_nm:1,epsilon_kj_mol:2};
  assert.throws(()=>pairReference([[0,0,0],[0,0,0]],model));assert.throws(()=>pairReference([[NaN,0,0],[1,0,0]],model));
});
test('OpenMM checker refuses wrong runtime and unrecognized schemas before accepting numbers',()=>{
  assert.throws(()=>validateOpenMM({manifest:{schema_version:2}}));
  assert.throws(()=>validateOpenMM({manifest:{schema_version:1,engine:'openmm_argon',platform:'Reference'}}));
  assert.equal(OPENMM_PARAMETERS.atom_count,32);assert.equal(OPENMM_PARAMETERS.steps,200);assert.equal(OPENMM_PARAMETERS.sample_interval,20);assert.equal(OPENMM_PARAMETERS.cpu_threads,1);
});

function fieldFixture(){
  const p=structuredClone(FIELD_PARAMETERS),views=[],series=[],frames=[];
  let grid=Array.from({length:16},(_,y)=>Array.from({length:16},(_,x)=>1+.2*Math.cos(2*Math.PI*(x+.5)/16)*Math.cos(4*Math.PI*(y+.5)/16)));
  // Independent tiny stencil fixture checks the validator's spectral formula.
  for(let step=0;step<=20;step++){
    if(step%5===0){let sum=0,dot=0,norm=0;const values=grid.flat();for(let y=0;y<16;y++)for(let x=0;x<16;x++){sum+=grid[y][x];const mode=Math.cos(2*Math.PI*(x+.5)/16)*Math.cos(4*Math.PI*(y+.5)/16);dot+=(grid[y][x]-1)*mode;norm+=mode*mode;}const mean=sum/256;
      series.push({step,time:step*.005,mean,integral:mean*16,mode_amplitude:dot/norm,variance:values.reduce((sum,x)=>sum+(x-mean)**2,0)/256});views.push({step,shape:[16,16],values:structuredClone(grid)});frames.push({step});
    }
    grid=grid.map((line,y)=>line.map((value,x)=>value+.016*(grid[y][(x+1)%16]+grid[y][(x+15)%16]+grid[(y+1)%16][x]+grid[(y+15)%16][x]-4*value)));
  }
  return {manifest:{schema_version:1,engine:'diffusion_2d',solver:{method:'five-point FTCS'},input:{parameters:p}},measurements:{schema_version:1,time_unit:'s',series},index:{schema_version:1,shape:[16,16],frames},views,result:{status:'completed',steps:20}};
}
test('spectral field checker independently verifies all stencil fixture cells and integrals',()=>{const result=validateField(fieldFixture());assert.equal(result.cell_comparisons,1280);assert.ok(result.max_cell_error<1e-13);assert.equal(FIELD_PARAMETERS.max_output_mb,64);});
test('field checker rejects a cosmetically correct summary with one changed numeric cell',()=>{const fixture=fieldFixture();fixture.views[4].values[3][8]+=.01;assert.throws(()=>validateField(fixture),/cell Fourier/);});
test('field checker rejects changed conservation, units and requested physics',()=>{for(const change of [x=>x.measurements.series[1].integral+=.01,x=>x.measurements.time_unit='ms',x=>x.manifest.input.parameters.diffusivity_um2_s=.4]){const fixture=fieldFixture();change(fixture);assert.throws(()=>validateField(fixture));}});

function boundaryFixture(){const report={fixture:'phaseforge.native-laboratory.v1',token:{appcontainer:1,less_privileged:1},provider_environment_present:[],matrix:[[4,1,0],[1,3,1],[0,1,2]],rhs:[1,2,3],solution:[2/9,1/9,13/9],linear_residual:0,linear_solve_elapsed_seconds:.001,host_observation_hold_seconds:2};for(const name of ['outside_read','outside_hardlink','outside_write','runtime_write','loopback_network','external_network'])report[name]={denied:true,winerror:5,errno:13};return report;}
test('boundary checker validates retained rational solution and permission denials',()=>{assert.equal(validateBoundary(boundaryFixture()).synthetic_canaries,6);});
test('boundary checker rejects timeouts labelled as denial, weak token and wrong solution',()=>{for(const change of [x=>x.external_network={denied:true,winerror:10060,errno:110},x=>x.token.less_privileged=0,x=>x.solution[0]+=.01,x=>x.provider_environment_present=['OPENAI_API_KEY']]){const fixture=boundaryFixture();change(fixture);assert.throws(()=>validateBoundary(fixture));}});

function original(){return {id:'11111111-1111-4111-8111-111111111111',deadline_at:'2026-09-12T01:02:03Z',input:{engine:'python_numpy',code:'fixture source',inputs:{fixture:1},sources:[],limits:{memory_mb:512,process_limit:1,wall_seconds:90,storage_mb:64}}};}
function replacement(old){return {...structuredClone(old),id:'22222222-2222-4222-8222-222222222222',input:{...structuredClone(old.input),restarted_from_job_id:old.id}};}
test('restart requires a new immutable attempt and preserves exact absolute budget and sources',()=>{const old=original();assert.equal(verifyRestart(old,replacement(old)),true);for(const change of [x=>x.id=old.id,x=>x.deadline_at='2026-09-12T01:03:03Z',x=>x.input.code+='changed',x=>x.input.inputs.fixture=2,x=>x.input.sources=[{job_id:'invented'}],x=>x.input.restarted_from_job_id='wrong']){const next=replacement(old);change(next);assert.throws(()=>verifyRestart(old,next));}});
test('restored cancelled and interrupted attempts preserve terminal state, source and lineage',()=>{const expected={...original(),state:'cancelled'},cancelled={...structuredClone(expected),events:[{kind:'cancelled',data:{}}]};assert.equal(verifyRestoredStopped(cancelled,expected),true);const paused={...expected,state:'paused'},job={...structuredClone(paused),events:[{kind:'interrupted',data:{}},{kind:'generated_restart_requested',data:{new_job_id:'replacement'}}]};assert.equal(verifyRestoredStopped(job,paused,{replacementId:'replacement'}),true);for(const change of [x=>x.state='completed',x=>x.input.inputs.fixture=7,x=>x.events.pop(),x=>x.events[1].data.new_job_id='wrong']){const altered=structuredClone(job);change(altered);assert.throws(()=>verifyRestoredStopped(altered,paused,{replacementId:'replacement'}));}});
test('passing acceptance requires every real lifecycle phase including third restored launch',()=>{const state={first_launch_passed:true,cancel:{passed:true},quit:{observed_active:true},restart:{passed:true},restored:{passed:true,index:3},checks:{openmm:{passed:true},field:{passed:true},boundary:{passed:true}}};assert.equal(acceptanceComplete(state),true);for(const key of ['first_launch_passed','cancel','quit','restart','restored','checks']){const next=structuredClone(state);delete next[key];assert.equal(acceptanceComplete(next),false);}state.restored.index=2;assert.equal(acceptanceComplete(state),false);});
test('solver PID matching uses creation time and bundled path, rejecting stale PID and host Python',()=>{const executable=path.resolve('fixture/environments/science-v4/python.exe'),job={created_at:'2026-09-12T01:00:00Z',events:[{kind:'solver_started',at:'2026-09-12T01:00:01Z',data:{pid:42,python:executable}}]},row={pid:42,created:Date.parse('2026-09-12T01:00:00.500Z')/1000,exe:executable};assert.deepEqual(solverProcess(job,[row],executable),row);assert.throws(()=>solverProcess(job,[{...row,created:row.created-20}],executable));assert.throws(()=>solverProcess(job,[{...row,exe:path.resolve('host/python.exe')}],executable));assert.throws(()=>solverProcess(job,[row,row],executable));});
test('exact evidence hash list detects changed retained files and excludes mutable coordinator state',()=>{
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-lab-evidence-'));try{const root=path.join(temporary,'laboratory');fs.mkdirSync(root);fs.writeFileSync(path.join(root,'state.json'),'mutable');fs.writeFileSync(path.join(root,'job.json'),'first');let rows=evidenceFiles(root,temporary);assert.equal(rows.length,1);assert.equal(rows[0].path,'laboratory/job.json');assert.equal(rows[0].sha256,crypto.createHash('sha256').update('first').digest('hex'));fs.writeFileSync(path.join(root,'job.json'),'changed');assert.notEqual(evidenceFiles(root,temporary)[0].sha256,rows[0].sha256);}finally{removeFixture(temporary);}
});
test('factory performs no API calls on construction and rejects mismatched persisted source binding',()=>{
  const temporary=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-lab-factory-'));try{let calls=0;const options={page:{},call(){calls++;throw Error('Unexpected API call');},remember:()=>[],observedProcesses:()=>[],workspace:temporary,output:temporary,projectId:'11111111-1111-4111-8111-111111111111',sourceCommit:'a'.repeat(40),version:'fixture',backendSha256:'b'.repeat(64)};const companion=createLaboratoryAcceptance(options);assert.equal(calls,0);assert.equal(acceptanceComplete(companion.evidence),false);fs.writeFileSync(path.join(temporary,'laboratory/state.json'),JSON.stringify({...companion.evidence,source_commit:'c'.repeat(40)}));assert.throws(()=>createLaboratoryAcceptance(options),/identity changed/);assert.equal(calls,0);}finally{removeFixture(temporary);}
});
