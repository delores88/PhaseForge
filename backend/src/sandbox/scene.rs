use anyhow::{bail, Context};
use serde_json::Value;
use std::collections::HashSet;

/// An authored, data-only scene is visual context, never an executable solver.
pub(super) fn validate(scene: &Value) -> anyhow::Result<()> {
    if serde_json::to_vec(scene)?.len() > 2 * 1024 * 1024 { bail!("scene exceeds the 2 MiB geometry budget"); }
    if scene["schema_version"] != "1.0" { bail!("scene.schema_version must be 1.0"); }
    let kinds = ["conceptual", "computed", "measured"];
    if !kinds.contains(&scene["provenance"]["kind"].as_str().unwrap_or("")) || scene["provenance"]["description"].as_str().unwrap_or("").trim().is_empty() { bail!("scene must declare its provenance kind and description"); }
    let nodes=scene["nodes"].as_array().context("scene.nodes must be an array")?;
    if nodes.len()>64 {bail!("scene supports at most 64 nodes");}
    let types=["atom","molecule","dna","virus","protein","planet","star","black_hole","field","streamlines","mesh","curve","surface","box","sphere"];
    let mut ids=HashSet::new();let(mut vertices,mut points,mut atoms,mut bonds)=(0,0,0,0);
    for node in nodes {
        let id=node["id"].as_str().context("scene node requires an id")?;
        if id.is_empty()||id.len()>100||!ids.insert(id){bail!("scene node ids must be unique, nonempty, and at most 100 bytes");}
        if !types.contains(&node["type"].as_str().unwrap_or("")){bail!("unsupported scene node type");}
        for key in ["position","rotation","scale"] {if !node[key].is_null(){ vector(&node[key],key)?; }}
        let params=&node["parameters"];
        for (key,count,max) in [("vertices",&mut vertices,24000),("points",&mut points,12000)] {
            if let Some(rows)=params[key].as_array(){*count+=rows.len();if *count>max{bail!("scene {key} geometry budget exceeded");}for row in rows{vector(row,key)?;}}
        }
        if let Some(rows)=params["atoms"].as_array(){atoms+=rows.len();if atoms>12000{bail!("scene atom budget exceeded");}for row in rows{vector(&row["position"],"atom position")?;}}
        if let Some(rows)=params["bonds"].as_array(){bonds+=rows.len();}
        if let Some(indices)=params["indices"].as_array(){
            let count=params["vertices"].as_array().map_or(0,|v|v.len());
            if indices.len()>72000||indices.len()%3!=0||indices.iter().any(|v|v.as_u64().is_none_or(|n|n>=count as u64)){bail!("scene mesh indices must reference complete valid triangles");}
        }
        for key in ["radius","length","thickness"]{if !params[key].is_null()&&params[key].as_f64().is_none_or(|n|!n.is_finite()||n<=0.0||n>1e12){bail!("scene {key} must be finite, positive, and at most 1e12");}}
        if node["type"] == "dna" { validate_dna(params)?; }
    }
    if let Some(rows)=scene["bonds"].as_array(){bonds+=rows.len();for row in rows{if !ids.contains(row["from"].as_str().unwrap_or(""))||!ids.contains(row["to"].as_str().unwrap_or("")){bail!("scene bonds must reference existing node ids");}}}
    if bonds>20000{bail!("scene bond budget exceeded");}
    if !scene["camera"].is_null(){vector(&scene["camera"]["position"],"camera position")?;vector(&scene["camera"]["target"],"camera target")?;}
    Ok(())
}
fn validate_dna(params:&Value)->anyhow::Result<()> {
    let radius=params["radius"].as_f64().unwrap_or(1.0);
    let turns=params["turns"].as_f64().unwrap_or(4.0);
    for (key,default,low,high) in [("radius",1.0,1e-8,1e12),("length",radius*7.0,1e-8,1e12),("turns",4.0,0.5,24.0),("thickness",radius*0.16,radius*1e-6,radius),("count",(turns*10.0).floor().min(160.0),1.0,160.0)] {
        let value=if params[key].is_null(){default}else{params[key].as_f64().with_context(||format!("DNA {key} must be numeric"))?};
        if !value.is_finite()||value<low||value>high||(key=="count"&&value.fract()!=0.0) {bail!("DNA {key} must be {}in [{low}, {high}]",if key=="count"{"an integer "}else{"finite "});}
    }
    if !params["colors"].is_null() {
        let colors=params["colors"].as_array().context("DNA colors must be an array")?;
        if !(2..=8).contains(&colors.len())||colors.iter().any(|value|value.as_str().is_none_or(|text|text.len()!=7||!text.starts_with('#')||!text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))) {bail!("DNA colors must contain 2–8 #RRGGBB colors: two backbones, then repeating base-half colors");}
    }
    Ok(())
}
fn vector(value:&Value,label:&str)->anyhow::Result<()>{
    let rows=value.as_array().with_context(||format!("scene {label} must have three coordinates"))?;
    if rows.len()!=3||rows.iter().any(|v|v.as_f64().is_none_or(|n|!n.is_finite()||n.abs()>1e15)){bail!("scene {label} must have three finite coordinates within ±1e15");}Ok(())
}
#[cfg(test)]mod tests{
    use super::*;use serde_json::json;
    fn scene()->Value{json!({"schema_version":"1.0","provenance":{"kind":"conceptual","description":"Illustrative surface"},"nodes":[{"id":"membrane","type":"virus","position":[0,0,0]}]})}
    #[test]fn accepts_labelled_context(){assert!(validate(&scene()).is_ok());}
    #[test]fn rejects_unbounded_or_invalid_geometry(){let mut s=scene();s["nodes"][0]["parameters"]=json!({"vertices":[[0,0,0]],"indices":[0,1,2]});assert!(validate(&s).is_err());s=scene();s["nodes"]=json!([s["nodes"][0],s["nodes"][0]]);assert!(validate(&s).is_err());}
    #[test]fn rejects_missing_provenance_and_executable_type(){let mut s=scene();s["provenance"]=Value::Null;assert!(validate(&s).is_err());s=scene();s["nodes"][0]["type"]=json!("javascript");assert!(validate(&s).is_err());}
    #[test]fn dna_requires_supported_exact_geometry_and_color_parameters(){let mut s=scene();s["nodes"][0]["type"]=json!("dna");s["nodes"][0]["parameters"]=json!({"radius":2.25,"length":10.0,"turns":2.6,"count":24,"thickness":0.19,"colors":["#2878D0","#E5B94C","#E95678","#43D6C5"]});assert!(validate(&s).is_ok());for (key,value) in [("count",json!(24.5)),("count",json!(161)),("turns",json!(25)),("thickness",json!(2.3)),("colors",json!(["blue","gold"])),("colors",json!(["#123456"]))] {let mut bad=s.clone();bad["nodes"][0]["parameters"][key]=value;assert!(validate(&bad).is_err(),"{key}");}}
}
