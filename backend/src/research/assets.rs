//! Explicit, bounded public-catalog intake. Callers supply catalog identifiers,
//! never arbitrary fetch URLs; downloaded material is evidence, not instructions.
use std::{sync::Arc, time::Duration};
use anyhow::{bail, Context};
use axum::{extract::{Path, State}, http::{header, StatusCode}, response::{IntoResponse, Response}, routing::{get, post}, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use futures_util::{stream, StreamExt};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use crate::{app::AppState, persistence::Database};

const MAX_JSON: usize=4*1024*1024;
const MAX_ASSET: usize=8*1024*1024;
const RCSB_SEARCH_DESCRIPTION:&str="Experimental structure archive search match; inspect its record and methods before drawing conclusions.";
static INTAKE: tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(2);

/// Shared fixed-host URL admission for retained numerical source files.
pub(crate) fn public_data_url(value:&str)->anyhow::Result<url::Url>{super::public_file::url(value)}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="snake_case")]
pub enum Catalog { Rcsb, Literature, Nasa, PublicFile }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub catalog: Catalog, pub accession: String, pub title: String, pub source_url: String,
    pub description: String, pub preview_url: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSearch {
    pub id:Uuid, pub project_id:Uuid, pub catalog:Catalog, pub query:String, pub created_at:DateTime<Utc>,
    pub status:String, pub error:Option<String>, pub response_sha256:Option<String>, pub results:Vec<Candidate>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchAsset {
    pub id:Uuid, pub project_id:Uuid, pub catalog:Catalog, pub accession:String, pub title:String,
    pub source_url:String, pub sha256:String, pub mime_type:String, pub size_bytes:usize,
    pub molecule_id:Option<Uuid>, pub created_at:DateTime<Utc>, pub provenance:String,
    #[serde(default)] pub source_description:String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest { pub catalog:Catalog, pub query:String, #[serde(default="default_limit")] pub limit:usize }
fn default_limit()->usize {5}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportRequest { pub catalog:Catalog, pub accession:String }

fn client()->anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(Duration::from_secs(25)).redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("PhaseForge/",env!("CARGO_PKG_VERSION")," public-research")).build()?)
}
async fn bounded(mut response:reqwest::Response, limit:usize)->anyhow::Result<Vec<u8>> {
    if !response.status().is_success(){bail!("Public catalog returned HTTP {}",response.status());}
    if response.content_length().is_some_and(|length|length>limit as u64){bail!("Public response exceeds the {} MiB intake limit",limit/1024/1024);}
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await? {
        if bytes.len().saturating_add(chunk.len())>limit {bail!("Public response exceeded its bounded intake limit");}
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn identifier(catalog:&Catalog, value:&str)->anyhow::Result<String> {
    let value=value.trim();
    if *catalog==Catalog::PublicFile {return Ok(super::public_file::url(value)?.to_string());}
    let valid=match catalog {
        Catalog::Rcsb=>value.len()==4 && value.as_bytes()[0].is_ascii_digit() && value.bytes().all(|b|b.is_ascii_alphanumeric()),
        Catalog::Nasa=>!value.is_empty() && value.len()<=120 && value.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'_' || b==b'-'),
        Catalog::Literature=>false,
        Catalog::PublicFile=>unreachable!(),
    };
    if !valid {bail!("Use a catalog accession, not a URL or executable file; RCSB imports currently support four-character PDB IDs");}
    Ok(if *catalog==Catalog::Rcsb {value.to_ascii_uppercase()} else {value.into()})
}
fn nasa_image_url(value:&str, accession:&str)->anyhow::Result<url::Url> {
    let mut url=url::Url::parse(value)?;
    // The official NASA index still emits legacy HTTP links. Upgrade only its
    // exact asset host; all actual downloads continue to use HTTPS without redirects.
    if url.scheme()=="http" && url.host_str()==Some("images-assets.nasa.gov") {url.set_scheme("https").map_err(|_|anyhow::anyhow!("Invalid NASA asset scheme"))?;}
    let prefix=format!("/image/{accession}/");
    if url.scheme()!="https" || url.host_str()!=Some("images-assets.nasa.gov") || url.port().is_some()
        || !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some()
        || !url.path().starts_with(&prefix) || url.path().contains('%') || url.path()[prefix.len()..].contains('/')
        || ![".jpg",".jpeg",".png"].iter().any(|suffix|url.path().to_ascii_lowercase().ends_with(suffix)) {
        bail!("NASA response did not identify an allowed static image asset");
    }
    Ok(url)
}
pub(super) fn image_mime(bytes:&[u8])->anyhow::Result<&'static str> {
    Ok(image_info(bytes)?.0)
}
pub(super) fn image_info(bytes:&[u8])->anyhow::Result<(&'static str,u32,u32)> {
    let (mime,width,height)=if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len()>=24 && &bytes[12..16]==b"IHDR" {
        ("image/png",u32::from_be_bytes(bytes[16..20].try_into().unwrap()),u32::from_be_bytes(bytes[20..24].try_into().unwrap()))
    } else if bytes.starts_with(&[0xff,0xd8]) {
        let mut at=2; let mut dimensions=None;
        while at+4<=bytes.len() {
            if bytes[at]!=0xff {break;} let marker=bytes[at+1]; at+=2;
            if marker==0xff {at-=1;continue;} if marker==0xd9 || marker==0xda {break;}
            let size=u16::from_be_bytes([bytes[at],bytes[at+1]]) as usize;
            if size<2 || at+size>bytes.len(){break;}
            if [0xc0,0xc1,0xc2].contains(&marker) && size>=7 {
                dimensions=Some((u16::from_be_bytes([bytes[at+5],bytes[at+6]]) as u32,u16::from_be_bytes([bytes[at+3],bytes[at+4]]) as u32));break;
            }
            at+=size;
        }
        let (width,height)=dimensions.context("JPEG dimensions could not be validated")?; ("image/jpeg",width,height)
    } else {bail!("Only bounded static PNG or JPEG images are imported");};
    if width==0 || height==0 || width>16384 || height>16384 || u64::from(width)*u64::from(height)>32_000_000 {bail!("Public image exceeds the 32 megapixel decoding budget");}
    Ok((mime,width,height))
}

