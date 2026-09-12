/** Frozen small installed-API references. No solver code is imported or executed. */
import assert from 'node:assert/strict';

export const HEAT_PARAMETERS=Object.freeze({nx:16,ny:16,length_x_m:.01,length_y_m:.02,conductivity_W_mK:.6,density_kg_m3:1000,heat_capacity_J_kgK:4184,dt_s:.01,steps:20,record_interval:5,boundary:'periodic',initial:Object.freeze({kind:'fourier',baseline_K:300,amplitude_K:20,mode_x:1,mode_y:2}),max_output_mb:64});
export const FLOW_PARAMETERS=Object.freeze({nx:16,ny:16,length_x_m:1,length_y_m:1,kinematic_viscosity_m2_s:.01,density_kg_m3:2,dt_s:.005,steps:20,record_interval:5,boundary:'periodic',initial:Object.freeze({kind:'taylor_green',velocity_m_s:.1,mode:1}),max_output_mb:64});
export const CONTINUUM_UNITS=Object.freeze({heat_conduction_2d:Object.freeze({temperature_K:'K',heat_flux_x_W_m2:'W/m^2',heat_flux_y_W_m2:'W/m^2'}),navier_stokes_2d:Object.freeze({vorticity_s_inv:'1/s',velocity_x_m_s:'m/s',velocity_y_m_s:'m/s',pressure_Pa:'Pa',divergence_s_inv:'1/s'})});
// Absolute bounds are frozen before the hosted run. Heat-flux roundoff amplifies
// temperature subtraction by k/dx; these are numerical integration checks only.
export const CONTINUUM_TOLERANCES=Object.freeze({temperature_K:1e-10,heat_flux_x_W_m2:1e-6,heat_flux_y_W_m2:1e-6,vorticity_s_inv:1e-10,velocity_x_m_s:1e-11,velocity_y_m_s:1e-11,pressure_Pa:1e-11,divergence_s_inv:1e-11});
const HASH=/^[a-f0-9]{64}$/;
function near(value,expected,tolerance,label){assert.ok(Number.isFinite(value)&&Math.abs(value-expected)<=tolerance,`${label}: observed ${value}, expected ${expected}, tolerance ${tolerance}`);return Math.abs(value-expected);}
function matrix(value,label){assert.ok(Array.isArray(value)&&value.length===16,`${label} rows`);value.forEach(row=>{assert.ok(Array.isArray(row)&&row.length===16,`${label} columns`);row.forEach(x=>assert.ok(Number.isFinite(x),`${label} finite`));});}

