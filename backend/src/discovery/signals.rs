//! Post-hoc diagnostics of recorded samples. These do not retroactively change success criteria.
use serde_json::{json,Value};
pub fn inspect(result:&Value)->Value {
    let rows=result["series"].as_array().into_iter().flatten().take(32).map(|series|{
        let points=series["points"].as_array().into_iter().flatten().filter_map(|p|Some((p[0].as_f64()?,p[1].as_f64()?))).filter(|(t,x)|t.is_finite()&&x.is_finite()).collect::<Vec<_>>();
        if points.len()<3{return json!({"name":series["name"],"status":"insufficient_samples"});}
        let n=points.len() as f64;let mean=points.iter().map(|(_,v)|v).sum::<f64>()/n;
        let variance=points.iter().map(|(_,v)|(v-mean).powi(2)).sum::<f64>()/n;
        if !mean.is_finite() || !variance.is_finite(){return json!({"name":series["name"],"status":"numerical_range_exceeded","note":"Saved values are finite but derived statistics overflow; inspect raw data and rescale before interpretation."});}
        let min=points.iter().map(|(_,v)|*v).fold(f64::INFINITY,f64::min);
        let max=points.iter().map(|(_,v)|*v).fold(f64::NEG_INFINITY,f64::max);
        let turn=points.windows(3).filter(|p|(p[1].1-p[0].1)*(p[2].1-p[1].1)<0.0).count();
        let lag=if variance>1e-30{Some(points.windows(2).map(|p|(p[0].1-mean)*(p[1].1-mean)).sum::<f64>()/((n-1.0)*variance))}else{None};
        let dt=points[1].0-points[0].0;
        let uniform=dt>0.0&&points.windows(2).all(|p|((p[1].0-p[0].0)-dt).abs()<=dt.abs()*1e-6+1e-12);
        // Bounded DFT on an evenly strided prefix; indicative frequency, no significance test.
        let stride=((points.len()+1023)/1024).max(1);let sampled=points.iter().step_by(stride).map(|p|p.1).collect::<Vec<_>>();
        let mut spectral=None;
        if uniform&&sampled.len()>=8&&variance>1e-30 {
            let count=sampled.len();let avg=sampled.iter().sum::<f64>()/count as f64;
            let mut best=(0usize,0.0);
            for k in 1..=(count/2).min(128){let (mut re,mut im)=(0.0,0.0);
                for (j,v) in sampled.iter().enumerate(){let angle=2.0*std::f64::consts::PI*k as f64*j as f64/count as f64; re+=(v-avg)*angle.cos();im+=(v-avg)*angle.sin();}
                let power=re*re+im*im;if power>best.1{best=(k,power);}
            }
            if best.0>0{spectral=Some(json!({"frequency":best.0 as f64/(count as f64*dt*stride as f64),"sample_stride":stride,"sample_count":count,"frequency_bins_checked":(count/2).min(128)}));}
        }
        json!({"name":series["name"],"unit":series["unit"],"samples":points.len(),"mean":mean,"standard_deviation":variance.sqrt(),"minimum":min,"maximum":max,"turning_points":turn,"lag_one_correlation":lag,
            "uniform_timestamps":uniform,"dominant_frequency_hint":spectral,"endpoint_difference":points.last().unwrap().1-points[0].1})
    }).collect::<Vec<_>>();
    json!({"series":rows,"boundary":"Exploratory diagnostics on saved samples, not full-resolution state. DFT hints may alias and do not establish periodicity, chaos, statistical significance, or novelty; require prospective tests."})
}
#[cfg(test)]mod tests{
 use super::*;
 #[test]fn constant_signal_has_no_period_claim(){let p=(0..64).map(|i|json!([i as f64,2.0])).collect::<Vec<_>>();let r=inspect(&json!({"series":[{"name":"x","unit":"","points":p}]}));assert!(r["series"][0]["dominant_frequency_hint"].is_null());}
 #[test]fn sampled_sine_frequency_is_recovered(){let p=(0..64).map(|i|json!([i as f64,(2.0*std::f64::consts::PI*4.0*i as f64/64.0).sin()])).collect::<Vec<_>>();let r=inspect(&json!({"series":[{"name":"x","points":p}]}));assert!((r["series"][0]["dominant_frequency_hint"]["frequency"].as_f64().unwrap()-0.0625).abs()<1e-12);}
}