pub async fn search_catalog(db:&Database, project:Uuid, request:SearchRequest)->anyhow::Result<AssetSearch> {
    db.get_project(project)?.context("Research world not found")?;
    if request.catalog==Catalog::PublicFile {bail!("Public files are imported by an explicit HTTPS URL; search supports RCSB, literature and NASA catalogs");}
    if request.query.trim().len()<3 || request.query.len()>1000 || !(1..=10).contains(&request.limit){bail!("Use a 3–1000 character query and 1–10 results");}
    let _permit=INTAKE.acquire().await?;
    let mut record=AssetSearch {id:Uuid::new_v4(),project_id:project,catalog:request.catalog,query:request.query.trim().into(),
        created_at:Utc::now(),status:"requesting".into(),error:None,response_sha256:None,results:vec![]};
    db.put_asset_search(&record)?;
    let result=search_inner(db,&record,request.limit).await;
    match result {
        Ok((results,hash))=>{record.status="completed".into();record.results=results;record.response_sha256=Some(hash);}
        Err(error)=>{record.status="failed".into();record.error=Some(format!("{error:#}"));}
    }
    db.put_asset_search(&record)?; Ok(record)
}
async fn search_inner(db:&Database, record:&AssetSearch, limit:usize)->anyhow::Result<(Vec<Candidate>,String)> {
    if record.catalog==Catalog::Literature {
        let (sources,hash,hits)=super::literature::retrieve(&record.query).await?;
        let search=super::Search {id:record.id,project_id:record.project_id,query:record.query.clone(),created_at:record.created_at,
            status:"completed".into(),error:None,response_sha256:Some(hash.clone()),total_hits:hits,results:sources};
        db.put_research_search(&search)?;
        return Ok((search.results.iter().take(limit).map(|source|Candidate {catalog:Catalog::Literature,accession:source.id.clone(),
            title:source.title.clone(),source_url:source.url.clone(),description:source.abstract_text.chars().take(1600).collect(),preview_url:None}).collect(),hash));
    }
    let client=client()?;
    let response=if record.catalog==Catalog::Rcsb {
        client.post("https://search.rcsb.org/rcsbsearch/v2/query").json(&json!({"query":{"type":"terminal","service":"full_text","parameters":{"value":record.query}},
            "return_type":"entry","request_options":{"paginate":{"start":0,"rows":limit},"results_content_type":["experimental"]}})).send().await?
    } else {
        client.get("https://images-api.nasa.gov/search").query(&[("q",record.query.as_str()),("media_type","image"),("page_size","10")]).send().await?
    };
    let empty=response.status()==StatusCode::NO_CONTENT;
    let bytes=bounded(response,MAX_JSON).await?;
    let hash=format!("{:x}",Sha256::digest(&bytes));
    if empty{return Ok((vec![],hash));}
    let value:Value=serde_json::from_slice(&bytes)?; let mut candidates=Vec::new();
    if record.catalog==Catalog::Rcsb {
        let rows=value["result_set"].as_array().context("Invalid RCSB search envelope")?;
        for row in rows.iter().take(limit) {
            let Ok(id)=identifier(&Catalog::Rcsb,row["identifier"].as_str().unwrap_or("")) else {continue;};
            candidates.push(Candidate {catalog:Catalog::Rcsb,title:format!("PDB {id}"),source_url:format!("https://www.rcsb.org/structure/{id}"),
                accession:id,description:RCSB_SEARCH_DESCRIPTION.into(),preview_url:None});
        }
        candidates=stream::iter(candidates).map(|mut candidate| {
            let client=client.clone();
            async move {
                let metadata=async {
                    let bytes=bounded(client.get(format!("https://data.rcsb.org/rest/v1/core/entry/{}",candidate.accession)).timeout(Duration::from_secs(10)).send().await?,MAX_JSON).await?;
                    serde_json::from_slice::<Value>(&bytes).context("Invalid RCSB entry metadata")
                }.await;
                if let Ok(metadata)=metadata {
                    if let Some(title)=metadata.pointer("/struct/title").and_then(Value::as_str){candidate.title=title.chars().take(500).collect();}
                    let methods=metadata["exptl"].as_array().map(|rows|rows.iter().filter_map(|row|row["method"].as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                    candidate.description=format!("PDB {}; method: {}. Reported resolution (angstrom): {}. Deposited structural model; inspect the archive record before interpreting.",candidate.accession,methods,metadata.pointer("/rcsb_entry_info/resolution_combined").unwrap_or(&Value::Null));
                }
                candidate
            }
        }).buffered(3).collect().await;
    } else {
        let rows=value.pointer("/collection/items").and_then(Value::as_array).context("Invalid NASA image search envelope")?;
        for row in rows.iter().take(limit) {
            let data=&row["data"][0]; let Ok(id)=identifier(&Catalog::Nasa,data["nasa_id"].as_str().unwrap_or("")) else {continue;};
            let preview=row["links"].as_array().and_then(|links|links.iter().find_map(|link|link["href"].as_str().and_then(|href|nasa_image_url(href,&id).ok()).map(|url|url.to_string())));
            candidates.push(Candidate {catalog:Catalog::Nasa,source_url:format!("https://images.nasa.gov/details/{id}"),accession:id,
                title:data["title"].as_str().unwrap_or("NASA image").chars().take(500).collect(),
                description:data["description"].as_str().unwrap_or("").chars().take(1800).collect(),preview_url:preview});
        }
    }
    Ok((candidates,hash))
}

pub async fn import_catalog(db:&Database,project:Uuid,request:ImportRequest)->anyhow::Result<ResearchAsset> {
    db.get_project(project)?.context("Research world not found")?;
    let accession=identifier(&request.catalog,&request.accession)?;
    if let Some(asset)=db.research_assets(project)?.into_iter().find(|asset|asset.catalog==request.catalog && asset.accession==accession){return Ok(asset);}
    let _permit=INTAKE.acquire().await?; let client=client()?;
    let source_url=if request.catalog==Catalog::PublicFile {accession.clone()}
    else if request.catalog==Catalog::Rcsb {format!("https://files.rcsb.org/download/{accession}.pdb")}
    else {
        let bytes=bounded(client.get(format!("https://images-api.nasa.gov/asset/{accession}")).send().await?,MAX_JSON).await?;
        let index:Value=serde_json::from_slice(&bytes)?;
        let mut urls=index.pointer("/collection/items").and_then(Value::as_array).context("Invalid NASA asset envelope")?.iter()
            .filter_map(|item|item["href"].as_str().and_then(|href|nasa_image_url(href,&accession).ok())).collect::<Vec<_>>();
        urls.sort_by_key(|url|if url.path().contains("~medium."){0}else if url.path().contains("~small."){1}else{2});
        urls.first().context("NASA did not supply a supported static image")?.to_string()
    };
    let bytes=bounded(client.get(&source_url).send().await?,MAX_ASSET).await?;
    let title=db.asset_searches(project)?.iter().flat_map(|search|search.results.iter()).find(|candidate|candidate.catalog==request.catalog && candidate.accession==accession)
        .map(|candidate|candidate.title.clone()).unwrap_or_else(||if request.catalog==Catalog::PublicFile {super::public_file::filename(&accession)}else{accession.clone()});
    save_download(db,project,request.catalog,accession,title,source_url,bytes)
}
#[derive(Default)]
struct PdbHeader {title:String,method:String,resolution:Option<f64>}
fn pdb_header(text:&str)->PdbHeader {
    fn append(target:&mut String,field:&str,limit:usize) {
        for word in field.split_whitespace() {
            if target.len()>=limit {break;}
            if !target.is_empty(){target.push(' ');}
            target.extend(word.chars().take(limit.saturating_sub(target.len())));
        }
    }
    let mut header=PdbHeader::default();
    // PDB record names occupy columns 1–6; continuation numbers are 9–10.
    // Read only the bounded header, never atom records or arbitrary REMARK text.
    for line in text.lines().take(20000) {
        let record=line.get(..6).unwrap_or(line).trim();
        if matches!(record,"ATOM"|"HETATM"|"MODEL"|"END"){break;}
        let field=line.get(10..line.len().min(80)).unwrap_or("").trim();
        match record {
            "TITLE"=>append(&mut header.title,field,500),
            "EXPDTA"=>append(&mut header.method,field,200),
            "REMARK" if line.get(6..10).is_some_and(|number|number.trim()=="2") && header.resolution.is_none()=>{
                if let Some(value)=field.strip_prefix("RESOLUTION.") {
                    let mut words=value.split_whitespace();
                    let resolution=words.next().and_then(|word|word.parse::<f64>().ok());
                    if words.next()==Some("ANGSTROMS.") {header.resolution=resolution.filter(|value|value.is_finite() && *value>0.);}
                }
            }
            _=>{},
        }
    }
    header
}
impl PdbHeader {
    fn description(&self,accession:&str)->String {
        if self.title.is_empty() && self.method.is_empty() && self.resolution.is_none(){return String::new();}
        let mut text=format!("PDB {accession}; metadata read from the downloaded PDB header.");
        if !self.title.is_empty(){text.push_str(&format!(" Title: {}.",self.title));}
        if !self.method.is_empty(){text.push_str(&format!(" Method: {}.",self.method));}
        if let Some(resolution)=self.resolution {text.push_str(&format!(" Reported resolution: {resolution} angstrom."));}
        text.push_str(" Deposited structural model; inspect archive methods and the represented components before interpreting.");text
    }
}
fn save_download(db:&Database,project:Uuid,catalog:Catalog,accession:String,mut title:String,source_url:String,bytes:Vec<u8>)->anyhow::Result<ResearchAsset> {
    if bytes.is_empty() || bytes.len()>MAX_ASSET {bail!("Downloaded asset is empty or exceeds 8 MiB");}
    let public=if catalog==Catalog::PublicFile {Some(super::public_file::validate(&source_url,&bytes)?)} else {None};
    let (mime,molecule_id,provenance)=if catalog==Catalog::Rcsb || public.as_ref().is_some_and(|file|file.mime=="chemical/x-pdb") {
        let text=std::str::from_utf8(&bytes).context("PDB structure is not UTF-8 text")?;
        let structure=crate::science::molecular::import_structure(crate::domain::ImportStructureRequest {project_id:Some(project),name:if catalog==Catalog::PublicFile {title.clone()}else{format!("{accession}.pdb")},
            format:crate::domain::MolecularFormat::Pdb,content:text.into()})?;
        let molecule_id=structure.id; db.put_molecule(&structure)?;
        ("chemical/x-pdb",Some(molecule_id),if catalog==Catalog::Rcsb {"RCSB PDB experimental archive coordinates. A deposited structural model, not a live biological measurement; absent bonds may be inferred locally. Preserve archive methods and uncertainty when interpreting.".to_owned()}
            else {"Imported PDB-format coordinates from the user-selected public source. Experimental/computational origin has not been independently verified; inspect source methods. Absent bonds may be inferred locally.".into()})
    } else if let Some(file)=&public { (file.mime,None,"User-selected public data file. Format and allocation bounds were checked; scientific accuracy, authorship, license and intended use are not inferred from the host. No file content is executed.".to_owned())
    } else { (image_mime(&bytes)?,None,"NASA Image and Video Library source image. It may be a photograph, processed observation, composite, or artist illustration; consult the linked source description and credit/usage terms. The image itself is not numerical simulation evidence.".to_owned()) };
    let mut source_description=if let Some(file)=&public {file.description.clone()}else{db.asset_searches(project)?.iter().flat_map(|search|search.results.iter())
        .find(|candidate|candidate.catalog==catalog && candidate.accession==accession).map(|candidate|candidate.description.clone()).unwrap_or_default()};
    if catalog==Catalog::Rcsb {
        let header=pdb_header(std::str::from_utf8(&bytes)?);
        if (title.trim().is_empty() || title.trim()==accession || title.trim()==format!("PDB {accession}")) && !header.title.is_empty(){title=header.title.clone();}
        if source_description.trim().is_empty() || source_description==RCSB_SEARCH_DESCRIPTION {
            let fallback=header.description(&accession);if !fallback.is_empty(){source_description=fallback;}
        }
    }
    let asset=ResearchAsset {id:Uuid::new_v4(),project_id:project,catalog,accession,title,source_url,sha256:format!("{:x}",Sha256::digest(&bytes)),
        mime_type:mime.into(),size_bytes:bytes.len(),molecule_id,created_at:Utc::now(),provenance,source_description};
    db.put_research_asset(&asset,&bytes)?;
    if let Some(mut profile)=public.and_then(|file|file.csv_profile) {
        profile["id"]=json!(asset.id);profile["project_id"]=json!(project);profile["name"]=json!(asset.title);profile["source_url"]=json!(asset.source_url);
        profile["created_at"]=json!(asset.created_at);profile["source_asset_id"]=json!(asset.id);
        profile["scope"]=json!("Local descriptive statistics only. Original CSV bytes are preserved as the linked public source asset; no causal or clinical inference.");
        db.put_research_data(asset.id,&profile)?;
    }
    Ok(asset)
}

pub fn context(db:&Database,project:Uuid)->anyhow::Result<Value> {
    let assets=db.research_assets(project)?.into_iter().take(12).map(|asset|json!({"id":asset.id,"catalog":asset.catalog,"accession":asset.accession,
        "title":asset.title,"source_url":asset.source_url,"sha256":asset.sha256,"molecule_id":asset.molecule_id,"provenance":asset.provenance,"source_description":asset.source_description})).collect::<Vec<_>>();
    Ok(json!(assets))
}

/// Opt-in discovery uses public catalog queries only. Original files and source
/// receipts stay local; remote text is never promoted to application instructions.
fn focused_query(prompt:&str)->String {
    let lower=prompt.to_ascii_lowercase();
    let words=lower.split(|c:char|!c.is_alphanumeric() && c!='-' && c!='*').filter(|word|!word.is_empty()).collect::<Vec<_>>();
    let has=|word:&str|words.contains(&word);
    let mut terms=Vec::new();
    let galactic_center = (lower.contains("milky way") || lower.contains("galactic center") || lower.contains("galactic centre")) && (lower.contains("black hole") || has("blackhole"));
    if has("sagittarius") || lower.contains("sgr a") || galactic_center {
        terms.push("Sagittarius A*");
        if has("s2"){terms.push("S2");}
        if has("orbit") || has("orbital") || has("star") || has("stars"){terms.push("orbit");}
        if has("precession") || has("schwarzschild"){terms.push("Schwarzschild precession");}
    } else if lower.contains("black hole") || has("blackhole") {
        terms.push("black hole");
        for (match_word,phrase) in [("accretion","accretion disk"),("photon","photon ring"),("kerr","Kerr"),("schwarzschild","Schwarzschild"),("orbit","orbit"),("orbital","orbit"),("lensing","gravitational lensing")] {
            if has(match_word) && !terms.contains(&phrase){terms.push(phrase);}
        }
    } else if has("hiv") || has("hiv-1") || has("aids") {
        terms.push("HIV-1");
        if has("env") || has("envelope") || has("spikes"){terms.push("Env trimer");}
        for word in ["protease","capsid","gp120","gp140","pgt122","pgt128","vrc01","neutralizing","inhibitor","drug","resistance"] {if has(word){terms.push(word);}}
    }
    if !terms.is_empty(){return terms.join(" ").chars().take(250).collect();}
    let mut seen=std::collections::HashSet::new();
    words.into_iter().filter(|word|word.len()>1 && !word.bytes().all(|b|b.is_ascii_digit()) && !matches!(*word,
        "please"|"create"|"creating"|"build"|"building"|"design"|"designing"|"show"|"render"|"make"|"making"|"help"|"me"|"my"|"our"|"we"|"you"|"your"|"want"|"need"|"would"|"could"|"should"|"can"|"will"|"must"|"the"|"of"|"to"|"and"|"with"|"without"|"for"|"from"|"in"|"on"|"at"|"as"|"is"|"it"|"its"|"be"|"by"|"an"|"or"|"that"|"this"|"these"|"those"|"about"|"into"|"before"|"after"|"then"|"using"|"use"|"used"|"based"|"first"|"next"|"new"|"best"|"full"|"complete"|"detailed"|"small"|"simple"|"bounded"|"executable"|"supported"|"available"|"local"|"public"|"research"|"researcher"|"scientific"|"question"|"experiment"|"experiments"|"simulation"|"simulate"|"model"|"models"|"scene"|"visualization"|"workbench"|"studio"|"source"|"sources"|"data"|"evidence"|"look"|"find"|"gather"|"retrieve"|"inspect"|"result"|"results"|"task"|"work"|"run"|"running"|"review"|"test"|"tests"|"step"|"steps"|"minutes"|"hours"|"perform"|"conduct"|"do"|"not"|"only"|"all"))
        .filter(|word|seen.insert(*word)).take(12).collect::<Vec<_>>().join(" ").chars().take(250).collect()
}
pub async fn gather(db:&Database,project:Uuid,prompt:&str,token:&CancellationToken)->anyhow::Result<Value> {
    let action=async {
        let query=focused_query(prompt);
        if query.len()<3{return Ok(json!({"status":"skipped","reason":"No usable public query"}));}
        let lower=prompt.to_ascii_lowercase();
        let pdb_id=prompt.split(|ch:char|!ch.is_ascii_alphanumeric()).find(|word|word.len()==4 && word.bytes().any(|b|b.is_ascii_alphabetic()) && identifier(&Catalog::Rcsb,word).is_ok());
        let molecular=pdb_id.is_some() || ["protein","molecule","hiv","virus","dna","enzyme","ligand","receptor"].iter().any(|word|lower.contains(word));
        let space=lower.contains("black hole") || lower.contains("sgr a") || lower.split(|c:char|!c.is_ascii_alphanumeric()).any(|word|matches!(word,"blackhole"|"sagittarius"|"planet"|"planets"|"nebula"|"galaxy"|"star"|"stars"|"nasa"|"spacecraft"));
        let mut reports=Vec::new();let mut imported=Vec::new();let mut warnings=Vec::new();
        let literature=search_catalog(db,project,SearchRequest {catalog:Catalog::Literature,query:query.clone(),limit:5}).await?;
        if let Some(error)=&literature.error {warnings.push(error.clone());} reports.push(literature.id);
        if molecular || space {
            let catalog=if molecular {Catalog::Rcsb}else{Catalog::Nasa};
            let asset_query=if catalog==Catalog::Nasa && query.starts_with("Sagittarius A*") {"Sagittarius A".into()}
                else if catalog==Catalog::Nasa && query.starts_with("black hole") {"black hole".into()} else {query.clone()};
            let search=search_catalog(db,project,SearchRequest {catalog:catalog.clone(),query:asset_query,limit:5}).await?;
            let selected=if catalog==Catalog::Rcsb {pdb_id.map(str::to_owned).or_else(||search.results.first().map(|candidate|candidate.accession.clone()))}
                else {search.results.first().map(|candidate|candidate.accession.clone())};
            if let Some(error)=&search.error {warnings.push(error.clone());} reports.push(search.id);
            if let Some(accession)=selected {
                match import_catalog(db,project,ImportRequest {catalog,accession}).await {Ok(asset)=>imported.push(asset.id),Err(error)=>warnings.push(format!("{error:#}"))}
            }
        }
        Ok(json!({"status":"completed","search_ids":reports,"asset_ids":imported,"warnings":warnings,"query":query,
            "scope":"At most two catalog searches (literature may try one alternate provider), bounded entry metadata requests, and one bounded asset import. Crossref covers general scholarly metadata; Europe PMC covers biomedical literature. Query keywords are a heuristic; the full researcher brief remains the task context. Search matches are candidates, not relevance or scientific proof; this is not exhaustive web research."}))
    };
    tokio::select! {biased; _=token.cancelled()=>bail!("Public research was cancelled"),result=action=>result}
}

pub fn routes()->Router<Arc<AppState>> {
    Router::new().route("/api/projects/:id/assets",get(list_assets)).route("/api/projects/:id/assets/search",post(search))
        .route("/api/projects/:id/assets/import",post(import)).route("/api/assets/:id/content",get(content))
}
struct Error(anyhow::Error);
impl From<anyhow::Error> for Error {fn from(error:anyhow::Error)->Self {Self(error)}}
impl IntoResponse for Error {fn into_response(self)->Response {(StatusCode::UNPROCESSABLE_ENTITY,Json(json!({"error":{"message":format!("{:#}",self.0)}}))).into_response()}}
async fn list_assets(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Json<Value>,Error> {
    state.database.get_project(id)?.context("Research world not found")?;
    Ok(Json(json!({"assets":state.database.research_assets(id)?,"searches":state.database.asset_searches(id)?})))
}
async fn search(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<SearchRequest>)->Result<Json<AssetSearch>,Error> {Ok(Json(search_catalog(&state.database,id,request).await?))}
async fn import(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>,Json(request):Json<ImportRequest>)->Result<Json<ResearchAsset>,Error> {Ok(Json(import_catalog(&state.database,id,request).await?))}
async fn content(State(state):State<Arc<AppState>>,Path(id):Path<Uuid>)->Result<Response,Error> {
    let asset=state.database.get_research_asset(id)?.context("Asset not found")?;
    let bytes=state.database.research_asset_bytes(id)?.context("Asset bytes not found")?;
    let extension=match asset.mime_type.as_str(){"image/png"=>"png","image/jpeg"=>"jpg","model/stl"=>"stl","model/gltf-binary"=>"glb","text/csv"=>"csv","application/json"=>"json",_=>"pdb"};
    let filename=if asset.catalog==Catalog::PublicFile {super::public_file::filename(&asset.source_url)}else{format!("{}.{}",asset.accession,extension)};
    let disposition=format!("inline; filename=\"{filename}\"");
    Ok(([(header::CONTENT_TYPE,asset.mime_type),(header::X_CONTENT_TYPE_OPTIONS,"nosniff".into()),(header::CONTENT_DISPOSITION,disposition)],bytes).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn public_queries_preserve_scientific_subjects_beyond_verbose_task_instructions() {
        let prompt="Please use public research before designing the first executable bounded experiment. Build a small local model with the available supported capabilities to inspect the S2 star orbit around Sagittarius A* and Schwarzschild precession.";
        let original=prompt.to_owned();assert_eq!(focused_query(prompt),"Sagittarius A* S2 orbit Schwarzschild precession");assert_eq!(prompt,original);
        assert_eq!(focused_query("Please build a detailed HIV-1 Env trimer with the neutralizing PGT122 Fab antibody"),"HIV-1 Env trimer pgt122 neutralizing");
        assert_eq!(focused_query("Use public research before building the first executable bounded experiment for Riemann zeta function zeros"),"riemann zeta function zeros");
        assert_eq!(focused_query("Render a Kerr black hole with an accretion disk and photon ring"),"black hole accretion disk photon ring Kerr");
        assert_eq!(focused_query("Build a simulation of the blackhole at the center of the milky way. Gather data on stars and other objects close by the super massive black hole"),"Sagittarius A* orbit");
        assert!(focused_query("please build the first complete experiment").is_empty());assert!(focused_query(&"unusualword ".repeat(10000)).len()<=250);
    }
    const ENV_HEADER:&str=concat!(
        "TITLE     CRYSTAL STRUCTURE OF THE BG505 SOSIP GP140 HIV-1 ENV TRIMER IN COMPLEX\n",
        "TITLE    2 WITH THE BROADLY NEUTRALIZING FAB PGT122\n",
        "EXPDTA    X-RAY DIFFRACTION\n",
        "REMARK   2\n",
        "REMARK   2 RESOLUTION.    4.70 ANGSTROMS.\n");
    #[test] fn canonical_pdb_header_joins_continuations_and_does_not_invent_resolution() {
        let header=pdb_header(ENV_HEADER);
        assert_eq!(header.title,"CRYSTAL STRUCTURE OF THE BG505 SOSIP GP140 HIV-1 ENV TRIMER IN COMPLEX WITH THE BROADLY NEUTRALIZING FAB PGT122");
        assert_eq!(header.method,"X-RAY DIFFRACTION");assert_eq!(header.resolution,Some(4.7));
        let nmr=pdb_header("EXPDTA    SOLUTION NMR;\nEXPDTA   2 SOLUTION SCATTERING\nREMARK   2 RESOLUTION. NOT APPLICABLE.\nATOM      1\nTITLE     THIS IS NOT A HEADER\nREMARK   2 RESOLUTION. 1.00 ANGSTROMS.\n");
        assert_eq!(nmr.method,"SOLUTION NMR; SOLUTION SCATTERING");assert_eq!(nmr.resolution,None);assert!(nmr.title.is_empty());
        for value in ["NaN","inf","-1","0"] {assert!(pdb_header(&format!("REMARK   2 RESOLUTION. {value} ANGSTROMS.\n")).resolution.is_none());}
        assert!(pdb_header("REMARK 200 RESOLUTION. 1.00 ANGSTROMS.\n").resolution.is_none());
        assert!(pdb_header("END\nTITLE     THIS IS NOT A HEADER\n").title.is_empty());
    }
    #[test] fn rcsb_import_retains_header_fallback_and_original_coordinates_without_metadata() {
        let atom="ATOM      1  CA  ALA A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n";
        let bytes=format!("{ENV_HEADER}{atom}").into_bytes();
        for placeholder in ["4NCO","PDB 4NCO",""] {
            let db=Database::open(std::path::Path::new(":memory:")).unwrap();let project=Uuid::new_v4();
            let asset=save_download(&db,project,Catalog::Rcsb,"4NCO".into(),placeholder.into(),"https://files.rcsb.org/download/4NCO.pdb".into(),bytes.clone()).unwrap();
            assert_eq!(asset.title,pdb_header(ENV_HEADER).title);
            assert!(asset.source_description.contains("downloaded PDB header"));assert!(asset.source_description.contains("Method: X-RAY DIFFRACTION"));
            assert!(asset.source_description.contains("Reported resolution: 4.7 angstrom"));assert!(asset.source_description.contains("FAB PGT122"));
            assert_eq!(asset.sha256,format!("{:x}",Sha256::digest(&bytes)));assert_eq!(db.research_asset_bytes(asset.id).unwrap().unwrap(),bytes);
            assert_eq!(db.get_molecule(asset.molecule_id.unwrap()).unwrap().unwrap().atoms[0].position,[1.,2.,3.]);
            assert_eq!(db.research_assets(project).unwrap()[0].source_description,asset.source_description);
        }
    }
    #[test] fn pdb_header_replaces_generic_search_description_but_preserves_catalog_metadata() {
        let bytes=format!("{ENV_HEADER}ATOM      1  CA  ALA A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n").into_bytes();
        for description in [RCSB_SEARCH_DESCRIPTION,"Catalog title, methods and curated resolution information"] {
            let db=Database::open(std::path::Path::new(":memory:")).unwrap();let project=Uuid::new_v4();
            db.put_asset_search(&AssetSearch {id:Uuid::new_v4(),project_id:project,catalog:Catalog::Rcsb,query:"4NCO".into(),created_at:Utc::now(),status:"completed".into(),error:None,response_sha256:None,
                results:vec![Candidate {catalog:Catalog::Rcsb,accession:"4NCO".into(),title:"Curated Env–Fab complex".into(),source_url:"https://www.rcsb.org/structure/4NCO".into(),description:description.into(),preview_url:None}]}).unwrap();
            let asset=save_download(&db,project,Catalog::Rcsb,"4NCO".into(),"Curated Env–Fab complex".into(),"https://files.rcsb.org/download/4NCO.pdb".into(),bytes.clone()).unwrap();
            assert_eq!(asset.title,"Curated Env–Fab complex");
            if description==RCSB_SEARCH_DESCRIPTION {assert!(asset.source_description.contains("downloaded PDB header"));}else{assert_eq!(asset.source_description,description);}
        }
    }
    #[test] fn catalog_intake_rejects_paths_and_external_hosts() {
        for id in ["https://localhost/x","../1HSG","1HSG.pdb","abcd","1HG"]{assert!(identifier(&Catalog::Rcsb,id).is_err());}
        assert_eq!(identifier(&Catalog::Rcsb,"1hsg").unwrap(),"1HSG");
        for url in ["ftp://images-assets.nasa.gov/image/x/x.jpg","https://evil.test/image/x/x.jpg","https://images-assets.nasa.gov/image/x/x.svg",
            "https://images-assets.nasa.gov/image/x/%2fsecret.jpg","https://images-assets.nasa.gov/image/x/../y/y.jpg"] {assert!(nasa_image_url(url,"x").is_err());}
        assert!(nasa_image_url("https://images-assets.nasa.gov/image/x/x~medium.jpg","x").is_ok());
        assert_eq!(nasa_image_url("http://images-assets.nasa.gov/image/x/x~medium.jpg","x").unwrap().scheme(),"https");
    }
    #[test] fn rejects_executable_images_and_decompression_budgets() {
        assert!(image_mime(b"<svg onload='run()'/>").is_err());
        let mut png=b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();png.extend_from_slice(&20000_u32.to_be_bytes());png.extend_from_slice(&20000_u32.to_be_bytes());
        assert!(image_mime(&png).is_err());
        png[16..20].copy_from_slice(&10_u32.to_be_bytes());png[20..24].copy_from_slice(&20_u32.to_be_bytes());assert_eq!(image_mime(&png).unwrap(),"image/png");
    }
    #[test] fn source_payload_is_hashed_persisted_and_linked_to_real_structure() {
        let db=Database::open(std::path::Path::new(":memory:")).unwrap();let project=Uuid::new_v4();
        let bytes=b"ATOM      1  CA  ALA A   1       1.000   2.000   3.000  1.00 20.00           C  \nEND\n".to_vec();
        let asset=save_download(&db,project,Catalog::Rcsb,"1HSG".into(),"Fixture".into(),"https://files.rcsb.org/download/1HSG.pdb".into(),bytes.clone()).unwrap();
        assert_eq!(asset.sha256,format!("{:x}",Sha256::digest(&bytes)));
        assert_eq!(db.research_asset_bytes(asset.id).unwrap().unwrap(),bytes);
        let structure=db.get_molecule(asset.molecule_id.unwrap()).unwrap().unwrap();assert_eq!(structure.atoms[0].position,[1.0,2.0,3.0]);
        assert_eq!(db.research_assets(project).unwrap().len(),1);assert!(db.research_assets(Uuid::new_v4()).unwrap().is_empty());
        db.delete_project(project).unwrap();assert!(db.research_asset_bytes(asset.id).unwrap().is_none());
    }
    #[test] fn public_csv_keeps_original_bytes_and_links_its_profile() {
        let db=Database::open(std::path::Path::new(":memory:")).unwrap();let project=Uuid::new_v4();
        let source="https://raw.githubusercontent.com/example/data/main/measurements.csv";
        let bytes=b"time,signal\n0,2\n1,4\n".to_vec();
        let asset=save_download(&db,project,Catalog::PublicFile,source.into(),"measurements.csv".into(),source.into(),bytes.clone()).unwrap();
        assert_eq!(asset.mime_type,"text/csv");assert_eq!(asset.source_url,source);assert!(asset.source_description.contains("time,signal"));
        assert_eq!(db.research_asset_bytes(asset.id).unwrap().unwrap(),bytes);
        let profiles=db.research_data(project).unwrap();assert_eq!(profiles.len(),1);assert_eq!(profiles[0]["source_asset_id"],json!(asset.id));assert_eq!(profiles[0]["rows"],2);
        let invalid=save_download(&db,project,Catalog::PublicFile,source.into(),"measurements.csv".into(),source.into(),b"<html>not a table</html>".to_vec());
        assert!(invalid.is_err());assert_eq!(db.research_assets(project).unwrap().len(),1);
    }
    #[tokio::test] #[ignore="Requires public GitHub raw-file network access"]
    async fn live_public_files_import_csv_and_self_contained_glb() {
        let db=Database::open(std::path::Path::new(":memory:")).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest {name:Some("Public file verification".into()),question:"Inspect source data and geometry".into()});db.put_project(&project).unwrap();
        let csv=import_catalog(&db,project.id,ImportRequest {catalog:Catalog::PublicFile,accession:"https://raw.githubusercontent.com/mwaskom/seaborn-data/master/iris.csv".into()}).await.unwrap();
        assert_eq!(csv.mime_type,"text/csv");assert_eq!(db.research_data(project.id).unwrap()[0]["rows"],150);
        let model=import_catalog(&db,project.id,ImportRequest {catalog:Catalog::PublicFile,accession:"https://raw.githubusercontent.com/KhronosGroup/glTF-Sample-Assets/main/Models/Box/glTF-Binary/Box.glb".into()}).await.unwrap();
        assert_eq!(model.mime_type,"model/gltf-binary");assert!(model.size_bytes>1000);assert!(model.source_description.contains("meshes"));
    }
    #[tokio::test] #[ignore="Requires public RCSB/NASA network access"]
    async fn live_public_catalogs_import_archive_coordinates_and_nasa_image() {
        let db=Database::open(std::path::Path::new(":memory:")).unwrap();
        let project=crate::domain::ResearchProject::new(crate::domain::CreateProjectRequest {name:Some("Public catalog verification".into()),question:"Inspect public source records".into()});
        db.put_project(&project).unwrap();
        let found=search_catalog(&db,project.id,SearchRequest {catalog:Catalog::Rcsb,query:"HIV protease".into(),limit:2}).await.unwrap();
        assert_eq!(found.status,"completed","{:?}",found.error);assert!(!found.results.is_empty());
        let structure=import_catalog(&db,project.id,ImportRequest {catalog:Catalog::Rcsb,accession:"1HSG".into()}).await.unwrap();
        assert!(db.get_molecule(structure.molecule_id.unwrap()).unwrap().unwrap().atoms.len()>1000);
        let nasa=search_catalog(&db,project.id,SearchRequest {catalog:Catalog::Nasa,query:"black hole".into(),limit:2}).await.unwrap();
        assert_eq!(nasa.status,"completed","{:?}",nasa.error);
        let image=import_catalog(&db,project.id,ImportRequest {catalog:Catalog::Nasa,accession:nasa.results[0].accession.clone()}).await.unwrap();
        assert!(image.mime_type.starts_with("image/"));assert!(image.size_bytes>1000);
    }
}
