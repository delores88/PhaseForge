//! Data-only publication of retained generated numerical states into the native player.
//! This validates representation and lineage, not the truth of a generated physical model.
use super::{LabJob, LaboratoryService};
use anyhow::{ensure, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs::OpenOptions, io::Write, path::Path};
use uuid::Uuid;
use tokio_util::sync::CancellationToken;

const LIMIT: usize = 32 * 1024 * 1024;
const VALUE_LIMIT: usize = 2_000_000;
fn hash(bytes: &[u8]) -> String {format!("{:x}", Sha256::digest(bytes))}
fn number(value: &Value) -> anyhow::Result<f64> {value.as_f64().filter(|v|v.is_finite()).context("Every state/coordinate must be finite numeric data")}
fn text<'a>(value: &'a Value, max: usize) -> anyhow::Result<&'a str> {value.as_str().filter(|v|!v.trim().is_empty()&&v.len()<=max).context("Required unit/model/scope text is absent or too long")}
fn vector(value: &Value) -> anyhow::Result<[f64;3]> {let v=value.as_array().context("Expected three coordinates")?;ensure!(v.len()==3,"Expected three coordinates");Ok([number(&v[0])?,number(&v[1])?,number(&v[2])?])}

pub fn capability() -> Value {
    json!({"schema":"phaseforge.simulation.v1","tool":"publish_simulation",
        "source":"A registered JSON output artifact from a completed same-project generated job with execution_purpose=scientific (or a historical scientific job). The publisher never executes HTML, Python, expressions or Blender source.",
        "required":{"schema":"phaseforge.simulation.v1","representation":"particle_trajectory OR scalar_field",
            "model":{"id":"bounded model identifier","description":"implemented equations/method","scope":"what was actually computed, including approximation","limitations":["explicit limitations"]},
            "time_unit":"physical time unit or explicitly defined nondimensional time",
            "frames":"At least two strictly increasing finite times. Numeric states must be retained, not interpolated in the viewer."},
        "particle":{"position_unit":"physical length or defined nondimensional length","topology":{"entities":[{"id":"stable body ID","display_radius":"positive display radius in position units","color":"optional #RRGGBB"}],"bonds":[],"boundary":"isolated"},"frames":[{"time":0,"entities":[{"id":"stable body ID","position":[0,0,0]}]}],"scope":"Isolated coordinates only in this publication contract; radii are display aids, not inferred measurements."},
        "field":{"length_unit":"um","field_unit":"explicit quantity unit","field_name":"observable","shape":["ny>=2","nx>=2"],"lengths_um":["positive Lx","positive Ly"],"boundary":"periodic or isolated","frames":[{"time":0,"values":[[0,0],[0,0]]}],"scope":"Uniform cell-centered rectangular scalar grid, row-major y,x, origin at0. Only micrometre grid coordinates are supported by this initial publication contract."},
        "limits":{"json_bytes":LIMIT,"particle_frames":10001,"field_frames":1001,"entities":10000,"numeric_state_values":VALUE_LIMIT,"chunk_frames":100},
        "result":"A separate immutable published_simulation job, result.representation, result.source_job_id and result.source_sha256. Native playback, trusted Blender video export and exact-state PNG observations reuse the validated retained numeric files. Rendering preserves declared units and isolated particle boundaries, shows generated-model provenance and does not establish model validity. Renderer size/coordinate limits still apply.",
        "scientific_validation":"not_established_by_publication; a finite-speed black-hole merger is not supplied by incoming-field approximation or by valid array formatting."})
}

