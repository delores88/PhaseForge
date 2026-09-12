/** Installed normal-API acceptance. Importing this module performs no native work. */
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';

const HERE=path.dirname(fileURLToPath(import.meta.url));
const ACTIVE=new Set(['queued','running','provisioning','waiting']);
const STATES=new Set([...ACTIVE,'completed','paused','cancelled','failed','timed_out']);
const UUID=/^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
const HASH=/^[a-f0-9]{64}$/;
const FIXTURE='phaseforge.native-laboratory.v1';
export const OPENMM_PARAMETERS=Object.freeze({atom_count:32,temperature_kelvin:120,density_g_cm3:0.8,steps:200,timestep_fs:1,seed:314159,platform:'CPU',thermostat:'langevin',friction_per_ps:1,sample_interval:20,chunk_frames:10,cpu_threads:1});
export const FIELD_PARAMETERS=Object.freeze({nx:16,ny:16,length_x_um:4,length_y_um:4,diffusivity_um2_s:0.2,dt_s:0.005,steps:20,record_interval:5,boundary:'periodic',initial:{kind:'fourier',baseline:1,amplitude:0.2,mode_x:1,mode_y:2},probes:[],max_output_mb:64});
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function bounded(promise,milliseconds=10000){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('Laboratory API request exceeded its bounded wait')),milliseconds);})]);}finally{clearTimeout(timer);}}
const samePath=(a,b)=>path.resolve(a).toLowerCase()===path.resolve(b).toLowerCase();
function finite(value){if(typeof value==='number')assert.ok(Number.isFinite(value),'Non-finite receipt');else if(Array.isArray(value))value.forEach(finite);else if(value&&typeof value==='object')Object.values(value).forEach(finite);return value;}
function json(bytes){return finite(JSON.parse(Buffer.from(bytes).toString('utf8').replace(/^\uFEFF/,'')));}
function near(actual,expected,tolerance,label){assert.ok(Number.isFinite(actual)&&Math.abs(actual-expected)<=tolerance,`${label}: observed ${actual}, expected ${expected}, tolerance ${tolerance}`);return Math.abs(actual-expected);}
function relative(name){assert.ok(typeof name==='string'&&name.length<=240&&!/[\\:\0]/.test(name)&&name.split('/').every(p=>p&&!['.','..'].includes(p)&&p.trimEnd()===p),'Unsafe artifact path');return name;}
function plain(file,{missing=false}={}){let current=path.resolve(file);for(;;){if(fs.existsSync(current)){const stat=fs.lstatSync(current);assert.ok(!stat.isSymbolicLink(),'Linked evidence path refused');if(stat.isFile())assert.equal(stat.nlink,1,'Hardlinked evidence file refused');}else assert.ok(missing,'Required evidence path absent');const parent=path.dirname(current);if(parent===current)break;current=parent;}return path.resolve(file);}
function write(file,value){file=plain(file,{missing:true});fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,JSON.stringify(finite(value),null,2)+'\n');}
function checkedJob(job,projectId){assert.ok(job&&UUID.test(job.id)&&job.project_id===projectId&&STATES.has(job.state)&&Array.isArray(job.events)&&job.input&&typeof job.input==='object','Unknown laboratory job schema');job.events.forEach((e,i)=>{assert.equal(e.sequence,i+1);assert.equal(typeof e.kind,'string');});return job;}
export function evidenceFiles(root,output){const pending=[plain(root)],rows=[];let total=0;while(pending.length){for(const entry of fs.readdirSync(pending.pop(),{withFileTypes:true})){const file=plain(path.join(entry.parentPath||entry.path,entry.name));if(entry.isDirectory()){pending.push(file);continue;}assert.ok(entry.isFile(),'Special evidence file refused');if(file===path.join(root,'state.json'))continue;const bytes=fs.readFileSync(file);total+=bytes.length;assert.ok(rows.length<5000&&total<256*1024*1024,'Laboratory evidence exceeds bound');rows.push({path:path.relative(output,file).split(path.sep).join('/'),bytes:bytes.length,sha256:sha(bytes)});}}return rows.sort((a,b)=>a.path.localeCompare(b.path));}

