/** GLTFLoader data that can influence generated shader source must stay typed. */
const fail=message=>{throw new Error(message);};
const object=(value,label)=>{if(!value||typeof value!=='object'||Array.isArray(value))fail(`${label} must be an object.`);return value;};
const number=(value,label,min=-1e15,max=1e15)=>{if(typeof value!=='number'||!Number.isFinite(value)||value<min||value>max)fail(`${label} must be a finite number in range.`);};
const vector=(value,length,label,min=-1e15,max=1e15)=>{if(!Array.isArray(value)||value.length!==length)fail(`${label} has an invalid vector.`);value.forEach(v=>number(v,label,min,max));};
const member=(value,key,check)=>{if(value[key]!==undefined)check(value[key],key);};
export const validTextureChannel=value=>Number.isInteger(value)&&value>=0&&value<=3;
const controlCharacters=/[\u0000-\u001f\u007f-\u009f\u2028\u2029]/u;
const label=value=>{if(typeof value!=='string'||value.length>1024||controlCharacters.test(value))fail('Material names must be single-line text of at most 1,024 characters.');};

// Values that GLTFLoader copies into material uniforms or uses to select built-in shaders.
const scalarFields={
  emissiveStrength:[0,1e15],clearcoatFactor:[0,1],clearcoatRoughnessFactor:[0,1],
  dispersion:[0,1e15],iridescenceFactor:[0,1],iridescenceIor:[1,1e15],
  iridescenceThicknessMinimum:[0,1e15],iridescenceThicknessMaximum:[0,1e15],
  sheenRoughnessFactor:[0,1],transmissionFactor:[0,1],thicknessFactor:[0,1e15],
  attenuationDistance:[0,1e15],ior:[1,1e15],specularFactor:[0,1],
  bumpFactor:[-1e15,1e15],anisotropyStrength:[0,1],anisotropyRotation:[-1e15,1e15],
};
const vectorFields={sheenColorFactor:3,attenuationColor:3,specularColorFactor:3};
const materialExtensions=new Set([
  'KHR_materials_unlit','KHR_materials_emissive_strength','KHR_materials_clearcoat',
  'KHR_materials_dispersion','KHR_materials_iridescence','KHR_materials_sheen',
  'KHR_materials_transmission','KHR_materials_volume','KHR_materials_ior',
  'KHR_materials_specular','EXT_materials_bump','KHR_materials_anisotropy',
]);

function textureInfo(value,textures,labelText){
  object(value,labelText);
  if(!Number.isInteger(value.index)||value.index<0||value.index>=textures.length)fail('Material texture index is invalid.');
  member(value,'texCoord',v=>{if(!validTextureChannel(v))fail('Texture texCoord must be an integer from 0 to 3.');});
  member(value,'scale',v=>number(v,'Normal texture scale'));
  member(value,'strength',v=>number(v,'Occlusion texture strength',0,1));
  if(value.extensions!==undefined){
    object(value.extensions,'Texture extensions');
    const transform=value.extensions.KHR_texture_transform;
    if(transform!==undefined){
      object(transform,'Texture transform');
      member(transform,'texCoord',v=>{if(!validTextureChannel(v))fail('Texture transform texCoord must be an integer from 0 to 3.');});
      member(transform,'offset',v=>vector(v,2,'Texture offset'));
      member(transform,'scale',v=>vector(v,2,'Texture scale'));
      member(transform,'rotation',v=>number(v,'Texture rotation'));
    }
  }
}

