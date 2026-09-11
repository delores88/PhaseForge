//! Trusted PDB intake: one retained source, one durable registered structure.
use super::*;
use crate::domain::{ImportStructureRequest, MolecularFormat, MolecularStructure};
use sha2::{Digest, Sha256};
use base64::Engine;
use std::collections::{BTreeMap, BTreeSet};

const PARSER: &[u8] = include_bytes!("../science/molecular.rs");
const RECEIPT_LIMIT: u64 = 32 * 1024 * 1024;
const MAX_BONDS: usize = 100_000;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { job_id: Uuid, path: String, sha256: String, units: String, name: String }

struct Reservation<'a> { service: &'a LaboratoryService, id: Uuid }
impl Drop for Reservation<'_> { fn drop(&mut self) { self.service.release(self.id); } }

fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }
fn structure_id(target: Uuid) -> Uuid {
    let hash = Sha256::digest(format!("phaseforge-retained-pdb-v1:{target}"));
    let mut bytes: [u8; 16] = hash[..16].try_into().unwrap();
    bytes[6] = (bytes[6] & 15) | 64; bytes[8] = (bytes[8] & 63) | 128;
    Uuid::from_bytes(bytes)
}

fn live_parent(service: &LaboratoryService, session: &LabJob, token: &CancellationToken) -> anyhow::Result<LabJob> {
    let parent = service.get(session.id)?;
    anyhow::ensure!(parent.project_id == session.project_id && parent.kind == "session", "Structure import requires this project's agent session");
    anyhow::ensure!(parent.active() && !token.is_cancelled() && parent.deadline_at.is_none_or(|at| at > Utc::now()), "The parent session is paused, stopped or out of time");
    Ok(parent)
}

fn retained_source(service: &LaboratoryService, project: Uuid, request: &Request) -> anyhow::Result<(LabJob, data::FileRef, Vec<u8>)> {
    anyhow::ensure!(request.units == "angstrom", "PDB coordinates must be explicitly identified as angstrom; no unit conversion is inferred");
    anyhow::ensure!(!request.name.trim().is_empty() && request.name.len() <= 180 && !request.name.chars().any(char::is_control), "Give the structure a bounded, nonempty name without control characters");
    anyhow::ensure!(request.sha256.len() == 64 && request.sha256.bytes().all(|b| b.is_ascii_hexdigit()), "Pin the source's exact SHA256");
    service.ensure_model_study_access(request.job_id)?;
    let (source, file) = data::owned(service, project, request.job_id)?;
    anyhow::ensure!(file.path == request.path && file.path.ends_with(".pdb") && file.sha256.eq_ignore_ascii_case(&request.sha256), "Choose the exact retained PDB path and source hash");
    if let Some(units) = source.result["profile"]["units"].as_object() {
        anyhow::ensure!(units.values().all(|unit| matches!(unit.as_str(), Some("angstrom" | "angstroms" | "Å" | "A"))), "Retained source units conflict with PDB angstrom coordinates");
    }
    let bytes = generated::bounded_file(&service.directory(source.id), &file.path, data::MAX_FILE as u64)?;
    anyhow::ensure!(!bytes.is_empty() && bytes.len() == file.size_bytes && digest(&bytes) == file.sha256, "Retained source bytes no longer match their exact hash and size");
    Ok((source, file, bytes))
}

fn admit(service: &LaboratoryService, session: &LabJob, target: Uuid, request: &Request, source: &LabJob, file: &data::FileRef, token: &CancellationToken) -> anyhow::Result<LabJob> {
    let _guard = service.gate.lock();
    let request_value = serde_json::to_value(request)?;
    if let Some(existing) = service.database.lab_record(target)? {
        anyhow::ensure!(existing.project_id == session.project_id && existing.parent_id == Some(session.id) && existing.kind == "structure" && existing.input["request"] == request_value && existing.input["source"] == json!(file) && existing.input["source_provenance"] == source.result["source"], "Structure import request ID is bound to a different source or request");
        return Ok(existing);
    }
    let parent = live_parent(service, session, token)?;
    service.create_locked(target, parent.project_id, Some(parent.id), "structure", &request.name, json!({"request":request_value,"source":file,"source_provenance":source.result["source"],"parser_sha256":digest(PARSER),"parser":"science::molecular::import_structure_bounded/PDB","max_bonds":MAX_BONDS,"units":"angstrom"}), parent.deadline_at)
}