struct Document { value: Value, count: usize, representation: String, bounds: Option<([f64;3],[f64;3])> }
fn validate(raw: &[u8]) -> anyhow::Result<Document> {
    ensure!(raw.len()<=LIMIT,"Publication JSON exceeds32MiB");
    let value: Value=serde_json::from_slice(raw)?;
    ensure!(value["schema"]=="phaseforge.simulation.v1","Use the data-only phaseforge.simulation.v1 publication schema");
    for key in ["id","description","scope"] {text(&value["model"][key],if key=="id"{120}else{4000})?;}
    let limitations=value["model"]["limitations"].as_array().context("Model limitations must be an explicit array")?;
    ensure!(!limitations.is_empty()&&limitations.len()<=32,"Retain1–32 model limitations; publication does not establish scientific validity");
    for entry in limitations {text(entry,1000)?;}
    text(&value["time_unit"],80)?;
    let representation=text(&value["representation"],32)?.to_owned();
    let frames=value["frames"].as_array().context("Retained numerical frames are required")?;
    ensure!((2..=10001).contains(&frames.len()),"Publication requires2–10001 numerical frames");
    let mut last=None;
    for frame in frames {let time=number(&frame["time"])?;ensure!(last.is_none_or(|prior|time>prior),"Physical times must strictly increase; repeated stills do not establish temporal sampling");last=Some(time);}
    let mut bounds=None;
    match representation.as_str() {
        "particle_trajectory"=>{
            text(&value["position_unit"],80)?;
            let topology=&value["topology"];
            ensure!(topology["boundary"]=="isolated","This particle publication supports explicitly isolated coordinates; no implicit periodic wrapping");
            ensure!(topology["bonds"].as_array().is_some_and(Vec::is_empty),"This initial particle publication supports an empty bond list");
            let entities=topology["entities"].as_array().context("Stable topology entities are required")?;
            ensure!(!entities.is_empty()&&entities.len()<=10000&&entities.len()*frames.len()*3<=VALUE_LIMIT,"Particle publication exceeds entity/value limits");
            let mut ids=BTreeSet::new();
            for entity in entities {
                ensure!(ids.insert(text(&entity["id"],120)?),"Entity IDs must be unique");
                ensure!(number(&entity["display_radius"])? >0.,"A positive display radius in position units is required");
                if let Some(color)=entity.get("color") {let color=text(color,7)?;ensure!(color.len()==7&&color.starts_with('#')&&color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),"Color must be #RRGGBB");}
            }
            let mut low=[f64::INFINITY;3];let mut high=[f64::NEG_INFINITY;3];
            for frame in frames {
                let states=frame["entities"].as_array().context("Particle frame needs numerical entities")?;
                ensure!(states.len()==entities.len(),"Every frame must retain all topology entities");
                let mut seen=BTreeSet::new();
                for state in states {
                    let id=text(&state["id"],120)?;
                    ensure!(ids.contains(id)&&seen.insert(id),"Frame IDs differ from stable topology");
                    let p=vector(&state["position"])?;
                    for axis in 0..3 {low[axis]=low[axis].min(p[axis]);high[axis]=high[axis].max(p[axis]);}
                }
            }
            bounds=Some((low,high));
        },
        "scalar_field"=>{
            ensure!(value["length_unit"]=="um","Published scalar grids use explicit micrometre coordinates; convert retained input before publication if necessary");
            text(&value["field_unit"],80)?;text(&value["field_name"],120)?;
            ensure!(matches!(value["boundary"].as_str(),Some("periodic"|"isolated")),"Declare periodic or isolated boundary");
            let shape=value["shape"].as_array().context("Grid shape is[ny,nx]")?;
            ensure!(shape.len()==2,"Grid shape is[ny,nx]");
            let ny=shape[0].as_u64().context("ny must be an integer")? as usize;
            let nx=shape[1].as_u64().context("nx must be an integer")? as usize;
            ensure!((2..=1024).contains(&ny)&&(2..=1024).contains(&nx)&&nx*ny*frames.len()<=VALUE_LIMIT&&frames.len()<=1001,"Grid publication exceeds fixed value limits or1001-frame publication bound");
            let lengths=value["lengths_um"].as_array().context("Grid lengths are[Lx,Ly] in micrometres")?;
            ensure!(lengths.len()==2&&number(&lengths[0])?>0.&&number(&lengths[1])?>0.,"Grid lengths must be positive");
            for frame in frames {
                let rows=frame["values"].as_array().context("Scalar values must be a numeric row-major matrix")?;
                ensure!(rows.len()==ny,"Frame row count differs fromny");
                for row in rows {let row=row.as_array().context("Scalar row must be an array")?;ensure!(row.len()==nx,"Frame column count differs fromnx");for value in row {number(value)?;}}
            }
        },
        _=>anyhow::bail!("Unsupported numerical representation; HTML/image/expression payloads are never executed"),
    }
    Ok(Document{count:frames.len(),value,representation,bounds})
}