export function validateContinuum({manifest,measurements,index,views,arrays,result,checkpoint,observations}){
  const engine=manifest.engine,heat=engine==='heat_conduction_2d';
  assert.ok(Object.hasOwn(CONTINUUM_UNITS,engine),'Expected an admitted continuum engine');
  const p=heat?HEAT_PARAMETERS:FLOW_PARAMETERS,units=CONTINUUM_UNITS[engine],primary=heat?'temperature_K':'vorticity_s_inv',fieldUnit=units[primary];
  assert.equal(manifest.schema_version,1);assert.deepEqual(manifest.input,{engine,parameters:p});
  assert.equal(manifest.platform.processor,'CPU');assert.deepEqual(manifest.units,{length:'m',time:'s',channels:units});
  assert.equal(manifest.solver.method,heat?'conservative five-point FTCS':'Fourier vorticity-streamfunction pseudospectral RK4, strict 2/3 dealiasing');assert.equal(manifest.solver.dtype,'float64');assert.equal(manifest.solver.boundary,'periodic');
  assert.equal(manifest.display_coordinates.length_unit,'um');assert.equal(manifest.display_coordinates.metres_to_display,1e6);
  for(const key of ['input_sha256','worker_sha256','shared_io_sha256','initial_state_sha256'])assert.match(manifest[key],HASH);
  assert.equal(manifest.environment.python,'3.13.15');assert.equal(manifest.environment.numpy,'2.4.6');assert.equal(manifest.environment.pillow,'12.3.0');
  assert.equal(index.schema_version,1);assert.equal(index.representation,'scalar_field');assert.deepEqual(index.shape,[16,16]);assert.deepEqual(index.axis_order,['y','x']);assert.equal(index.grid_location,'cell_center');assert.equal(index.boundary,'periodic');
  assert.equal(index.field_name,primary);assert.equal(index.field_unit,fieldUnit);assert.equal(index.time_unit,'s');assert.equal(index.length_unit,'um');assert.deepEqual(index.channels,units);assert.deepEqual(index.lengths_um,[p.length_x_m*1e6,p.length_y_m*1e6]);assert.equal(index.solver_coordinates.length_unit,'m');
  for(const axis of ['x','y']){assert.equal(index[axis+'_um'].length,16);assert.equal(index.solver_coordinates[axis+'_m'].length,16);for(let n=0;n<16;n++){const coordinate=(n+.5)*p['length_'+axis+'_m']/16;near(index.solver_coordinates[axis+'_m'][n],coordinate,1e-15,'SI cell centre');near(index[axis+'_um'][n],coordinate*1e6,1e-8,'display unit conversion');}}
  assert.equal(index.frame_count,5);assert.equal(index.frames.length,5);assert.equal(index.start_time,0);near(index.end_time,p.steps*p.dt_s,1e-14,'retained endpoint');
  assert.equal(measurements.schema_version,1);assert.equal(measurements.time_unit,'s');assert.equal(measurements.field_unit,fieldUnit);assert.equal(measurements.series.length,5);assert.equal(views.length,5);assert.equal(arrays.length,5);
  assert.equal(result.status,'completed');assert.equal(result.engine,engine);assert.equal(result.steps,20);near(result.simulated_time_s,p.steps*p.dt_s,1e-14,'result endpoint');assert.deepEqual(result.initial,measurements.series[0]);assert.deepEqual(result.final,measurements.series.at(-1));
  assert.equal(checkpoint.schema_version,1);assert.equal(checkpoint.step,20);near(checkpoint.time_s,p.steps*p.dt_s,1e-14,'checkpoint endpoint');assert.deepEqual(checkpoint.index,index);assert.deepEqual(checkpoint.measurements,measurements);assert.deepEqual(checkpoint.observations,observations);
  for(const key of ['input_sha256','worker_sha256','shared_io_sha256','initial_state_sha256','numpy_version'])assert.equal(checkpoint[key],manifest[key]);
  assert.equal(observations.schema_version,1);assert.equal(observations.images.length,5);
  const dx=p.length_x_m/16,dy=p.length_y_m/16,kx=2*Math.PI*(heat?p.initial.mode_x:p.initial.mode)/p.length_x_m,ky=2*Math.PI*(heat?p.initial.mode_y:p.initial.mode)/p.length_y_m;
  const alpha=heat?p.conductivity_W_mK/(p.density_kg_m3*p.heat_capacity_J_kgK):null;
  const z=heat?null:-p.kinematic_viscosity_m2_s*(kx*kx+ky*ky)*p.dt_s;
  const factor=heat?1-4*alpha*p.dt_s*((Math.sin(kx*dx/2)/dx)**2+(Math.sin(ky*dy/2)/dy)**2):1+z+z*z/2+z**3/6+z**4/24;
  assert.ok(factor>0&&factor<1);
  const maxima=Object.fromEntries(Object.keys(units).map(key=>[key,0])),seen=new Set();
  for(let n=0;n<5;n++){
    const step=n*5,time=step*p.dt_s,frame=index.frames[n],view=views[n],raw=arrays[n],row=measurements.series[n],image=observations.images[n];
    assert.equal(frame.step,step);near(frame.time,time,1e-14,'frame time');assert.equal(view.step,step);near(view.time,time,1e-14,'view time');assert.equal(row.step,step);near(row.time_s,time,1e-14,'measurement time');assert.deepEqual(view.shape,[16,16]);assert.equal(view.field_unit,fieldUnit);assert.deepEqual(frame.channels,units);
    for(const [key,hashkey]of [['path','sha256'],['view_path','view_sha256'],['state_path','state_sha256']]){assert.equal(typeof frame[key],'string');assert.match(frame[hashkey],HASH);assert.ok(!seen.has(frame[key]),'Retained frame path reused');seen.add(frame[key]);}
    assert.equal(row.field_path,frame.path);assert.equal(row.field_sha256,frame.sha256);assert.equal(row.state_path,frame.state_path);assert.equal(row.state_sha256,frame.state_sha256);
    assert.equal(raw.state_sha256,frame.state_sha256);assert.equal(raw.primary_sha256,frame.sha256);assert.deepEqual(Object.keys(raw.channels).sort(),Object.keys(units).sort());assert.deepEqual(view.values,raw.channels[primary],'Displayed cells must match authoritative NPZ and NPY');
    assert.equal(image.step,step);near(image.time,time,1e-14,'observation time');assert.match(image.sha256,HASH);assert.deepEqual(image.field_source,{path:frame.path,sha256:frame.sha256});assert.deepEqual(image.color_scale,index.color_scale);
    Object.entries(raw.channels).forEach(([key,value])=>matrix(value,key));
    const amplitude=(heat?p.initial.amplitude_K:p.initial.velocity_m_s)*factor**step;
    for(let y=0;y<16;y++)for(let x=0;x<16;x++){
      const X=kx*(x+.5)*dx,Y=ky*(y+.5)*dy,sx=Math.sin(X),cx=Math.cos(X),sy=Math.sin(Y),cy=Math.cos(Y);
      const expected=heat?{temperature_K:p.initial.baseline_K+amplitude*cx*cy,heat_flux_x_W_m2:p.conductivity_W_mK*amplitude*Math.sin(kx*dx)/dx*sx*cy,heat_flux_y_W_m2:p.conductivity_W_mK*amplitude*Math.sin(ky*dy)/dy*cx*sy}:{vorticity_s_inv:2*kx*amplitude*sx*sy,velocity_x_m_s:amplitude*sx*cy,velocity_y_m_s:-amplitude*cx*sy,pressure_Pa:p.density_kg_m3*amplitude**2/4*(Math.cos(2*X)+Math.cos(2*Y)),divergence_s_inv:0};
      for(const key of Object.keys(units))maxima[key]=Math.max(maxima[key],near(raw.channels[key][y][x],expected[key],CONTINUUM_TOLERANCES[key],key+' analytic cell'));
    }
    const values=raw.channels[primary].flat(),mean=values.reduce((s,v)=>s+v,0)/256;
    near(row.mean,mean,1e-10,'measured mean');near(row.min,Math.min(...values),1e-10,'measured min');near(row.max,Math.max(...values),1e-10,'measured max');near(row.variance,values.reduce((s,v)=>s+(v-mean)**2,0)/256,1e-9,'measured variance');
    if(heat){near(row.mean,300,1e-10,'conserved mean temperature');near(row.mode_amplitude_K,amplitude,1e-10,'discrete temperature amplitude');near(row.thermal_energy_per_depth_J_m,300*p.length_x_m*p.length_y_m*p.density_kg_m3*p.heat_capacity_J_kgK,1e-5,'conserved thermal energy per depth');}
    else{near(row.kinetic_energy_per_mass_m2_s2,amplitude**2/4,1e-12,'Taylor-Green kinetic energy');near(row.enstrophy_s_inv2,kx*kx*amplitude**2/2,1e-11,'Taylor-Green enstrophy');for(const key of ['divergence_max_s_inv','mean_velocity_x_m_s','mean_velocity_y_m_s','circulation_m2_s','mean_pressure_Pa'])near(row[key],0,1e-11,key);near(row.advective_cfl,p.dt_s*amplitude*Math.cos(Math.PI/16)**2*(1/dx+1/dy),1e-12,'advective CFL');}
  }
  const first=arrays[0].channels[primary].flat();near(index.color_scale.min,Math.min(...first),1e-10,'fixed color low');near(index.color_scale.max,Math.max(...first),1e-10,'fixed color high');
  if(heat)near(result.relative_energy_drift,0,1e-12,'thermal energy drift');else near(result.relative_kinetic_energy_change,factor**40-1,1e-11,'kinetic energy decay');
  assert.notDeepEqual(arrays[0].channels[primary],arrays.at(-1).channels[primary],'Actual retained field must evolve');
  return {passed:true,engine,frames:5,primary_cell_comparisons:1280,all_channel_cell_comparisons:1280*Object.keys(units).length,discrete_decay_factor:factor,max_channel_errors:maxima,tolerances:CONTINUUM_TOLERANCES,raw_npy_npz_and_view_equal:true,checkpoint_aliases_equal:true,scope:heat?'Synthetic SI periodic conduction: discrete Fourier temperature, central-difference heat flux and conserved energy per depth. No material calibration.':'Synthetic periodic incompressible Taylor-Green flow: RK4 modal decay, velocity/vorticity, mean-zero pressure, kinetic energy, enstrophy and divergence. No general turbulence or experimental calibration.'};
}
