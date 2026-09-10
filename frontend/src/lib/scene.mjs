/** Data-only scientific scene format. Never evaluates expressions, scripts or URLs. */
export const SCENE_VERSION = '1.0';
export const SCENE_TYPES = ['atom','molecule','dna','virus','protein','planet','star','black_hole','field','streamlines','mesh','curve','surface','box','sphere'];
export const SCENE_LIMITS = Object.freeze({nodes:64, vertices:24000, points:12000, atoms:12000, bonds:20000});
export const ELEMENT_COLORS = {H:'#dce5ef',C:'#87a8bc',N:'#6188ff',O:'#f26d86',S:'#edc36e',P:'#eeb377',F:'#80c99c',CL:'#80c99c',FE:'#dca372',MG:'#8ac7a0'};
export const PALETTE = ['#63d5cf','#89abff','#c398f4','#f2ac82','#e4cd8a','#86cbb0','#ee95b1','#94c8e7'];
const number = (v,fallback=0,min=-1e15,max=1e15) => (typeof v==='number' && Number.isFinite(v)) ? Math.min(max,Math.max(min,v)) : fallback;
const vector = (v,fallback=[0,0,0],min=-1e15,max=1e15) => Array.isArray(v)&&v.length===3&&v.every(n=>typeof n==='number'&&Number.isFinite(n)) ? v.map(n=>number(n,0,min,max)) : [...fallback];
const text = (v,fallback='',max=400) => typeof v==='string'?v.slice(0,max):fallback;
const color = (v,fallback='#63d5cf') => typeof v==='string'&&/^#[\da-f]{6}$/i.test(v)?v:fallback;
const validPoint = p=>Array.isArray(p)&&p.length===3&&p.every(n=>typeof n==='number'&&Number.isFinite(n)&&Math.abs(n)<=1e15);

function molecularDisplayRadius(atoms=[],bonds=[]) {
  const distance=(a,b)=>Math.hypot(...a.position.map((v,i)=>v-b.position[i]));
  const byId=new Map(atoms.map(a=>[a.id,a])),distances=[];
  for(const bond of bonds){const a=byId.get(bond.from),b=byId.get(bond.to);if(a&&b){const d=distance(a,b);if(d>0&&Number.isFinite(d))distances.push(d);}}
  if(!distances.length){
    // Coordinate-only imports need scale-aware glyphs too. Bound the nearest-
    // neighbor sample so choosing a display radius cannot become an all-pairs job.
    const sample=atoms.slice(0,512);
    for(const a of sample.slice(0,128)){let nearest=Infinity;for(const b of sample){if(a===b)continue;const d=distance(a,b);if(d>0)nearest=Math.min(nearest,d);}if(Number.isFinite(nearest))distances.push(nearest);}
  }
  distances.sort((a,b)=>a-b);
  return distances.length?Math.max(1e-15,Math.min(1e12,distances[Math.floor(distances.length/2)]*.24)):.29;
}

