import {useMemo,useState} from "react";
import {finite,number} from "@/lib/lab";
export default function MetricChart({series=[]}){
  const [name,setName]=useState("");const selected=series.find(s=>s.name===name)||series[0];
  const [hover,setHover]=useState(null);
  const model=useMemo(()=>{
    const points=(selected?.points||[]).filter(p=>p.length>=2&&finite(p[0])!==null&&finite(p[1])!==null);
    if(points.length<2)return null;
    let min=Infinity,max=-Infinity;for(const [,v] of points){min=Math.min(min,v);max=Math.max(max,v);}
    const t0=points[0][0],t1=points.at(-1)[0];const span=max-min||Math.max(Math.abs(max)*.01,1e-12);
    const stride=Math.max(1,Math.ceil(points.length/600));
    const poly=points.filter((_,i)=>i%stride===0||i===points.length-1).map(([t,v])=>`${60+((t-t0)/(t1-t0||1))*790},${180-((v-min)/span)*135}`).join(" ");
    return {points,poly,min,max,span,t0,t1,stride};
  },[selected]);
  return <section className="metricChart"><header><div><h3>How the measurement changed</h3><p>Recorded simulation samples, not model-generated values.</p></div><select value={selected?.name||""} aria-label="Metric to plot" onChange={e=>{setName(e.target.value);setHover(null);}}>{series.map(s=><option key={s.name} value={s.name}>{s.name.replaceAll("_"," ")}{s.unit?` (${s.unit})`:""}</option>)}</select></header>
    {!model?<p className="labNotice">This run did not retain enough time-series points for a chart.</p>:<><svg viewBox="0 0 920 225" role="img" aria-label={`${selected.name} over simulation time`} onMouseMove={e=>{const rect=e.currentTarget.getBoundingClientRect();const x=(e.clientX-rect.left)/rect.width*920;const ratio=Math.max(0,Math.min(1,(x-60)/790));setHover(model.points[Math.round(ratio*(model.points.length-1))]);}} onMouseLeave={()=>setHover(null)}>
      <line x1="60" y1="180" x2="850" y2="180"/><line x1="60" y1="45" x2="850" y2="45" className="chartGuide"/>
      <polyline points={model.poly} fill="none" stroke="var(--accent)" strokeWidth="2"/>
      <text x="60" y="30">{number(model.max)} {selected.unit}</text><text x="60" y="215">t {number(model.t0)}</text><text x="850" y="215" textAnchor="end">t {number(model.t1)}</text><text x="855" y="180">{number(model.min)}</text>
    </svg><div className="chartReadout">{hover?<>t = {number(hover[0])} · value = <strong>{number(hover[1])} {selected.unit}</strong></>:<>Minimum {number(model.min)} · maximum {number(model.max)} · {model.points.length.toLocaleString()} retained samples{model.stride>1?" (chart display downsampled)":""}</>}</div></>}
  </section>;
}
