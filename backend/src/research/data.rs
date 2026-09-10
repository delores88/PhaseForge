//! Small local CSV profiles; never substitute these descriptive statistics for
//! validation, causal inference, molecular simulation or evidence of a cure.
use anyhow::{bail,Result};use serde_json::{json,Value};use sha2::{Digest,Sha256};
pub fn profile(text:&str)->Result<Value>{
    if text.len()>1024*1024{bail!("CSV limit is 1 MiB; use a scoped deidentified data table");}
    let rows=parse(text)?;if rows.len()<2{bail!("CSV requires headers and data");}
    let header=&rows[0];let mut seen=std::collections::HashSet::new();
    if header.is_empty()||header.len()>128||header.iter().any(|h|h.trim().is_empty()||h.len()>160||!seen.insert(h.trim())){bail!("CSV requires 1–128 unique bounded nonempty headers");}
    if rows.iter().any(|r|r.len()!=header.len()){bail!("ragged CSV rows; none silently dropped");}
    let columns=header.iter().enumerate().map(|(i,name)|{
        let(mut count,mut missing,mut nonnumeric,mut nonfinite)=(0usize,0usize,0usize,0usize);let(mut mean,mut m2,mut min,mut max)=(0.0f64,0.0f64,f64::INFINITY,f64::NEG_INFINITY);
        for row in rows.iter().skip(1){let v=row[i].trim();if v.is_empty()||matches!(v.to_ascii_lowercase().as_str(),"na"|"n/a"|"null"|"missing"){missing+=1;continue;}
            match v.parse::<f64>(){Ok(x) if x.is_finite()=>{count+=1;let d=x-mean;mean+=d/count as f64;m2+=d*(x-mean);min=min.min(x);max=max.max(x);},Ok(_)=>nonfinite+=1,Err(_)=>nonnumeric+=1,}
        }
        let valid=mean.is_finite()&&m2.is_finite();json!({"name":name,"finite_numeric":count,"missing":missing,"nonnumeric":nonnumeric,"nonfinite":nonfinite,
            "mean":if count>0&&valid{Some(mean)}else{None},"minimum":if count>0{Some(min)}else{None},"maximum":if count>0{Some(max)}else{None},"sample_std_dev":if count>1&&valid{Some((m2/(count-1) as f64).max(0.0).sqrt())}else{None},"aggregate_overflow":!valid})
    }).collect::<Vec<_>>();
    Ok(json!({"schema_version":"phaseforge-csv-profile/1","sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"rows":rows.len()-1,"columns":columns,
        "scope":"Local descriptive statistics only. Raw rows are NOT retained; preserve the original CSV for reproduction. Blank/NA/N/A/null/missing count as missing; NaN/infinity count separately. Summaries and column names can enter later model context; use deidentified data. No causal/clinical inference."}))
}
fn parse(s:&str)->Result<Vec<Vec<String>>>{let mut chars=s.trim_start_matches('\u{feff}').chars().peekable();let(mut rows,mut row,mut field)=(Vec::new(),Vec::new(),String::new());let(mut quoted,mut closed)=(false,false);
    while let Some(c)=chars.next(){if quoted{if c=='"'{if chars.peek()==Some(&'"'){field.push('"');chars.next();}else{quoted=false;closed=true;}}else{field.push(c);}}
    else{match c{'"' if field.is_empty()&&!closed=>quoted=true,','=>{row.push(std::mem::take(&mut field));closed=false;},'\r'|'\n'=>{if c=='\r'&&chars.peek()==Some(&'\n'){chars.next();}row.push(std::mem::take(&mut field));rows.push(std::mem::take(&mut row));closed=false;},_ if closed&&!c.is_whitespace()=>bail!("unexpected text after CSV quote"),_ if closed=>{},'"'=>bail!("unexpected quote in CSV"),_=>field.push(c)}}
        if row.len()>128||rows.len()>20000||field.len()>65536{bail!("CSV row/column/field bound exceeded");}
    }if quoted{bail!("unterminated CSV quote");}if !row.is_empty()||!field.is_empty()||closed{row.push(field);rows.push(row);}Ok(rows)
}
#[cfg(test)]mod tests{use super::*;
#[test]fn quotes_missingness_and_statistics(){let p=profile("x,label\r\n1,\"a,b\"\r\n3,\"c\"\"d\"\r\nNA,unknown\r\n").unwrap();assert_eq!(p["rows"],3);assert_eq!(p["columns"][0]["mean"],2.0);assert_eq!(p["columns"][0]["missing"],1);}
#[test]fn nonfinite_is_not_zero(){let p=profile("x\nNaN\nInfinity\n").unwrap();assert_eq!(p["columns"][0]["nonfinite"],2);assert!(p["columns"][0]["mean"].is_null());}
#[test]fn malformed_is_not_silently_repaired(){for s in ["x,x\n1,2\n","x,y\n1\n","x\n\"bad"]{assert!(profile(s).is_err());}}
}
