use super::*;

const PDB: &str = "HEADER    SYNTHETIC TWO-CHAIN TEST\nATOM      1  N   GLY C   1      10.000   2.000  -3.000  1.00 20.00           N  \nATOM      2  CA  GLY C   1      11.450   2.000  -3.000  1.00 20.00           C  \nATOM      3  N   ALA D   8      20.000   5.000   4.000  1.00 20.00           N  \nATOM      4  CA  ALA D   8      21.450   5.000   4.000  1.00 20.00           C  \nEND\n";

fn fixture(text: &str) -> (tempfile::TempDir, LaboratoryService, LabJob, Value) {
    let dir = tempfile::tempdir().unwrap();
    let config = AppConfig { data_directory:dir.path().into(), ..Default::default() };
    let database = Database::open(&config.database_path()).unwrap();
    let project = crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest { name:Some("Trusted PDB fixture".into()), question:"Preserve exact coordinates".into() });
    database.put_project(&project).unwrap();
    let service = LaboratoryService::new(database, config).unwrap();
    let parent = service.create(Uuid::new_v4(), project.id, None, "session", "Import an existing structure", json!({"time_limit_seconds":null}), None).unwrap();
    let upload = data::Attachment::Upload(data::Upload { name:"fixture.pdb".into(), mime_type:String::new(), size_bytes:text.len(), content:text.into(), units:Default::default() });
    let (files, prepared) = data::prepare(&service, project.id, Uuid::new_v4(), Some(parent.id), &[upload]).unwrap();
    data::persist(&service, project.id, prepared).unwrap();
    let file = &files[0];
    let args = json!({"job_id":file.job_id,"path":file.path,"sha256":file.sha256,"units":"angstrom","name":"Two-chain synthetic fixture"});
    (dir, service, parent, args)
}

fn prepare_boundary(service: &LaboratoryService, parent: &LabJob, args: &Value, target: Uuid) -> (LabJob, Value) {
    let request: Request = serde_json::from_value(args.clone()).unwrap();
    let (source, file, bytes) = retained_source(service, parent.project_id, &request).unwrap();
    let job = admit(service, parent, target, &request, &source, &file, &CancellationToken::new()).unwrap();
    let receipt = make_receipt(&job, &bytes).unwrap();
    write_json(&service.directory(target).join("import-receipt.json"), &receipt).unwrap();
    (job, receipt)
}

#[test]
fn retained_pdb_import_preserves_chains_absolute_coordinates_and_one_identity() {
    let (_dir, service, parent, args) = fixture(PDB); let id = Uuid::new_v4(); let token = CancellationToken::new();
    let result = import(&service, &parent, id, &args, &token).unwrap();
    let again = import(&service, &parent, id, &args, &token).unwrap(); assert_eq!(again, result);
    assert_eq!(result["units"], "angstrom"); assert_eq!(result["atom_count"], 4); assert_eq!(result["chain_count"], 2); assert_eq!(result["coordinate_residue_count"], 2);
    assert_eq!(result["chains"][0]["author_chain_id"], "C"); assert_eq!(result["chains"][1]["author_chain_id"], "D");
    let structures = service.database.list_molecules(Some(parent.project_id), 10).unwrap(); assert_eq!(structures.len(), 1);
    assert_eq!(structures[0].id, structure_id(id)); assert_eq!(structures[0].atoms[0].position, [10.,2.,-3.]); assert_eq!(structures[0].atoms[2].position, [20.,5.,4.]);
    assert!(!structures[0].warnings.is_empty());
    let saved = service.get(id).unwrap(); assert_eq!(saved.deadline_at, None); assert_eq!(saved.parent_id, Some(parent.id)); assert_eq!(saved.events.iter().filter(|event| event.kind == "completed").count(), 1);
    assert_eq!(saved.result["source"]["sha256"], args["sha256"]); assert!(!service.executing(id));
    assert!(!service.list(Some(parent.project_id)).unwrap().iter().any(|job| job.kind == "solver" || job.kind == "generated"));
    let mut changed = args.clone(); changed["name"] = json!("Different request"); assert!(import(&service, &parent, id, &changed, &token).is_err());
}