/** Validate and replace names in JSON that will be re-encoded before GLTFLoader. */
export function prepareGLBShaderMetadata(json){
  const materials=json.materials===undefined?[]:json.materials,textures=json.textures===undefined?[]:json.textures,samplers=json.samplers===undefined?[]:json.samplers;
  if(!Array.isArray(materials)||!Array.isArray(textures)||!Array.isArray(samplers))fail('GLB materials, textures and samplers must be arrays.');
  for(const sampler of samplers){
    object(sampler,'Texture sampler');
    for(const [key,allowed] of Object.entries({magFilter:[9728,9729],minFilter:[9728,9729,9984,9985,9986,9987],wrapS:[33071,33648,10497],wrapT:[33071,33648,10497]}))member(sampler,key,v=>{if(!allowed.includes(v))fail(`Texture sampler ${key} is invalid.`);});
  }
  for(const texture of textures){object(texture,'Texture');member(texture,'sampler',v=>{if(!Number.isInteger(v)||v<0||v>=samplers.length)fail('Texture sampler index is invalid.');});}
  for(const [index,material] of materials.entries()){
    object(material,'Material');
    if(material.name!==undefined)label(material.name);
    if(material.extras!==undefined)object(material.extras,'Material extras');
    const humanLabel=material.extras?.phaseforgeMaterialLabel??material.name;
    if(humanLabel!==undefined)label(humanLabel);
    if(humanLabel!==undefined)material.extras={...material.extras,phaseforgeMaterialLabel:humanLabel};
    material.name=`phaseforge_material_${index}`;
    member(material,'alphaMode',v=>{if(!['OPAQUE','MASK','BLEND'].includes(v))fail('Material alphaMode is invalid.');});
    member(material,'alphaCutoff',v=>number(v,'Material alpha cutoff',0));
    member(material,'doubleSided',v=>{if(typeof v!=='boolean')fail('Material doubleSided must be boolean.');});
    member(material,'emissiveFactor',v=>vector(v,3,'Material emissive factor',0,1));
    for(const key of ['normalTexture','occlusionTexture','emissiveTexture'])member(material,key,v=>textureInfo(v,textures,key));
    if(material.pbrMetallicRoughness!==undefined){
      const pbr=object(material.pbrMetallicRoughness,'PBR material');
      member(pbr,'baseColorFactor',v=>vector(v,4,'Base color factor',0,1));
      for(const key of ['metallicFactor','roughnessFactor'])member(pbr,key,v=>number(v,key,0,1));
      for(const key of ['baseColorTexture','metallicRoughnessTexture'])member(pbr,key,v=>textureInfo(v,textures,key));
    }
    if(material.extensions!==undefined){
      object(material.extensions,'Material extensions');
      for(const [extensionName,extension] of Object.entries(material.extensions)){
        if(!materialExtensions.has(extensionName))continue; // Unknown extensions remain data-only userData.
        object(extension,extensionName);
        for(const [key,value] of Object.entries(extension)){
          if(scalarFields[key])number(value,key,...scalarFields[key]);
          else if(vectorFields[key])vector(value,vectorFields[key],key,0,1);
          else if(key.endsWith('Texture'))textureInfo(value,textures,key);
        }
      }
    }
  }
}

/** Last boundary after every loader, before an Object3D can reach WebGLRenderer. */
export function enforceSafeModelMaterials(root){
  const allowedTypes=new Set(['MeshStandardMaterial','MeshPhysicalMaterial','MeshBasicMaterial','LineBasicMaterial','PointsMaterial']);
  const materials=new Set(),textures=new Set();
  root.traverse(object3d=>{
    for(const material of Array.isArray(object3d.material)?object3d.material:[object3d.material]){
      if(!material||materials.has(material))continue;
      if(material.isMaterial!==true||!allowedTypes.has(material.type)||material.isShaderMaterial||material.isRawShaderMaterial)fail('Only built-in GLB surface, line and point materials are supported.');
      if(material.vertexShader!==undefined||material.fragmentShader!==undefined||material.glslVersion!=null)fail('Imported custom shader source is not supported.');
      if(material.precision!=null&&!['highp','mediump','lowp'].includes(material.precision))fail('Material shader precision is invalid.');
      for(const [key,value] of Object.entries(material.defines??{}))if(!['STANDARD','PHYSICAL'].includes(key)||value!=='')fail('Imported shader defines are not supported.');
      const humanLabel=material.userData?.phaseforgeMaterialLabel??material.name;
      if(humanLabel!==undefined)label(humanLabel);
      material.userData={...material.userData,phaseforgeMaterialLabel:humanLabel};
      material.name=`phaseforge_material_${materials.size}`;
      materials.add(material);
      for(const value of Object.values(material))for(const texture of Array.isArray(value)?value:[value]){
        if(texture?.isTexture!==true||textures.has(texture))continue;
        if(!validTextureChannel(texture.channel))fail('Loaded texture channel must be an integer from 0 to 3.');
        textures.add(texture);
      }
      material.needsUpdate=true;
    }
  });
  return {materials:materials.size,textures:textures.size};
}
