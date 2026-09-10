//! General scholarly metadata for opt-in research mode. Fixed endpoints only;
//! returned abstracts and titles are evidence, never instructions or full papers.
use std::{collections::HashSet,time::Duration};
use anyhow::{bail,Context};
use serde_json::Value;
use sha2::{Digest,Sha256};
use super::Source;

type Retrieved=(Vec<Source>,String,Option<u64>);
static CROSSREF_GATE:tokio::sync::Semaphore=tokio::sync::Semaphore::const_new(1);
pub(super) fn biomedical(query:&str)->bool {
    query.split(|c:char|!c.is_ascii_alphanumeric()).any(|word|matches!(word.to_ascii_lowercase().as_str(),
        "hiv"|"aids"|"protein"|"proteins"|"enzyme"|"enzymes"|"dna"|"rna"|"virus"|"viruses"|"viral"|"ligand"|"receptor"|"drug"|"clinical"|"cancer"|"molecular"|"molecule"|"molecules"|"biology"|"biological"))
}
pub(super) async fn retrieve(query:&str)->anyhow::Result<Retrieved> {
    if !(3..=1000).contains(&query.trim().len()){bail!("Literature query must be 3–1000 characters");}
    let biomedical=biomedical(query);
    let primary=if biomedical {super::retrieve(query).await}else{crossref(query).await};
    if primary.as_ref().is_ok_and(|(sources,_,_)|!sources.is_empty()){return primary;}
    // One alternate catalog only, no repeated retry or broad crawling.
    let alternate=if biomedical {crossref(query).await}else{super::retrieve(query).await};
    choose_result(primary,alternate)
}
fn choose_result(primary:anyhow::Result<Retrieved>,alternate:anyhow::Result<Retrieved>)->anyhow::Result<Retrieved> {
    match (primary,alternate) {
        (_,Ok(result)) if !result.0.is_empty()=>Ok(result),
        (Ok(result),Ok(_))=>Ok(result),
        (Err(_),Ok(result))=>Ok(result),
        (Ok(_),Err(error))=>Err(error.context("Alternate literature catalog failed after the primary returned no sources")),
        (Err(first),Err(second))=>bail!("Both literature catalogs failed: {first:#}; alternate: {second:#}"),
    }
}
async fn crossref(query:&str)->anyhow::Result<Retrieved> {
    let _permit=CROSSREF_GATE.acquire().await?;
    let client=reqwest::Client::builder().timeout(Duration::from_secs(25)).redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("PhaseForge/",env!("CARGO_PKG_VERSION")," public-literature (https://github.com/delores88/PhaseForge)")).build()?;
    let mut response=client.get("https://api.crossref.org/works")
        .query(&[("query.bibliographic",query),("rows","15"),("select","DOI,title,author,issued,published,abstract")]).send().await?;
    if !response.status().is_success(){bail!("Crossref returned HTTP {}",response.status());}
    const MAX_BYTES:usize=4*1024*1024;
    if response.content_length().is_some_and(|length|length>MAX_BYTES as u64){bail!("Crossref response exceeds 4 MiB");}
    let mut bytes=Vec::new();
    while let Some(chunk)=response.chunk().await?{if bytes.len().saturating_add(chunk.len())>MAX_BYTES{bail!("Crossref response exceeds 4 MiB");}bytes.extend_from_slice(&chunk);}
    let value:Value=serde_json::from_slice(&bytes).context("Invalid Crossref JSON")?;
    Ok((parse_crossref(&value)?,format!("{:x}",Sha256::digest(&bytes)),value.pointer("/message/total-results").and_then(Value::as_u64)))
}
fn publication_date(row:&Value)->String {
    let parts=row.pointer("/published/date-parts/0").or_else(||row.pointer("/issued/date-parts/0")).and_then(Value::as_array);
    let Some(parts)=parts else{return String::new();};
    let Some(year)=parts.first().and_then(Value::as_i64).filter(|year|(1..=9999).contains(year)) else{return String::new();};
    let mut date=format!("{year:04}");
    if let Some(month)=parts.get(1).and_then(Value::as_u64).filter(|month|(1..=12).contains(month)) {
        date.push_str(&format!("-{month:02}"));
        if let Some(day)=parts.get(2).and_then(Value::as_u64).filter(|day|(1..=31).contains(day)){date.push_str(&format!("-{day:02}"));}
    }
    date
}
fn parse_crossref(value:&Value)->anyhow::Result<Vec<Source>> {
    let rows=value.pointer("/message/items").and_then(Value::as_array).context("Invalid Crossref envelope, not an empty successful search")?;
    let mut seen=HashSet::new();let mut sources=Vec::new();
    for row in rows.iter().take(15) {
        let Some(doi)=row["DOI"].as_str().map(str::trim) else{continue;};
        let Some((prefix,suffix))=doi.strip_prefix("10.").and_then(|doi|doi.split_once('/')) else{continue;};
        if prefix.len()<4 || !prefix.bytes().all(|b|b.is_ascii_digit()) || suffix.is_empty() || doi.len()>300 || doi.chars().any(|c|c.is_control()||c.is_whitespace()) {continue;}
        let id=format!("CROSSREF:{}",doi.to_ascii_lowercase());if !seen.insert(id.clone()){continue;}
        let title=super::plain(row.pointer("/title/0").and_then(Value::as_str).unwrap_or(""),1200);if title.is_empty(){continue;}
        // Build a DOI resolver link ourselves; never trust supplied arbitrary URLs.
        let mut url=url::Url::parse("https://doi.org/").expect("fixed DOI URL");url.set_path(&format!("/{doi}"));
        let authors=row["author"].as_array().into_iter().flatten().take(30).map(|author| {
            let name=author["name"].as_str().map(str::to_owned).unwrap_or_else(||format!("{} {}",author["given"].as_str().unwrap_or(""),author["family"].as_str().unwrap_or("")));
            super::plain(&name,200)
        }).collect::<Vec<_>>().join(", ").chars().take(2000).collect();
        let abstract_text=super::plain(row["abstract"].as_str().unwrap_or(""),16000);let date=publication_date(row);
        sources.push(Source{id,title,authors,year:date.chars().take(4).collect(),publication_date:date,doi:doi.into(),url:url.into(),
            scope:if abstract_text.is_empty(){"Crossref deposited scholarly metadata only; no abstract supplied or full text read"}else{"Crossref deposited scholarly metadata and abstract excerpt; full text not read"}.into(),abstract_text});
    }
    Ok(sources)
}