export function normalizeScene(input) {
  if(!input || typeof input!=='object' || !Array.isArray(input.nodes)) return null;
  const warnings=[]; const seen=new Set(); const budget={...SCENE_LIMITS};
  const warn=s=>{if(warnings.length<16&&!warnings.includes(s))warnings.push(s);};
  if(input.nodes.length>SCENE_LIMITS.nodes)warn(`Display limited to ${SCENE_LIMITS.nodes} scene nodes.`);
  if(input.schema_version && input.schema_version!==SCENE_VERSION)warn('Unrecognized scene version; supported geometry fields only are displayed.');
  const nodes=[];
  for(const [i,source] of input.nodes.slice(0,SCENE_LIMITS.nodes).entries()) {
    if(!source || !SCENE_TYPES.includes(source.type)){warn('Unsupported scene geometry was omitted.');continue;}
    const id=text(source.id,`node-${i}`,100);if(seen.has(id)){warn('Duplicate node identifiers were omitted.');continue;}seen.add(id);
    const raw=source.parameters&&typeof source.parameters==='object'?source.parameters:{};
    const parameters={};
    for(const [key,fallback,min,max] of [['radius',1,1e-15,1e12],['length',5,1e-15,1e12],['thickness',.08,1e-15,1e9],['turns',3,.1,30],['count',64,4,512],['amplitude',.35,0,100],['frequency',3,.01,100],['seed',1,0,2147483647],['opacity',1,.08,1]]) parameters[key]=number(raw[key],fallback,min,max);
    parameters.rings=raw.rings===true;parameters.cutaway=raw.cutaway===true;
    parameters.representation=['ribbon','surface','ball_and_stick','space_filling'].includes(raw.representation)?raw.representation:'ribbon';
    parameters.shape=['wave','torus','mobius','ellipsoid'].includes(raw.shape)?raw.shape:'wave';
    parameters.element=text(raw.element,'C',3).toUpperCase();
    parameters.colors=(Array.isArray(raw.colors)?raw.colors:[]).slice(0,8).map(c=>color(c));
    for(const key of ['points','vertices']) {
      if(!Array.isArray(raw[key]))continue;
      const allowance=Math.min(key==='vertices'?12000:2048,budget[key]);
      parameters[key]=raw[key].slice(0,allowance).filter(validPoint).map(p=>[...p]);budget[key]-=parameters[key].length;
      if(raw[key].length>parameters[key].length)warn('Invalid or over-budget geometry coordinates were omitted.');
    }
    if(source.type==='mesh'&&Array.isArray(raw.vertices)&&raw.vertices.slice(0,12000).some(p=>!validPoint(p))){warn('A mesh with invalid vertex coordinates was omitted to preserve its topology.');continue;}
    if(Array.isArray(raw.indices)&&parameters.vertices){
      parameters.indices=[];
      // Reject a malformed triangle as a unit. Filtering individual indices would
      // silently connect unrelated vertices into a different surface.
      for(let k=0;k+2<Math.min(raw.indices.length,72000,parameters.vertices.length*12);k+=3){
        const triangle=raw.indices.slice(k,k+3);
        if(triangle.every(n=>Number.isInteger(n)&&n>=0&&n<parameters.vertices.length))parameters.indices.push(...triangle);
        else warn('Invalid mesh triangles were omitted.');
      }
    }
    if(Array.isArray(raw.atoms)) {
      const ids=new Set();parameters.atoms=[];
      for(const [a,atom] of raw.atoms.slice(0,Math.min(4000,budget.atoms)).entries()) {
        if(!atom||!validPoint(atom.position))continue;
        const atomId=text(String(atom.id??a),String(a),100);if(ids.has(atomId))continue;ids.add(atomId);
        parameters.atoms.push({id:atomId,element:text(atom.element,'C',3).toUpperCase(),position:[...atom.position]});
      }
      budget.atoms-=parameters.atoms.length;
      if(raw.atoms.length>parameters.atoms.length)warn('Invalid or over-budget atom records were omitted.');
      parameters.bonds=(Array.isArray(raw.bonds)?raw.bonds:[]).slice(0,budget.bonds).filter(b=>b&&ids.has(String(b.from))&&ids.has(String(b.to))&&String(b.from)!==String(b.to)).map(b=>({from:String(b.from),to:String(b.to),order:Math.round(number(b.order,1,1,3))}));budget.bonds-=parameters.bonds.length;
    }
    if(source.type==='molecule'&&!(typeof raw.radius==='number'&&Number.isFinite(raw.radius)))parameters.radius=molecularDisplayRadius(parameters.atoms,parameters.bonds);
    if(!(typeof raw.thickness==='number'&&Number.isFinite(raw.thickness)))parameters.thickness=parameters.radius*(source.type==='molecule'?.24:.08);
    if(!(typeof raw.length==='number'&&Number.isFinite(raw.length)))parameters.length=parameters.radius*5;
    if(source.type==='mesh'&&(!parameters.vertices?.length||!parameters.indices?.length)){warn('A mesh without valid indexed triangles was omitted.');continue;}
    nodes.push({id,type:source.type,label:text(source.label,id,160),description:text(source.description,'',1200),entity_id:text(source.entity_id,'',100),position:vector(source.position),rotation:vector(source.rotation),scale:typeof source.scale==='number'?Array(3).fill(number(source.scale,1,1e-8,1e8)):vector(source.scale,[1,1,1],1e-8,1e8),color:color(source.color,PALETTE[i%PALETTE.length]),parameters});
  }
  const byId=new Set(nodes.map(n=>n.id));
  const bonds=(Array.isArray(input.bonds)?input.bonds:[]).slice(0,Math.min(5000,budget.bonds)).filter(b=>b&&byId.has(String(b.from))&&byId.has(String(b.to))&&String(b.from)!==String(b.to)).map(b=>({from:String(b.from),to:String(b.to),order:Math.round(number(b.order,1,1,3))}));
  const provenance={kind:['conceptual','computed','measured'].includes(input.provenance?.kind)?input.provenance.kind:'conceptual',description:text(input.provenance?.description,'Authored visual geometry. Its shape is not independent simulation evidence.',2000)};
  return {schema_version:SCENE_VERSION,title:text(input.title,'Scientific scene',200),units:text(input.units,'model units',80),provenance,nodes,bonds,camera:input.camera?{position:vector(input.camera.position,[8,5,10]),target:vector(input.camera.target)}:null,warnings};
}

export function sceneSourceFromEvidence(run,manifest) {
  const saved=run?.result?.visualization?.scene;
  if(saved)return saved;
  // Never relabel an old run using an unrelated current revision.
  if(run && run.manifest_id!==manifest?.id)return null;
  return manifest?.visualization?.scene||manifest?.scene||null;
}

export function sceneFromEvidence(run,manifest) {
  return normalizeScene(sceneSourceFromEvidence(run,manifest));
}

