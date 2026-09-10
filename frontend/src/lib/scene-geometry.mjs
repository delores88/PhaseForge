import {ELEMENT_COLORS,PALETTE,fibonacciSphere,helixPoints,potentialStreamlines} from './scene.mjs';

/** Procedural Three.js geometry, with no arbitrary code execution or asset fetches. */
export function buildSceneGraph(THREE,spec,{quality='balanced'}={}) {
  const root=new THREE.Group(),objects=new Map(),pickables=[],resources={geometries:new Set(),materials:new Set()};
  const detail=quality==='high'?1:quality==='low'?.45:.7;
  // Limit aggregate procedural tessellation independently from JSON payload limits.
  const complexity=Math.max(.25,Math.min(1,12/Math.max(1,spec.nodes.length)))*detail;
  const geo=g=>(resources.geometries.add(g),g);
  const material=(color,options={})=>{const m=new THREE.MeshStandardMaterial({color,roughness:.48,metalness:.12,...options});resources.materials.add(m);return m;};
  const reducedDetail=quality==='low'||spec.nodes.length>12;
  const sphere=geo(new THREE.SphereGeometry(1,reducedDetail?10:20,reducedDetail?7:14));
  const cylinder=geo(new THREE.CylinderGeometry(1,1,1,quality==='low'?6:10));
  const matrix=new THREE.Matrix4(),quaternion=new THREE.Quaternion(),up=new THREE.Vector3(0,1,0);
  const vec=p=>new THREE.Vector3(...p);
  function mesh(group,geometry,mat,position,scale) {
    const m=new THREE.Mesh(geometry,mat);if(position)m.position.copy(vec(position));if(scale)m.scale.set(...(Array.isArray(scale)?scale:[scale,scale,scale]));group.add(m);return m;
  }
  function spheres(group,points,radii,colors,options={}) {
    if(!points.length)return;
    const m=new THREE.InstancedMesh(sphere,material('#ffffff',options),points.length),c=new THREE.Color();
    points.forEach((p,i)=>{const r=Array.isArray(radii)?radii[i]:radii;matrix.makeScale(r,r,r);matrix.setPosition(...p);m.setMatrixAt(i,matrix);m.setColorAt(i,c.set(Array.isArray(colors)?colors[i%colors.length]:colors));});
    m.instanceMatrix.needsUpdate=true;if(m.instanceColor)m.instanceColor.needsUpdate=true;group.add(m);return m;
  }
  function rods(group,segments,radius,colors) {
    if(!segments.length)return;
    const m=new THREE.InstancedMesh(cylinder,material('#ffffff'),segments.length),c=new THREE.Color();
    segments.forEach(([a,b],i)=>{const av=vec(a),bv=vec(b),direction=bv.clone().sub(av),length=direction.length(),r=Array.isArray(radius)?radius[i]:radius;quaternion.setFromUnitVectors(up,direction.normalize());matrix.compose(av.add(bv).multiplyScalar(.5),quaternion,new THREE.Vector3(r,length,r));m.setMatrixAt(i,matrix);m.setColorAt(i,c.set(Array.isArray(colors)?colors[i%colors.length]:colors));});
    m.instanceMatrix.needsUpdate=true;if(m.instanceColor)m.instanceColor.needsUpdate=true;group.add(m);return m;
  }
  function tube(group,points,radius,color,options={}) {
    if(points.length<2)return null;
    const curve=new THREE.CatmullRomCurve3(points.map(vec));
    const geometry=geo(new THREE.TubeGeometry(curve,Math.min(300,Math.max(12,Math.ceil(points.length*complexity))),radius,quality==='low'?5:8,false));
    return mesh(group,geometry,material(color,options));
  }
  function surface(group,p,color) {
    const uCount=Math.ceil(64*complexity),vCount=Math.ceil(40*complexity),vertices=[],indices=[],colors=[],c=new THREE.Color(color);
    for(let i=0;i<=uCount;i++)for(let j=0;j<=vCount;j++) {
      const u=i/uCount,v=j/vCount,a=u*Math.PI*2,b=v*Math.PI*2;let x,y,z;
      if(p.shape==='torus'){x=(1+.36*Math.cos(b))*Math.cos(a);y=.36*Math.sin(b);z=(1+.36*Math.cos(b))*Math.sin(a);}
      else if(p.shape==='mobius'){const w=(v-.5)*.9;x=(1+w*Math.cos(a/2))*Math.cos(a);y=w*Math.sin(a/2);z=(1+w*Math.cos(a/2))*Math.sin(a);}
      else if(p.shape==='ellipsoid'){x=Math.cos(a)*Math.sin(v*Math.PI);y=1.5*Math.cos(v*Math.PI);z=.65*Math.sin(a)*Math.sin(v*Math.PI);}
      else{x=(u-.5)*4;z=(v-.5)*4;y=p.amplitude*Math.sin(x*p.frequency)*Math.cos(z*p.frequency);}
      vertices.push(x*p.radius,y*p.radius,z*p.radius);const cc=c.clone().offsetHSL((y+.5)*.07,0,y*.05);colors.push(cc.r,cc.g,cc.b);
      if(i<uCount&&j<vCount){const k=i*(vCount+1)+j;indices.push(k,k+1,k+vCount+1,k+1,k+vCount+2,k+vCount+1);}
    }
    const g=geo(new THREE.BufferGeometry());g.setAttribute('position',new THREE.Float32BufferAttribute(vertices,3));g.setAttribute('color',new THREE.Float32BufferAttribute(colors,3));g.setIndex(indices);g.computeVertexNormals();mesh(group,g,material('#ffffff',{vertexColors:true,side:THREE.DoubleSide,roughness:.38}));
  }
  function molecule(group,p,color) {
    // Fallback is explicitly a visual scaffold, never an inferred chemical structure.
    const atoms=p.atoms?.length?p.atoms:Array.from({length:18},(_,i)=>{const ring=Math.floor(i/6),a=(i%6)*Math.PI/3;return {id:String(i),element:i%7===0?'N':i%11===0?'O':'C',position:[Math.cos(a)*.92+ring*1.65-1.65,Math.sin(a)*.92,Math.sin(i*1.9)*.16]};});
    const byId=new Map(atoms.map(a=>[a.id,a]));
    const bonds=p.atoms?.length?(p.bonds||[]):atoms.map((a,i)=>({from:a.id,to:String(Math.floor(i/6)*6+(i+1)%6),order:1})).concat([{from:'0',to:'9',order:1},{from:'6',to:'15',order:1}]);
    const atomRadii=atoms.map(a=>p.radius*(a.element==='H'?.6:1));
    const atomMesh=spheres(group,atoms.map(a=>a.position),atomRadii.map(r=>r*(p.representation==='space_filling'?2.6:1)),atoms.map(a=>ELEMENT_COLORS[a.element]||color));
    atomMesh.userData.atomRecords=atoms;
    if(p.representation!=='space_filling') {
      const segments=[],colors=[],radii=[];
      for(const b of bonds){
        const a=byId.get(b.from),z=byId.get(b.to);if(!a||!z)continue;
        const av=vec(a.position),bv=vec(z.position),direction=bv.clone().sub(av),length=direction.length(),order=b.order||1;if(!(length>0))continue;
        const radius=Math.min(p.thickness,p.radius*(order>1?.18:.3),length*.08);
        direction.normalize();const axis=Math.abs(direction.y)<.9?new THREE.Vector3(0,1,0):new THREE.Vector3(1,0,0);
        const separation=Math.min(Math.max(p.radius*.38,radius*2.5),p.radius*.8,length*.25),side=direction.cross(axis).normalize().multiplyScalar(separation);
        for(let j=0;j<order;j++){const offset=side.clone().multiplyScalar(j-(order-1)/2),aa=av.clone().add(offset),bb=bv.clone().add(offset),middle=aa.clone().lerp(bb,.5);segments.push([aa.toArray(),middle.toArray()],[middle.toArray(),bb.toArray()]);radii.push(radius,radius);colors.push(ELEMENT_COLORS[a.element]||color,ELEMENT_COLORS[z.element]||color);}
      }
      rods(group,segments,radii,colors);
    }
    if(p.representation==='surface'){
      // A visual van der Waals envelope, not an isosurface from electron-density data.
      spheres(group,atoms.map(a=>a.position),atomRadii.map(r=>r*2.24),color,{transparent:true,opacity:.3,depthWrite:false,roughness:.2});
    }
  }
  function dna(group,p,color) {
    const count=Math.ceil(Math.min(320,p.turns*64)*complexity),a=helixPoints({...p,count}),b=helixPoints({...p,count,phase:Math.PI});
    tube(group,a,p.thickness*1.6,color);tube(group,b,p.thickness*1.6,p.colors[1]||'#ba9bef');
    const pairs=Math.min(180,Math.ceil(p.turns*10)),aa=helixPoints({...p,count:pairs}),bb=helixPoints({...p,count:pairs,phase:Math.PI}),segments=[],colors=[];
    for(let i=0;i<pairs;i++){const midpoint=aa[i].map((x,k)=>(x+bb[i][k])/2);segments.push([aa[i],midpoint],[midpoint,bb[i]]);colors.push(i%2?'#83bed5':'#e4bf88',i%2?'#d7a6c3':'#83ccb5');}
    rods(group,segments,p.thickness*.75,colors);spheres(group,aa.concat(bb),p.thickness*1.85,[color,p.colors[1]||'#ba9bef']);
  }
  function protein(group,p,color) {
    const points=p.points?.length>1?p.points:Array.from({length:Math.ceil(240*complexity)},(_,i)=>{const t=i/(Math.ceil(240*complexity)-1)*Math.PI*5;return [p.radius*(Math.sin(t)+.48*Math.sin(t*3.15)),p.radius*(Math.cos(t*.72)+.35*Math.sin(t*3.15)),p.radius*(Math.cos(t*.43)+.4*Math.cos(t*3.15))];});
    if(p.representation==='surface'){spheres(group,points,Math.max(p.radius*.25,p.thickness),color,{roughness:.7});return;}
    const curve=new THREE.CatmullRomCurve3(points.map(vec)),segments=Math.min(400,Math.max(32,Math.floor(points.length*1.5))),frames=curve.computeFrenetFrames(segments,false),vertices=[],indices=[],colors=[];
    const width=p.radius*.17,thickness=p.radius*.055;
    for(let i=0;i<=segments;i++){const at=curve.getPointAt(i/segments);const c=new THREE.Color(color).offsetHSL((i/segments-.5)*.18,0,0);for(const [s,t] of [[-1,-1],[1,-1],[1,1],[-1,1]]){const point=at.clone().addScaledVector(frames.normals[i],s*width).addScaledVector(frames.binormals[i],t*thickness);vertices.push(...point.toArray());colors.push(c.r,c.g,c.b);}if(i<segments)for(let k=0;k<4;k++){const a=i*4+k,b=i*4+(k+1)%4;indices.push(a,b,a+4,b,b+4,a+4);}}
    const g=geo(new THREE.BufferGeometry());g.setAttribute('position',new THREE.Float32BufferAttribute(vertices,3));g.setAttribute('color',new THREE.Float32BufferAttribute(colors,3));g.setIndex(indices);g.computeVertexNormals();mesh(group,g,material('#ffffff',{vertexColors:true,side:THREE.DoubleSide,roughness:.3}));
  }
  function virus(group,p,color) {
    const radius=p.radius;
    const envelope=geo(new THREE.SphereGeometry(radius,Math.ceil(72*complexity),Math.ceil(52*complexity),p.cutaway?.6:0,p.cutaway?Math.PI*1.55:Math.PI*2));
    const attr=envelope.attributes.position;
    for(let i=0;i<attr.count;i++){const x=attr.getX(i)/radius,y=attr.getY(i)/radius,z=attr.getZ(i)/radius,f=1+.018*Math.sin(x*21+y*12)*Math.cos(z*18);attr.setXYZ(i,x*radius*f,y*radius*f,z*radius*f);}envelope.computeVertexNormals();
    mesh(group,envelope,material(color,{side:THREE.DoubleSide,roughness:.85,metalness:.08}));
    const points=fibonacciSphere(Math.ceil(1100*complexity),radius*1.01).filter(([x,,z])=>!p.cutaway||Math.atan2(z,x)<-.1||Math.atan2(z,x)>.65);
    spheres(group,points,radius*.027,['#80b6c3','#8dbecf','#719dad'],{roughness:.9});
    const spikes=fibonacciSphere(Math.ceil(p.count*complexity),radius).filter(([x,,z])=>!p.cutaway||Math.atan2(z,x)<-.1||Math.atan2(z,x)>.65),segments=[],heads=[];
    for(const s of spikes){const normal=vec(s).normalize(),a=normal.clone().multiplyScalar(radius*1.03),b=normal.clone().multiplyScalar(radius*1.25);segments.push([a.toArray(),b.toArray()]);const side=normal.clone().cross(new THREE.Vector3(.2,.8,.1)).normalize(),other=normal.clone().cross(side);for(let k=0;k<3;k++){const angle=k*Math.PI*2/3;heads.push(b.clone().addScaledVector(side,Math.cos(angle)*radius*.068).addScaledVector(other,Math.sin(angle)*radius*.068).toArray());}}
    rods(group,segments,radius*.023,'#c8b8d6');spheres(group,heads,radius*.085,['#c19bc6','#cca6c8','#b894c0']);
    if(p.cutaway){const core=mesh(group,geo(new THREE.ConeGeometry(radius*.47,radius*1.12,7)),material('#ddba87',{roughness:.6}));core.rotation.z=.3;core.position.x=.15*radius;
      for(let i=0;i<2;i++){const pts=helixPoints({radius:radius*.21,length:radius*.9,turns:2.8,phase:i*Math.PI,count:75});tube(group,pts,radius*.025,i?'#df927e':'#ecd6a6');}}
  }
  function planet(group,p,color,star=false) {
    const g=geo(new THREE.SphereGeometry(p.radius,Math.ceil(80*complexity),Math.ceil(56*complexity))),positions=g.attributes.position,colors=[],base=new THREE.Color(color);
    for(let i=0;i<positions.count;i++){
      const x=positions.getX(i)/p.radius,y=positions.getY(i)/p.radius,z=positions.getZ(i)/p.radius;
      const noise=Math.sin(x*12+p.seed)*Math.cos(y*17+z*11)+.5*Math.sin(x*27-y*16+p.seed);
      const bands=Math.sin(y*27+noise*.7),c=base.clone();c.offsetHSL(star?.025*noise:.025*bands,0,star?.12*noise:.08*bands);
      colors.push(c.r,c.g,c.b);if(!p.rings&&!star){const amount=1+.012*noise;positions.setXYZ(i,positions.getX(i)*amount,positions.getY(i)*amount,positions.getZ(i)*amount);}
    }
    g.setAttribute('color',new THREE.Float32BufferAttribute(colors,3));g.computeVertexNormals();mesh(group,g,material('#ffffff',{vertexColors:true,roughness:.84,...(star?{emissive:color,emissiveIntensity:.65}:{})}));
    if(p.rings){const g=geo(new THREE.RingGeometry(p.radius*1.35,p.radius*2.05,Math.ceil(140*complexity),18));const pos=g.attributes.position,colors=[];for(let i=0;i<pos.count;i++){const r=Math.hypot(pos.getX(i),pos.getY(i))/p.radius,c=new THREE.Color(color).offsetHSL(.015, -.12,Math.sin(r*160)*.05+.05);colors.push(c.r,c.g,c.b);}g.setAttribute('color',new THREE.Float32BufferAttribute(colors,3));const ring=mesh(group,g,material('#ffffff',{vertexColors:true,side:THREE.DoubleSide,transparent:true,opacity:.78,roughness:1}));ring.rotation.x=-Math.PI/2+.25;ring.rotation.y=.16;}
    if(!star){const atmosphere=mesh(group,sphere,material(color,{transparent:true,opacity:.08,side:THREE.BackSide,depthWrite:false,roughness:1}),null,p.radius*1.055);atmosphere.renderOrder=2;}
  }
  function blackHole(group,p,color) {
    mesh(group,sphere,material('#020305',{roughness:1,metalness:0}),null,p.radius);
    const ring=geo(new THREE.RingGeometry(p.radius*1.2,p.radius*4.0,Math.ceil(150*complexity),Math.ceil(45*complexity))),positions=ring.attributes.position,colors=[];
    for(let i=0;i<positions.count;i++){const x=positions.getX(i),y=positions.getY(i),r=Math.hypot(x,y)/p.radius,a=Math.atan2(y,x),intensity=Math.sin(r*58+a*8)*.05;const c=new THREE.Color(color).lerp(new THREE.Color('#8c4978'),(r-1.2)/3);c.offsetHSL(0,0,intensity+.17*(4-r)/3);colors.push(c.r,c.g,c.b);positions.setZ(i,Math.sin(a*3+r*12)*p.radius*.008);}
    ring.setAttribute('color',new THREE.Float32BufferAttribute(colors,3));const disk=mesh(group,ring,material('#ffffff',{vertexColors:true,emissive:color,emissiveIntensity:.24,side:THREE.DoubleSide,roughness:.8}));disk.rotation.x=-Math.PI/2+.23;disk.rotation.y=-.25;
    const photon=mesh(group,geo(new THREE.TorusGeometry(p.radius*1.06,p.radius*.025,8,Math.ceil(100*complexity))),material('#fff1ce',{emissive:'#f7cf8e',emissiveIntensity:1}));photon.rotation.copy(disk.rotation);
  }
  function flow(group,p,color) {
    const lines=p.points?.length>1?[p.points]:potentialStreamlines(Math.ceil(Math.min(p.count,64)*complexity),p.radius);
    lines.forEach((points,i)=>tube(group,points,p.thickness,color,{transparent:true,opacity:.45+.45*(i%5)/4,roughness:.7}));
    if(p.points?.length>1)return;
    const arrows=[];
    for(const [i,points] of lines.entries()){const index=Math.floor(points.length*(.25+(i%3)*.23));if(points[index+2])arrows.push([points[index],points[index+2]]);}
    rods(group,arrows,p.thickness*2.3,'#b5f1df');
  }
  for(const node of spec.nodes) {
    const group=new THREE.Group();group.name=node.id;group.userData.node=node;group.position.set(...node.position);group.rotation.set(...node.rotation);group.scale.set(...node.scale);root.add(group);objects.set(node.id,group);
    const p=node.parameters,c=node.color;
    if(node.type==='molecule')molecule(group,p,c);
    else if(node.type==='dna')dna(group,p,c);
    else if(node.type==='virus')virus(group,p,c);
    else if(node.type==='protein')protein(group,p,c);
    else if(node.type==='planet'||node.type==='star')planet(group,p,c,node.type==='star');
    else if(node.type==='black_hole')blackHole(group,p,c);
    else if(node.type==='field'||node.type==='streamlines')flow(group,p,c);
    else if(node.type==='surface')surface(group,p,c);
    else if(node.type==='curve')tube(group,p.points||[],p.thickness,c);
    else if(node.type==='mesh'){const g=geo(new THREE.BufferGeometry());g.setAttribute('position',new THREE.Float32BufferAttribute(p.vertices.flat(),3));g.setIndex(p.indices);g.computeVertexNormals();mesh(group,g,material(c,{side:THREE.DoubleSide,transparent:p.opacity<1,opacity:p.opacity}));}
    else if(node.type==='box')mesh(group,geo(new THREE.BoxGeometry(p.radius*2,p.radius*2,p.radius*2)),material(c));
    else mesh(group,sphere,material(node.type==='atom'?(ELEMENT_COLORS[p.element]||c):c,{roughness:.4}),null,p.radius);
    group.traverse(o=>{if(o.isMesh){o.userData.sceneNodeId=node.id;pickables.push(o);}});
  }
  const bondObjects=[];
  for(const bond of spec.bonds){const a=objects.get(bond.from),b=objects.get(bond.to);if(!a||!b)continue;const group=new THREE.Group();root.add(group);const m=rods(group,[[a.position.toArray(),b.position.toArray()]],.045,'#a5beca');bondObjects.push({mesh:m,a,b});}
  function updateBonds(){for(const {mesh:m,a,b} of bondObjects){const direction=b.position.clone().sub(a.position),length=direction.length();quaternion.setFromUnitVectors(up,direction.normalize());matrix.compose(a.position.clone().lerp(b.position,.5),quaternion,new THREE.Vector3(.045,length,.045));m.setMatrixAt(0,matrix);m.instanceMatrix.needsUpdate=true;}}
  function dispose(){resources.geometries.forEach(g=>g.dispose());resources.materials.forEach(m=>m.dispose());root.clear();}
  return {root,objects,pickables,updateBonds,dispose,resources};
}
