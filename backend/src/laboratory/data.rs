//! Immutable project data. Intake verifies allocation/format bounds, not scientific validity.
use super::*;
use sha2::{Digest,Sha256};
use std::collections::BTreeMap;
use axum::{Router,Json,extract::{State,Path as ApiPath,Query},routing::get};
use base64::Engine;

pub const MAX_FILES:usize=8;
pub const MAX_FILE:usize=2*1024*1024;
pub const MAX_BATCH:usize=8*1024*1024;
pub const TEXT_FORMATS:&[&str]=&["txt","md","json","csv","toml","yaml","yml","pdb","sdf","mol","xyz"];

#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Upload {
    pub name:String,#[serde(default)]pub mime_type:String,#[serde(default)]pub size_bytes:usize,pub content:String,
    #[serde(default)]pub units:BTreeMap<String,String>,
}
#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    pub job_id:Uuid,pub path:String,pub name:String,pub sha256:String,pub size_bytes:usize,pub mime_type:String,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(untagged)]
pub enum Attachment { Upload(Upload), Reference(FileRef) }
pub struct PreparedFile { id:Uuid,parent:Option<Uuid>,input:Value,bytes:Vec<u8>,profile:Value,file:FileRef }

fn digest(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn file_id(request:Uuid,index:usize)->Uuid{
    let hash=Sha256::digest(format!("phaseforge-project-file-v1:{request}:{index}"));
    let mut bytes:[u8;16]=hash[..16].try_into().unwrap();bytes[6]=(bytes[6]&15)|64;bytes[8]=(bytes[8]&63)|128;Uuid::from_bytes(bytes)
}
fn name_format(name:&str)->anyhow::Result<&str>{
    anyhow::ensure!(!name.is_empty()&&name.len()<=180&&!name.chars().any(|c|c.is_control()||"<>:\"/\\|?*".contains(c))&&!name.ends_with([' ','.']),"Use a plain filename of at most 180 bytes without paths or control characters");
    let ext=name.rsplit('.').next().context("File extension required")?;
    anyhow::ensure!(TEXT_FORMATS.contains(&ext.to_ascii_lowercase().as_str()),"Supported project files: {}. Executables, scripts, archives and binary objects are not accepted.",TEXT_FORMATS.join(", "));
    Ok(ext)
}
fn validate_text(name:&str,bytes:&[u8],units:&BTreeMap<String,String>)->anyhow::Result<(String,Value)>{
    let ext=name_format(name)?.to_ascii_lowercase();
    anyhow::ensure!(!bytes.is_empty()&&bytes.len()<=MAX_FILE,"Files must contain 1 byte to 2 MiB");
    let text=std::str::from_utf8(bytes).context("Project files must be valid UTF-8 text; binary or damaged encodings are not silently converted")?;
    anyhow::ensure!(!text.contains('\0'),"Binary NUL bytes are not accepted as text data");
    anyhow::ensure!(units.len()<=128&&units.iter().all(|(key,value)|!key.trim().is_empty()&&key.len()<=160&&!value.trim().is_empty()&&value.len()<=80&&!key.chars().chain(value.chars()).any(char::is_control)),"Declare at most 128 bounded column/unit labels");
    let mut profile=match ext.as_str(){
        "csv"=>{
            anyhow::ensure!(!text.trim_start_matches('\u{feff}').trim_start().starts_with('<')&&!text.starts_with("#!"),"The file is markup or executable content, not CSV");
            let mut profile=crate::research::data::profile(text)?;
            profile["scope"]=json!("Descriptive statistics from retained original bytes. Missing, nonfinite and nonnumeric values remain explicit. No rows are silently removed; this is not causal or predictive validation.");
            for key in units.keys(){anyhow::ensure!(profile["columns"].as_array().unwrap().iter().any(|column|column["name"]==*key),"Unit label {key} does not match a CSV column");}
            profile
        },
        "json"=>{
            let value:Value=serde_json::from_slice(bytes).context("Malformed JSON data")?;
            anyhow::ensure!(value.is_object()||value.is_array(),"JSON data must be an object or array");
            let mut pending=vec![(&value,0)];let mut count=0;
            while let Some((value,depth))=pending.pop(){count+=1;anyhow::ensure!(count<=250000&&depth<=64,"JSON exceeds the structural complexity budget");match value{Value::Object(map)=>pending.extend(map.values().map(|value|(value,depth+1))),Value::Array(rows)=>pending.extend(rows.iter().map(|value|(value,depth+1))),_=>{}}}
            json!({"format":"json","top_level":if value.is_array(){"array"}else{"object"},"elements":value.as_array().map(Vec::len),"schema_status":"JSON syntax checked; engine schema and scientific meaning unspecified"})
        },
        _=>json!({"format":ext,"schema_status":"Retained UTF-8 text; scientific format/engine compatibility must be checked before use","lines":text.lines().count()}),
    };
    profile["format"]=json!(ext);
    if ext=="csv"{profile["schema_status"]=json!("Headers and rectangular CSV records checked; engine column mappings still require validation");}
    profile["units"]=json!(units);profile["units_status"]=json!(if units.is_empty(){"unspecified; ask for or establish units before quantitative use"}else{"user-declared; not independently verified"});
    profile["engine_compatibility"]=json!("not established");
    let mime=match ext.as_str(){"csv"=>"text/csv","json"=>"application/json","md"=>"text/markdown",_=>"text/plain"};
    Ok((mime.into(),profile))
}

pub fn prepare(service:&LaboratoryService,project:Uuid,request:Uuid,parent:Option<Uuid>,attachments:&[Attachment])->anyhow::Result<(Vec<FileRef>,Vec<PreparedFile>)>{
    service.database.get_project(project)?.context("Project not found")?;
    anyhow::ensure!(attachments.len()<=MAX_FILES,"A message accepts at most eight files");
    let mut total=0;let mut refs=vec![];let mut prepared=vec![];
    for (index,attachment) in attachments.iter().enumerate(){
        match attachment{
            Attachment::Reference(reference)=>{
                let (job,file)=owned(service,project,reference.job_id)?;
                anyhow::ensure!(&file==reference,"File reference does not match its retained provenance");
                verify_bytes(service,&job,&file)?;total+=file.size_bytes;refs.push(file);
            },
            Attachment::Upload(upload)=>{
                let bytes=upload.content.as_bytes();total+=bytes.len();
                anyhow::ensure!(upload.size_bytes==0||upload.size_bytes==bytes.len(),"Attachment {} has a different actual UTF-8 byte count",upload.name);
                let (mime,profile)=validate_text(&upload.name,bytes,&upload.units)?;
                let id=file_id(request,index);let path=format!("source.{}",name_format(&upload.name)?.to_ascii_lowercase());
                let file=FileRef{job_id:id,path,name:upload.name.clone(),sha256:digest(bytes),size_bytes:bytes.len(),mime_type:mime};
                let input=json!({"origin":"user_upload","file":file,"units":upload.units,"request_id":request,"attachment_index":index});
                if let Some(existing)=service.database.lab_record(id)?{anyhow::ensure!(existing.kind=="data"&&existing.project_id==project&&existing.input==input&&existing.parent_id==parent,"File request ID is already bound to different data");}
                refs.push(file.clone());prepared.push(PreparedFile{id,parent,input,bytes:bytes.to_vec(),profile,file});
            },
        }
        anyhow::ensure!(total<=MAX_BATCH,"Combined attachments exceed 8 MiB");
    }
    Ok((refs,prepared))
}
pub fn persist(service:&LaboratoryService,project:Uuid,files:Vec<PreparedFile>)->anyhow::Result<()> {
    // Every input has been validated before the first write. I/O failures retain
    // recoverable records but never start an agent or silently accept part of a batch.
    for file in files{
        let job=service.create(file.id,project,file.parent,"data",&file.file.name,file.input,None)?;
        if job.state=="completed"{verify_bytes(service,&job,&file.file)?;continue;}
        save_snapshot(service,&job,&file.file,&file.bytes,&file.profile,json!({"origin":"user_upload","received_at":job.created_at,"source_name":file.file.name}))?;
    }
    Ok(())
}
fn save_snapshot(service:&LaboratoryService,job:&LabJob,file:&FileRef,bytes:&[u8],profile:&Value,provenance:Value)->anyhow::Result<()> {
    let _guard=service.gate.lock();let mut latest=service.get(job.id)?;
    if latest.state=="completed"{let (_,retained)=owned(service,job.project_id,job.id)?;anyhow::ensure!(&retained==file,"Source identity changed during intake");verify_bytes(service,&latest,&retained)?;return Ok(());}
    anyhow::ensure!(digest(bytes)==file.sha256&&bytes.len()==file.size_bytes,"Source bytes do not match provenance");
    let target=service.directory(job.id).join(&file.path);
    if target.exists(){anyhow::ensure!(digest(&std::fs::read(&target)?)==file.sha256,"A retained file changed; no source bytes were overwritten");}
    else{let temp=service.directory(job.id).join(format!("{}.tmp",Uuid::new_v4()));std::fs::write(&temp,bytes)?;std::fs::rename(temp,&target)?;}
    write_json(&service.directory(job.id).join("provenance.json"),&json!({"file":file,"source":provenance,"profile":profile,"scientific_validity":"not established"}))?;
    latest.result=json!({"file":file,"profile":profile,"source":provenance});latest.state="completed".into();latest.error=None;
    latest.event("completed","Original data bytes and source provenance are retained. Scientific suitability still requires validation.",json!({"file":file}));latest.completed_at=Some(latest.updated_at);service.database.put_lab_record(&latest)?;
    Ok(())
}
pub fn owned(service:&LaboratoryService,project:Uuid,id:Uuid)->anyhow::Result<(LabJob,FileRef)>{
    let job=service.get(id)?;anyhow::ensure!(job.project_id==project&&job.kind=="data"&&job.state=="completed","Choose a completed data file belonging to this project");
    let file:FileRef=serde_json::from_value(job.result["file"].clone()).context("Saved file provenance is incomplete")?;
    anyhow::ensure!(file.job_id==id,"File identity does not match its job");super::safe_relative(&file.path)?;Ok((job,file))
}
fn verify_bytes(service:&LaboratoryService,job:&LabJob,file:&FileRef)->anyhow::Result<Vec<u8>>{
    let path=service.path(job.id,&file.path)?;anyhow::ensure!(std::fs::metadata(&path)?.len()==file.size_bytes as u64&&file.size_bytes<=MAX_FILE,"Retained data size changed or exceeds its bound");
    let bytes=std::fs::read(path)?;anyhow::ensure!(digest(&bytes)==file.sha256,"Retained data hash changed; do not use this source");Ok(bytes)
}
pub fn list_files(service:&LaboratoryService,project:Uuid,offset:usize,limit:usize)->anyhow::Result<Value>{
    service.database.get_project(project)?.context("Project not found")?;
    let jobs=service.list(Some(project))?.into_iter().filter(|job|job.kind=="data"&&job.state=="completed").collect::<Vec<_>>();
    let limit=limit.clamp(1,50);let entries=jobs.iter().skip(offset).take(limit).map(|job|json!({"file":job.result["file"],"source_url":job.result["source"]["source_url"],"origin":job.result["source"]["origin"],"profile":{"format":job.result["profile"]["format"],"rows":job.result["profile"]["rows"],"column_count":job.result["profile"]["columns"].as_array().map(Vec::len),"units_status":job.result["profile"]["units_status"],"engine_compatibility":"not established"},"created_at":job.created_at})).collect::<Vec<_>>();
    Ok(json!({"files":entries,"total":jobs.len(),"next_offset":if offset+limit<jobs.len(){Some(offset+limit)}else{None}}))
}
pub fn read_file(service:&LaboratoryService,project:Uuid,id:Uuid,offset:usize,max_bytes:usize)->anyhow::Result<Value>{
    let (job,file)=owned(service,project,id)?;let bytes=verify_bytes(service,&job,&file)?;let text=std::str::from_utf8(&bytes)?;
    anyhow::ensure!(offset<=bytes.len()&&text.is_char_boundary(offset),"Read offset must be a UTF-8 boundary from the preceding next_offset");
    let mut end=offset.saturating_add(max_bytes.clamp(1,60000)).min(bytes.len());while !text.is_char_boundary(end){end-=1;}
    anyhow::ensure!(end>offset||offset==bytes.len(),"Read limit is too small for the next UTF-8 character");
    Ok(json!({"file":file,"offset":offset,"next_offset":if end<bytes.len(){Some(end)}else{None},"text":&text[offset..end],"complete":offset==0&&end==bytes.len(),"source":job.result["source"],"untrusted_data":true}))
}
pub fn profile_file(service:&LaboratoryService,project:Uuid,id:Uuid)->anyhow::Result<Value>{
    let (job,file)=owned(service,project,id)?;verify_bytes(service,&job,&file)?;
    Ok(json!({"file":file,"profile":job.result["profile"],"source":job.result["source"],"scientific_validity":"Descriptive intake only. Units, schema mapping, controls and engine suitability require explicit validation."}))
}

pub fn acquisition_url(value:&str)->anyhow::Result<url::Url>{
    let url=crate::research::assets::public_data_url(value)?;
    let name=url.path_segments().and_then(|parts|parts.last()).context("Choose a complete source file URL")?;
    let extension=name_format(name)?.to_ascii_lowercase();
    anyhow::ensure!(["csv","json","pdb"].contains(&extension.as_str()),"Public numerical acquisition accepts CSV, JSON and PDB source files");Ok(url)
}
async fn bounded_download(mut response:reqwest::Response,token:&CancellationToken)->anyhow::Result<(Vec<u8>,Value)>{
    anyhow::ensure!(response.status().is_success(),"Public data source returned HTTP {}; redirects are not followed",response.status());
    anyhow::ensure!(response.content_length().is_none_or(|bytes|bytes<=MAX_FILE as u64),"Public data exceeds the 2 MiB source-file limit");
    let provenance=json!({"origin":"public_https","final_url":response.url().as_str(),"received_at":Utc::now(),"http_status":response.status().as_u16(),
        "etag":response.headers().get("etag").and_then(|v|v.to_str().ok()),"last_modified":response.headers().get("last-modified").and_then(|v|v.to_str().ok()),
        "scientific_validity":"Source authenticity, units, licensing and suitability must be checked; an allowed host is not a scientific validation"});
    let mut bytes=vec![];
    loop{let chunk=tokio::select!{biased;_=token.cancelled()=>bail!("Data acquisition stopped"),chunk=response.chunk()=>chunk?};let Some(chunk)=chunk else{break};anyhow::ensure!(bytes.len().saturating_add(chunk.len())<=MAX_FILE,"Public response exceeded its bounded intake limit");bytes.extend_from_slice(&chunk);}
    Ok((bytes,provenance))
}
fn download_client()->anyhow::Result<reqwest::Client>{Ok(reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none()).connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(30)).user_agent("PhaseForge scientific data intake").build()?)}
pub async fn acquire(service:&LaboratoryService,project:Uuid,parent:Uuid,target:Uuid,args:&Value,token:&CancellationToken)->anyhow::Result<Value>{
    anyhow::ensure!(!token.is_cancelled(),"Stopped before public data acquisition");
    let parent_job=service.get(parent)?;anyhow::ensure!(parent_job.project_id==project&&parent_job.active(),"Data acquisition requires this project's active parent session");
    let source=args["url"].as_str().context("An explicit public source URL is required")?;let url=acquisition_url(source)?;
    let units:BTreeMap<String,String>=args.get("units").cloned().map(serde_json::from_value).transpose()?.unwrap_or_default();
    let name=url.path_segments().and_then(|parts|parts.last()).unwrap().to_string();
    // Validate metadata before creating an intake job or using the network.
    anyhow::ensure!(units.len()<=128&&units.iter().all(|(key,value)|!key.trim().is_empty()&&key.len()<=160&&!value.trim().is_empty()&&value.len()<=80&&!key.chars().chain(value.chars()).any(char::is_control)),"Invalid declared units");
    let input=json!({"origin":"public_https","source_url":url.as_str(),"units":units,"name":name});
    let job=service.create(target,project,Some(parent),"data",&name,input,None)?;
    if job.state=="completed"{let (_,file)=owned(service,project,target)?;verify_bytes(service,&job,&file)?;return Ok(json!({"file":file,"source":job.result["source"],"profile":job.result["profile"],"reused_snapshot":true}));}
    let outcome=async{
        let receipt_path=service.directory(target).join("download-receipt.json");
        let (bytes,mut provenance)=if receipt_path.is_file(){
            let receipt:Value=serde_json::from_slice(&std::fs::read(&receipt_path)?)?;
            anyhow::ensure!(receipt["source_url"]==url.as_str(),"Saved download receipt belongs to a different URL");
            let bytes=base64::engine::general_purpose::STANDARD.decode(receipt["bytes_base64"].as_str().context("Saved download bytes are missing")?)?;
            anyhow::ensure!(bytes.len()<=MAX_FILE&&receipt["sha256"]==digest(&bytes),"Saved source receipt hash or bounds are invalid");(bytes,receipt["provenance"].clone())
        }else{
            service.event(target,"acquiring_data","Retrieving the explicitly selected public source file with bounded HTTPS intake.",json!({"source_url":url.as_str()}))?;
            let client=download_client()?;
            let response=tokio::select!{biased;_=token.cancelled()=>bail!("Stopped before source response arrived"),response=client.get(url.clone()).send()=>response?};
            let (bytes,provenance)=bounded_download(response,token).await?;
            write_json(&receipt_path,&json!({"source_url":url.as_str(),"sha256":digest(&bytes),"provenance":provenance,"bytes_base64":base64::engine::general_purpose::STANDARD.encode(&bytes)}))?;(bytes,provenance)
        };
        let (mime,profile)=validate_text(&name,&bytes,&units)?;
        provenance["source_url"]=json!(url.as_str());
        let file=FileRef{job_id:target,path:format!("source.{}",name_format(&name)?.to_ascii_lowercase()),name:name.clone(),sha256:digest(&bytes),size_bytes:bytes.len(),mime_type:mime};
        save_snapshot(service,&job,&file,&bytes,&profile,provenance.clone())?;
        Ok::<_,anyhow::Error>(json!({"file":file,"source":provenance,"profile":profile,"reused_snapshot":false}))
    }.await;
    if let Err(error)=&outcome{service.update(target,|job|{if job.active(){job.state=if token.is_cancelled(){"paused"}else{"failed"}.into();}job.error=Some(format!("{error:#}"));job.event("intake_failed",format!("{error:#}"),json!({"original_bytes_retained":service.directory(target).join("download-receipt.json").is_file()}));})?;}
    outcome
}

