import {useState} from "react";
import {Activity,ChevronDown,Cpu,MemoryStick} from "lucide-react";
import {useLiveRuntime} from "@/lib/liveRuntime";
import {bytes,number,percent,memoryPercent} from "@/lib/lab";
function Spark({values}){
  const finite=values.filter(v=>v!==null&&Number.isFinite(v));if(finite.length<2)return null;
  // All resource sparklines share a truthful fixed 0–100% scale.
  const points=finite.map((v,i)=>`${i*120/(finite.length-1)},${24-Math.max(0,Math.min(100,v))*.24}`).join(" ");
  return <svg viewBox="0 0 120 24" className="resourceSpark" aria-hidden="true"><polyline points={points} fill="none" stroke="currentColor" strokeWidth="1.6"/></svg>;
}
function Gauge({label,value,detail,values=[]}){
  const v=percent(value);return <div className={`resourceGauge ${v!==null&&v>=90?"resourceGauge--high":""}`} title={detail}>
    <span>{label}</span><strong>{v===null?"Unavailable":`${number(v)}%`}</strong><Spark values={values}/>
    <div className="resourceTrack"><i style={{width:v===null?0:`${v}%`}}/></div><small>{detail}</small>
  </div>;
}
export default function ResourceMonitor({full=false}){
  const [expanded,setExpanded]=useState(full);const {telemetry:t,error,receivedAt}=useLiveRuntime();
  const stale=!!error||!t?.sampled_at||Date.now()-Date.parse(t.sampled_at)>12000;
  const gpu=t?.gpus?.[0];const gpuStale=!t?.gpu_sampled_at||Date.now()-Date.parse(t.gpu_sampled_at)>15000;
  const history=t?.history||[];
  return <section className={`resourceMonitor ${expanded?"resourceMonitor--open":""}`} aria-label="Live machine resources">
    <button type="button" className="resourceToggle" onClick={()=>setExpanded(v=>!v)} aria-expanded={expanded}>
      <Activity size={14}/><strong>Machine resources</strong><span className={stale?"resourceStale":"resourceLive"}>{stale?"Waiting / stale":"Live"}</span>
      <span><Cpu size={12}/>CPU {stale?"—":`${Math.round(t.cpu_percent)}%`}</span>
      <span><MemoryStick size={12}/>RAM {stale?"—":`${bytes(t.ram_used_bytes)} / ${bytes(t.ram_total_bytes)}`}</span>
      <span>GPU {stale||gpuStale||gpu?.utilization_percent==null?"Unavailable":`${Math.round(gpu.utilization_percent)}%`}</span>
      <span>VRAM {stale||gpuStale?"—":bytes(gpu?.memory_used_bytes)}</span><ChevronDown size={14}/>
    </button>
    {expanded&&<div className="resourceDetails">
      {stale&&<p className="labNotice">{error||"Collecting a second CPU sample…"} Last measurements are not live. Unavailable is not zero.</p>}
      <div className="resourceGrid">
        <Gauge label="System CPU" value={stale?null:t?.cpu_percent} detail={`${t?.logical_cores||"—"} logical cores · all processes`} values={history.map(h=>h.cpu_percent)}/>
        <Gauge label="System RAM pressure" value={stale?null:t?.ram_percent} detail={`${bytes(t?.ram_used_bytes)} / ${bytes(t?.ram_total_bytes)} · available ${bytes(t?.ram_available_bytes)}`} values={history.map(h=>h.ram_percent)}/>
        {(t?.gpus||[]).map(g=><div key={g.id} className="gpuResourcePair"><Gauge label={g.name} value={gpuStale||stale?null:g.utilization_percent} detail={`${g.source} · device-wide`} values={history.map(h=>h.gpus?.find(v=>v.id===g.id)?.utilization_percent??null)}/><Gauge label="GPU dedicated memory" value={gpuStale||stale?null:memoryPercent(g.memory_used_bytes,g.memory_total_bytes)} detail={`${bytes(g.memory_used_bytes)} / ${bytes(g.memory_total_bytes)}${g.temperature_c==null?"":` · ${g.temperature_c}°C`}`} values={history.map(h=>{const v=h.gpus?.find(v=>v.id===g.id);return memoryPercent(v?.memory_used_bytes,v?.memory_total_bytes);})}/></div>)}
      </div>
      <p className="resourceProcess">PhaseForge backend process: <strong>{bytes(t?.backend_memory_bytes)}</strong> RAM · {number(t?.backend_cpu_percent)}% of total CPU capacity. Browser, other processes, and external engines are not included in the backend-process total.</p>
      <p className="resourceFootnote">{t?.gpu_note||"GPU counters have not been sampled."} CPU/RAM sampled every ~2 seconds; GPU every ~6 seconds. High usage indicates pressure, not a measured bottleneck or automatic throttling.</p>
    </div>}
  </section>;
}