/** Scalar pair loop independent of the worker's vectorized instrument. */
export function pairReference(positions,model){
  const length=model.box_nm[0],cutoff=model.cutoff_nm,start=model.switch_nm,sigma=model.sigma_nm,epsilon=model.epsilon_kj_mol;
  assert.ok(length>0&&cutoff>start&&start>0&&sigma>0&&epsilon>0);
  let energy=0,virial=0;
  for(let i=0;i<positions.length;i++)for(let j=0;j<i;j++){
    let r2=0;for(let k=0;k<3;k++){let d=positions[i][k]-positions[j][k];d-=length*Math.round(d/length);r2+=d*d;}
    const r=Math.sqrt(r2);assert.ok(r>0&&Number.isFinite(r));if(r>=cutoff)continue;
    const q6=(sigma/r)**6,u=4*epsilon*(q6*q6-q6),raw=24*epsilon*(2*q6*q6-q6);
    let s=1,derivative=0;if(r>start){const x=(r-start)/(cutoff-start);s=1-10*x**3+15*x**4-6*x**5;derivative=(-30*x*x+60*x**3-30*x**4)/(cutoff-start);}
    energy+=s*u;virial+=s*raw-r*u*derivative;
  }return {energy,virial};
}
export function validateOpenMM({manifest,measurements,trajectory,result,frames}){
  assert.equal(manifest.schema_version,1);assert.equal(manifest.engine,'openmm_argon');assert.equal(manifest.platform,'CPU');assert.equal(manifest.platform_properties.Threads,'1');
  for(const [key,value]of Object.entries(OPENMM_PARAMETERS))assert.deepEqual(manifest.parameters[key],value);
  assert.equal(measurements.schema_version,1);assert.equal(measurements.units.pressure,'bar');assert.equal(measurements.units.energy,'kJ/mol');assert.equal(measurements.units.time,'ps');
  assert.equal(result.status,'completed');assert.equal(result.completed_steps,200);assert.equal(result.frame_count,11);assert.equal(measurements.series.length,11);assert.equal(frames.length,11);
  assert.equal(trajectory.schema_version,1);
  let energyError=0,virialError=0,pressureError=0;
  for(let index=0;index<11;index++){
    const row=measurements.series[index],frame=frames[index],step=index*20;
    assert.equal(row.step,step);assert.equal(frame.step,step);assert.equal(frame.entities.length,32);
    near(row.time_ps,step*.001,1e-12,'OpenMM time');near(frame.time,row.time_ps,1e-12,'frame time');
    near(row.total_energy_kj_mol,row.potential_energy_kj_mol+row.kinetic_energy_kj_mol,1e-10,'energy sum');
    near(row.temperature_kelvin,2*row.kinetic_energy_kj_mol/(93*.00831446261815324),1e-9,'temperature instrument');
    const reference=pairReference(frame.entities.map(entity=>entity.position),manifest.model);
    energyError=Math.max(energyError,near(row.potential_energy_kj_mol,reference.energy,.002,'CPU pair energy'));
    virialError=Math.max(virialError,near(row.pair_virial_kj_mol,reference.virial,1e-8,'pair virial'));
    const pressure=(2*row.kinetic_energy_kj_mol+reference.virial)/(3*manifest.model.box_nm[0]**3)*(1e25/6.02214076e23);
    pressureError=Math.max(pressureError,near(row.pressure_bar,pressure,1e-7,'pressure instrument'));
  }
  return {passed:true,frames:11,independent_pair_frames:11,max_energy_error_kj_mol:energyError,max_virial_error_kj_mol:virialError,max_pressure_error_bar:pressureError,scope:'Small deterministic CPU integration, scalar pair energy/virial and thermodynamic instruments; no equilibrium, calibration, biological or broad scientific acceptance claim'};
}
export function validateField({manifest,measurements,index,views,result}){
  const p=FIELD_PARAMETERS;assert.equal(manifest.schema_version,1);assert.equal(manifest.engine,'diffusion_2d');assert.equal(manifest.solver.method,'five-point FTCS');
  for(const [key,value]of Object.entries(p))assert.deepEqual(manifest.input.parameters[key],value);
  assert.equal(measurements.schema_version,1);assert.equal(measurements.time_unit,'s');assert.equal(index.schema_version,1);assert.deepEqual(index.shape,[16,16]);assert.equal(result.status,'completed');assert.equal(result.steps,20);
  assert.equal(measurements.series.length,5);assert.equal(views.length,5);assert.equal(index.frames.length,5);
  const factor=1-4*p.diffusivity_um2_s*p.dt_s*((Math.sin(Math.PI/16)/(4/16))**2+(Math.sin(2*Math.PI/16)/(4/16))**2);
  let maximum=0;
  views.forEach((view,i)=>{const step=i*5,row=measurements.series[i],amplitude=.2*factor**step;assert.equal(row.step,step);assert.equal(view.step,step);assert.deepEqual(view.shape,[16,16]);
    near(row.time,step*p.dt_s,1e-14,'field time');near(row.mean,1,1e-12,'mean');near(row.integral,16,1e-11,'integral');near(row.mode_amplitude,amplitude,1e-12,'discrete Fourier amplitude');near(row.variance,amplitude**2/4,1e-12,'variance');
    assert.equal(view.values.length,16);view.values.forEach((line,y)=>{assert.equal(line.length,16);line.forEach((value,x)=>{const expected=1+amplitude*Math.cos(2*Math.PI*(x+.5)/16)*Math.cos(4*Math.PI*(y+.5)/16);maximum=Math.max(maximum,near(value,expected,1e-12,'cell Fourier reference'));});});
  });return {passed:true,frames:5,cell_comparisons:1280,discrete_decay_factor:factor,max_cell_error:maximum,scope:'Independent discrete Fourier solution and conservation of a synthetic periodic passive scalar; no physical calibration claim'};
}
export function validateBoundary(report){
  assert.equal(report.fixture,FIXTURE);assert.deepEqual(report.token,{appcontainer:1,less_privileged:1});assert.deepEqual(report.provider_environment_present,[]);
  assert.deepEqual(report.matrix,[[4,1,0],[1,3,1],[0,1,2]]);assert.deepEqual(report.rhs,[1,2,3]);assert.equal(report.solution.length,3);
  const expected=[2/9,1/9,13/9];report.solution.forEach((x,i)=>near(x,expected[i],1e-12,'independent rational solution'));
  report.matrix.forEach((row,i)=>near(row.reduce((sum,a,j)=>sum+a*report.solution[j],0),report.rhs[i],1e-12,'independent matrix residual'));
  for(const name of ['outside_read','outside_hardlink','outside_write','runtime_write','loopback_network','external_network']){assert.equal(report[name]?.denied,true,name);assert.ok([5,10013].includes(report[name].winerror)||report[name].errno===13,'Denial must be permission-based, not timeout/refusal');}
  assert.ok(report.linear_residual<1e-12);assert.ok(Number.isFinite(report.linear_solve_elapsed_seconds)&&report.linear_solve_elapsed_seconds>=0);assert.equal(report.host_observation_hold_seconds,2);return {passed:true,token:report.token,solution:report.solution,synthetic_canaries:6,linear_solve_elapsed_seconds:report.linear_solve_elapsed_seconds,host_observation_hold_seconds:2,scope:'Actual LPAC token, retained numerical solution and denied synthetic file/network probes; no claim that every possible attack was tested'};
}
export function solverProcess(job,records,expectedExecutable){const starts=job.events.filter(event=>event.kind==='solver_started');assert.equal(starts.length,1,'Expected one actual solver process launch');const event=starts[0];assert.ok(Number.isInteger(event.data.pid));const created=Date.parse(job.created_at)/1000,at=Date.parse(event.at)/1000;const matches=records.filter(row=>row.pid===event.data.pid&&Number.isFinite(row.created)&&row.created>=created-1&&row.created<=at+1&&samePath(row.exe,expectedExecutable));assert.equal(matches.length,1,'Solver PID must match one sampled bundled-runtime process creation');if(event.data.python)assert.ok(samePath(event.data.python,expectedExecutable),'Solver launch path must be bundled science-v5');return matches[0];}
export function verifyRestart(oldJob,newJob){assert.notEqual(newJob.id,oldJob.id);assert.equal(newJob.input.restarted_from_job_id,oldJob.id);assert.equal(newJob.deadline_at,oldJob.deadline_at);for(const key of ['engine','code','inputs','sources','limits'])assert.deepEqual(newJob.input[key],oldJob.input[key]);return true;}
export function verifyRestoredStopped(job,expected,{replacementId}={}){assert.equal(job.id,expected.id);assert.equal(job.state,expected.state);assert.equal(job.deadline_at,expected.deadline_at);assert.deepEqual(job.input,expected.input);assert.ok(job.events.some(event=>event.kind===(replacementId?'interrupted':'cancelled')));if(replacementId)assert.ok(job.events.some(event=>event.kind==='generated_restart_requested'&&event.data.new_job_id===replacementId));return true;}
export function acceptanceComplete(state){return Boolean(state.first_launch_passed&&state.cancel?.passed&&state.quit?.observed_active&&state.restart?.passed&&state.restored?.passed&&state.restored.index===3&&state.checks?.openmm?.passed&&state.checks?.field?.passed&&state.checks?.boundary?.passed);}

