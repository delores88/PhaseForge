//! Streaming observations independent of display sampling and ODE state dimension.
//! Extrema are sampled, not rigorous continuous bounds. Durations use linear
//! interpolation; counts/first passages use hysteretic endpoint detections.
use std::collections::{BTreeMap,HashMap};
use anyhow::{bail,Result};
use crate::{domain::{TrajectoryMeasure,TrajectoryReducer as Kind},sandbox::Expression};
#[derive(Clone,Default)]struct Sum{value:f64,correction:f64}
impl Sum{fn add(&mut self,x:f64){let y=x-self.correction;let next=self.value+y;self.correction=(next-self.value)-y;self.value=next;}}
pub struct Tracker{items:Vec<Item>,start:f64,time:f64}
struct Item{spec:TrajectoryMeasure,expr:Expression,last:f64,low:f64,high:f64,area:Sum,below:Sum,above:Sum,inside:bool,entries:usize,first:Option<f64>}
impl Tracker {
    pub fn new(specs:&[TrajectoryMeasure],context:&HashMap<String,f64>,time:f64)->Result<Self>{
        if !time.is_finite(){bail!("initial trajectory time must be finite");}
        let mut items=Vec::with_capacity(specs.len());
        for spec in specs{let expr=Expression::parse(&spec.expression)?;let value=expr.eval(context)?;
            if !value.is_finite()||!spec.threshold.is_finite()||!spec.hysteresis.is_finite()||spec.hysteresis<0.0{bail!("invalid initial trajectory measurement {}",spec.name);}
            let inside=if matches!(spec.reducer,Kind::EntriesAbove|Kind::FirstAbove){value>spec.threshold}else{value<spec.threshold};
            items.push(Item{spec:spec.clone(),expr,last:value,low:value,high:value,area:Sum::default(),below:Sum::default(),above:Sum::default(),inside,entries:0,first:if inside{Some(0.0)}else{None}});
        }Ok(Self{items,start:time,time})
    }
    pub fn observe(&mut self,context:&HashMap<String,f64>,time:f64)->Result<()> {
        let dt=time-self.time;if !dt.is_finite()||dt<=0.0{bail!("trajectory sample times must increase");}
        for a in &mut self.items{let v=a.expr.eval(context)?;if !v.is_finite(){bail!("nonfinite trajectory measure {}",a.spec.name);}
            a.low=a.low.min(v);a.high=a.high.max(v);a.area.add(dt*(0.5*a.last+0.5*v));
            a.below.add(dt*fraction(a.last,v,a.spec.threshold));a.above.add(dt*fraction(-a.last,-v,-a.spec.threshold));
            let (x,k)=if matches!(a.spec.reducer,Kind::EntriesAbove|Kind::FirstAbove){(-v,-a.spec.threshold)}else{(v,a.spec.threshold)};
            if !a.inside && x<k-a.spec.hysteresis{a.inside=true;a.entries+=1;if a.first.is_none(){a.first=Some(time-self.start);}}
            else if a.inside && x>=k+a.spec.hysteresis{a.inside=false;}
            if !a.area.value.is_finite()||!a.below.value.is_finite()||!a.above.value.is_finite(){bail!("trajectory accumulation overflow for {}",a.spec.name);}
            if matches!(a.spec.reducer,Kind::Range) && !(a.high-a.low).is_finite(){bail!("trajectory range overflow for {}",a.spec.name);}
            a.last=v;
        }self.time=time;Ok(())
    }
    pub fn values(&self)->BTreeMap<String,f64>{
        let elapsed=self.time-self.start;let mut out=BTreeMap::new();
        for a in &self.items{let v=match a.spec.reducer{
            Kind::Minimum=>a.low,Kind::Maximum=>a.high,Kind::Range=>a.high-a.low,Kind::Integral=>a.area.value,
            Kind::TimeMean=>if elapsed>0.0{a.area.value/elapsed}else{a.last},
            Kind::DurationBelow=>a.below.value,Kind::DurationAbove=>a.above.value,
            Kind::EntriesBelow|Kind::EntriesAbove=>a.entries as f64,
            Kind::FirstBelow|Kind::FirstAbove=>a.first.unwrap_or(elapsed),
        };out.insert(a.spec.name.clone(),v);
            if matches!(a.spec.reducer,Kind::FirstBelow|Kind::FirstAbove){out.insert(format!("{}_observed",a.spec.name),if a.first.is_some(){1.0}else{0.0});}
        }out
    }
    pub fn extend(&self,context:&mut HashMap<String,f64>){context.extend(self.values());}
}
fn fraction(a:f64,b:f64,k:f64)->f64{if a<k&&b<k{1.0}else if a>=k&&b>=k{0.0}else{let f=((k-a)/(b-a)).clamp(0.0,1.0);if a<k{f}else{1.0-f}}}
#[cfg(test)]mod tests{
    use super::*;
    fn spec(kind:Kind)->TrajectoryMeasure{TrajectoryMeasure{name:"value".into(),expression:"x".into(),reducer:kind,unit:"1".into(),threshold:0.0,hysteresis:0.0}}
    fn observe(kind:Kind,samples:&[(f64,f64)])->BTreeMap<String,f64>{let c=|v|HashMap::from([("x".to_owned(),v)]);let mut t=Tracker::new(&[spec(kind)],&c(samples[0].1),samples[0].0).unwrap();for &(time,v) in &samples[1..]{t.observe(&c(v),time).unwrap();}t.values()}
    #[test]fn extrema_not_endpoint(){assert_eq!(observe(Kind::Maximum,&[(0.,0.),(1.,9.),(2.,0.)])["value"],9.);assert_eq!(observe(Kind::Minimum,&[(0.,0.),(1.,-4.),(2.,0.)])["value"],-4.);}
    #[test]fn time_weighted_mean(){assert_eq!(observe(Kind::TimeMean,&[(0.,0.),(1.,2.),(4.,2.)])["value"],1.75);}
    #[test]fn integral_and_duration(){assert_eq!(observe(Kind::Integral,&[(0.,0.),(1.,2.),(4.,2.)])["value"],7.);assert_eq!(observe(Kind::DurationBelow,&[(0.,1.),(2.,-1.)])["value"],1.);}
    #[test]fn initial_inside_not_entry(){assert_eq!(observe(Kind::EntriesBelow,&[(0.,-1.),(1.,-2.)])["value"],0.);}
    #[test]fn repeated_encounters(){assert_eq!(observe(Kind::EntriesBelow,&[(0.,1.),(1.,-1.),(2.,1.),(3.,-1.)])["value"],2.);}
    #[test]fn first_passage_is_censored(){let r=observe(Kind::FirstAbove,&[(0.,-2.),(2.,-1.)]);assert_eq!(r["value"],2.);assert_eq!(r["value_observed"],0.);}
    #[test]fn transient_crossing_not_erased(){let r=observe(Kind::FirstAbove,&[(0.,-1.),(1.,1.),(2.,-1.)]);assert_eq!(r["value"],1.);assert_eq!(r["value_observed"],1.);}
    #[test]fn hysteresis_suppresses_chatter(){let c=|x|HashMap::from([("x".into(),x)]);let mut s=spec(Kind::EntriesBelow);s.hysteresis=0.1;let mut t=Tracker::new(&[s],&c(1.),0.).unwrap();for (i,v) in [-0.2,0.02,-0.02,0.2,-0.2].iter().enumerate(){t.observe(&c(*v),(i+1) as f64).unwrap();}assert_eq!(t.values()["value"],2.);}
}
