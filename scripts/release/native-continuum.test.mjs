/** Synthetic independent references only; this never launches the app/worker. */
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';
import {HEAT_PARAMETERS,FLOW_PARAMETERS,CONTINUUM_UNITS,validateContinuum} from './native-continuum.mjs';
import {createLaboratoryAcceptance,acceptanceComplete} from './native-laboratory.mjs';

const H='a'.repeat(64),matrix=fn=>Array.from({length:16},(_,y)=>Array.from({length:16},(_,x)=>fn(x,y))),mean=a=>a.flat().reduce((s,x)=>s+x,0)/256;
function fixture(engine){
  const heat=engine==='heat_conduction_2d',p=structuredClone(heat?HEAT_PARAMETERS:FLOW_PARAMETERS),units=CONTINUUM_UNITS[engine],primary=heat?'temperature_K':'vorticity_s_inv',fieldUnit=units[primary];
  const manifest={schema_version:1,engine,input:{engine,parameters:p},platform:{processor:'CPU'},units:{length:'m',time:'s',channels:units},solver:{method:heat?'conservative five-point FTCS':'Fourier vorticity-streamfunction pseudospectral RK4, strict 2/3 dealiasing',dtype:'float64',boundary:'periodic'},display_coordinates:{length_unit:'um',metres_to_display:1e6},environment:{python:'3.13.15',numpy:'2.4.6',pillow:'12.3.0'},numpy_version:'2.4.6',input_sha256:H,worker_sha256:H,shared_io_sha256:H,initial_state_sha256:H};
  const index={schema_version:1,representation:'scalar_field',field_name:primary,field_unit:fieldUnit,time_unit:'s',length_unit:'um',shape:[16,16],axis_order:['y','x'],grid_location:'cell_center',boundary:'periodic',channels:units,lengths_um:[p.length_x_m*1e6,p.length_y_m*1e6],x_um:Array.from({length:16},(_,x)=>(x+.5)*p.length_x_m/16*1e6),y_um:Array.from({length:16},(_,y)=>(y+.5)*p.length_y_m/16*1e6),solver_coordinates:{length_unit:'m',x_m:Array.from({length:16},(_,x)=>(x+.5)*p.length_x_m/16),y_m:Array.from({length:16},(_,y)=>(y+.5)*p.length_y_m/16)},frames:[],frame_count:5,start_time:0,end_time:20*p.dt_s};
  const views=[],arrays=[],measurements={schema_version:1,time_unit:'s',field_unit:fieldUnit,series:[]},observations={schema_version:1,images:[]};
  const dx=p.length_x_m/16,dy=p.length_y_m/16;
  let temperature=matrix((x,y)=>300+20*Math.cos(2*Math.PI*(x+.5)/16)*Math.cos(4*Math.PI*(y+.5)/16));
  for(let step=0;step<=20;step++){
    if(step%5===0){
      let channels;
      if(heat){channels={temperature_K:structuredClone(temperature),heat_flux_x_W_m2:matrix((x,y)=>-.6*(temperature[y][(x+1)%16]-temperature[y][(x+15)%16])/(2*dx)),heat_flux_y_W_m2:matrix((x,y)=>-.6*(temperature[(y+1)%16][x]-temperature[(y+15)%16][x])/(2*dy))};}
      else{
        // Continuous exact Taylor–Green fields, independent of the checker's
        // discrete RK4 amplification polynomial. At this fixed timestep its
        // O(dt^4) global error lies below the preregistered bounds.
        const U=.1*Math.exp(-.01*8*Math.PI*Math.PI*step*.005),wave=(x,y)=>[2*Math.PI*(x+.5)/16,2*Math.PI*(y+.5)/16];
        channels={vorticity_s_inv:matrix((x,y)=>{const [X,Y]=wave(x,y);return 4*Math.PI*U*Math.sin(X)*Math.sin(Y);}),velocity_x_m_s:matrix((x,y)=>{const [X,Y]=wave(x,y);return U*Math.sin(X)*Math.cos(Y);}),velocity_y_m_s:matrix((x,y)=>{const [X,Y]=wave(x,y);return -U*Math.cos(X)*Math.sin(Y);}),pressure_Pa:matrix((x,y)=>{const [X,Y]=wave(x,y);return .5*U*U*(Math.cos(2*X)+Math.cos(2*Y));}),divergence_s_inv:matrix(()=>0)};
      }
      const values=channels[primary],average=mean(values),flat=values.flat(),frame={step,time:step*p.dt_s,path:`fields/field-${step}.npy`,sha256:H,view_path:`fields/view-${step}.json`,view_sha256:H,state_path:`fields/state-${step}.npz`,state_sha256:H,channels:units};
      const row={step,time_s:step*p.dt_s,mean:average,min:Math.min(...flat),max:Math.max(...flat),variance:mean(values.map(line=>line.map(v=>(v-average)**2))),field_path:frame.path,field_sha256:H,state_path:frame.state_path,state_sha256:H};
      if(heat){const basis=matrix((x,y)=>Math.cos(2*Math.PI*(x+.5)/16)*Math.cos(4*Math.PI*(y+.5)/16));row.mode_amplitude_K=mean(matrix((x,y)=>(values[y][x]-average)*basis[y][x]))/mean(basis.map(line=>line.map(v=>v*v)));row.thermal_energy_per_depth_J_m=average*256*dx*dy*p.density_kg_m3*p.heat_capacity_J_kgK;}
      else{const u=channels.velocity_x_m_s,v=channels.velocity_y_m_s;Object.assign(row,{kinetic_energy_per_mass_m2_s2:.5*mean(matrix((x,y)=>u[y][x]**2+v[y][x]**2)),enstrophy_s_inv2:.5*mean(values.map(line=>line.map(w=>w*w))),divergence_max_s_inv:0,mean_velocity_x_m_s:mean(u),mean_velocity_y_m_s:mean(v),circulation_m2_s:average*p.length_x_m*p.length_y_m,advective_cfl:p.dt_s*(Math.max(...u.flat().map(Math.abs))/dx+Math.max(...v.flat().map(Math.abs))/dy),mean_pressure_Pa:mean(channels.pressure_Pa)});}
      if(step===0)index.color_scale={min:row.min,max:row.max,map:'linear_blue_orange'};
      index.frames.push(frame);views.push({step,time:frame.time,shape:[16,16],field_unit:fieldUnit,values:structuredClone(values)});arrays.push({channels,state_sha256:H,primary_sha256:H});measurements.series.push(row);observations.images.push({step,time:frame.time,sha256:H,field_source:{path:frame.path,sha256:H},color_scale:index.color_scale});
    }
    if(heat){const r=p.conductivity_W_mK*p.dt_s/(p.density_kg_m3*p.heat_capacity_J_kgK);temperature=matrix((x,y)=>temperature[y][x]+r*((temperature[y][(x+1)%16]+temperature[y][(x+15)%16]-2*temperature[y][x])/(dx*dx)+(temperature[(y+1)%16][x]+temperature[(y+15)%16][x]-2*temperature[y][x])/(dy*dy)));}
  }
  const checkpoint={schema_version:1,step:20,time_s:20*p.dt_s,index,measurements,observations};for(const key of ['input_sha256','worker_sha256','shared_io_sha256','initial_state_sha256','numpy_version'])checkpoint[key]=manifest[key];
  const result={status:'completed',engine,steps:20,simulated_time_s:20*p.dt_s,initial:measurements.series[0],final:measurements.series.at(-1)};
  if(heat)result.relative_energy_drift=(result.final.thermal_energy_per_depth_J_m-result.initial.thermal_energy_per_depth_J_m)/result.initial.thermal_energy_per_depth_J_m;else result.relative_kinetic_energy_change=result.final.kinetic_energy_per_mass_m2_s2/result.initial.kinetic_energy_per_mass_m2_s2-1;
  return {manifest,index,views,arrays,measurements,checkpoint,observations,result};
}