#[test]
fn retained_pdb_import_rejects_foreign_incomplete_wrong_hash_units_path_and_links() {
    let (_dir, service, parent, args) = fixture(PDB); let token = CancellationToken::new();
    for (key, value) in [("sha256",json!("0".repeat(64))),("units",json!("nm")),("path",json!("../source.pdb")),("chains",json!(["C"]))] {
        let mut bad = args.clone(); bad[key] = value; assert!(import(&service,&parent,Uuid::new_v4(),&bad,&token).is_err());
    }
    let source = Uuid::parse_str(args["job_id"].as_str().unwrap()).unwrap();
    let mut foreign = parent.clone(); foreign.project_id = Uuid::new_v4(); assert!(import(&service,&foreign,Uuid::new_v4(),&args,&token).is_err());
    service.update(source, |job| job.state="paused".into()).unwrap(); assert!(import(&service,&parent,Uuid::new_v4(),&args,&token).is_err());
    service.update(source, |job| {job.state="completed".into();job.result["profile"]["units"]=json!({"coordinates":"nm"});}).unwrap(); assert!(import(&service,&parent,Uuid::new_v4(),&args,&token).is_err());
    service.update(source, |job| job.result["profile"]["units"]=json!({})).unwrap();
    let path=service.directory(source).join("source.pdb"); let alias=service.directory(source).join("alias.pdb"); std::fs::hard_link(&path,&alias).unwrap();
    #[cfg(windows)] assert!(import(&service,&parent,Uuid::new_v4(),&args,&token).is_err());
    std::fs::remove_file(alias).unwrap(); std::fs::write(&path,PDB.replace("10.000","99.000")).unwrap(); assert!(import(&service,&parent,Uuid::new_v4(),&args,&token).is_err());
    assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
}

#[test]
fn retained_pdb_import_parser_failures_leave_no_registered_structure() {
    for text in ["HEADER NOT COORDINATES\nEND\n".to_owned(), PDB.replace("10.000", "broken"), PDB.replace("10.000", "   NaN"), PDB.replace("ATOM  ", "HETATM").replace("  N   GLY", "  氮  GLY")] {
        let (_dir,service,parent,args)=fixture(&text); let id=Uuid::new_v4();
        assert!(import(&service,&parent,id,&args,&CancellationToken::new()).is_err());
        assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
        assert_eq!(service.get(id).unwrap().state,"failed"); assert!(!service.executing(id));
        assert!(!service.directory(id).join("import-receipt.json").exists());
    }
    for prefix in [" ATOM ", "ATOM\t "] {
        let text=PDB.replace("ATOM      3  N   ALA",&format!("{prefix}    3  氮  ZZZ"));
        let (_dir,service,parent,args)=fixture(&text); let id=Uuid::new_v4();
        assert!(import(&service,&parent,id,&args,&CancellationToken::new()).unwrap_err().to_string().contains("ASCII"));
        assert!(!service.executing(id)); assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
    }
}

#[test]
fn retained_pdb_import_recovers_receipt_and_database_commit_boundaries_without_duplicates() {
    for boundary in ["receipt", "registered", "terminal_write_error"] {
        let (_dir,service,parent,args)=fixture(PDB); let id=Uuid::new_v4(); let (job,receipt)=prepare_boundary(&service,&parent,&args,id);
        if boundary != "receipt" {
            let bytes=std::fs::read(service.directory(id).join("import-receipt.json")).unwrap();
            service.update(id,|job|job.progress["import_receipt_sha256"]=json!(digest(&bytes))).unwrap();
            let structure:MolecularStructure=serde_json::from_value(receipt["structure"].clone()).unwrap(); service.database.put_molecule(&structure).unwrap();
        }
        if boundary == "terminal_write_error" { service.update(id,|job|{job.state="failed".into();job.error=Some("Retained fixture: terminal database write failed after molecule insertion".into());}).unwrap(); }
        let original=std::fs::read(service.directory(id).join("import-receipt.json")).unwrap();
        // Simulate restart normalization, then the actual explicit parent resume.
        let recovered=LaboratoryService::new(service.database.clone(),service.config.clone()).unwrap();
        recovered.update(parent.id,|job|job.state="running".into()).unwrap();
        let result=import(&recovered,&parent,id,&args,&CancellationToken::new()).unwrap();
        assert_eq!(result["structure_id"],json!(structure_id(job.id))); assert_eq!(std::fs::read(recovered.directory(id).join("import-receipt.json")).unwrap(),original);
        assert_eq!(recovered.database.list_molecules(Some(parent.project_id),10).unwrap().len(),1);
        assert_eq!(recovered.get(id).unwrap().events.iter().filter(|event|event.kind=="completed").count(),1);
        assert_eq!(import(&recovered,&parent,id,&args,&CancellationToken::new()).unwrap(),result);
    }
}

#[test]
fn retained_pdb_import_cannot_publish_after_parent_stop_deadline_or_child_cancellation() {
    for state in ["paused","cancelled","completed","expired","token"] {
        let (_dir,service,parent,args)=fixture(PDB); let id=Uuid::new_v4(); let (_job,receipt)=prepare_boundary(&service,&parent,&args,id); let token=CancellationToken::new();
        if state=="expired" {service.update(parent.id,|job|job.deadline_at=Some(Utc::now()-chrono::Duration::seconds(1))).unwrap();}
        else if state=="token" {token.cancel();} else {service.update(parent.id,|job|job.state=state.into()).unwrap();}
        assert!(commit(&service,&parent,id,&receipt,&token).is_err()); assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
        assert!(import(&service,&parent,Uuid::new_v4(),&args,&token).is_err());
    }
    let (_dir,service,parent,args)=fixture(PDB); let id=Uuid::new_v4(); prepare_boundary(&service,&parent,&args,id); service.stop(id,"cancelled").unwrap();
    assert!(import(&service,&parent,id,&args,&CancellationToken::new()).is_err()); assert_eq!(service.get(id).unwrap().state,"cancelled");
    assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
}

