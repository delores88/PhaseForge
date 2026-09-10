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
    }
    if let Some(rows)=scene["bonds"].as_array(){bonds+=rows.len();for row in rows{if !ids.contains(row["from"].as_str().unwrap_or(""))||!ids.contains(row["to"].as_str().unwrap_or("")){bail!("scene bonds must reference existing node ids");}}}
    if bonds>20000{bail!("scene bond budget exceeded");}
    if !scene["camera"].is_null(){vector(&scene["camera"]["position"],"camera position")?;vector(&scene["camera"]["target"],"camera target")?;}
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
}