fn retain(path:&Path,data:&[u8])->anyhow::Result<()> {
    if let Some(parent)=path.parent(){std::fs::create_dir_all(parent)?;}
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file)=>{file.write_all(data)?;file.sync_all()?;Ok(())},
        Err(error) if error.kind()==std::io::ErrorKind::AlreadyExists=>{
            let metadata=std::fs::symlink_metadata(path)?;
            ensure!(metadata.is_file()&&!metadata.file_type().is_symlink(),"Retained publication member is not an ordinary file");
            #[cfg(windows)] {use std::os::windows::fs::MetadataExt;ensure!(metadata.file_attributes()&0x400==0,"Retained publication contains a reparse point");}
            ensure!(std::fs::read(path)?==data,"Original publication differs; create a new immutable publication");Ok(())
        },
        Err(error)=>Err(error.into()),
    }
}
fn save(root:&Path,name:&str,value:&Value)->anyhow::Result<String>{let data=serde_json::to_vec(value)?;retain(&root.join(name),&data)?;Ok(hash(&data))}

fn npy(rows:&[Value],ny:usize,nx:usize)->anyhow::Result<Vec<u8>> {
    let mut header=format!("{{'descr': '<f8', 'fortran_order': False, 'shape': ({ny}, {nx}), }}");
    let padding=(64-(10+header.len()+1)%64)%64;header.push_str(&" ".repeat(padding));header.push('\n');
    let mut out=b"\x93NUMPY\x01\x00".to_vec();out.extend_from_slice(&(header.len() as u16).to_le_bytes());out.extend_from_slice(header.as_bytes());
    for row in rows {for value in row.as_array().context("Scalar row absent")? {out.extend_from_slice(&number(value)?.to_le_bytes());}}
    Ok(out)
}