#[test]
fn retained_pdb_import_rejects_corrupt_receipts_and_registered_coordinates() {
    for committed in [false,true] {
        let (_dir,service,parent,args)=fixture(PDB); let id=Uuid::new_v4();
        if committed {import(&service,&parent,id,&args,&CancellationToken::new()).unwrap();}
        else {prepare_boundary(&service,&parent,&args,id);}
        let path=service.directory(id).join("import-receipt.json"); let mut receipt:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        receipt["structure"]["atoms"][0]["position"][0]=json!(999.);
        write_json(&path,&receipt).unwrap(); assert!(import(&service,&parent,id,&args,&CancellationToken::new()).is_err());
    }
    let (_dir,service,parent,args)=fixture(PDB); let id=Uuid::new_v4(); import(&service,&parent,id,&args,&CancellationToken::new()).unwrap();
    let mut structure=service.database.get_molecule(structure_id(id)).unwrap().unwrap(); structure.atoms[0].position[0]=999.; service.database.put_molecule(&structure).unwrap();
    assert!(import(&service,&parent,id,&args,&CancellationToken::new()).is_err());
}

#[test]
fn retained_pdb_import_reservation_and_budget_remain_immutable_on_replay() {
    let (_dir,service,parent,args)=fixture(PDB); let id=Uuid::new_v4();
    let deadline=Utc::now()+chrono::Duration::minutes(5);
    service.update(parent.id,|job|job.deadline_at=Some(deadline)).unwrap();
    prepare_boundary(&service,&parent,&args,id);
    let bytes=std::fs::read(service.directory(id).join("import-receipt.json")).unwrap();
    let token=CancellationToken::new(); service.acquire_child(id,&token).unwrap();
    assert!(import(&service,&parent,id,&args,&token).is_err());
    assert_eq!(std::fs::read(service.directory(id).join("import-receipt.json")).unwrap(),bytes);
    assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
    service.release(id);
    let first=import(&service,&parent,id,&args,&token).unwrap(); assert_eq!(service.get(id).unwrap().deadline_at,Some(deadline));
    service.stop(parent.id,"paused").unwrap(); let before=serde_json::to_value(service.get(id).unwrap()).unwrap();
    assert_eq!(import(&service,&parent,id,&args,&token).unwrap(),first);
    assert_eq!(serde_json::to_value(service.get(id).unwrap()).unwrap(),before);
}

#[test]
fn retained_pdb_import_dense_connectivity_stops_before_unbounded_receipt_expansion() {
    // Two dense groups one angstrom apart imply 250,000 candidate bonds.
    // This is an intentionally invalid synthetic input, not a biological model.
    let mut dense=String::from("HEADER    DENSE SYNTHETIC BOND-CAP FIXTURE\n");
    for index in 0..1000 {
        dense.push_str(&format!("ATOM  {:>5}  C   GLY A{:>4}    {:>8.3}{:>8.3}{:>8.3}  1.00 20.00           C  \n",index+1,index+1,if index<500 {0.0} else {1.0},0.0,0.0));
    }
    let (_dir,service,parent,args)=fixture(&dense); let id=Uuid::new_v4();
    let error=import(&service,&parent,id,&args,&CancellationToken::new()).unwrap_err();
    assert!(error.to_string().contains("100000-bond import limit"),"{error:#}");
    assert_eq!(service.get(id).unwrap().state,"failed"); assert!(!service.executing(id));
    assert!(!service.directory(id).join("import-receipt.json").exists());
    assert!(service.database.list_molecules(Some(parent.project_id),10).unwrap().is_empty());
    // The bounded entry point uses the same parser and preserves valid fields.
    let request=||ImportStructureRequest{project_id:Some(parent.project_id),name:"Coordinate comparison".into(),format:MolecularFormat::Pdb,content:PDB.into()};
    let original=crate::science::molecular::import_structure(request()).unwrap();
    let bounded=crate::science::molecular::import_structure_bounded(request(),MAX_BONDS).unwrap();
    assert_eq!(serde_json::to_value(original.atoms).unwrap(),serde_json::to_value(bounded.atoms).unwrap());
    assert_eq!(serde_json::to_value(original.bonds).unwrap(),serde_json::to_value(bounded.bonds).unwrap());
    assert!(crate::science::molecular::import_structure_bounded(request(),1).is_err());
}
