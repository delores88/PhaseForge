const ensure=(ok,message)=>{if(!ok)throw Error(message);};
const finite=v=>typeof v==='number'&&Number.isFinite(v);
const hash=v=>typeof v==='string'&&/^[a-f0-9]{64}$/.test(v);
export const nrRelative=path=>{ensure(typeof path==='string'&&path.length>0&&!/[\\:\0]/.test(path)&&path.split('/').every(p=>p&&p!=='.'&&p!=='..'),'Unsafe NR artifact path');return path;};
export async function nrPinnedJson(raw,descriptor){
  const bytes=raw instanceof Uint8Array?raw:new Uint8Array(raw);
  ensure(bytes.byteLength<=32*1024*1024&&hash(descriptor?.sha256),'Unbounded or unpinned NR artifact');
  if(descriptor.bytes!==undefined)ensure(descriptor.bytes===bytes.byteLength,'NR artifact byte count differs');
  const digest=Array.from(new Uint8Array(await globalThis.crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');
  ensure(digest===descriptor.sha256,'NR artifact changed after its retained receipt');
  return JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
}
function inventoryMember(result,descriptor){
  nrRelative(descriptor?.path);
  ensure(Array.isArray(result.artifacts)&&result.artifacts.some(row=>row.path===descriptor.path&&row.sha256===descriptor.sha256&&row.bytes===descriptor.bytes),'NR diagnostic artifact is outside its pinned inventory');
  return descriptor;
}
export async function loadNRDiagnostics(job,loadBytes){
  const pin=job.result?.diagnostics;
  ensure(pin&&pin.path==='diagnostics/result.json'&&hash(pin.sha256),'No pinned NR diagnostic result is available yet');
  const result=await nrPinnedJson(await loadBytes(pin.path),pin);
  const gauge=result.schema==='phaseforge.nr-gauge-result.v1';
  ensure(gauge||result.schema==='phaseforge.nr-black-hole-result.v1','Unsupported NR diagnostic result');
  ensure((gauge?result.engine:result.engine_identity?.engine_id)===job.input?.engine,'NR engine identity differs');
  ensure((gauge?result.execution?.job_id:result.job_id)===job.id,'NR diagnostics belong to another execution');
  ensure(gauge?(result.fulfillment?.black_hole_collision_validated===false&&result.fulfillment?.physical_radiation===false):result.fulfillment?.requested_0_999c_collision===false,'Unsupported scientific fulfillment claim');
  const descriptor=inventoryMember(result,gauge?result.slice_index:result.diagnostic_slices);
  const base='diagnostics/';
  const rawIndex=await nrPinnedJson(await loadBytes(base+descriptor.path),descriptor);
  let measurements=null;
  if(gauge){inventoryMember(result,result.measurements);measurements=await nrPinnedJson(await loadBytes(base+result.measurements.path),result.measurements);}
  const index=normalizeNRIndex(rawIndex,{gauge,measurements});
  for(const frame of index.frames)inventoryMember(result,frame.data);
  return {...index,result,base,indexPin:descriptor,loadFrame:async number=>{
    ensure(Number.isInteger(number)&&number>=0&&number<index.frames.length,'Invalid recorded NR frame');
    const entry=index.frames[number];
    return normalizeNRFrame(await nrPinnedJson(await loadBytes(base+entry.data.path),entry.data),entry,index);
  }};
}
export function normalizeNRIndex(raw,{gauge=false,measurements=null}={}){
  ensure(raw?.schema===(gauge?'phaseforge.nr-diagnostic-slices.v1':'phaseforge.nr-amr-diagnostic-slices.v1'),'Unsupported NR slice index');
  ensure(raw.units?.length==='L'&&raw.units?.time==='L/c'&&raw.units?.si_mapping===null&&raw.time_unit==='L/c','NR diagnostic code units changed');
  ensure(raw.interpolation==='none'&&raw.grid_location==='cell_center'&&JSON.stringify(raw.axis_order)==='["y","x"]','NR slices must use exact cell-centre samples');
  ensure(Array.isArray(raw.frames)&&raw.frames.length>0&&raw.frames.length<=4096&&raw.frame_count===raw.frames.length,'Invalid NR frame inventory');
  for(let i=0;i<raw.frames.length;i++){
    const f=raw.frames[i];ensure(f.index===i&&finite(f.time)&&f.scientific_time===f.time&&f.time_unit==='L/c'&&Number.isInteger(f.cycle)&&f.cycle>=0&&(i===0||f.time>raw.frames[i-1].time),'NR physical times must strictly increase');
    nrRelative(f.data?.path);ensure(hash(f.data?.sha256),'NR numeric frame pin is missing');
  }
  ensure(raw.initial_time===raw.frames[0].time&&raw.final_time===raw.frames.at(-1).time,'NR time endpoints differ');
  const names=gauge?['alpha','gxx','Kxx','hamiltonian']:['chi','lapse','hamiltonian'];
  const units=gauge?['1','1','1/L','1/L^2']:['1','1','1/L^2'];
  const channels={};for(let i=0;i<names.length;i++){const name=names[i];ensure(raw.channels?.[name]?.unit===units[i],'NR channel units differ');channels[name]={...raw.channels[name],label:raw.channels[name].label||name};}
  let ranges=raw.channel_ranges;
  if(gauge){
    ensure(measurements?.schema==='phaseforge.nr-gauge-measurements.v1'&&measurements.series?.length===raw.frames.length,'Gauge measurement series is missing');
    ranges=Object.fromEntries(names.map(name=>[name,{min:Math.min(...measurements.series.map((s,i)=>{ensure(s.time===raw.frames[i].time&&finite(s.channels_3d?.[name]?.min),'Gauge colour-scale measurement differs');return s.channels_3d[name].min;})),max:Math.max(...measurements.series.map(s=>{ensure(finite(s.channels_3d?.[name]?.max),'Gauge colour-scale measurement differs');return s.channels_3d[name].max;}))}]));
  }
  for(const name of names)ensure(finite(ranges?.[name]?.min)&&finite(ranges?.[name]?.max)&&ranges[name].min<=ranges[name].max,'Invalid fixed NR colour scale');
  return {...raw,gauge,channels,ranges,start:raw.initial_time,end:raw.final_time};
}
export function normalizeNRFrame(raw,entry,index){
  let blocks,plane;
  if(index.gauge){
    ensure(raw?.schema==='phaseforge.nr-diagnostic-slice-data.v1'&&JSON.stringify(raw.shape)===JSON.stringify(entry.shape)&&raw.z===entry.plane?.coordinate,'Gauge slice geometry differs');
    const g=entry.source_block?.geometry;ensure(Array.isArray(g)&&g.length===6&&g.every(finite),'Gauge block geometry missing');
    const edges=(lo,hi,n)=>Array.from({length:n+1},(_,i)=>lo+(hi-lo)*i/n);
    blocks=[{logical:entry.source_block.logical,native_logical_level:entry.source_block.logical?.[3],geometry:g,shape:raw.shape,x:raw.x,y:raw.y,z:raw.z,x_edges:edges(g[0],g[1],raw.shape[1]),y_edges:edges(g[2],g[3],raw.shape[0]),channels:raw.channels}];
    plane={requested_coordinate:entry.plane.coordinate,actual_z_min:raw.z,actual_z_max:raw.z,actual_z_varies_by_block:false};
  }else{
    ensure(raw?.schema==='phaseforge.nr-amr-diagnostic-slice.v1'&&raw.time===entry.time&&raw.scientific_time===entry.time&&raw.cycle===entry.cycle&&raw.coordinate_unit==='L'&&raw.time_unit==='L/c','NR retained frame identity differs');
    ensure(raw.interpolation==='none'&&raw.grid_location==='cell_center','NR cell values cannot be resampled');
    ensure(raw.source_metric?.sha256===entry.source_metric?.sha256&&raw.source_constraints?.sha256===entry.source_constraints?.sha256,'NR native frame provenance differs');
    blocks=raw.blocks;plane=raw.plane;
  }
  ensure(Array.isArray(blocks)&&blocks.length>0&&blocks.length<=65536,'Invalid NR block inventory');
  let cells=0;const logical=new Set();
  for(const block of blocks){
    ensure(Array.isArray(block.logical)&&block.logical.length===4&&block.logical.every(Number.isInteger)&&!logical.has(block.logical.join(',')),'Duplicate or invalid AMR block');logical.add(block.logical.join(','));
    ensure(Number.isInteger(block.native_logical_level)&&block.native_logical_level===block.logical[3]&&finite(block.z),'AMR level or z centre differs');
    const [ny,nx]=block.shape||[];ensure(Number.isInteger(nx)&&nx>0&&nx<=4096&&Number.isInteger(ny)&&ny>0&&ny<=4096,'Invalid NR block shape');cells+=nx*ny;ensure(cells<=262144,'NR slice cell bound exceeded');
    for(const [axis,n] of [['x',nx],['y',ny]]){
      const c=block[axis],e=block[`${axis}_edges`];ensure(Array.isArray(c)&&c.length===n&&Array.isArray(e)&&e.length===n+1&&[...c,...e].every(finite),'NR cell coordinates missing');
      ensure(e.every((v,i)=>i===0||v>e[i-1])&&c.every((v,i)=>v>e[i]&&v<e[i+1]),'NR cells do not match their native bounds');
    }
    for(const name of Object.keys(index.channels)){
      const rows=block.channels?.[name];ensure(Array.isArray(rows)&&rows.length===ny&&rows.every(r=>Array.isArray(r)&&r.length===nx&&r.every(finite)),'Nonfinite or malformed NR numerical channel');
    }
  }
  if(!index.gauge)ensure(cells===raw.cell_count&&blocks.length===entry.block_count&&cells===entry.cell_count,'NR block/cell count differs');
  return {time:entry.time,cycle:entry.cycle,blocks,plane,cellCount:cells,reductionMask:raw.reduction_mask||null,
    bounds:{xmin:Math.min(...blocks.map(b=>b.x_edges[0])),xmax:Math.max(...blocks.map(b=>b.x_edges.at(-1))),ymin:Math.min(...blocks.map(b=>b.y_edges[0])),ymax:Math.max(...blocks.map(b=>b.y_edges.at(-1)))}};
}
export function nrCellAt(frame,x,y){
  for(const b of [...frame.blocks].sort((a,b)=>b.native_logical_level-a.native_logical_level)){
    const ix=b.x_edges.findIndex((lo,i)=>i<b.x.length&&x>=lo&&(x<b.x_edges[i+1]||i===b.x.length-1&&x===b.x_edges[i+1]));
    const iy=b.y_edges.findIndex((lo,i)=>i<b.y.length&&y>=lo&&(y<b.y_edges[i+1]||i===b.y.length-1&&y===b.y_edges[i+1]));
    if(ix>=0&&iy>=0)return {logical:b.logical,level:b.native_logical_level,x:b.x[ix],y:b.y[iy],z:b.z,ix,iy,values:Object.fromEntries(Object.entries(b.channels).map(([name,rows])=>[name,rows[iy][ix]]))};
  }return null;
}
export function nrTransform(bounds,width,height,view={}){
  const zoom=finite(view?.zoom)?Math.max(.05,Math.min(100,view.zoom)):1;
  const cx=finite(view?.x)?view.x:(bounds.xmin+bounds.xmax)/2,cy=finite(view?.y)?view.y:(bounds.ymin+bounds.ymax)/2;
  const scale=Math.min((width-72)/(bounds.xmax-bounds.xmin),(height-60)/(bounds.ymax-bounds.ymin))*.93*zoom;
  return {scale,cx,cy,toScreen:(x,y)=>[width/2+(x-cx)*scale,height/2-(y-cy)*scale],toData:(x,y)=>[cx+(x-width/2)/scale,cy-(y-height/2)/scale]};
}
export function nrColor(value,range){
  const t=range.max>range.min?Math.max(0,Math.min(1,(value-range.min)/(range.max-range.min))):.5;
  return `rgb(${Math.round(22+233*t)},${Math.round(35+158*t)},${Math.round(80-25*t)})`;
}