#[derive(Deserialize)]pub struct UploadBatch{#[serde(default)]pub request_id:Option<Uuid>,pub files:Vec<Attachment>}
#[derive(Deserialize)]struct Page{#[serde(default)]offset:usize,limit:Option<usize>,max_bytes:Option<usize>}
pub fn routes()->Router<Arc<crate::app::AppState>>{Router::new()
    .route("/api/projects/:project/laboratory/files",get(list_route).post(upload_route))
    .route("/api/projects/:project/laboratory/files/:id/content",get(read_route))
    .route("/api/projects/:project/laboratory/files/:id/profile",get(profile_route))}
async fn upload_route(State(state):State<Arc<crate::app::AppState>>,ApiPath(project):ApiPath<Uuid>,Json(request):Json<UploadBatch>)->Result<Json<Value>,super::api::Error>{
    let id=request.request_id.unwrap_or_else(Uuid::new_v4);let (files,prepared)=prepare(&state.laboratory,project,id,None,&request.files)?;persist(&state.laboratory,project,prepared)?;Ok(Json(json!({"files":files,"request_id":id})))
}
async fn list_route(State(state):State<Arc<crate::app::AppState>>,ApiPath(project):ApiPath<Uuid>,Query(page):Query<Page>)->Result<Json<Value>,super::api::Error>{Ok(Json(list_files(&state.laboratory,project,page.offset,page.limit.unwrap_or(20))?))}
async fn read_route(State(state):State<Arc<crate::app::AppState>>,ApiPath((project,id)):ApiPath<(Uuid,Uuid)>,Query(page):Query<Page>)->Result<Json<Value>,super::api::Error>{Ok(Json(read_file(&state.laboratory,project,id,page.offset,page.max_bytes.unwrap_or(18000))?))}
async fn profile_route(State(state):State<Arc<crate::app::AppState>>,ApiPath((project,id)):ApiPath<(Uuid,Uuid)>)->Result<Json<Value>,super::api::Error>{Ok(Json(profile_file(&state.laboratory,project,id)?))}

#[cfg(test)]
#[path="data_tests.rs"]
mod tests;