fn make_receipt(job: &LabJob, bytes: &[u8]) -> anyhow::Result<Value> {
    anyhow::ensure!(job.input["parser_sha256"] == digest(PARSER), "The PDB parser changed before this import was committed; make a new explicit import request");
    let text = std::str::from_utf8(bytes).context("PDB source is not UTF-8 text")?;
    anyhow::ensure!(!text.contains('\0'), "PDB source contains binary NUL bytes");
    // Bound the existing parser's pairwise diagnostics before invoking it.
    let records = text.lines().filter(|line| matches!(line.get(..line.len().min(6)).unwrap_or("").trim(), "ATOM" | "HETATM")).collect::<Vec<_>>();
    anyhow::ensure!((1..=12_000).contains(&records.len()), "Trusted illustration import requires 1–12000 coordinate records; choose a smaller explicitly sourced complex");
    anyhow::ensure!(records.iter().all(|line| line.is_ascii()), "PDB fixed-width coordinate fields must be ASCII; malformed Unicode atom records are not passed to the trusted parser");
    let mut structure = crate::science::molecular::import_structure_bounded(ImportStructureRequest { project_id:Some(job.project_id), name:job.title.clone(), format:MolecularFormat::Pdb, content:text.into() }, MAX_BONDS)?;
    anyhow::ensure!(structure.atoms.iter().all(|atom| atom.position.iter().all(|v| v.is_finite() && v.abs() <= 1e9)), "PDB coordinates must be finite and within the renderer's coordinate limits");
    structure.id = structure_id(job.id); structure.created_at = job.created_at;
    let mut chains: BTreeMap<String, (usize, BTreeSet<(String, String)>)> = BTreeMap::new();
    for atom in &structure.atoms {
        let entry = chains.entry(atom.chain_id.clone()).or_default(); entry.0 += 1;
        entry.1.insert((atom.residue_id.clone(), atom.residue_name.clone()));
    }
    let chains = chains.into_iter().map(|(chain,(atoms,residues))| json!({"author_chain_id":chain,"atom_count":atoms,"coordinate_residue_count":residues.len()})).collect::<Vec<_>>();
    let registered_structure_sha256 = digest(&serde_json::to_vec(&structure)?);
    let header = text.lines().filter(|line| ["HEADER", "TITLE ", "EXPDTA", "COMPND", "SOURCE", "REMARK   2"].iter().any(|prefix| line.starts_with(prefix))).collect::<Vec<_>>().join("\n").chars().take(6000).collect::<String>();
    let result = json!({"job_id":job.id,"structure_id":structure.id,"state":"completed","source":job.input["source"],"source_provenance":job.input["source_provenance"],"source_header_unverified":header,"parser_sha256":job.input["parser_sha256"],"registered_structure_sha256":registered_structure_sha256,"structure_hash_format":"SHA256 of compact serde_json MolecularStructure bytes retained in import-receipt.json","units":"angstrom","atom_count":structure.atoms.len(),"coordinate_residue_count":structure.diagnostics.residue_count,"chain_count":chains.len(),"chains":chains,"warnings":structure.warnings,"coordinate_frame":"All imported first-model chains remain together in original PDB author coordinates. No independent recentering, scaling, chain selection or missing-coordinate reconstruction.","limitations":["PDB syntax and source integrity are checked; experimental origin, resolution, missing residues and biological suitability require the cited archive record.","The trusted parser reports its first/default conformer policy and skipped models/alternate locations; inferred bonds are not experimentally established chemistry.","Imported coordinates are not a simulation-ready force field, a complete CAR or cell, or evidence of treatment efficacy.","Blender uses an interpolated van der Waals display envelope, not electron density. Authored cell placement and inset magnification must remain explicit."],"next":"Use structure_id for the whole complex, or bind this one structure_id to one protein/molecule node in an authored scene. Keep interface chains in that one binding so the renderer applies one common fit. Preserve source pins and inspect the actual render."});
    Ok(json!({"schema":"phaseforge.retained-pdb-import.v1","input":job.input,"source_bytes_base64":base64::engine::general_purpose::STANDARD.encode(bytes),"structure":structure,"result":result}))
}

fn read_receipt(service: &LaboratoryService, job: &LabJob, source_bytes: &[u8]) -> anyhow::Result<Value> {
    let bytes = generated::bounded_file(&service.directory(job.id), "import-receipt.json", RECEIPT_LIMIT)?;
    let receipt: Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(receipt["schema"] == "phaseforge.retained-pdb-import.v1" && receipt["input"] == job.input, "Retained import receipt identity changed");
    let snapshot = base64::engine::general_purpose::STANDARD.decode(receipt["source_bytes_base64"].as_str().context("Import source snapshot missing")?)?;
    anyhow::ensure!(snapshot == source_bytes && digest(&snapshot) == job.input["source"]["sha256"], "Retained import source snapshot changed");
    let structure: MolecularStructure = serde_json::from_value(receipt["structure"].clone())?;
    anyhow::ensure!(structure.id == structure_id(job.id) && structure.project_id == Some(job.project_id) && structure.created_at == job.created_at && receipt["result"]["job_id"] == json!(job.id) && receipt["result"]["structure_id"] == json!(structure.id) && receipt["result"]["source"] == job.input["source"] && receipt["result"]["registered_structure_sha256"] == digest(&serde_json::to_vec(&structure)?), "Retained import structure or source receipt changed");
    if let Some(expected) = job.progress["import_receipt_sha256"].as_str() {
        anyhow::ensure!(digest(&bytes) == expected, "Pinned import receipt bytes changed");
    } else {
        // A crash between file publication and database pinning leaves an
        // unanchored receipt. Recompute locally with the same trusted parser;
        // this never creates a second structure or calls a model/network.
        anyhow::ensure!(make_receipt(job, source_bytes)? == receipt, "Uncommitted import receipt differs from trusted parsing of its source");
    }
    Ok(receipt)
}