#[cfg(test)]mod tests {
    use super::*;use serde_json::json;
    fn fixture()->Value {json!({"status":"ok","message":{"total-results":42,"items":[
        {"DOI":"10.1051/0004-6361/202038" ,"title":["<i>Sagittarius A*</i> &amp; the S2 orbit"],"author":[{"given":"A.","family":"Researcher"}],
         "published":{"date-parts":[[2020,4,16]]},"abstract":"<jats:p>Measured <jats:italic>orbital</jats:italic> motion.</jats:p>","URL":"javascript:run()"},
        {"DOI":"10.1051/0004-6361/202038","title":["Duplicate"]},
        {"DOI":"10.1234/math","title":["A mathematical result"],"issued":{"date-parts":[[2018]]}},
        {"DOI":"https://evil.test/record","title":["Rejected external URL"]}]}})}
    #[test]fn crossref_retains_dates_abstract_scope_and_safe_doi_links() {
        let sources=parse_crossref(&fixture()).unwrap();assert_eq!(sources.len(),2);
        assert_eq!(sources[0].publication_date,"2020-04-16");assert_eq!(sources[0].year,"2020");assert!(sources[0].title.contains("Sagittarius A*"));
        assert!(!sources[0].abstract_text.contains('<'));assert!(sources[0].abstract_text.contains("orbital"));assert!(sources[0].scope.contains("full text not read"));
        assert_eq!(sources[0].url,"https://doi.org/10.1051/0004-6361/202038");assert_eq!(sources[1].publication_date,"2018");assert!(sources[1].abstract_text.is_empty());
        assert!(sources[1].scope.contains("no abstract supplied"));
    }
    #[test]fn invalid_crossref_and_failures_are_not_successful_empty_results() {
        assert!(parse_crossref(&json!({"error":"rate limited"})).is_err());
        assert!(choose_result(Err(anyhow::anyhow!("first failed")),Err(anyhow::anyhow!("second failed"))).is_err());
        let sources=parse_crossref(&fixture()).unwrap();
        assert_eq!(choose_result(Err(anyhow::anyhow!("primary offline")),Ok((sources,"hash".into(),Some(42)))).unwrap().0.len(),2);
        assert!(choose_result(Ok((vec![],"hash".into(),Some(0))),Err(anyhow::anyhow!("alternate offline"))).is_err());
    }
    #[test]fn general_science_uses_crossref_and_biomedicine_keeps_europe_pmc() {
        assert!(!biomedical("Sagittarius A* S2 orbit"));assert!(!biomedical("Riemann zeta function zeros"));assert!(biomedical("HIV-1 Env PGT122"));assert!(biomedical("protein dynamics"));
    }
    #[tokio::test]#[ignore="Requires one public Crossref query, no model call"]
    async fn live_crossref_sagittarius_s2_metadata() {
        let (sources,hash,hits)=crossref("Sagittarius A* S2 orbit").await.unwrap();assert!(!sources.is_empty());assert_eq!(hash.len(),64);assert!(hits.is_some_and(|n|n>0));
        assert!(sources.iter().any(|source|{let title=source.title.to_ascii_lowercase();title.contains("sagittarius") || title.contains("s2") || title.contains("galactic center")}));
        let db=crate::persistence::Database::open(std::path::Path::new(":memory:")).unwrap();let project=uuid::Uuid::new_v4();
        db.put_research_search(&super::super::Search{id:uuid::Uuid::new_v4(),project_id:project,query:"Sagittarius A* S2 orbit".into(),created_at:chrono::Utc::now(),status:"completed".into(),error:None,response_sha256:Some(hash),total_hits:hits,results:sources.clone()}).unwrap();
        assert_eq!(db.research_searches(project).unwrap()[0].results[0].doi,sources[0].doi);
        for source in sources.iter().take(3){println!("{} | {} | {}",source.title,source.publication_date,source.url);}
    }
}