export function createLaboratoryAcceptance({page,call,remember,observedProcesses,workspace,output,projectId,sourceCommit,version,backendSha256}){
  assert.ok(UUID.test(projectId)&&/^[a-f0-9]{40}$/.test(sourceCommit)&&HASH.test(backendSha256));
  assert.equal(typeof observedProcesses,'function','Pass the native creation-time process history callback');
  const root=plain(path.join(output,'laboratory'),{missing:true}),statePath=path.join(root,'state.json');fs.mkdirSync(root,{recursive:true});
  const binding={source_commit:sourceCommit,version,backend_sha256:backendSha256,project_id:projectId};
  const state=fs.existsSync(statePath)?json(fs.readFileSync(plain(statePath))):{schema:FIXTURE,...binding,active_elapsed_ms:0,jobs:{},checks:{},artifact_sets:[],phases:[]};
  for(const [key,value]of Object.entries(binding))assert.equal(state[key],value,'Laboratory evidence identity changed');assert.equal(state.schema,FIXTURE);
  const save=()=>write(statePath,state);
  const api=(route,body)=>bounded(call(page,route,body));
  const get=async id=>checkedJob(await api(`/api/laboratory/jobs/${id}`),projectId);
  let phaseBegan=0;
  const remaining=()=>290000-state.active_elapsed_ms-(phaseBegan?Date.now()-phaseBegan:0);
  async function phase(name,run){const began=Date.now();phaseBegan=began;try{assert.ok(remaining()>0,'Laboratory acceptance exceeded five-minute active-work bound');const result=await run();assert.ok(remaining()>0,'Laboratory acceptance exceeded five-minute active-work bound');state.phases.push({name,passed:true,elapsed_ms:Date.now()-began,workspace:path.resolve(workspace)});return result;}catch(error){state.phases.push({name,passed:false,error:String(error.stack||error),elapsed_ms:Date.now()-began});throw error;}finally{state.active_elapsed_ms+=Date.now()-began;phaseBegan=0;save();}}
  async function until(predicate,label,timeout=90000){const end=Date.now()+Math.min(timeout,Math.max(1,remaining()));while(Date.now()<end){const value=await predicate();if(value)return value;await sleep(150);}throw Error(`Timed out waiting for ${label}`);}
  async function fetchArtifact(id,name,{optional=false}={}){
    assert.ok(UUID.test(id));relative(name);
    const result=await bounded(page.evaluate(async({route,limit})=>{const response=await fetch(route,{signal:AbortSignal.timeout(9000)});const bytes=new Uint8Array(await response.arrayBuffer());if(bytes.length>limit)throw Error('Artifact exceeds acceptance byte bound');if(!response.ok)return {status:response.status,error:new TextDecoder().decode(bytes).slice(0,2000)};let encoded='';for(let i=0;i<bytes.length;i+=32768)encoded+=String.fromCharCode(...bytes.subarray(i,i+32768));return {status:200,base64:btoa(encoded)};},{route:`/api/laboratory/jobs/${id}/artifacts/${name.split('/').map(encodeURIComponent).join('/')}`,limit:16*1024*1024}));
    if(result.status!==200){if(optional&&[404,500].includes(result.status)&&/not found|cannot find|readable after|unavailable|os error 2|os error 3/i.test(result.error||''))return null;throw Error(`Laboratory artifact ${id}/${name}: ${result.status} ${result.error}`);}
    return Buffer.from(result.base64,'base64');
  }
  const runtime=path.join(workspace,'environments/python-numpy-v4/runtime/python.exe');
  const processRows=()=>remember().filter(row=>samePath(row.exe,runtime));
  async function create(name,kind,input,time=90){const job=checkedJob(await api(`/api/projects/${projectId}/laboratory/jobs`,{request_id:crypto.randomUUID(),kind,title:`Native acceptance: ${name}`,input,time_limit_seconds:time}),projectId);state.jobs[name]=job.id;save();return job;}
  const generated=(mode,seconds,extra={})=>({engine:'python_numpy',code:fs.readFileSync(path.join(HERE,'laboratory-boundary.py'),'utf8'),inputs:{fixture:FIXTURE,mode,...(seconds?{seconds}:{}),...extra},sources:[],limits:{memory_mb:512,process_limit:1,wall_seconds:90,storage_mb:64}});
  async function completed(id){return until(async()=>{const job=await get(id);if(ACTIVE.has(job.state)){remember();return null;}assert.equal(job.state,'completed',JSON.stringify(job));return job;},`completed numerical job ${id}`);}
  async function started(id){return until(async()=>{const job=await get(id);assert.ok(ACTIVE.has(job.state),`Generated attempt stopped before active-process observation: ${JSON.stringify(job)}`);if(!job.events.some(e=>e.kind==='isolated_started'))return null;const rows=processRows();assert.ok(rows.length<=1,'Ambiguous LPAC process identity');if(!rows.length)return null;const row=rows[0];assert.ok(Number.isInteger(row.pid)&&Number.isFinite(row.created));return {job,process:row};},`LPAC process identity for ${id}`);}
  async function drained(id,process){return until(async()=>{const job=await get(id);if(ACTIVE.has(job.state)||remember().some(row=>row.pid===process.pid&&row.created===process.created))return null;const bytes=await fetchArtifact(id,'generated-artifacts.json',{optional:true});if(!bytes)return null;const inventory=json(bytes);assert.equal(inventory.schema_version,1);assert.ok(Array.isArray(inventory.artifacts));const workInput=await fetchArtifact(id,'work/input.json',{optional:true});return workInput?{job,inventory}:null;},`process and artifact drain for ${id}`);}
  async function capture(job,label){
    assert.ok(!ACTIVE.has(job.state));const directory=path.join(root,'jobs',job.id,label);write(path.join(directory,'job.json'),job);
    const records=[],cache=new Map();let total=0;
    async function retain(name,expected){assert.ok(remaining()>0,'Laboratory active-work budget exhausted');if(cache.has(name))return cache.get(name);const bytes=await fetchArtifact(job.id,name);const row={path:name,bytes:bytes.length,sha256:sha(bytes)};if(expected?.sha256)assert.equal(row.sha256,expected.sha256,`Registered artifact hash ${name}`);if(expected?.bytes!==undefined)assert.equal(row.bytes,expected.bytes);total+=bytes.length;assert.ok(total<=64*1024*1024&&records.length<512,'Fixture artifact set exceeds acceptance bounds');const dest=plain(path.join(directory,'artifacts',relative(name)),{missing:true});fs.mkdirSync(path.dirname(dest),{recursive:true});fs.writeFileSync(dest,bytes);records.push(row);cache.set(name,bytes);return bytes;}
    const input=await retain('input.json');assert.deepEqual(json(input),job.input);
    if(job.kind==='generated'){
      for(const name of ['source.py','source-input.json','prepared.json','generated-artifacts.json'])await retain(name);
      assert.equal((await retain('source.py')).toString('utf8'),job.input.code);assert.deepEqual(json(await retain('source-input.json')),job.input.inputs);
      const inventory=json(await retain('generated-artifacts.json'));assert.equal(inventory.schema_version,1);assert.ok(Array.isArray(inventory.artifacts));
      for(const row of inventory.artifacts){assert.ok(row.path.startsWith('work/')&&HASH.test(row.sha256)&&Number.isSafeInteger(row.bytes));await retain(row.path,row);}
      if(job.state==='completed'){
        for(const name of ['execution.json','manifest.json','result.json'])await retain(name);
        const manifest=json(await retain('manifest.json'));assert.equal(manifest.schema_version,1);assert.equal(manifest.job_id,job.id);assert.equal(manifest.outcome.boundary,'windows_lpac_no_network');assert.equal(manifest.outcome.exit_code,0);
        const seen=state.processes?.[job.id];if(seen)assert.equal(manifest.outcome.process_id,seen.pid);
      }
    }else{
      const process=solverProcess(job,observedProcesses(),path.join(workspace,'environments/science-v5/python.exe'));state.processes??={};state.processes[job.id]=process;write(path.join(directory,'process-observation.json'),{job_id:job.id,event:job.events.find(event=>event.kind==='solver_started'),process});
      for(const name of ['worker-input.json','manifest.json','result.json','measurements.json','checkpoint.json','observations/index.json'])await retain(name);
      const manifest=json(await retain('manifest.json'));const worker=job.input.engine==='openmm_argon'?'scientific_worker.py':'field_worker.py';await retain(worker,{sha256:manifest.worker_sha256});assert.equal(sha(fs.readFileSync(path.resolve(HERE,'../../tools',worker))),manifest.worker_sha256);
      const checkpoint=json(await retain('checkpoint.json'));if(job.input.engine==='openmm_argon'){await retain(checkpoint.checkpoint_path,{sha256:checkpoint.checkpoint_sha256});for(const name of ['restart.json','system.xml','integrator.xml'])await retain(name);}else await retain('checkpoint.npz',{sha256:checkpoint.checkpoint_sha256});
      const observations=json(await retain('observations/index.json'));assert.equal(observations.schema_version,1);assert.ok(Array.isArray(observations.images));for(const image of observations.images)await retain(image.path,{sha256:image.sha256});
      if(job.input.engine==='openmm_argon'){
        const trajectory=json(await retain('trajectory/index.json'));await retain('topology.json');const frames=[];
        assert.ok(Array.isArray(trajectory.chunks)&&trajectory.chunks.length<=12);
        for(const chunk of trajectory.chunks){const data=json(await retain(chunk.path,{sha256:chunk.sha256}));assert.equal(data.schema_version,1);frames.push(...data.frames);await retain(chunk.arrays_path,{sha256:chunk.arrays_sha256});}
        state.checks.openmm=validateOpenMM({manifest,measurements:json(await retain('measurements.json')),trajectory,result:json(await retain('result.json')),frames});
      }else{
        assert.equal(job.input.engine,'diffusion_2d');const index=json(await retain('fields/index.json')),views=[];assert.ok(Array.isArray(index.frames)&&index.frames.length<=5);
        for(const frame of index.frames){await retain(frame.path,{sha256:frame.sha256});views.push(json(await retain(frame.view_path,{sha256:frame.view_sha256})));}
        state.checks.field=validateField({manifest,measurements:json(await retain('measurements.json')),index,views,result:json(await retain('result.json'))});
      }
    }
    const receipt={job_id:job.id,label,state:job.state,input_sha256:sha(input),artifacts:records,total_bytes:total};write(path.join(directory,'artifact-receipts.json'),receipt);
    if(job.state==='completed')state.artifact_sets.push(receipt);save();return {receipt,cache};
  }
  async function firstLaunch(){return phase('first_launch',async()=>{
    assert.ok(!state.first_launch_passed&&Object.keys(state.jobs).length===0,'Fresh laboratory acceptance required');
    await capture(await completed((await create('openmm','solver',{engine:'openmm_argon',parameters:OPENMM_PARAMETERS})).id),'completed');
    await capture(await completed((await create('field','solver',{engine:'diffusion_2d',parameters:FIELD_PARAMETERS})).id),'completed');
    const canaries=path.join(root,'canaries');fs.mkdirSync(canaries,{recursive:true});const readCanary=path.join(canaries,'read.txt'),writeCanary=path.join(canaries,'must-not-write.txt'),runtimeCanary=path.join(path.dirname(runtime),'release-must-not-write.txt');
    fs.writeFileSync(plain(readCanary,{missing:true}),'Synthetic release boundary canary; contains no user data.');const before=sha(fs.readFileSync(readCanary));assert.ok(!fs.existsSync(writeCanary)&&!fs.existsSync(runtimeCanary));
    let connections=0,received=0;const server=net.createServer(socket=>{connections++;socket.on('data',data=>{received+=data.length;});socket.end();});
    await new Promise((resolve,reject)=>{server.once('error',reject);server.listen(0,'127.0.0.1',resolve);});
    try{
      const job=await create('boundary','generated',generated('boundary',null,{read_canary:readCanary,write_canary:writeCanary,runtime_canary:runtimeCanary,listening_port:server.address().port}));
      const observed=await started(job.id);state.processes??={};state.processes[job.id]=observed.process;
      const done=await completed(job.id);await drained(job.id,observed.process);const captured=await capture(done,'completed');state.checks.boundary=validateBoundary(json(captured.cache.get('work/numerical.json')));
      assert.equal(connections,0);assert.equal(received,0);assert.equal(sha(fs.readFileSync(readCanary)),before);assert.ok(!fs.existsSync(writeCanary)&&!fs.existsSync(runtimeCanary));
      state.checks.boundary.host_canary_receipt={read_sha256:before,read_unchanged:true,outside_write_absent:true,runtime_write_absent:true,loopback_connections:connections,loopback_received_bytes:received};
    }finally{await new Promise(resolve=>server.close(resolve));}
    const cancelled=await create('explicit_cancel','generated',generated('sleep',60));const observed=await started(cancelled.id);state.processes??={};state.processes[cancelled.id]=observed.process;
    await api(`/api/laboratory/jobs/${cancelled.id}/control`,{action:'cancel'});const stopped=await drained(cancelled.id,observed.process);assert.equal(stopped.job.state,'cancelled');const cancelledCapture=await capture(stopped.job,'cancelled');state.cancel={passed:true,job_id:cancelled.id,job:stopped.job,artifact_receipt:cancelledCapture.receipt,process:observed.process,process_gone:true,registered_artifacts_readable:true};
    const usage=await api('/api/usage');assert.equal(usage.totals.total_tokens,0);state.provider_tokens=0;state.first_launch_passed=true;save();return state;
  });}
  async function beforeQuit(){return phase('before_quit',async()=>{
    assert.ok(state.first_launch_passed&&!state.quit);const job=await create('quit_active','generated',generated('sleep',8),240);const observed=await started(job.id);state.processes??={};state.processes[job.id]=observed.process;
    const input=await fetchArtifact(job.id,'input.json');assert.deepEqual(json(input),observed.job.input);const sourceArtifacts=[];
    for(const name of ['input.json','source.py','source-input.json','prepared.json']){const bytes=name==='input.json'?input:await fetchArtifact(job.id,name);sourceArtifacts.push({path:name,bytes:bytes.length,sha256:sha(bytes)});const destination=path.join(root,'quit-active-artifacts',name);fs.mkdirSync(path.dirname(destination),{recursive:true});fs.writeFileSync(plain(destination,{missing:true}),bytes);if(name==='source.py')assert.equal(bytes.toString('utf8'),observed.job.input.code);if(name==='source-input.json')assert.deepEqual(json(bytes),observed.job.input.inputs);}
    state.quit={observed_active:true,job_id:job.id,job:observed.job,process:observed.process,input_sha256:sha(input),source_artifacts:sourceArtifacts,deadline_at:observed.job.deadline_at};write(path.join(root,'quit-active.json'),state.quit);save();return state.quit;
  });}
  async function reopen(index){return phase(`reopen_${index}`,async()=>{
    assert.ok([2,3].includes(index)&&state.quit?.observed_active);
    if(index===2){
      assert.ok(!state.restart);const old=await get(state.quit.job_id);assert.equal(old.state,'paused');assert.ok(old.events.some(event=>event.kind==='interrupted'),'Normal runtime recovery must retain explicit interruption event');assert.equal(old.deadline_at,state.quit.deadline_at);assert.equal(sha(await fetchArtifact(old.id,'input.json')),state.quit.input_sha256);write(path.join(root,'reopen-2-original.json'),old);
      const next=checkedJob(await api(`/api/laboratory/jobs/${old.id}/control`,{action:'resume'}),projectId);verifyRestart(old,next);state.jobs.resumed=next.id;const observed=await started(next.id);state.processes??={};state.processes[next.id]=observed.process;const done=await completed(next.id);await drained(next.id,observed.process);verifyRestart(old,done);const captured=await capture(done,'completed');const result=json(captured.cache.get('work/result.json'));assert.deepEqual(result.token,{appcontainer:1,less_privileged:1});assert.equal(result.seconds,8);assert.equal(sha(await fetchArtifact(old.id,'input.json')),state.quit.input_sha256);
      state.restart={passed:true,original_job_id:old.id,new_job_id:done.id,original_input_unchanged:true,identical_source_and_inputs:true,exact_deadline_preserved:true,deadline_at:done.deadline_at,process:observed.process};save();return state.restart;
    }
    assert.ok(state.restart?.passed&&!state.restored);const checks=[];
    for(const set of state.artifact_sets){const job=await get(set.job_id);assert.equal(job.state,'completed');assert.equal(sha(await fetchArtifact(job.id,'input.json')),set.input_sha256);for(const row of set.artifacts){const bytes=await fetchArtifact(job.id,row.path);assert.equal(bytes.length,row.bytes);assert.equal(sha(bytes),row.sha256,`Restored artifact changed: ${job.id}/${row.path}`);}checks.push({job_id:job.id,artifacts_checked:set.artifacts.length,input_sha256:set.input_sha256});write(path.join(root,'restored-jobs',`${job.id}.json`),job);}
    assert.equal(checks.length,4);const stoppedChecks=[];
    for(const [kind,expected,artifacts,replacementId]of [['explicit_cancel',state.cancel.job,state.cancel.artifact_receipt.artifacts,null],['quit_original',{...state.quit.job,state:'paused'},state.quit.source_artifacts,state.restart.new_job_id]]){const job=await get(expected.id);verifyRestoredStopped(job,expected,{replacementId});for(const row of artifacts){const bytes=await fetchArtifact(job.id,row.path);assert.equal(bytes.length,row.bytes);assert.equal(sha(bytes),row.sha256,`Restored stopped-attempt artifact changed: ${job.id}/${row.path}`);}write(path.join(root,'restored-jobs',`${job.id}.json`),job);stoppedChecks.push({kind,job_id:job.id,state:job.state,artifacts_checked:artifacts.length,input_unchanged:true,source_and_inputs_unchanged:true,lineage_verified:true});}
    const usage=await api('/api/usage');assert.equal(usage.totals.total_tokens,0);state.restored={passed:true,index:3,workspace:path.resolve(workspace),jobs:checks,stopped_jobs:stoppedChecks,scope:'Normal API reads from the caller-provided closed-backup restored data directory; backup/connection-closure proof is in the parent native receipt'};
    assert.ok(acceptanceComplete(state));assert.ok(remaining()>0,'Laboratory acceptance exceeded five-minute active-work bound');write(path.join(root,'completion-state.json'),state);const receipt={schema:FIXTURE,passed:true,...binding,scope:'Actual installed normal authenticated laboratory API; bounded CPU OpenMM and field references, generated NumPy/LPAC checks, explicit cancellation, normal-Quit interruption, immutable restart and exact restored-artifact hashes. No provider/model calls or Blender/ML scientific acceptance.',jobs:state.jobs,checks:{openmm:true,diffusion:true,lpac:true,cancel:true,quit_recovery:true,backup_restore:true},numerical_checks:state.checks,cancel:state.cancel,quit:state.quit,restart:state.restart,restored:state.restored,provider_tokens:0,artifact_sets:state.artifact_sets,evidence_files:evidenceFiles(root,path.resolve(output)),active_elapsed_ms:state.active_elapsed_ms+Date.now()-phaseBegan,limitations:['Storage is a monitored soft limit; no hard quota claim.','Small fixed numerical references do not establish broad physical/biological predictive validity.','The parent native process receipts establish final normal-Quit process/service drainage.']};write(path.join(output,'LABORATORY_ACCEPTANCE.json'),receipt);save();return receipt;
  });}
  return {firstLaunch,beforeQuit,reopen,evidence:state};
}
