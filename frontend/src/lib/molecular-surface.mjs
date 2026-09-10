// Coordinate-derived display envelopes. This is not an electron-density map,
// solvent-excluded surface, force field, or molecular-dynamics calculation.
export const DISPLAY_RADII_ANGSTROM={H:1.2,C:1.7,N:1.55,O:1.52,F:1.47,P:1.8,S:1.8,CL:1.75,BR:1.85,I:1.98,FE:1.8,MG:1.73,ZN:1.39,CA:1.94};
// Linear-light vertex colors retain chain contrast under physically based lights.
const CHAIN_COLORS=[[.08,.32,.36],[.37,.17,.07],[.24,.12,.34],[.12,.29,.1],[.36,.27,.07],[.14,.24,.37]];
const validPosition=p=>Array.isArray(p)&&p.length===3&&p.every(n=>typeof n==='number'&&Number.isFinite(n)&&Math.abs(n)<=1e15);
export function prepareMolecularCoordinates(structure) {
  if(!Array.isArray(structure?.atoms)||!structure.atoms.length)throw new Error('No atomic coordinates were supplied.');
  if(structure.atoms.length>50000)throw new Error('The interactive surface supports up to 50,000 atoms. Use a selected molecular assembly or the Blender renderer for a larger structure.');
  const seen=new Set(),atoms=structure.atoms.map((a,i)=>{
    if(!validPosition(a.position))throw new Error(`Atom ${a.id??i} has invalid coordinates.`);
    const id=String(a.id??i);if(seen.has(id))throw new Error('Atom identifiers must be unique.');seen.add(id);
    return {...a,id,element:String(a.element||'C').toUpperCase(),position:[...a.position]};
  });
  const unit=String(structure.coordinate_unit||structure.units||'angstrom').toLowerCase();
  const unitScale=({angstrom:1,'ångström':1,'å':1,angstroms:1,nm:.1,nanometer:.1,nanometers:.1,m:1e-10,meter:1e-10,pm:100})[unit];
  if(!unitScale)throw new Error('Specify molecular coordinate units as angstrom, nm, pm, or m.');
  return {atoms,unitScale,units:unitScale===1?'Å':unit};
}

export function molecularBackboneSegments(structure) {
  const {atoms,unitScale,units}=prepareMolecularCoordinates(structure),chains=new Map();
  for(const atom of atoms){
    if(atom.hetero||!['CA','P'].includes(String(atom.name).trim().toUpperCase()))continue;
    const key=`${atom.chain_id||''}:${String(atom.name).trim().toUpperCase()}`;
    if(!chains.has(key))chains.set(key,[]);chains.get(key).push(atom);
  }
  const segments=[];
  for(const chain of chains.values()){
    let segment=[];
    for(const atom of chain){
      const previous=segment.at(-1),distance=previous?Math.hypot(...atom.position.map((v,i)=>v-previous.position[i])):0;
      if(previous&&(distance>(atom.name.trim()==='P'?8:5)*unitScale||distance<1e-5*unitScale)){
        if(segment.length>1)segments.push(segment);segment=[];
      }
      segment.push(atom);
    }
    if(segment.length>1)segments.push(segment);
  }
  return {segments,atoms,unitScale,units};
}

export function createMolecularBackbone(THREE,structure) {
  const {segments,atoms,unitScale,units}=molecularBackboneSegments(structure),root=new THREE.Group();
  if(!segments.length)throw new Error('This structure has no connected protein Cα or nucleic-acid P backbone coordinates. Choose Molecular surface or Atomic volumes.');
  segments.forEach((segment,index)=>{
    const curve=new THREE.CatmullRomCurve3(segment.map(a=>new THREE.Vector3(...a.position))),steps=Math.min(4000,segment.length*6),frames=curve.computeFrenetFrames(steps,false),positions=[],indices=[];
    for(let i=0;i<=steps;i++){
      const point=curve.getPointAt(i/steps),width=unitScale*.85;
      for(const sign of [-1,1])positions.push(...point.clone().addScaledVector(frames.normals[i],sign*width).toArray());
      if(i<steps){const v=i*2;indices.push(v,v+1,v+2,v+1,v+3,v+2);}
    }
    const geometry=new THREE.BufferGeometry();geometry.setAttribute('position',new THREE.Float32BufferAttribute(positions,3));geometry.setIndex(indices);geometry.computeVertexNormals();
    const color=new THREE.Color().fromArray(CHAIN_COLORS[index%CHAIN_COLORS.length]),mesh=new THREE.Mesh(geometry,new THREE.MeshPhysicalMaterial({color,side:THREE.DoubleSide,roughness:.5,metalness:0,clearcoat:.15}));mesh.name=`Backbone · chain ${segment[0].chain_id||'unnamed'}`;root.add(mesh);
  });
  return {root,atoms,meta:{atomCount:atoms.length,units,unitScale,description:'Ribbon follows supplied Cα or phosphate coordinates, interpolated between adjacent backbone sites. Missing chain segments remain disconnected. Ribbon width is a display choice; no secondary structure is inferred.'}};
}

