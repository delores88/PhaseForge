//! Fixed-host, non-executable public file validation. Files remain data and never
//! become Python, browser scripts, Blender scenes or application instructions.
use anyhow::{bail,Context};
use serde_json::{json,Value};

const HOSTS:&[&str]=&["raw.githubusercontent.com","zenodo.org","www.ebi.ac.uk","ftp.ebi.ac.uk","files.rcsb.org","data.eso.org","www.eso.org","archive.ics.uci.edu","physionet.org","data.nist.gov","srdata.nist.gov","data.nasa.gov"];
const EXTENSIONS:&[&str]=&["csv","json","pdb","stl","glb","png","jpg","jpeg"];
pub(super) fn url(value:&str)->anyhow::Result<url::Url> {
    if value.len()>2048 || value.chars().any(char::is_control){bail!("Public file URL must be bounded HTTPS text without control characters");}
    let parsed=url::Url::parse(value).context("Enter a complete HTTPS URL to a public scientific/data file")?;
    if parsed.scheme()!="https" || !HOSTS.contains(&parsed.host_str().unwrap_or("")) || parsed.port().is_some()
        || !parsed.username().is_empty() || parsed.password().is_some() || parsed.query().is_some() || parsed.fragment().is_some() {
        bail!("Public file imports require HTTPS on these exact hosts: {}. Redirects, credentials, query strings, custom ports and other hosts are unsupported.",HOSTS.join(", "));
    }
    if !EXTENSIONS.contains(&extension(&parsed).as_str()){bail!("Supported public files are CSV, JSON, PDB, STL, self-contained GLB, PNG and JPEG; HTML, SVG, scripts, executables and .blend files are not accepted");}
    Ok(parsed)
}
fn extension(url:&url::Url)->String {url.path().rsplit('.').next().unwrap_or("").to_ascii_lowercase()}
pub(super) fn filename(value:&str)->String {
    let path=url::Url::parse(value).ok().and_then(|url|url.path_segments().and_then(|parts|parts.last()).map(str::to_owned)).unwrap_or_else(||"public-data".into());
    let name=path.chars().filter(|ch|ch.is_ascii_alphanumeric() || matches!(ch,'.'|'_'|'-')).take(180).collect::<String>();
    if name.is_empty(){"public-data".into()}else{name}
}
pub(super) struct ValidatedFile {pub mime:&'static str,pub description:String,pub csv_profile:Option<Value>}
fn preview(text:&str)->String {text.chars().filter(|ch|!ch.is_control() || *ch=='\n' || *ch=='\t').take(2000).collect()}
pub(super) fn validate(source:&str,bytes:&[u8])->anyhow::Result<ValidatedFile> {
    let url=url(source)?;
    if bytes.is_empty() || bytes.len()>8*1024*1024 {bail!("Public files must contain 1 byte to 8 MiB");}
    let ext=extension(&url);
    let mut file=ValidatedFile {mime:"application/octet-stream",description:String::new(),csv_profile:None};
    match ext.as_str() {
        "csv"=>{
            let text=std::str::from_utf8(bytes).context("CSV must be UTF-8 text")?;
            let trimmed=text.trim_start_matches('\u{feff}').trim_start();
            if trimmed.starts_with('<') || trimmed.starts_with("#!") || text.contains('\0'){bail!("The response is markup/executable content, not a CSV table");}
            let profile=super::data::profile(text)?;
            file.mime="text/csv";file.description=format!("Public CSV: {} data rows, {} columns. Untrusted raw data preview (up to 2,000 characters):\n{}",profile["rows"],profile["columns"].as_array().map_or(0,Vec::len),preview(text));file.csv_profile=Some(profile);
        }
        "json"=>{
            let value:Value=serde_json::from_slice(bytes).context("Public JSON is malformed; HTML or executable files are not accepted")?;
            if !value.is_object() && !value.is_array(){bail!("Public JSON must contain a data object or array");}
            check_json_budget(&value,false)?;
            file.mime="application/json";file.description=format!("Public JSON data. Untrusted raw data preview (up to 2,000 characters):\n{}",preview(std::str::from_utf8(bytes)?));
        }
        "pdb"=>{
            let text=std::str::from_utf8(bytes).context("PDB must be UTF-8 text")?;
            if !text.lines().any(|line|line.starts_with("ATOM  ")||line.starts_with("HETATM")){bail!("Public PDB does not contain structural coordinate records");}
            file.mime="chemical/x-pdb";file.description=format!("Public PDB-format structure; experimental or computed origin must be checked at its source. Header preview:\n{}",preview(text));
        }
        "png"|"jpg"|"jpeg"=>{
            file.mime=super::assets::image_mime(bytes)?;
            if (ext=="png")!=(file.mime=="image/png"){bail!("Image bytes do not match the file extension");}
            file.description="Public static image. Interpretation, processing, credit and usage terms require the linked source; it is not automatically classified as a measured observation.".into();
        }
        "stl"=>{let triangles=stl(bytes)?;file.mime="model/stl";file.description=format!("Public STL mesh: {triangles} triangles. STL does not encode reliable units, source uncertainty or material properties; confirm units before scientific interpretation or fabrication.");}
        "glb"=>{let summary=glb(bytes)?;file.mime="model/gltf-binary";file.description=format!("Public self-contained GLB geometry: {summary}. No external assets, script execution or compressed decoder extensions are allowed. Confirm source units, provenance and geometry before interpretation.");}
        _=>unreachable!(),
    }
    Ok(file)
}
fn check_json_budget(value:&Value,forbid_urls:bool)->anyhow::Result<()> {
    let mut stack=vec![(value,0)];let mut count=0;
    while let Some((value,depth))=stack.pop() {
        count+=1;if count>250_000 || depth>64 {bail!("Public JSON exceeds the bounded structural complexity budget");}
        match value {
            Value::Object(object)=>for (key,child) in object {if forbid_urls && ["uri","url"].contains(&key.as_str()){bail!("GLB external URLs and data URIs are not allowed; export a self-contained file");}stack.push((child,depth+1));},
            Value::Array(array)=>for child in array {stack.push((child,depth+1));},_=>{}
        }
    }
    Ok(())
}
fn u32le(bytes:&[u8],at:usize)->u32 {u32::from_le_bytes(bytes[at..at+4].try_into().unwrap())}
fn glb_shader_metadata(value:&Value)->anyhow::Result<()> {
    // GLTFLoader forwards these labels to Material.name. The viewer replaces
    // them with trusted shader identifiers before loading and again afterwards;
    // intake still rejects multiline/control payloads before saving raw bytes.
    if let Some(materials)=value.get("materials") {
        let materials=materials.as_array().context("GLB materials must be an array")?;
        for material in materials {
            let material=material.as_object().context("GLB materials must contain objects")?;
            if let Some(name)=material.get("name") {
                let name=name.as_str().context("GLB material names must be text")?;
                if name.encode_utf16().count()>1024 || name.chars().any(|ch|ch.is_control()||matches!(ch,'\u{2028}'|'\u{2029}')) {
                    bail!("GLB material names must be single-line labels of at most 1,024 UTF-16 code units without control characters");
                }
            }
        }
    }
    // Cover all core and extension texture-info paths, including texture
    // transforms; no caller-controlled string may become a *_MAP_UV macro.
    // check_json_budget has already bounded the complete tree before this walk.
    let mut stack=vec![value];
    while let Some(value)=stack.pop() {
        match value {
            Value::Object(object)=>for (key,child) in object {
                if key=="texCoord" && child.as_f64().is_none_or(|number|number.fract()!=0.0||!(0.0..=3.0).contains(&number)) {
                    bail!("GLB texture coordinates must be numeric integers from 0 through 3");
                }
                stack.push(child);
            },
            Value::Array(array)=>stack.extend(array),
            _=>{}
        }
    }
    Ok(())
}
fn stl(bytes:&[u8])->anyhow::Result<usize> {
    if bytes.len()>=84 {
        let count=u32le(bytes,80) as usize;
        if count.checked_mul(50).and_then(|n|n.checked_add(84))==Some(bytes.len()) {
            if count==0 || count>150_000 {bail!("STL requires 1–150,000 triangles within the public file budget");}
            for at in (84..bytes.len()).step_by(50) {for coordinate in 0..12 {
                let value=f32::from_le_bytes(bytes[at+coordinate*4..at+coordinate*4+4].try_into().unwrap());
                if !value.is_finite() || value.abs()>1e15 {bail!("STL has nonfinite or unbounded coordinates");}
            }}
            return Ok(count);
        }
    }
    let text=std::str::from_utf8(bytes).context("STL is neither a complete binary mesh nor UTF-8 ASCII STL")?;
    if !text.is_ascii(){bail!("ASCII STL must contain ASCII geometry records");}
    let lines=text.lines().map(str::trim).filter(|line|!line.is_empty()).collect::<Vec<_>>();
    if lines.len()<9 || !lines[0].to_ascii_lowercase().starts_with("solid") || !lines.last().unwrap().to_ascii_lowercase().starts_with("endsolid") {bail!("ASCII STL lacks complete solid records");}
    let geometry=&lines[1..lines.len()-1];
    if geometry.len()%7!=0 || geometry.len()/7>150_000 {bail!("ASCII STL has incomplete or excessive facets");}
    fn vector(line:&str,prefix:&str)->anyhow::Result<()> {
        let lower=line.to_ascii_lowercase();let tail=lower.strip_prefix(prefix).context("Invalid STL geometry record")?;
        let parts=tail.split_whitespace().collect::<Vec<_>>();
        if parts.len()!=3 || parts.iter().any(|part|part.parse::<f64>().ok().is_none_or(|number|!number.is_finite()||number.abs()>1e15)){bail!("Invalid STL coordinate vector");}Ok(())
    }
    for facet in geometry.chunks_exact(7) {
        vector(facet[0],"facet normal ")?;
        if !facet[1].eq_ignore_ascii_case("outer loop") || !facet[5].eq_ignore_ascii_case("endloop") || !facet[6].eq_ignore_ascii_case("endfacet"){bail!("Invalid ASCII STL facet structure");}
        for vertex in &facet[2..5] {vector(vertex,"vertex ")?;}
    }
    Ok(geometry.len()/7)
}
fn glb(bytes:&[u8])->anyhow::Result<Value> {
    if bytes.len()<20 || &bytes[..4]!=b"glTF" || u32le(bytes,4)!=2 || u32le(bytes,8) as usize!=bytes.len() {bail!("Public GLB must be a complete version 2 binary glTF file");}
    let mut at=12;let mut metadata=None;let mut binary=None;
    while at+8<=bytes.len() {
        let len=u32le(bytes,at) as usize;let kind=u32le(bytes,at+4);at+=8;
        if len%4!=0 || len>bytes.len()-at {bail!("Invalid GLB chunk length");}
        match kind {
            0x4e4f534a if metadata.is_none() && binary.is_none()=>{metadata=Some(serde_json::from_slice::<Value>(&bytes[at..at+len]).context("Invalid GLB JSON chunk")?);}
            0x004e4942 if metadata.is_some() && binary.is_none()=>{binary=Some(&bytes[at..at+len]);}
            _=>bail!("GLB must have one JSON chunk followed by at most one embedded binary chunk"),
        }
        at+=len;
    }
    if at!=bytes.len(){bail!("Truncated GLB chunk header");}
    let value=metadata.context("GLB metadata missing")?;check_json_budget(&value,true)?;
    if value["asset"]["version"]!="2.0" {bail!("Unsupported GLB asset version");}
    glb_shader_metadata(&value)?;
    if value["extensionsUsed"].as_array().into_iter().flatten().any(|ext|["KHR_draco_mesh_compression","EXT_meshopt_compression","KHR_texture_basisu"].contains(&ext.as_str().unwrap_or(""))) {bail!("Public GLB must contain uncompressed geometry and PNG/JPEG textures");}
    let buffers=value["buffers"].as_array().map(Vec::as_slice).unwrap_or(&[]);let bin=binary.unwrap_or(&[]);
    if buffers.len()>1 || buffers.first().is_some_and(|buffer|buffer["byteLength"].as_u64().is_none_or(|length|length>bin.len() as u64)){bail!("GLB embedded buffer does not match its declared size");}
    let views=value["bufferViews"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    for view in views {
        let length=view["byteLength"].as_u64().context("Invalid GLB buffer view length")?;let offset=view["byteOffset"].as_u64().unwrap_or(0);
        if view["buffer"]!=0 || offset.checked_add(length).is_none_or(|end|end>bin.len() as u64) {bail!("GLB buffer view escapes the embedded buffer");}
    }
    let nodes=value["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if nodes.len()>4096 {bail!("Public GLB exceeds 4,096 scene nodes");}
    let accessors=value["accessors"].as_array().map(Vec::as_slice).unwrap_or(&[]);let mut allocated=0_u64;
    for accessor in accessors {
        let count=accessor["count"].as_u64().context("Invalid GLB accessor count")?;
        let components=match accessor["type"].as_str(){Some("SCALAR")=>1,Some("VEC2")=>2,Some("VEC3")=>3,Some("VEC4")|Some("MAT2")=>4,Some("MAT3")=>9,Some("MAT4")=>16,_=>bail!("Unsupported GLB accessor type")};
        let size=match accessor["componentType"].as_u64(){Some(5120|5121)=>1,Some(5122|5123)=>2,Some(5125|5126)=>4,_=>bail!("Unsupported GLB component type")};
        allocated=allocated.saturating_add(count.saturating_mul(components*size));
        if count>1_000_000 || allocated>32*1024*1024 {bail!("GLB accessor allocation exceeds the public intake budget");}
        if accessor.get("sparse").is_some(){bail!("Public GLB sparse accessors require a fully expanded export");}
        let view=accessor["bufferView"].as_u64().and_then(|index|views.get(index as usize)).context("GLB accessor must reference an embedded buffer view")?;
        let offset=accessor["byteOffset"].as_u64().unwrap_or(0);
        let stride=view["byteStride"].as_u64().unwrap_or(components*size);
        if stride<components*size || stride>256 || offset.saturating_add(count.saturating_sub(1).saturating_mul(stride)).saturating_add(if count>0{components*size}else{0})>view["byteLength"].as_u64().unwrap() {bail!("GLB accessor escapes its buffer view");}
        if accessor["componentType"]==5126 {
            let start=view["byteOffset"].as_u64().unwrap_or(0)+offset;
            for item in 0..count {for component in 0..components {
                let at=(start+item*stride+component*4) as usize;let number=f32::from_le_bytes(bin[at..at+4].try_into().unwrap());
                if !number.is_finite() || number.abs()>1e15 {bail!("GLB contains nonfinite or unbounded geometry values");}
            }}
        }
    }
    let images=value["images"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if images.len()>8 {bail!("Public GLB supports at most eight embedded images");}
    let mut texture_pixels=0_u64;
    for image in images {
        let index=image["bufferView"].as_u64().context("GLB images must use embedded buffer views")? as usize;
        let view=views.get(index).context("Invalid GLB image buffer view")?;
        let offset=view["byteOffset"].as_u64().unwrap_or(0) as usize;let length=view["byteLength"].as_u64().unwrap() as usize;
        let (mime,width,height)=super::assets::image_info(&bin[offset..offset+length])?;
        texture_pixels=texture_pixels.saturating_add(u64::from(width)*u64::from(height));
        if width>8192 || height>8192 || texture_pixels>32_000_000 {bail!("GLB embedded textures exceed their shared decoding budget");}
        if image["mimeType"]!=mime {bail!("Embedded GLB image does not match its declared PNG/JPEG MIME type");}
    }
    let meshes=value["meshes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if meshes.is_empty() || meshes.len()>4096 {bail!("Public GLB requires 1–4,096 meshes");}
    let mut primitive_count=0;
    for mesh in meshes {
        let primitives=mesh["primitives"].as_array().context("GLB mesh primitives are missing")?;
        if primitives.is_empty(){bail!("GLB mesh contains no geometry primitives");}
        primitive_count+=primitives.len();if primitive_count>8192 {bail!("GLB exceeds 8,192 geometry primitives");}
        for primitive in primitives {
            let position=primitive["attributes"]["POSITION"].as_u64().and_then(|index|accessors.get(index as usize)).context("GLB primitive requires a valid POSITION accessor")?;
            if position["type"]!="VEC3" || position["componentType"]!=5126 || position["count"].as_u64().unwrap_or(0)==0 {bail!("GLB primitive positions must contain finite three-dimensional float vectors");}
            if let Some(index)=primitive.get("indices") {
                let indices=index.as_u64().and_then(|index|accessors.get(index as usize)).context("Invalid GLB index accessor")?;
                if indices["type"]!="SCALAR" || ![5121,5123,5125].contains(&indices["componentType"].as_u64().unwrap_or(0)) {bail!("GLB index accessor must contain unsigned integer indices");}
                let view=&views[indices["bufferView"].as_u64().unwrap() as usize];
                let size=match indices["componentType"].as_u64().unwrap(){5121=>1,5123=>2,_=>4};
                let stride=view["byteStride"].as_u64().unwrap_or(size);
                let start=view["byteOffset"].as_u64().unwrap_or(0)+indices["byteOffset"].as_u64().unwrap_or(0);
                for row in 0..indices["count"].as_u64().unwrap() {
                    let at=(start+row*stride) as usize;
                    let index=match size {1=>u64::from(bin[at]),2=>u64::from(u16::from_le_bytes(bin[at..at+2].try_into().unwrap())),_=>u64::from(u32le(bin,at))};
                    if index>=position["count"].as_u64().unwrap() {bail!("GLB index points outside its position accessor");}
                }
            }
        }
    }
    let mut colors=vec![0_u8;nodes.len()];
    for start in 0..nodes.len() {
        let mut stack=vec![(start,false,0_usize)];
        while let Some((index,leaving,depth))=stack.pop() {
            if leaving {colors[index]=2;continue;}
            if colors[index]==1 || depth>128 {bail!("GLB node hierarchy is cyclic or too deep");}
            if colors[index]==2 {continue;}colors[index]=1;stack.push((index,true,depth));
            for child in nodes[index]["children"].as_array().into_iter().flatten() {
                let child=child.as_u64().filter(|index|*index<nodes.len() as u64).context("GLB node references an unknown child")? as usize;
                stack.push((child,false,depth+1));
            }
            if nodes[index].get("mesh").is_some_and(|mesh|mesh.as_u64().is_none_or(|index|index>=meshes.len() as u64)) {bail!("GLB node references an unknown mesh");}
        }
    }
    Ok(json!({"meshes":meshes.len(),"nodes":nodes.len(),"binary_bytes":bin.len(),"images":images.len()}))
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn public_file_urls_reject_local_hosts_credentials_queries_and_executable_formats() {
        for input in ["http://zenodo.org/records/1/files/data.csv","https://127.0.0.1/data.json","https://localhost/data.csv","https://www.ebi.ac.uk.evil.test/data.json",
            "https://user:secret@zenodo.org/data.json","https://zenodo.org/data.csv?token=secret","https://raw.githubusercontent.com:8443/test.json",
            "https://raw.githubusercontent.com/u/r/main/page.html","https://zenodo.org/model.blend","https://zenodo.org/image.svg","https://zenodo.org/run.py"] {assert!(url(input).is_err(),"accepted {input}");}
        assert!(url("https://raw.githubusercontent.com/u/r/main/data.csv").is_ok());
        assert_eq!(filename("https://zenodo.org/records/1/files/a%20file.csv"),"a20file.csv");
    }
    #[test]fn data_content_must_match_its_extension_and_profiles_remain_bounded() {
        let csv=validate("https://zenodo.org/data.csv",b"x,y\n1,3\n2,4\n").unwrap();assert_eq!(csv.csv_profile.unwrap()["rows"],2);
        for (name,bytes) in [("data.json",b"<html>blocked</html>".as_slice()),("data.csv",b"<html>fake,csv\n1,2".as_slice()),("mesh.stl",b"MZ executable".as_slice()),("mesh.glb",b"#!/bin/python".as_slice())] {
            assert!(validate(&format!("https://zenodo.org/{name}"),bytes).is_err());
        }
        assert!(validate("https://zenodo.org/data.json",b"{\"measurement\":1.5}").is_ok());
    }
    fn glb_fixture(value:Value,bin:&[u8])->Vec<u8> {
        let mut metadata=serde_json::to_vec(&value).unwrap();while metadata.len()%4!=0 {metadata.push(b' ');}
        let mut result=b"glTF".to_vec();result.extend(2_u32.to_le_bytes());result.extend(((12+8+metadata.len()+8+bin.len()) as u32).to_le_bytes());
        result.extend((metadata.len() as u32).to_le_bytes());result.extend(0x4e4f534a_u32.to_le_bytes());result.extend(metadata);
        result.extend((bin.len() as u32).to_le_bytes());result.extend(0x004e4942_u32.to_le_bytes());result.extend(bin);result
    }
    fn textured_glb_fixture()->(Value,Vec<u8>) {
        let mut bin=Vec::new();
        for number in [0.0_f32,0.,0.,1.,0.,0.,0.,1.,0.,0.,0.,1.,0.,0.,1.] {bin.extend(number.to_le_bytes());}
        // Complete 1x1 transparent PNG. Parser fixtures never create GPU shaders.
        bin.extend([137,80,78,71,13,10,26,10,0,0,0,13,73,72,68,82,0,0,0,1,0,0,0,1,8,6,0,0,0,31,21,196,137,0,0,0,11,73,68,65,84,120,156,99,96,0,2,0,0,5,0,1,122,94,171,63,0,0,0,0,73,69,78,68,174,66,96,130]);
        let value=json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":128}],
            "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":24},{"buffer":0,"byteOffset":60,"byteLength":68}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}],
            "images":[{"bufferView":2,"mimeType":"image/png"}],"textures":[{"source":0}],
            "materials":[{"name":"Protein surface.001","pbrMetallicRoughness":{"baseColorTexture":{"index":0,"texCoord":0}}}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1,"TEXCOORD_1":1,"TEXCOORD_2":1,"TEXCOORD_3":1},"material":0}]}],
            "nodes":[{"mesh":0}],"scenes":[{"nodes":[0]}],"scene":0});
        (value,bin)
    }
    #[test]fn glb_preserves_ordinary_material_labels_and_rejects_multiline_or_untyped_names() {
        let (base,bin)=textured_glb_fixture();
        for name in [String::new(),"Protein surface.001 (measured α)".into(),"a".repeat(1024),"🧬".repeat(512)] {
            let mut value=base.clone();value["materials"][0]["name"]=json!(name);
            assert!(validate("https://zenodo.org/model.glb",&glb_fixture(value,&bin)).is_ok());
        }
        let invalid=[json!("surface\nbreak"),json!("surface\rbreak"),json!("surface\tbreak"),json!("name\0"),json!("name\u{7f}"),json!("name\u{85}"),json!("name\u{2028}"),json!("name\u{2029}"),json!("a".repeat(1025)),json!("🧬".repeat(513)),json!(false),json!(0),Value::Null,json!(["surface"]),json!({"label":"surface"})];
        for name in invalid {
            let mut value=base.clone();value["materials"][0]["name"]=name;
            assert!(validate("https://zenodo.org/model.glb",&glb_fixture(value,&bin)).is_err());
        }
        for materials in [Value::Null,json!({"0":{"name":"surface"}}),json!([null]),json!(["surface"])] {
            let mut value=base.clone();value["materials"]=materials;
            assert!(glb(&glb_fixture(value,&bin)).is_err());
        }
    }
    #[test]fn glb_texture_channels_are_typed_and_bounded_across_core_and_extension_paths() {
        let (mut base,bin)=textured_glb_fixture();
        base["materials"][0]["normalTexture"]=json!({"index":0,"texCoord":0});
        base["materials"][0]["extensions"]=json!({"KHR_materials_clearcoat":{"clearcoatFactor":1,"clearcoatTexture":{"index":0,"texCoord":0}}});
        base["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["extensions"]=json!({"KHR_texture_transform":{"texCoord":0}});
        base["extensionsUsed"]=json!(["KHR_texture_transform","KHR_materials_clearcoat"]);
        let paths=["/materials/0/pbrMetallicRoughness/baseColorTexture/texCoord","/materials/0/normalTexture/texCoord",
            "/materials/0/extensions/KHR_materials_clearcoat/clearcoatTexture/texCoord",
            "/materials/0/pbrMetallicRoughness/baseColorTexture/extensions/KHR_texture_transform/texCoord"];
        for path in paths {
            for channel in [json!(0),json!(1),json!(2),json!(3),json!(0.0),json!(3.0)] {
                let mut value=base.clone();*value.pointer_mut(path).unwrap()=channel;
                assert!(glb(&glb_fixture(value,&bin)).is_ok(),"rejected numeric channel at {path}");
            }
            for channel in [json!(-1),json!(4),json!(1.5),json!("1"),json!("channel\nbreak"),json!(true),Value::Null,json!([]),json!({"channel":0})] {
                let mut value=base.clone();*value.pointer_mut(path).unwrap()=channel;
                assert!(validate("https://zenodo.org/model.glb",&glb_fixture(value,&bin)).is_err(),"accepted invalid channel at {path}");
            }
        }
        let mut value=base;value["extras"]=json!({"nested":[{"texCoord":"0"}]});
        assert!(glb(&glb_fixture(value,&bin)).is_err());
    }
    #[test]fn glb_disallows_external_payloads_and_expansive_allocations() {
        let base=json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}],"nodes":[{"mesh":0}]});
        assert!(glb(&glb_fixture(base.clone(),&[0;36])).is_ok());
        let mut value=base.clone();value["buffers"][0]["uri"]=json!("https://localhost/private");assert!(glb(&glb_fixture(value,&[0;36])).is_err());
        value=base.clone();value["accessors"][0]["count"]=json!(999999999);assert!(glb(&glb_fixture(value,&[0;36])).is_err());
        value=base.clone();value["bufferViews"][0]["byteOffset"]=json!(3);assert!(glb(&glb_fixture(value,&[0;36])).is_err());
        value=base;value["nodes"][0]["children"]=json!([0]);assert!(glb(&glb_fixture(value,&[0;36])).is_err());
    }
    #[test]fn stl_requires_complete_finite_triangle_records() {
        let good=b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid t\n";
        assert_eq!(stl(good).unwrap(),1);assert!(stl(&good[..good.len()-15]).is_err());
        let invalid=String::from_utf8(good.to_vec()).unwrap().replace("vertex 1 0 0","vertex NaN 0 0");assert!(stl(invalid.as_bytes()).is_err());
    }
}
