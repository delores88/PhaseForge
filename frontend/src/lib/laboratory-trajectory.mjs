/** Authoritative trajectory records stay on disk. Only a bounded display cache lives here. */
export function normalizeTrajectoryIndex(value) {
  const chunks=(value.chunks||[]).map((c,i)=>({
    ...c,path:c.path||c.file||c.filename,
    start:Number(c.start_time??c.start??c.first_time),end:Number(c.end_time??c.end??c.last_time),
    count:Number(c.frame_count??c.frames??c.count??0),number:i,
  }));
  if(chunks.some(c=>!c.path||c.path.includes('..')||!Number.isFinite(c.start)||!Number.isFinite(c.end)||c.end<c.start))throw new Error('Trajectory index contains an invalid chunk.');
  for(let i=1;i<chunks.length;i++)if(chunks[i].start<chunks[i-1].start)throw new Error('Trajectory chunks are not time ordered.');
  return {...value,chunks,start:Number(value.start_time??chunks[0]?.start??0),end:Number(value.end_time??chunks.at(-1)?.end??0),
    time_unit:value.time_unit||value.units?.time||'ps',frame_count:Number(value.frame_count??chunks.reduce((n,c)=>n+c.count,0))};
}

export function validateFrames(value) {
  const frames=Array.isArray(value)?value:value.frames;
  if(!Array.isArray(frames)||!frames.length)throw new Error('A trajectory chunk has no recorded frames.');
  for(let i=0;i<frames.length;i++){
    const f=frames[i];
    if(!Number.isFinite(f.time)||(i&&f.time<=frames[i-1].time)||!Array.isArray(f.entities))throw new Error('Trajectory frames contain invalid or unordered times.');
    const ids=new Set();
    for(const e of f.entities){if(e.id==null||ids.has(String(e.id))||!Array.isArray(e.position)||e.position.length!==3||!e.position.every(Number.isFinite))throw new Error('Trajectory contains invalid entity coordinates.');ids.add(String(e.id));}
  }
  return frames;
}

export function bracketFrames(frames,time) {
  let lo=0,hi=frames.length-1;
  while(lo<hi){const m=Math.ceil((lo+hi)/2);if(frames[m].time<=time)lo=m;else hi=m-1;}
  const left=frames[lo],right=frames[Math.min(lo+1,frames.length-1)];
  return {left,right,index:lo,alpha:right.time>left.time?Math.max(0,Math.min(1,(time-left.time)/(right.time-left.time))):0};
}

/** Minimum-image interpolation is display-only and requires declared periodic, wrapped coordinates. */
export function interpolateRecordedPosition(a,b,alpha,box,periodic=false) {
  if(!b||alpha===0)return [...a];
  return a.map((v,i)=>{let delta=b[i]-v;const length=box?.[i];if(periodic&&length>0)delta-=Math.round(delta/length)*length;const position=v+delta*alpha;return periodic&&length>0?((position%length)+length)%length:position;});
}

export function playbackTimeStep(elapsedSeconds,start,end,durationSeconds,speed=1) {
  if(![elapsedSeconds,start,end,durationSeconds,speed].every(Number.isFinite)||durationSeconds<=0||end<start)return 0;
  return Math.max(0,elapsedSeconds)*(end-start)/durationSeconds*Math.max(0,speed);
}

export class TrajectoryStore {
  constructor({index,loadJson,loadBytes,maxBytes=32*1024*1024,maxChunks=4,onCache}){
    this.index=normalizeTrajectoryIndex(index);this.loadJson=loadJson;this.loadBytes=loadBytes;this.maxBytes=maxBytes;this.maxChunks=maxChunks;this.onCache=onCache;
    this.cache=new Map();this.pending=new Map();this.bytes=0;this.closed=false;
  }
  updateIndex(index){this.index=normalizeTrajectoryIndex(index);}
  async chunk(number){
    if(this.closed)throw new DOMException('Viewer closed','AbortError');
    if(this.cache.has(number)){const cached=this.cache.get(number);this.cache.delete(number);this.cache.set(number,cached);return cached.frames;}
    if(this.pending.has(number))return this.pending.get(number);
    const description=this.index.chunks[number];if(!description)return [];
    const pending=(async()=>{
      let value;
      if(this.loadBytes){
        if(!/^[0-9a-f]{64}$/i.test(description.sha256||''))throw Error('A committed trajectory digest is required.');
        const raw=await this.loadBytes(description.path);
        if(raw.byteLength>64*1024*1024)throw Error('A trajectory chunk exceeds the 64 MiB display limit.');
        const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',raw)),v=>v.toString(16).padStart(2,'0')).join('');
        if(digest!==description.sha256.toLowerCase())throw Error('Recorded trajectory bytes do not match their scientific digest.');
        value=JSON.parse(new TextDecoder().decode(raw));
      }else value=await this.loadJson(description.path);
      if(this.closed)throw new DOMException('Viewer closed','AbortError');
      const frames=validateFrames(value),bytes=JSON.stringify(value).length*2;
      // Refuse a malformed monolithic artifact instead of silently dropping numerical states.
      if(bytes>64*1024*1024)throw new Error('This trajectory chunk exceeds 64 MiB. Rechunk the saved trajectory for interactive inspection.');
      this.cache.set(number,{frames,bytes});this.bytes+=bytes;
      while(this.cache.size>1&&(this.cache.size>this.maxChunks||this.bytes>this.maxBytes)){const first=this.cache.keys().next().value;this.bytes-=this.cache.get(first).bytes;this.cache.delete(first);}
      this.onCache?.({chunks:this.cache.size,bytes:this.bytes,totalChunks:this.index.chunks.length});return frames;
    })();
    this.pending.set(number,pending);try{return await pending;}finally{this.pending.delete(number);}
  }
  async sample(time){
    const chunks=this.index.chunks;if(!chunks.length)throw new Error('No trajectory frames have been recorded yet.');
    const t=Math.min(this.index.end,Math.max(this.index.start,time));let lo=0,hi=chunks.length-1;
    while(lo<hi){const m=Math.ceil((lo+hi)/2);if(chunks[m].start<=t)lo=m;else hi=m-1;}
    const frames=await this.chunk(lo),sample=bracketFrames(frames,t);
    if(sample.left===frames.at(-1)&&t>sample.left.time&&lo+1<chunks.length){const next=await this.chunk(lo+1);sample.right=next[0];sample.alpha=Math.max(0,Math.min(1,(t-sample.left.time)/(sample.right.time-sample.left.time)));}
    return {...sample,time:t,chunk:lo,frameIndex:chunks.slice(0,lo).reduce((n,c)=>n+c.count,0)+sample.index};
  }
  dispose(){this.closed=true;this.cache.clear();this.pending.clear();this.bytes=0;}
}