test('heat spectral reference checks every stencil-evolved temperature and physical flux cell',()=>{const result=validateContinuum(fixture('heat_conduction_2d'));assert.equal(result.all_channel_cell_comparisons,3840);assert.ok(result.max_channel_errors.temperature_K<1e-11);});
test('flow discrete reference matches independent continuous Taylor–Green fields at the frozen small step',()=>{const result=validateContinuum(fixture('navier_stokes_2d'));assert.equal(result.all_channel_cell_comparisons,6400);assert.ok(result.max_channel_errors.velocity_x_m_s<1e-12);});
test('a correct temperature picture cannot conceal corrupted authoritative flux or state pins',()=>{for(const mutate of [f=>f.arrays[4].channels.heat_flux_x_W_m2[8][3]+=.01,f=>f.arrays[2].state_sha256='b'.repeat(64),f=>f.arrays[1].channels.temperature_K[5][2]+=.01]){const f=fixture('heat_conduction_2d');mutate(f);assert.throws(()=>validateContinuum(f));}});
test('flow reference rejects wrong pressure sign, missing velocity, false incompressibility and wrong energy',()=>{for(const mutate of [f=>f.arrays[3].channels.pressure_Pa[0][0]*=-1,f=>delete f.arrays[4].channels.velocity_x_m_s,f=>f.arrays[1].channels.divergence_s_inv[5][3]=.01,f=>f.measurements.series[2].kinetic_energy_per_mass_m2_s2*=2]){const f=fixture('navier_stokes_2d');mutate(f);assert.throws(()=>validateContinuum(f));}});
test('units, cell orientation, physical time, fixed range and immutable checkpoint remain required',()=>{for(const mutate of [f=>f.index.length_unit='m',f=>f.index.solver_coordinates.x_m[2]*=1e6,f=>f.index.axis_order=['x','y'],f=>f.views[2].time=0,f=>f.index.color_scale.max+=1,f=>f.index.frames[4].state_path=f.index.frames[3].state_path,f=>f.checkpoint={...f.checkpoint,index:{...f.index,end_time:99}},f=>f.manifest.input.parameters.density_kg_m3=3]){const f=fixture('heat_conduction_2d');mutate(f);assert.throws(()=>validateContinuum(f));}});
test('completion and still-frame substitution cannot satisfy a temporal continuum reference',()=>{for(const mutate of [f=>f.result.status='running',f=>f.index.frame_count=1,f=>f.views[4]=f.views[0],f=>f.observations.images.pop()]){const f=fixture('navier_stokes_2d');mutate(f);assert.throws(()=>validateContinuum(f));}});

