use super::*;
fn fixture()->(tempfile::TempDir,LaboratoryService,Uuid){
    let dir=tempfile::tempdir().unwrap();let config=AppConfig{data_directory:dir.path().into(),..Default::default()};let database=Database::open(&config.database_path()).unwrap();
    let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:None,question:"Retained source fixture".into()});database.put_project(&project).unwrap();
    let service=LaboratoryService::new(database,config).unwrap();(dir,service,project.id)
}
fn upload(name:&str,content:&str)->Attachment{Attachment::Upload(Upload{name:name.into(),mime_type:"untrusted/claimed-mime".into(),size_bytes:content.len(),content:content.into(),units:Default::default()})}
fn save(service:&LaboratoryService,project:Uuid,request:Uuid,attachments:&[Attachment])->Vec<FileRef>{let (refs,prepared)=prepare(service,project,request,None,attachments).unwrap();persist(service,project,prepared).unwrap();refs}

#[test]fn originals_hashes_units_and_missingness_are_retained_without_raw_job_payloads(){
    let (_dir,service,project)=fixture();let text="\u{feff}time,label,value\r\n0,\"a,b\",1\r\n1,μ,NA\r\n2,x,NaN\r\n";
    let mut source=upload("measurements.csv",text);if let Attachment::Upload(file)=&mut source{file.units.insert("time".into(),"s".into());}
    let refs=save(&service,project,Uuid::new_v4(),&[source]);let file=&refs[0];let (job,stored)=owned(&service,project,file.job_id).unwrap();
    assert_eq!(stored,file.clone());assert_eq!(file.mime_type,"text/csv");assert_eq!(verify_bytes(&service,&job,file).unwrap(),text.as_bytes());assert_eq!(file.sha256,digest(text.as_bytes()));
    assert!(job.input.get("content").is_none());assert!(!job.input.to_string().contains("a,b"));
    let profile=profile_file(&service,project,file.job_id).unwrap();assert_eq!(profile["profile"]["rows"],3);assert_eq!(profile["profile"]["columns"][2]["missing"],1);assert_eq!(profile["profile"]["columns"][2]["nonfinite"],1);assert_eq!(profile["profile"]["units"]["time"],"s");assert_eq!(profile["profile"]["engine_compatibility"],"not established");
}
#[test]fn all_inputs_are_validated_before_any_file_is_admitted(){
    let (_dir,service,project)=fixture();let request=Uuid::new_v4();
    assert!(prepare(&service,project,request,None,&[upload("valid.csv","x\n1\n"),upload("invalid.json","{")]).is_err());assert!(service.list(None).unwrap().is_empty());
    for source in [upload("../escape.csv","x\n1"),upload("program.py","print(1)"),upload("binary.txt","a\0b"),upload("empty.txt",""),upload("ragged.csv","x,y\n1\n"),upload("big.csv",&"x".repeat(1024*1024+1))]{assert!(prepare(&service,project,request,None,&[source]).is_err());}
    let mut forged=upload("wrong.txt","actual");if let Attachment::Upload(file)=&mut forged{file.size_bytes=1;};assert!(prepare(&service,project,request,None,&[forged]).is_err());
    assert!(serde_json::from_value::<Attachment>(json!({"name":"a.txt","content":"x","execute":true})).is_err());assert!(service.list(None).unwrap().is_empty());
}
#[test]fn retry_reuses_identity_and_rejects_changed_or_foreign_sources(){
    let (_dir,service,project)=fixture();let request=Uuid::new_v4();let input=upload("saved.txt","Original evidence");
    let first=save(&service,project,request,&[input.clone()]);let second=save(&service,project,request,&[input]);assert_eq!(first,second);assert_eq!(service.list(None).unwrap().len(),1);
    assert_eq!(service.get(first[0].job_id).unwrap().events.iter().filter(|event|event.kind=="completed").count(),1);
    assert!(prepare(&service,project,request,None,&[upload("saved.txt","Changed evidence")]).is_err());
    let other=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest{name:None,question:"Other project".into()});service.database.put_project(&other).unwrap();
    assert!(read_file(&service,other.id,first[0].job_id,0,100).is_err());assert!(prepare(&service,other.id,Uuid::new_v4(),None,&[Attachment::Reference(first[0].clone())]).is_err());
    std::fs::write(service.directory(first[0].job_id).join(&first[0].path),"Modified evidence").unwrap();assert!(read_file(&service,project,first[0].job_id,0,100).is_err());
}
#[test]fn bounded_pages_preserve_utf8_and_never_silently_replace_split_characters(){
    let (_dir,service,project)=fixture();let text="Aμ🙂\r\nB";let file=save(&service,project,Uuid::new_v4(),&[upload("utf8.txt",text)]).remove(0);
    let mut offset=0;let mut recovered=String::new();loop{let page=read_file(&service,project,file.job_id,offset,5).unwrap();recovered.push_str(page["text"].as_str().unwrap());let Some(next)=page["next_offset"].as_u64() else{break};offset=next as usize;}
    assert_eq!(recovered,text);assert!(read_file(&service,project,file.job_id,2,5).is_err());assert_eq!(list_files(&service,project,0,1).unwrap()["files"].as_array().unwrap().len(),1);
}
#[test]fn public_sources_require_exact_https_hosts_and_data_formats(){
    for value in ["http://files.rcsb.org/download/1HSG.pdb","https://127.0.0.1/source.csv","https://localhost/source.csv","https://files.rcsb.org.evil.test/source.csv","https://user:secret@files.rcsb.org/source.csv","https://files.rcsb.org:8443/source.csv","https://files.rcsb.org/source.csv?token=x","https://files.rcsb.org/source.csv#fragment","https://raw.githubusercontent.com/x/y/run.py","https://zenodo.org/file.zip"]{assert!(acquisition_url(value).is_err(),"{value}");}
    for value in ["https://files.rcsb.org/download/1HSG.pdb","https://archive.ics.uci.edu/data/measurements.csv","https://physionet.org/files/example/data.csv","https://data.nist.gov/example/data.json"]{assert!(acquisition_url(value).is_ok(),"{value}");}
}
#[tokio::test]async fn transport_rejects_redirects_streamed_oversize_and_cancellation(){
    use axum::{body::Body,http::{StatusCode,header},response::IntoResponse};
    let app=Router::new().route("/redirect",get(||async{(StatusCode::FOUND,[(header::LOCATION,"/ok")])})).route("/ok",get(||async{"x\n1\n"}))
        .route("/stream",get(||async{Body::from_stream(futures_util::stream::iter((0..3).map(|_|Ok::<_,std::io::Error>(vec![b'x';1024*1024])))).into_response()}));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base=format!("http://{}",listener.local_addr().unwrap());let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    // Only the transport unit fixture bypasses URL admission, using loopback.
    let client=download_client().unwrap();let token=CancellationToken::new();
    let redirect=client.get(format!("{base}/redirect")).send().await.unwrap();assert!(bounded_download(redirect,&token).await.unwrap_err().to_string().contains("redirects"));
    let oversized=client.get(format!("{base}/stream")).send().await.unwrap();assert!(bounded_download(oversized,&token).await.unwrap_err().to_string().contains("intake limit"));
    let response=client.get(format!("{base}/ok")).send().await.unwrap();token.cancel();assert!(bounded_download(response,&token).await.is_err());server.abort();
}
#[tokio::test]async fn received_public_snapshot_recovers_provenance_without_refetch(){
    let (_dir,service,project)=fixture();let parent=Uuid::new_v4();service.create(parent,project,None,"session","Data request",json!({}),None).unwrap();
    let source="https://raw.githubusercontent.com/example/test-fixture/main/data.csv";let target=Uuid::new_v4();let input=json!({"origin":"public_https","source_url":source,"units":{},"name":"data.csv"});
    service.create(target,project,Some(parent),"data","data.csv",input,None).unwrap();let bytes=b"time,value\n0,2\n1,4\n";
    write_json(&service.directory(target).join("download-receipt.json"),&json!({"source_url":source,"sha256":digest(bytes),"bytes_base64":base64::engine::general_purpose::STANDARD.encode(bytes),"provenance":{"origin":"isolated_test_receipt","received_at":"2026-09-11T00:00:00Z","etag":"fixture-tag"}})).unwrap();
    let token=CancellationToken::new();let result=acquire(&service,project,parent,target,&json!({"url":source}),&token).await.unwrap();
    assert_eq!(result["source"]["etag"],"fixture-tag");assert_eq!(result["source"]["source_url"],source);assert_eq!(result["file"]["sha256"],digest(bytes));assert_eq!(result["profile"]["columns"][1]["mean"],3.0);
    let again=acquire(&service,project,parent,target,&json!({"url":source}),&token).await.unwrap();assert_eq!(again["reused_snapshot"],true);assert_eq!(service.get(target).unwrap().events.iter().filter(|event|event.kind=="completed").count(),1);
}