fn commit(service: &LaboratoryService, session: &LabJob, id: Uuid, receipt: &Value, token: &CancellationToken) -> anyhow::Result<Value> {
    let _guard = service.gate.lock(); let mut job = service.get(id)?;
    if job.state != "completed" {
        live_parent(service, session, token)?;
        anyhow::ensure!(job.active(), "The structure import was stopped before publication");
    }
    let receipt_bytes = generated::bounded_file(&service.directory(id), "import-receipt.json", RECEIPT_LIMIT)?;
    anyhow::ensure!(serde_json::from_slice::<Value>(&receipt_bytes)? == *receipt, "Import receipt changed before publication");
    let receipt_hash = digest(&receipt_bytes);
    if let Some(expected) = job.progress["import_receipt_sha256"].as_str() { anyhow::ensure!(expected == receipt_hash, "Pinned import receipt bytes changed"); }
    else { job.progress["import_receipt_sha256"] = json!(receipt_hash); service.database.put_lab_record(&job)?; }
    let structure: MolecularStructure = serde_json::from_value(receipt["structure"].clone())?;
    if let Some(existing) = service.database.get_molecule(structure.id)? {
        anyhow::ensure!(serde_json::to_value(existing)? == receipt["structure"], "Registered structure differs from its immutable import receipt");
    } else {
        anyhow::ensure!(job.state != "completed", "A completed import's registered structure is missing");
        live_parent(service, session, token)?;
        service.database.put_molecule(&structure)?;
    }
    if job.state != "completed" {
        job.result = receipt["result"].clone(); job.state = "completed".into(); job.error = None;
        job.event("completed", "Trusted PDB coordinates registered with original source pins; biological validity is not inferred.", json!({"structure_id":structure.id,"source_sha256":job.input["source"]["sha256"]}));
        job.completed_at = Some(job.updated_at); service.database.put_lab_record(&job)?;
    } else { anyhow::ensure!(job.result == receipt["result"], "Completed import result changed"); }
    Ok(job.result)
}

pub fn import(service: &LaboratoryService, session: &LabJob, target: Uuid, args: &Value, token: &CancellationToken) -> anyhow::Result<Value> {
    let mut request: Request = serde_json::from_value(args.clone())?; request.sha256.make_ascii_lowercase();
    let (source, file, bytes) = retained_source(service, session.project_id, &request)?;
    let job = admit(service, session, target, &request, &source, &file, token)?;
    if job.state == "completed" { return commit(service, session, target, &read_receipt(service, &job, &bytes)?, token); }
    let execution = service.acquire_child(target, token)?;
    let _reservation = Reservation { service, id:target };
    let outcome = (|| {
        {
            let _guard = service.gate.lock(); live_parent(service, session, &execution)?;
            let mut current = service.get(target)?;
            let interrupted = current.state == "paused" && current.events.last().is_some_and(|event| event.kind == "interrupted");
            // A returned database error can occur after molecule insertion but
            // before the terminal job write. Reconcile only that exact pinned,
            // already-registered structure; never retry arbitrary failed parses.
            let partial_commit = if current.state == "failed" && current.progress["import_receipt_sha256"].is_string() {
                let receipt = read_receipt(service, &current, &bytes)?;
                service.database.get_molecule(structure_id(target))?.map(serde_json::to_value).transpose()?.is_some_and(|stored| stored == receipt["structure"])
            } else { false };
            anyhow::ensure!(current.active() || interrupted || partial_commit, "The structure import was explicitly stopped or failed; inspect the retained attempt before requesting a new import");
            current.state = "running".into(); current.deadline_at = service.get(session.id)?.deadline_at; service.database.put_lab_record(&current)?;
        }
        let path = service.directory(target).join("import-receipt.json");
        if !path.exists() {
            let receipt = make_receipt(&job, &bytes)?;
            anyhow::ensure!(serde_json::to_vec_pretty(&receipt)?.len() as u64 <= RECEIPT_LIMIT, "Parsed import exceeds the bounded receipt size; no unreadable receipt was published");
            let _guard = service.gate.lock(); live_parent(service, session, &execution)?;
            anyhow::ensure!(service.get(target)?.active(), "The structure import stopped during parsing");
            write_json(&path, &receipt)?;
            let mut current = service.get(target)?;
            current.progress["import_receipt_sha256"] = json!(digest(&serde_json::to_vec_pretty(&receipt)?));
            service.database.put_lab_record(&current)?;
        }
        commit(service, session, target, &read_receipt(service, &service.get(target)?, &bytes)?, &execution)
    })();
    if let Err(error) = &outcome {
        let _ = service.update(target, |job| { if job.active() { job.state = if execution.is_cancelled() { "paused" } else { "failed" }.into(); job.error = Some(format!("{error:#}")); job.event("import_failed", format!("{error:#}"), json!({"source_sha256":file.sha256})); } });
    }
    outcome
}

#[cfg(test)]
#[path = "structure_tests.rs"]
mod tests;
