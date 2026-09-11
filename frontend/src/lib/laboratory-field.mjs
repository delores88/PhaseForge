const finite=Number.isFinite;
const safePath=path=>typeof path==='string'&&/^fields\/[a-zA-Z0-9_.-]+\.json$/.test(path);
export function normalizeFieldIndex(value){
  const [ny,nx]=value.shape||[],[lx,ly]=value.lengths_um||[],frames=value.frames;
  if(value.representation!=='scalar_field'||!Number.isInteger(nx)||!Number.isInteger(ny)||nx<2||ny<2||nx*ny>1048576||!(lx>0)||!(ly>0)||value.axis_order?.join(',')!=='y,x'||value.grid_location!=='cell_center')throw Error('Unsupported or invalid recorded field grid.');
  if(!Array.isArray(frames)||!frames.length||frames.some((f,i)=>!finite(f.time)||!safePath(f.view_path)||!Number.isInteger(f.step)||(i>0&&f.time<=frames[i-1].time)))throw Error('Field index must contain ordered, recorded scalar states.');
  if(!finite(value.color_scale?.min)||!finite(value.color_scale?.max)||value.color_scale.max<value.color_scale.min)throw Error('A fixed numerical color range is required.');
  if(value.x_um?.length!==nx||value.y_um?.length!==ny||![...value.x_um,...value.y_um].every(finite))throw Error('Field cell coordinates are missing.');
  return {...value,nx,ny,lx,ly,start:frames[0].time,end:frames.at(-1).time,frame_count:frames.length};
}
export function recordedFieldNumber(index,time){let left=0,right=index.frames.length-1;while(left<right){const mid=Math.ceil((left+right)/2);if(index.frames[mid].time<=time)left=mid;else right=mid-1;}return left;}
export function validateFieldFrame(value,index,record){
  if(value.time!==record.time||value.step!==record.step||value.shape?.join(',')!==index.shape.join(',')||value.field_unit!==index.field_unit||!Array.isArray(value.values)||value.values.length!==index.ny||value.values.some(row=>!Array.isArray(row)||row.length!==index.nx||!row.every(finite)))throw Error('Recorded field values disagree with their index.');
  return {...value,source_sha256:record.sha256,view_sha256:record.view_sha256,source_path:record.path};
}
function hex(value,fallback){return /^#[0-9a-f]{6}$/i.test(value||'')?[1,3,5].map(i=>parseInt(value.slice(i,i+2),16)):fallback;}
export function fieldColor(value,scale,presentation={}){
  const a=hex(presentation.colorLow,scale.stops?.[0]?.[1]||[20,38,80]),b=hex(presentation.colorHigh,scale.stops?.at(-1)?.[1]||[255,190,60]);
  const t=scale.max===scale.min ? .5 : Math.max(0,Math.min(1,(value-scale.min)/(scale.max-scale.min))),contrast=Math.max(.5,Math.min(2,Number(presentation.contrast)||1));
  return a.map((v,i)=>Math.round(Math.max(0,Math.min(255,((v+(b[i]-v)*t)/255-.5)*contrast*255+127.5))));
}
export function fieldCell(index,u,v){const x=Math.min(index.nx-1,Math.max(0,Math.floor(u*index.nx))),y=Math.min(index.ny-1,Math.max(0,Math.floor(v*index.ny)));return {x,y,position:[index.x_um[x],index.y_um[y]]};}
export class FieldStore{
  constructor({index,loadBytes,maxFrames=8,maxBytes=32*1024*1024}){this.index=normalizeFieldIndex(index);this.loadBytes=loadBytes;this.maxFrames=maxFrames;this.maxBytes=maxBytes;this.cache=new Map();this.pending=new Map();this.bytes=0;this.closed=false;this.loadChain=Promise.resolve();this.wanted=null;}
  updateIndex(index){const next=normalizeFieldIndex(index);if(next.shape.join(',')!==this.index.shape.join(',')||JSON.stringify(next.color_scale)!==JSON.stringify(this.index.color_scale)||next.lengths_um.join(',')!==this.index.lengths_um.join(','))throw Error('The committed field grid or fixed color scale changed.');this.index=next;}
  async frame(number,{latestOnly=false}={}){
    if(this.closed)throw new DOMException('Viewer closed','AbortError');
    const record=this.index.frames[number];if(!record)throw Error('Recorded field does not exist.');if(latestOnly)this.wanted=record.view_path;
    if(this.cache.has(record.view_path)){const cached=this.cache.get(record.view_path);this.cache.delete(record.view_path);this.cache.set(record.view_path,cached);return cached.value;}
    if(this.pending.has(record.view_path))return this.pending.get(record.view_path);
    const promise=this.loadChain.catch(()=>{}).then(async()=>{if(this.closed||(latestOnly&&this.wanted!==record.view_path))throw new DOMException('Superseded field request','AbortError');const bytes=await this.loadBytes(record.view_path);if(this.closed)throw new DOMException('Viewer closed','AbortError');if(bytes.byteLength>64*1024*1024)throw Error('A field display artifact exceeds 64 MiB.');
      if(record.view_sha256){const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');if(digest!==record.view_sha256)throw Error('Field view checksum does not match the committed record.');}
      const value=validateFieldFrame(JSON.parse(new TextDecoder().decode(bytes)),this.index,record),size=this.index.nx*this.index.ny*8;
      this.cache.set(record.view_path,{value,size});this.bytes+=size;
      while(this.cache.size>1&&(this.cache.size>this.maxFrames||this.bytes>this.maxBytes)){const key=this.cache.keys().next().value;this.bytes-=this.cache.get(key).size;this.cache.delete(key);}return value;
    }).finally(()=>this.pending.delete(record.view_path));this.loadChain=promise;this.pending.set(record.view_path,promise);return promise;
  }
  close(){this.closed=true;this.cache.clear();this.pending.clear();this.bytes=0;}
}
