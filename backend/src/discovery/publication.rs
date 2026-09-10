//! Portable, offline research exports. No upload, submission, credentials or chat history.
use std::collections::{BTreeMap,BTreeSet};
use anyhow::{bail,Context};
use serde_json::{json,Value};
use sha2::{Digest,Sha256};
use crate::{persistence::Database,domain::RunRecord};
use super::{types::*,math,notebook};
const MAX_BUNDLE:usize=128*1024*1024;
type Files=BTreeMap<String,Vec<u8>>;
fn add(files:&mut Files,name:impl Into<String>,data:impl Into<Vec<u8>>)->anyhow::Result<()> {
    let data=data.into();if files.values().map(Vec::len).sum::<usize>()+data.len()>MAX_BUNDLE{bail!("research export exceeds 128 MiB; use a smaller focused study rather than silently truncating evidence");}
    files.insert(name.into(),data);Ok(())
}
fn json_file(files:&mut Files,name:impl Into<String>,data:&impl serde::Serialize)->anyhow::Result<()> {add(files,name,serde_json::to_vec_pretty(data)?)}
fn digest(data:&[u8])->String {format!("{:x}",Sha256::digest(data))}
fn csv(value:&str)->String {
    // Prevent formula execution when scientific labels are opened in a spreadsheet.
    let value=if matches!(value.chars().next(),Some('='|'+'|'-'|'@')){format!("'{value}")}else{value.into()};
    format!("\"{}\"",value.replace('"',"\"\""))
}
fn escaped(s:&str)->String {s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;")}
fn bib(s:&str)->String{s.replace('\\'," ").replace('{',"\\{").replace('}',"\\}").replace('\n'," ")}

pub fn build(db:&Database,study:&Study)->anyhow::Result<Vec<u8>>{
    let book=notebook::load(db,study.project_id)?;let gates=notebook::checklist(db,&book,study);
    let mut files=Files::new();
    let mut plans=serde_json::to_value(db.research_plans(study.project_id)?)?;
    for p in plans.as_array_mut().into_iter().flatten(){for t in p["plan"]["tasks"].as_array_mut().into_iter().flatten(){if let Some(o)=t.as_object_mut(){o.remove("next_prompt");}}}
    json_file(&mut files,"research/plans.json",&plans)?;
    json_file(&mut files,"research/source-searches.json",&db.research_searches(study.project_id)?)?;
    json_file(&mut files,"research/task-records.json",&db.research_tasks(study.project_id)?)?;
    json_file(&mut files,"research/dataset-profiles.json",&db.research_data(study.project_id)?)?;
    // Immutable verification snapshots, controls, worker results and prior-work history.
    // No partial in-flight record is exported as a completed research snapshot.
    let dossiers=db.list_dossiers()?.into_iter().filter(|d|d.protocol.study_id==study.id).collect::<Vec<_>>();
    for d in &dossiers {
        if matches!(d.state.as_str(),"running"|"stopping"){bail!("stop independent verification before exporting a consistent research package");}
        let (worker,reference)=crate::assurance::service::frozen_sources(d)?;
        add(&mut files,format!("verification/{}/verification_worker.py",d.id),worker.as_bytes())?;
        add(&mut files,format!("verification/{}/reference_ode.py",d.id),reference.as_bytes())?;
        json_file(&mut files,format!("verification/{}/dossier.json",d.id),d)?;
        json_file(&mut files,format!("verification/{}/assessment.json",d.id),&crate::assurance::service::assessment(db,d)?)?;
        json_file(&mut files,format!("verification/{}/input.json",d.id),&json!({"protocol":d.protocol,"manifest":d.manifest,"source_run":d.source_run,
            "controls":d.controls,"parameters":d.parameters,"protocol_hash":d.protocol_hash,"worker_source_hash":d.worker_source_hash,"reference_source_hash":d.reference_source_hash}))?;
    }
    add(&mut files,"verification_worker.py",crate::assurance::service::WORKER.as_bytes())?;
    add(&mut files,"verification/READ_ME.md",b"# Independent verification records\n\nEach dossier freezes the exact selected run, input protocol, comparison corpus, controls and source hashes. No external novelty certification. To repeat a worker, copy THAT DOSSIER DIRECTORY including its version-matched verification_worker.py, reference_ode.py and input.json to a new folder outside this bundle; run python -I verification_worker.py FOLDER. Run verify_bundle.py before generating new files. Source hashes must match; new program versions require a new verification record.\n".to_vec())?;
    json_file(&mut files,"study.json",study)?;
    json_file(&mut files,"research-record.json",&book)?;
    json_file(&mut files,"readiness.json",&gates)?;
    json_file(&mut files,"summary.json",&super::service::report(study))?;
    json_file(&mut files,format!("manifests/{}.json",study.base_manifest.id),&study.base_manifest)?;
    let mut runs=BTreeMap::<String,RunRecord>::new();let mut manifests=BTreeSet::new();
    for trial in &study.trials {
        if let Some(id)=trial.manifest_id {if manifests.insert(id){let manifest=db.get_manifest(id)?.context("an immutable study manifest is missing; incomplete exports are not silently accepted")?;json_file(&mut files,format!("manifests/{id}.json"),&manifest)?;}}
        if let Some(id)=trial.run_id {
            let run=db.get_run(id)?.context("a study run is missing; restore it before exporting")?;
            json_file(&mut files,format!("runs/{id}.json"),&run)?;
            runs.insert(id.to_string(),run);
        }
    }
    // Include off-campaign evidence explicitly cited by claims, with exact immutable inputs.
    for claim in &book.claims {for id in claim.supporting_runs.iter().chain(&claim.contradicting_runs){
        if !runs.contains_key(&id.to_string()) {
            let run=db.get_run(*id)?.context("claim evidence is missing")?;
            if run.project_id!=study.project_id{bail!("claim evidence belongs to another project");}
            let m=db.get_manifest(run.manifest_id)?.context("claim manifest missing")?;
            json_file(&mut files,format!("manifests/{}.json",m.id),&m)?;
            json_file(&mut files,format!("runs/{id}.json"),&run)?;runs.insert(id.to_string(),run);
        }
    }}
    let metric_names=study.trials.iter().flat_map(|t|t.metrics.keys().cloned()).collect::<BTreeSet<_>>();
    let mut table=String::from("trial_id,run_id,phase,state,eligible,cell");
    for p in &study.recipe.parameters{table.push_str(&format!(",{}",csv(&format!("parameter:{}",p.target))));}
    for name in &metric_names{table.push_str(&format!(",{}",csv(name)));}table.push('\n');
    for trial in &study.trials {
        table.push_str(&format!("{},{},{},{},{},{}",trial.id,trial.run_id.map(|v|v.to_string()).unwrap_or_default(),trial.phase,trial.state,trial.eligible,csv(trial.cell.as_deref().unwrap_or(""))));
        for x in &trial.values{table.push_str(&format!(",{x}"));}
        for key in &metric_names{table.push(',');if let Some(value)=trial.metrics.get(key){table.push_str(&value.to_string());}}table.push('\n');
    }
    add(&mut files,"tables/all-trials.csv",table.into_bytes())?;
    for (id,run) in &runs {
        if let Some(result)=&run.result {
            for (index,series) in result["series"].as_array().into_iter().flatten().enumerate() {
                let mut csv_data=String::from("time,value\n");
                for p in series["points"].as_array().into_iter().flatten(){if let (Some(t),Some(v))=(p[0].as_f64(),p[1].as_f64()){csv_data.push_str(&format!("{t},{v}\n"));}}
                add(&mut files,format!("series/{id}-{index}.csv"),csv_data.into_bytes())?;
            }
        }
    }
    add(&mut files,"figures/search-observations.svg",search_figure(study).into_bytes())?;
    let figure_run=study.finalist_ids.first().and_then(|id|study.trials.iter().find(|t|t.id==*id)).and_then(|t|t.run_id).and_then(|id|runs.get(&id.to_string()));
    if let Some(run)=figure_run {
        if let Some(series)=run.result.as_ref().and_then(|r|r["series"].as_array()).and_then(|s|s.first()) {
            add(&mut files,"figures/finalist-signal.svg",signal_figure(series).into_bytes())?;
            json_file(&mut files,"signal-diagnostics.json",&super::signals::inspect(run.result.as_ref().unwrap()))?;
        }
    }
    let references=book.references.iter().enumerate().map(|(i,r)|{
        json!({"id":format!("ref{}",i+1),"type":"article-journal","DOI":r.doi,"title":r.title,"URL":r.url,
            "author":r.authors.iter().map(|name|json!({"literal":name})).collect::<Vec<_>>(),"issued":r.year.map(|y|json!({"date-parts":[[y]]}))})
    }).collect::<Vec<_>>();
    json_file(&mut files,"references.csl.json",&references)?;
    let bibliography=book.references.iter().enumerate().map(|(i,r)|format!("@article{{ref{},\n title = {{{}}},\n author = {{{}}},\n year = {{{}}},\n doi = {{{}}},\n url = {{{}}}\n}}\n",i+1,bib(&r.title),bib(&r.authors.join(" and ")),r.year.map(|y|y.to_string()).unwrap_or_default(),bib(&r.doi),bib(&r.url))).collect::<Vec<_>>().join("\n");
    add(&mut files,"references.bib",bibliography.into_bytes())?;
    add(&mut files,"manuscript.md",manuscript(study,&book,&gates).into_bytes())?;
    // Intentionally omit token costs, credentials, endpoints, prompts and conversations.
    let usage=db.list_usage_records()?.into_iter().filter(|u|u.project_id==Some(study.project_id)).map(|u|json!({"request_id":u.request_id,"provider":u.provider,"model":u.model,"purpose":u.purpose,"attempt":u.attempt,"status":u.status,"started_at":u.started_at,"usage":u.usage})).collect::<Vec<_>>();
    json_file(&mut files,"ai-usage-disclosure.json",&usage)?;
    add(&mut files,"reference_ode.py",include_bytes!("../../../tools/reference_ode.py").to_vec())?;
    add(&mut files,"verify_bundle.py",include_bytes!("../../../tools/verify_bundle.py").to_vec())?;
    add(&mut files,"README.md",format!(r#"# PhaseForge research bundle

Study: {}
This is an author-review package, not a publication or a certificate of novelty.
Recipe hash: `{}`. Numerical software release: {}.

## Contents
`study.json` contains the immutable protocol and every attempted trial, including rejected/failed work.
`runs/` and `manifests/` retain exact inputs, seeds, outputs and runtime diagnostics.
`tables/all-trials.csv` includes raw measured values; `series/` contains recorded series.
`figures/` contains editable SVG plots made from those recorded results, not generated illustrations.
`manuscript.md` is an evidence-linked working draft with mandatory author review gaps.
`references.bib` and CSL JSON preserve recorded metadata; metadata does not mean a paper was read.
`ai-usage-disclosure.json` lists project-level model usage without prompts, endpoints, pricing or keys.
`ro-crate-metadata.json` describes this payload; it does not assert validation of scientific content.

## Verify integrity (Python 3.10+, standard library only)
```
python verify_bundle.py .
```
Hashes detect changed bytes; they do not authenticate who generated them. Archive this ZIP/code version in an appropriate repository yourself.

## Independent ODE implementation check
For a completed state-vector run, use the matching manifest UUID from its JSON record:
```
python reference_ode.py manifests/MANIFEST_UUID.json --run runs/RUN_UUID.json --out reference-result.json
```
Run the integrity check before writing new result files into this directory (new files are intentionally flagged).
Use `--refinement 2` or `4` for separately implemented timestep studies. Explicitly choose tolerances appropriate to each metric.
This verifier supports Euler/RK4 first-order ODEs, not particle/QM/MM or native engine workflows.
It independently parses arithmetic and integrates f64 values; it does not validate the physical model.

## Reproduce in PhaseForge
Import the chosen manifest's draft fields into the same PhaseForge release, verify settings, then approve its local run.
No provider key is needed to replay a numerical manifest. The software source ZIP must be archived alongside this research package.

## Disclosure boundary
Data license specified by authors: {}
Review all scientific claims, missing/failed comparisons, data rights, authorship, AI use and journal requirements before submission.
Nothing has been uploaded or submitted by PhaseForge.
"#,study.recipe.title,study.recipe_hash,env!("CARGO_PKG_VERSION"),book.data_license).into_bytes())?;
    let timestamp=chrono::Utc::now().to_rfc3339();
    let mut graph=vec![json!({"@id":"ro-crate-metadata.json","@type":"CreativeWork","about":{"@id":"./"},"conformsTo":{"@id":"https://w3id.org/ro/crate/1.1"}}),
        json!({"@id":"./","@type":"Dataset","name":book.title,"description":study.recipe.hypothesis,"datePublished":timestamp,"license":book.data_license,
            "hasPart":files.keys().map(|k|json!({"@id":k})).collect::<Vec<_>>(),"author":book.authors.iter().enumerate().map(|(i,_)|json!({"@id":format!("#author-{i}")})).collect::<Vec<_>>()})];
    for (name,data) in &files {graph.push(json!({"@id":name,"@type":"File","name":name,"contentSize":data.len(),"sha256":digest(data)}));}
    for (i,author) in book.authors.iter().enumerate(){graph.push(json!({"@id":format!("#author-{i}"),"@type":"Person","name":author.name,"identifier":author.orcid,"description":author.contributions}));}
    json_file(&mut files,"ro-crate-metadata.json",&json!({"@context":"https://w3id.org/ro/crate/1.1/context","@graph":graph}))?;
    let checksums=files.iter().map(|(name,bytes)|(name.clone(),digest(bytes))).collect::<BTreeMap<_,_>>();
    json_file(&mut files,"checksums.json",&checksums)?;
    zip_stored(&files)
}
fn manuscript(study:&Study,book:&Notebook,gates:&Value)->String {
    let s=math::summary(study);let mut output=format!("# {}\n\n**WORKING DRAFT — author review required; not automatically publishable.**\n\nAuthors: {}\n\n## Abstract\n\nThis study explored an authored computational model using {:?}. {} exploration trials were planned; {} candidates met baseline feasibility criteria and {} were rejected. These are numerical observations, not evidence of novelty. Human authors must supply motivation, justified conclusions, and comparison to prior work.\n\n## Research question and hypothesis\n\n{}\n\n## Methods\n\n### Scientific boundary\n{}\n\n### Frozen protocol\n{}\n\nRecipe hash: `{}`. Seed: {}. Wall allocation: {} s; per-trial cap: {} s. Finalists are tested at h/2 and h/4 using the same f64 engine and common seed. Agreement tolerance is {} + {} × max(|baseline|,|h/2|,|h/4|). This is not independent replication.\n\n### Parameter ranges\n",book.title,book.authors.iter().map(|a|a.name.as_str()).collect::<Vec<_>>().join(", "),study.recipe.strategy,study.recipe.exploration_trials,s["eligible"],s["rejected"],book.hypothesis,study.base_manifest.scientific_boundary,book.protocol,study.recipe_hash,study.recipe.seed,study.recipe.wall_seconds,study.recipe.per_trial_seconds,study.recipe.absolute_tolerance,study.recipe.relative_tolerance);
    for p in &study.recipe.parameters {output.push_str(&format!("- `{}`: [{}, {}]\n",p.target,p.minimum,p.maximum));}
    output.push_str("\n## Results and claim-to-evidence ledger\n\nSee all-trials.csv for every measured outcome, including failures and infeasible candidates. Numerical eligibility is not a validated scientific claim.\n\n");
    for claim in &book.claims {output.push_str(&format!("### {} ({})\n\n{}\n\nSupporting runs: {}\n\nCounter-evidence runs: {}\n\nLimitations: {}\n\nPrior-work comparison: {}\n\nHuman reviewer: {}\n\n",claim.id,claim.kind,claim.statement,claim.supporting_runs.iter().map(|id|id.to_string()).collect::<Vec<_>>().join(", "),claim.contradicting_runs.iter().map(|id|id.to_string()).collect::<Vec<_>>().join(", "),claim.limitations,claim.literature_comparison,claim.reviewed_by));}
    output.push_str(&format!("## Discussion / author notes\n\n{}\n\n## Independent verification\n\n{}\n\n## Data availability\n\n{}\n\n## Code availability\n\n{}\n\n## AI assistance disclosure\n\n{}\n\n## References\n\nSee references.bib and references.csl.json. Verify relevance and read the underlying works; titles/metadata are not substantive scientific evidence.\n\n## Unresolved submission gates\n",book.notes,book.independent_validation_notes,book.data_availability,book.code_availability,book.ai_disclosure));
    for gap in gates["gaps"].as_array().into_iter().flatten(){output.push_str(&format!("- {}\n",gap.as_str().unwrap_or("Review needed")));}output
}
fn search_figure(study:&Study)->String {
    let g=&study.recipe.objectives[0];let points=study.trials.iter().filter(|t|t.phase=="explore"&&t.eligible).filter_map(|t|t.metrics.get(&g.metric).map(|v|(t.index as f64,*v))).collect::<Vec<_>>();
    svg_plot(&points,"Feasible exploration candidates (excluded trials remain in CSV)","Trial index",&g.metric)
}
fn signal_figure(series:&Value)->String {
    let points=series["points"].as_array().into_iter().flatten().filter_map(|p|Some((p[0].as_f64()?,p[1].as_f64()?))).collect::<Vec<_>>();
    svg_plot(&points,"Recorded finalist signal (points; no interpolation)","Model time",series["name"].as_str().unwrap_or("value"))
}
fn svg_plot(points:&[(f64,f64)],title:&str,xlabel:&str,ylabel:&str)->String {
    let points=points.iter().filter(|(x,y)|x.is_finite()&&y.is_finite()).copied().collect::<Vec<_>>();
    let xmin=points.iter().map(|p|p.0).fold(f64::INFINITY,f64::min);let xmax=points.iter().map(|p|p.0).fold(f64::NEG_INFINITY,f64::max);
    let ymin=points.iter().map(|p|p.1).fold(f64::INFINITY,f64::min);let ymax=points.iter().map(|p|p.1).fold(f64::NEG_INFINITY,f64::max);
    let mut svg=format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="960" height="540" viewBox="0 0 960 540"><rect width="960" height="540" fill="white"/><g font-family="sans-serif" fill="#18283d"><text x="80" y="35" font-size="17">{}</text><path d="M90 70V455H900" fill="none" stroke="#546477"/><text x="450" y="520">{}</text><text transform="translate(24,315) rotate(-90)">{}</text>"##,escaped(title),escaped(xlabel),escaped(ylabel));
    if points.is_empty() || ![xmin,xmax,ymin,ymax,xmax-xmin,ymax-ymin].iter().all(|v|v.is_finite()){svg.push_str("<text x=\"160\" y=\"250\">No finite plotting range; inspect retained CSV and run evidence.</text>");}
    else {
        for (x,y) in &points{let px=90.0+810.0*(x-xmin)/(xmax-xmin).max(f64::EPSILON);let py=455.0-370.0*(y-ymin)/(ymax-ymin).max(f64::EPSILON);svg.push_str(&format!(r##"<circle cx="{px:.3}" cy="{py:.3}" r="2.4" fill="#216caa"/>"##));}
        svg.push_str(&format!(r##"<text x="70" y="480">{xmin:.5e}</text><text x="810" y="480">{xmax:.5e}</text><text x="95" y="67">{ymax:.5e}</text><text x="95" y="443">{ymin:.5e}</text>"##));
    }svg.push_str("</g></svg>");svg
}

/// Standard ZIP32 STORE writer. UTF-8 names, CRC32, central directory; no native library required.
pub fn zip_stored(files:&Files)->anyhow::Result<Vec<u8>>{
    if files.len()>65535{bail!("ZIP entry limit exceeded");}
    fn u16w(b:&mut Vec<u8>,v:u16){b.extend_from_slice(&v.to_le_bytes());}
    fn u32w(b:&mut Vec<u8>,v:u32){b.extend_from_slice(&v.to_le_bytes());}
    let mut out=Vec::new();let mut directory=Vec::new();
    for (name,data) in files {
        if name.starts_with('/')||name.contains('\\')||name.split('/').any(|s|s==".."||s.is_empty())||name.len()>65535{bail!("unsafe ZIP entry");}
        let offset=u32::try_from(out.len())?;let size=u32::try_from(data.len())?;let crc=crc32(data);
        u32w(&mut out,0x04034b50);for n in [20,0x800,0,0,0x5821]{u16w(&mut out,n);}u32w(&mut out,crc);u32w(&mut out,size);u32w(&mut out,size);u16w(&mut out,name.len() as u16);u16w(&mut out,0);out.extend_from_slice(name.as_bytes());out.extend_from_slice(data);
        u32w(&mut directory,0x02014b50);for n in [20,20,0x800,0,0,0x5821]{u16w(&mut directory,n);}u32w(&mut directory,crc);u32w(&mut directory,size);u32w(&mut directory,size);u16w(&mut directory,name.len() as u16);for _ in 0..4{u16w(&mut directory,0);}u32w(&mut directory,0);u32w(&mut directory,offset);directory.extend_from_slice(name.as_bytes());
    }
    let start=u32::try_from(out.len())?;let len=u32::try_from(directory.len())?;out.extend(directory);
    u32w(&mut out,0x06054b50);u16w(&mut out,0);u16w(&mut out,0);u16w(&mut out,files.len() as u16);u16w(&mut out,files.len() as u16);u32w(&mut out,len);u32w(&mut out,start);u16w(&mut out,0);Ok(out)
}
fn crc32(bytes:&[u8])->u32 {
    let mut c=0xffff_ffffu32;
    for byte in bytes {c^=*byte as u32;for _ in 0..8{c=if c&1!=0{(c>>1)^0xedb8_8320}else{c>>1};}}
    !c
}
#[cfg(test)]mod tests {
    use super::*;
    #[test]fn crc_reference_vector(){assert_eq!(crc32(b"123456789"),0xcbf43926);}
    #[test]fn zip_signature_and_unsafe_names(){let mut f=Files::new();f.insert("hello.txt".into(),b"hello".to_vec());let z=zip_stored(&f).unwrap();assert_eq!(&z[..4],b"PK\x03\x04");assert_eq!(&z[z.len()-22..z.len()-18],b"PK\x05\x06");f.insert("../bad".into(),vec![]);assert!(zip_stored(&f).is_err());}
}