export function fibonacciSphere(count,radius=1) {
  return Array.from({length:Math.max(0,Math.floor(count))},(_,i)=>{const y=1-2*(i+.5)/count,r=Math.sqrt(Math.max(0,1-y*y)),angle=i*Math.PI*(3-Math.sqrt(5));return [radius*r*Math.cos(angle),radius*y,radius*r*Math.sin(angle)];});
}

export function helixPoints({turns=3,length=5,radius=1,phase=0,count=160}={}) {
  return Array.from({length:Math.max(2,Math.floor(count))},(_,i)=>{const t=i/(Math.max(2,Math.floor(count))-1),a=t*turns*2*Math.PI+phase;return [radius*Math.cos(a),(t-.5)*length,radius*Math.sin(a)];});
}

/** Analytic potential flow around a unit sphere. These streamlines are illustrative,
 * steady, inviscid and incompressible; there is no viscosity or turbulence solver. */
export function potentialFlowVelocity([x,y,z],radius=1) {
  const r=Math.hypot(x,y,z);if(r<radius*1.001)return [0,0,0];
  const a3=radius**3,r3=r**3,r5=r**5;
  return [1+a3/(2*r3)-3*a3*x*x/(2*r5),-3*a3*x*y/(2*r5),-3*a3*x*z/(2*r5)];
}

export function potentialStreamlines(count=28,radius=1) {
  return Array.from({length:Math.min(96,Math.max(4,Math.floor(count)))},(_,i)=>{
    const angle=i*2.39996323,offset=radius*(.25+2.2*Math.sqrt((i+.5)/count));let p=[-4*radius,offset*Math.cos(angle),offset*Math.sin(angle)];const points=[];
    const dt=.075*radius;
    for(let j=0;j<150;j++){points.push([...p]);const k1=potentialFlowVelocity(p,radius),k2=potentialFlowVelocity(p.map((v,k)=>v+k1[k]*dt/2),radius),k3=potentialFlowVelocity(p.map((v,k)=>v+k2[k]*dt/2),radius),k4=potentialFlowVelocity(p.map((v,k)=>v+k3[k]*dt),radius);p=p.map((v,k)=>v+dt*(k1[k]+2*k2[k]+2*k3[k]+k4[k])/6);if(p[0]>radius*4)break;}
    return points;
  });
}

export const SCENE_PRESETS = [
  {id:'molecular',name:'Molecular systems',detail:'Membrane, surface proteins & scaffold'},
  {id:'dna',name:'Structural biology',detail:'Double helix & protein ribbon'},
  {id:'flow',name:'Flow & fields',detail:'Analytic streamlines around a body'},
  {id:'cosmos',name:'Orbital systems',detail:'Planetary surfaces & accretion disk'},
];

export function exampleScene(preset='molecular') {
  const base={schema_version:SCENE_VERSION,units:'illustrative units',provenance:{kind:'conceptual',description:'Procedural scene study. Geometry is illustrative and is not a retained solver result, an experimentally determined structure, or evidence of efficacy.'}};
  const node=(id,type,position,color,parameters={},scale=1)=>({id,type,label:id.replaceAll('_',' '),position,color,parameters,scale});
  if(preset==='flow')return normalizeScene({...base,title:'Potential flow · sphere',provenance:{kind:'conceptual',description:'Streamlines integrated from the analytic steady potential-flow field around a sphere. Inviscid and incompressible; this preview does not model viscosity, wakes or turbulence.'},nodes:[node('obstacle','sphere',[0,0,0],'#8995af',{radius:1}),node('velocity_field','streamlines',[0,0,0],'#6ce1cc',{radius:1,count:42,thickness:.012})]});
  if(preset==='dna')return normalizeScene({...base,title:'Helical architecture',nodes:[node('double_helix','dna',[-2.2,0,0],'#7cdad1',{turns:3.5,length:8,radius:.92}),node('protein_fold','protein',[2.5,-.6,0],'#bd9af5',{radius:1.15,count:280,representation:'ribbon'},1.15)]});
  if(preset==='cosmos')return normalizeScene({...base,title:'Worlds across scales',nodes:[node('ringed_planet','planet',[-2.2,.4,0],'#d1a783',{radius:1.6,rings:true,seed:12}),node('accretion_disk','black_hole',[3.2,-.5,-1],'#f8bf80',{radius:.65},1.2),node('satellite','planet',[-.5,2.7,-1.5],'#94bacf',{radius:.38,seed:7})]});
  return normalizeScene({...base,title:'Molecular architecture',nodes:[node('viral_envelope','virus',[-1.8,0,0],'#79b3d4',{radius:1.65,count:88,cutaway:true}),node('molecular_scaffold','molecule',[2.3,.3,0],'#77d9c3',{representation:'ball_and_stick'},.8),node('protein_ribbon','protein',[2.6,-2.2,-1.5],'#c6a0ea',{radius:.5},.6)]});
}