export async function molecularDensity(structure,{resolution=104,signal,onProgress,yieldControl=async()=>{}}={}) {
  const {atoms,unitScale,units}=prepareMolecularCoordinates(structure);
  const size=Math.max(32,Math.min(144,Math.floor(resolution))),size2=size*size,size3=size**3;
  const min=[Infinity,Infinity,Infinity],max=[-Infinity,-Infinity,-Infinity];let largest=0;
  const prepared=atoms.map(atom=>{
    const radius=((DISPLAY_RADII_ANGSTROM[atom.element]||1.7)+.25)*unitScale;largest=Math.max(largest,radius);
    atom.position.forEach((v,i)=>{min[i]=Math.min(min[i],v);max[i]=Math.max(max[i],v);});
    let hash=0;for(const c of String(atom.chain_id||'A'))hash=(hash*31+c.charCodeAt(0))>>>0;
    return {atom,radius,color:CHAIN_COLORS[hash%CHAIN_COLORS.length]};
  });
  const center=min.map((v,i)=>(v+max[i])/2),span=Math.max(...max.map((v,i)=>v-min[i]))+largest*7,origin=center.map(v=>v-span/2),spacing=span/size;
  const field=new Float32Array(size3),palette=new Float32Array(size3*3);let operations=0;
  for(let a=0;a<prepared.length;a++){
    if(signal?.aborted)throw new DOMException('Cancelled','AbortError');
    const {atom,radius,color}=prepared[a],point=atom.position.map((v,i)=>(v-origin[i])/spacing),range=radius*3/spacing,denominator=(radius/spacing)**2/Math.log(2);
    const lo=point.map(v=>Math.max(1,Math.floor(v-range))),hi=point.map(v=>Math.min(size-2,Math.ceil(v+range)));
    for(let z=lo[2];z<=hi[2];z++){const dz2=(z-point[2])**2;for(let y=lo[1];y<=hi[1];y++){const yz2=dz2+(y-point[1])**2,offset=z*size2+y*size;for(let x=lo[0];x<=hi[0];x++){
      if(++operations>100_000_000)throw new Error('Surface sampling exceeded the interactive work budget. Choose lower surface detail or use a Blender render.');
      const d2=yz2+(x-point[0])**2;if(d2>range*range)continue;const weight=Math.exp(-d2/denominator),index=offset+x;
      // Union of atom-radius envelopes preserves molecular crevices. Summing
      // all overlapping Gaussians inflates dense proteins into smooth blobs.
      if(weight>field[index]){field[index]=weight;const c=index*3;palette[c]=color[0];palette[c+1]=color[1];palette[c+2]=color[2];}
    }}}
    if(a%128===0){onProgress?.((a+1)/atoms.length*.85);await yieldControl();}
  }
  onProgress?.(.9);
  return {field,palette,resolution:size,center,span,spacing,atoms,units,unitScale,atomCount:atoms.length,operations,isolation:.5,
    description:'Interpolated union of atomic display envelopes generated from supplied coordinates and element display radii. Surface shape is a visualization approximation, not measured electron density or a solvent-excluded surface.'};
}

export async function createMolecularSurface(THREE,structure,options={}) {
  const [{MarchingCubes},data]=await Promise.all([import('three/examples/jsm/objects/MarchingCubes.js'),molecularDensity(structure,options)]);
  if(options.signal?.aborted)throw new DOMException('Cancelled','AbortError');
  const material=new THREE.MeshPhysicalMaterial({color:'#ffffff',vertexColors:true,roughness:.56,metalness:0,clearcoat:.13,clearcoatRoughness:.55});
  const marching=new MarchingCubes(data.resolution,material,false,true,300000);
  try{
    marching.field.set(data.field);marching.palette.set(data.palette);marching.isolation=data.isolation;marching.update();
    if(!marching.count||marching.count>900000)throw new Error('The requested surface exceeds its mesh budget. Reduce surface detail or render the structure in Blender.');
    // Keep only populated triangles. Release marching-cubes scratch fields after
    // generation rather than retaining large empty GPU and CPU allocations.
    const geometry=new THREE.BufferGeometry();
    for(const name of ['position','normal','color'])geometry.setAttribute(name,new THREE.BufferAttribute(marching.geometry.attributes[name].array.slice(0,marching.count*3),3));
    geometry.computeBoundingBox();geometry.computeBoundingSphere();
    const mesh=new THREE.Mesh(geometry,material);mesh.scale.setScalar(data.span/2);mesh.position.set(...data.center);mesh.name=structure.name||'Molecular surface';mesh.userData={molecularSurface:true,structureId:structure.id||null,atomCount:data.atomCount,units:data.units,provenance:data.description};
    options.onProgress?.(1);return {root:mesh,atoms:data.atoms,meta:{...data,field:undefined,palette:undefined,atoms:undefined}};
  }catch(error){material.dispose();throw error;}finally{marching.geometry.dispose();}
}