const BUILD_ARRAY_FIXTURE=String.raw`
import io,json,pathlib,struct,sys,zipfile
request=json.load(sys.stdin)
root=pathlib.Path(request['root'])
def npy(rows):
    assert len(rows)==16 and all(len(row)==16 for row in rows)
    header=repr({'descr':'<f8','fortran_order':False,'shape':(16,16)}).encode('ascii')
    header+=b' '*((-(10+len(header)+1))%64)+b'\n'
    return b'\x93NUMPY\x01\x00'+struct.pack('<H',len(header))+header+struct.pack('<256d',*[v for row in rows for v in row])
for frame in request['frames']:
    primary=root/frame['path'];primary.parent.mkdir(parents=True,exist_ok=True)
    primary.write_bytes(npy(frame['channels'][request['primary']]))
    with zipfile.ZipFile(root/frame['state_path'],'w') as archive:
        for name,rows in frame['channels'].items():
            with archive.open(name+'.npy','w',force_zip64=True) as member:member.write(npy(rows))
`;

test('actual continuum capture admits an explicit host decoder and keeps the process receipt for both engines',{timeout:30000},async()=>{
  // This is a retained-data software fixture. Transport and observed solver
  // identity are mocked; the host Python decoder is a real isolated invocation.
  // It must not be called installed/native or scientific execution evidence.
  const python=execFileSync(process.env.PHASEFORGE_ACCEPTANCE_PYTHON||'python',['-I','-B','-c','import sys; print(sys.executable)'],{encoding:'utf8',windowsHide:true,timeout:10000}).trim();
  assert.ok(path.isAbsolute(python));
  const previous=process.env.PHASEFORGE_ACCEPTANCE_PYTHON;process.env.PHASEFORGE_ACCEPTANCE_PYTHON=python;
  const parent=fs.realpathSync.native(os.tmpdir()),temporary=fs.realpathSync.native(fs.mkdtempSync(path.join(parent,'phaseforge-continuum-capture-'))),here=path.dirname(fileURLToPath(import.meta.url));
  const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
  try{
    for(const engine of ['heat_conduction_2d','navier_stokes_2d']){
      const f=fixture(engine),directory=path.join(temporary,engine),source=path.join(directory,'source'),output=path.join(directory,'evidence');fs.mkdirSync(source,{recursive:true});
      execFileSync(python,['-I','-B','-c',BUILD_ARRAY_FIXTURE],{input:JSON.stringify({root:source,primary:f.index.field_name,frames:f.index.frames.map((frame,n)=>({...frame,channels:f.arrays[n].channels}))}),windowsHide:true,timeout:10000});
      const put=(name,value)=>{const file=path.join(source,name);fs.mkdirSync(path.dirname(file),{recursive:true});const bytes=Buffer.isBuffer(value)?value:Buffer.from(JSON.stringify(value)+'\n');fs.writeFileSync(file,bytes);return sha(bytes);};
      for(const [file,key]of [['continuum_worker.py','worker_sha256'],['field_worker.py','shared_io_sha256']]){const bytes=fs.readFileSync(path.resolve(here,'../../tools',file));f.manifest[key]=put(file,bytes);f.checkpoint[key]=f.manifest[key];}
      f.index.frames.forEach((frame,n)=>{
        frame.sha256=sha(fs.readFileSync(path.join(source,frame.path)));frame.state_sha256=sha(fs.readFileSync(path.join(source,frame.state_path)));frame.view_sha256=put(frame.view_path,f.views[n]);
        Object.assign(f.measurements.series[n],{field_sha256:frame.sha256,state_sha256:frame.state_sha256});
        const image=f.observations.images[n];image.path=`observations/fixture-${frame.step}.png`;image.sha256=put(image.path,Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/l9sAAAAASUVORK5CYII=','base64'));image.field_source.sha256=frame.sha256;
      });
      const projectId=crypto.randomUUID(),id=crypto.randomUUID(),created='2026-09-12T01:00:00Z',observation={pid:4242,created:Date.parse(created)/1000+.1,exe:path.join(directory,'environments/science-v5/python.exe')};
      const job={id,project_id:projectId,kind:'solver',state:'completed',title:'Synthetic retained capture fixture',created_at:created,input:structuredClone(f.manifest.input),events:[{sequence:1,kind:'solver_started',at:'2026-09-12T01:00:01Z',data:{pid:observation.pid}}]};
      for(const [name,value]of [['input.json',job.input],['worker-input.json',job.input],['manifest.json',f.manifest],['result.json',f.result],['measurements.json',f.measurements],['checkpoint.json',f.checkpoint],['observations/index.json',f.observations],['fields/index.json',f.index]])put(name,value);
      const reads=[];const page={evaluate:async(_callback,{route})=>{const prefix=`/api/laboratory/jobs/${id}/artifacts/`;assert.ok(route.startsWith(prefix));const name=route.slice(prefix.length).split('/').map(decodeURIComponent).join('/');assert.ok(!name.split('/').includes('..'));reads.push(name);return {status:200,base64:fs.readFileSync(path.join(source,name)).toString('base64')};}};
      const acceptance=createLaboratoryAcceptance({page,call(){throw Error('Capture cannot start or control an application job');},remember:()=>[],observedProcesses:()=>[observation],workspace:directory,output,projectId,sourceCommit:'a'.repeat(40),version:'retained-software-fixture',backendSha256:'b'.repeat(64)});
      const captured=await acceptance.captureRetainedJob(job,'fixture');const key=engine==='heat_conduction_2d'?'heat':'flow';
      assert.equal(acceptance.evidence.checks[key].passed,true);assert.equal(acceptance.evidence.checks[key].all_channel_cell_comparisons,key==='heat'?3840:6400);assert.equal(acceptanceComplete(acceptance.evidence),false,'Capture fixtures cannot satisfy installed acceptance');
      assert.deepEqual(acceptance.evidence.processes[id],observation);const receipt=JSON.parse(fs.readFileSync(path.join(output,'laboratory/jobs',id,'fixture/process-observation.json'),'utf8'));assert.deepEqual(receipt,{job_id:id,event:job.events[0],process:observation});
      assert.equal(reads.filter(name=>name.endsWith('.npz')).length,5);assert.equal(reads.filter(name=>name.endsWith('.npy')).length,5);assert.equal(captured.receipt.artifacts.filter(row=>row.path.endsWith('.npz')).length,5);
      // The same capture boundary must reject changed retained archive bytes
      // before invoking a decoder or recording a passing numerical check.
      const altered=path.join(source,f.index.frames[0].state_path);fs.appendFileSync(altered,'changed');
      await assert.rejects(()=>acceptance.captureRetainedJob(job,'changed-archive'),/Registered artifact hash/);
    }
  }finally{
    if(previous===undefined)delete process.env.PHASEFORGE_ACCEPTANCE_PYTHON;else process.env.PHASEFORGE_ACCEPTANCE_PYTHON=previous;
    assert.equal(path.dirname(temporary),parent);assert.match(path.basename(temporary),/^phaseforge-continuum-capture-/);assert.equal(fs.realpathSync.native(temporary),temporary);fs.rmSync(temporary,{recursive:true,force:true});
  }
});