pub fn publish(service:&LaboratoryService,session:&LabJob,target:Uuid,args:&Value,parent_token:&CancellationToken)->anyhow::Result<Value> {
    ensure!(!parent_token.is_cancelled(),"Stopped before publication");
    let source_id:Uuid=serde_json::from_value(args["source_job_id"].clone())?;
    let source=service.get(source_id)?;
    ensure!(source.project_id==session.project_id&&source.kind=="generated"&&source.state=="completed"&&!service.executing(source_id),"Publish only a completed inactive same-project generated calculation");
    service.ensure_model_study_access(source_id)?;
    ensure!(source.input["execution_purpose"]!="presentation","Presentation-only generation cannot be relabeled as a scientific calculation");
    let path=text(&args["path"],500)?;let expected=text(&args["sha256"],64)?;
    let raw=service.read_generated_artifact(source_id,path)?;
    ensure!(expected.len()==64&&hash(&raw)==expected,"Source numerical artifact differs from its supplied SHA256");
    let document=validate(&raw)?;
    let source_manifest=service.read_generated_artifact(source_id,"manifest.json")?;
    let code=service.read_generated_artifact(source_id,"source.py")?;
    ensure!(hash(&code)==hash(source.input["code"].as_str().context("Original generated source missing")?.as_bytes()),"Original executed code differs from its retained source");
    let input=json!({"engine":"generated_temporal","source_job_id":source_id,"source_path":path,"source_sha256":expected,
        "source_manifest_sha256":hash(&source_manifest),"source_code_sha256":hash(&code),"publication_version":1,
        "representation":document.representation,"model":document.value["model"]});
    let job=service.create_active_child(target,session.id,"published_simulation",&format!("Published numerical states: {}",document.value["model"]["id"].as_str().unwrap()),input.clone())?;
    ensure!(job.input==input,"Publication target is bound to another source");
    ensure!(matches!(job.state.as_str(),"queued"|"paused"|"completed"),"Publication is stopped; use a new explicit publication request");
    let token=service.acquire_child(target,parent_token)?;
    struct Release<'a>{service:&'a LaboratoryService,id:Uuid}
    impl Drop for Release<'_>{fn drop(&mut self){let _=self.service.update(self.id,|job|{if job.active(){job.state="paused".into();job.event("publication_attention","Publication did not reach its immutable completion receipt; retained source and partial members remain available.",json!({"resumable_with_same_tool_receipt":true}));}});self.service.release(self.id);}}
    let _release=Release{service,id:target};
    let stopped=||->anyhow::Result<()>{ensure!(!token.is_cancelled(),"Publication stopped; retained source and partial immutable members remain available");Ok(())};
    stopped()?;
    service.update(target,|job|{if matches!(job.state.as_str(),"queued"|"paused")&&!token.is_cancelled(){job.state="running".into();}})?;
    let root=service.directory(target);
    retain(&root.join("source-simulation.json"),&raw)?;
    retain(&root.join("source-execution-manifest.json"),&source_manifest)?;
    let d=&document.value;let frames=d["frames"].as_array().unwrap();
    let manifest=json!({"schema":"phaseforge.published-simulation.v1","engine":"generated_temporal","source":input,
        "model":d["model"],"scientific_scope":d["model"]["scope"],"scientific_validation":"not_established_by_publication",
        "validation":"Every published numerical state is finite, shape/identity checked, and at strictly increasing physical time. Original generated code and output artifact pins are retained; no new scientific computation occurred."});
    save(&root,"manifest.json",&manifest)?;
    let index_path;
    if document.representation=="particle_trajectory" {
        index_path="trajectory/index.json";
        let (low,high)=document.bounds.unwrap();
        let topology=json!({"schema_version":1,"boundary":"isolated","entities":d["topology"]["entities"],"bonds":[],"units":{"position":d["position_unit"],"time":d["time_unit"]}});
        save(&root,"topology.json",&topology)?;
        let mut chunks=vec![];
        for (n,group) in frames.chunks(100).enumerate() {
            stopped()?;
            let path=format!("trajectory/chunk-{n:05}.json");
            let digest=save(&root,&path,&json!({"frames":group}))?;
            chunks.push(json!({"path":path,"sha256":digest,"start_frame":n*100,"end_frame":n*100+group.len()-1,"frame_count":group.len(),"start_time":group.first().unwrap()["time"],"end_time":group.last().unwrap()["time"],"bounds":{"min":low,"max":high}}));
        }
        save(&root,index_path,&json!({"schema_version":1,"representation":"particle_trajectory","boundary":"isolated","time_unit":d["time_unit"],"position_unit":d["position_unit"],"frame_count":document.count,"start_time":frames.first().unwrap()["time"],"end_time":frames.last().unwrap()["time"],"chunks":chunks,"bounds":{"min":low,"max":high},"topology_path":"topology.json"}))?;
    } else {
        index_path="fields/index.json";
        let ny=d["shape"][0].as_u64().unwrap() as usize;let nx=d["shape"][1].as_u64().unwrap() as usize;
        let lx=number(&d["lengths_um"][0])?;let ly=number(&d["lengths_um"][1])?;
        let mut low=f64::INFINITY;let mut high=f64::NEG_INFINITY;let mut records=vec![];
        for (step,frame) in frames.iter().enumerate() {
            stopped()?;
            let rows=frame["values"].as_array().unwrap();
            for row in rows {for value in row.as_array().unwrap() {let v=number(value)?;low=low.min(v);high=high.max(v);}}
            let path=format!("fields/field-{step:08}.npy");let raw=npy(rows,ny,nx)?;retain(&root.join(&path),&raw)?;
            let view_path=format!("fields/view-{step:08}.json");let view_hash=save(&root,&view_path,&json!({"step":step,"time":frame["time"],"shape":d["shape"],"field_unit":d["field_unit"],"values":frame["values"]}))?;
            records.push(json!({"step":step,"time":frame["time"],"path":path,"sha256":hash(&raw),"view_path":view_path,"view_sha256":view_hash}));
        }
        // Retain the measured range, including a constant field; the viewer owns display expansion.
        let display_high=high;
        save(&root,index_path,&json!({"schema_version":1,"representation":"scalar_field","time_unit":d["time_unit"],"length_unit":"um","field_name":d["field_name"],"field_unit":d["field_unit"],"shape":[ny,nx],"lengths_um":[lx,ly],"x_um":(0..nx).map(|x|(x as f64+0.5)*lx/nx as f64).collect::<Vec<_>>(),"y_um":(0..ny).map(|y|(y as f64+0.5)*ly/ny as f64).collect::<Vec<_>>(),"axis_order":["y","x"],"grid_location":"cell_center","boundary":d["boundary"],"color_scale":{"min":low,"max":display_high},"frames":records}))?;
    }
    let index=std::fs::read(root.join(index_path))?;
    // Capability declarations describe publication-time support. A later renderer
    // must not rewrite an old immutable receipt while replaying this tool.
    let presentation_support=if root.join("result.json").is_file(){service.read_json(target,"result.json")?["presentation"].clone()}else{
        json!({"native_playback":true,"exports":true,"capture":true,"scope":"Trusted rendering of completed retained numeric data with source/model pins and declared units; no scientific rerun or model-validity certification. Renderer size/coordinate limits apply."})
    };
    let result=json!({"status":"completed","engine":"generated_temporal","representation":document.representation,"source_job_id":source_id,
        "source_path":path,"source_sha256":expected,"source_manifest_sha256":hash(&source_manifest),"source_code_sha256":hash(&code),
        "model":d["model"],"frame_count":document.count,"time_unit":d["time_unit"],"start_time":frames.first().unwrap()["time"],"end_time":frames.last().unwrap()["time"],"index_path":index_path,"index_sha256":hash(&index),
        "scientific_validation":"not_established_by_publication","scientific_rerun":false,
        "presentation":presentation_support});
    save(&root,"result.json",&result)?;
    stopped()?;
    service.update(target,|job|{if job.active()&&!token.is_cancelled(){job.state="completed".into();job.result=result.clone();job.progress=json!({"fraction":1.0});if !job.events.iter().any(|event|event.kind=="completed"){job.event("completed","Retained generated numerical states validated and published for native playback. Model validity remains unestablished.",json!({"source_job_id":source_id,"source_sha256":expected,"representation":document.representation}));}}})?;
    ensure!(service.get(target)?.state=="completed","Publication stopped before final delivery");
    Ok(json!({"job_id":target,"state":"completed","representation":document.representation,"result":result,"next":"Open the native numerical player and check_deliverable against the requested model scope. Publication is not a scientific-validity certificate."}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::AppConfig,persistence::Database,domain::{CreateProjectRequest,ResearchProject}};
    fn particle()->Value {json!({"schema":"phaseforge.simulation.v1","representation":"particle_trajectory","model":{"id":"linear-test","description":"Recorded analytic x=t","scope":"One isolated synthetic particle","limitations":["No force law validated"]},"time_unit":"s","position_unit":"m","topology":{"boundary":"isolated","entities":[{"id":"a","display_radius":0.1}],"bonds":[]},"frames":[{"time":0,"entities":[{"id":"a","position":[0,0,0]}]},{"time":1,"entities":[{"id":"a","position":[1,0,0]}]}]})}
    #[test]fn numerical_publication_rejects_stills_claimed_counts_and_changed_identity(){
        let good=particle();assert_eq!(validate(&serde_json::to_vec(&good).unwrap()).unwrap().count,2);
        for variant in 0..4 {let mut value=good.clone();match variant {0=>value["frames"][1]["time"]=json!(0),1=>value["frames"][1]["entities"][0]["id"]=json!("other"),2=>value["frames"]=json!([value["frames"][0].clone()]),_=>value["frames"][1]["entities"][0]["position"]=json!(["sin(t)",0,0])};assert!(validate(&serde_json::to_vec(&value).unwrap()).is_err());}
    }
    #[test]fn publication_isolated_units_and_field_npy_are_explicit(){
        let mut value=particle();value["topology"]["boundary"]=json!("periodic");assert!(validate(&serde_json::to_vec(&value).unwrap()).is_err());
        let binary=npy(&[json!([1.,2.]),json!([3.,4.])],2,2).unwrap();assert!(binary.starts_with(b"\x93NUMPY\x01\x00"));assert_eq!(&binary[binary.len()-8..],&4f64.to_le_bytes());
    }
    #[test]fn immutable_publication_member_cannot_be_overwritten(){let temp=tempfile::tempdir().unwrap();let path=temp.path().join("source.json");retain(&path,b"original").unwrap();retain(&path,b"original").unwrap();assert!(retain(&path,b"replacement").is_err());assert_eq!(std::fs::read(path).unwrap(),b"original");}
    fn service_fixture()->(tempfile::TempDir,LaboratoryService,LabJob,LabJob,Value){
        let temp=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:temp.path().canonicalize().unwrap(),..Default::default()};
        let db=Database::open(&config.database_path()).unwrap();let project=ResearchProject::new(CreateProjectRequest{name:Some("Publication contract fixture".into()),question:"Retain synthetic numeric states without executing source".into()});db.put_project(&project).unwrap();
        let service=LaboratoryService::new(db,config).unwrap();
        let parent=service.create(Uuid::new_v4(),project.id,None,"session","Publication fixture",json!({}),None).unwrap();
        let source=service.create(Uuid::new_v4(),project.id,Some(parent.id),"generated","Synthetic execution receipt fixture",json!({"code":"# fixed fixture source; never executed","execution_purpose":"scientific"}),None).unwrap();
        let root=service.directory(source.id);let data=serde_json::to_vec(&particle()).unwrap();
        retain(&root.join("source.py"),source.input["code"].as_str().unwrap().as_bytes()).unwrap();
        save(&root,"manifest.json",&json!({"fixture":"not actual scientific execution"})).unwrap();
        retain(&root.join("work/states.json"),&data).unwrap();save(&root,"generated-artifacts.json",&json!({"artifacts":[{"path":"work/states.json","bytes":data.len(),"sha256":hash(&data)}]})).unwrap();
        service.update(source.id,|j|j.state="completed".into()).unwrap();
        let args=json!({"source_job_id":source.id,"path":"work/states.json","sha256":hash(&data)});(temp,service,parent,source,args)
    }
    #[test]fn publication_service_replays_exact_lineage_without_new_scientific_job(){
        let (_temp,service,parent,source,args)=service_fixture();let id=Uuid::new_v4();let token=CancellationToken::new();
        let first=publish(&service,&parent,id,&args,&token).unwrap();let index=std::fs::read(service.directory(id).join("trajectory/index.json")).unwrap();
        let second=publish(&service,&parent,id,&args,&token).unwrap();assert_eq!(first,second);
        assert_eq!(first["result"]["source_job_id"],json!(source.id));assert_eq!(first["result"]["scientific_rerun"],false);
        assert_eq!(std::fs::read(service.directory(id).join("trajectory/index.json")).unwrap(),index);
        assert_eq!(service.get(id).unwrap().events.iter().filter(|event|event.kind=="completed").count(),1);
        assert!(!service.executing(id));assert_eq!(service.list(Some(parent.project_id)).unwrap().len(),3);
        std::fs::write(service.directory(source.id).join("work/states.json"),b"{}").unwrap();
        assert!(publish(&service,&parent,id,&args,&token).is_err());assert_eq!(std::fs::read(service.directory(id).join("trajectory/index.json")).unwrap(),index);
    }
    #[test]fn publication_rejects_cross_project_presentation_sources_and_precancelled_work(){
        let (_temp,service,parent,source,args)=service_fixture();let mut foreign=parent.clone();foreign.project_id=Uuid::new_v4();
        assert!(publish(&service,&foreign,Uuid::new_v4(),&args,&CancellationToken::new()).is_err());
        let token=CancellationToken::new();token.cancel();assert!(publish(&service,&parent,Uuid::new_v4(),&args,&token).is_err());
        service.update(source.id,|job|job.input["execution_purpose"]=json!("presentation")).unwrap();
        assert!(publish(&service,&parent,Uuid::new_v4(),&args,&CancellationToken::new()).is_err());assert_eq!(service.list(Some(parent.project_id)).unwrap().len(),2);
    }
}
