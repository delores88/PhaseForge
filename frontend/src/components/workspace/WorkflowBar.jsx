import {useEffect,useState} from "react";
import {Check,LoaderCircle,Square} from "lucide-react";
import {useLiveRuntime} from "@/lib/liveRuntime";
import {phaseDetails} from "@/lib/lab";
export default function WorkflowBar({projectId,run,hasManifest,explaining,onStop,pendingRevision}){
  const {workflow,error}=useLiveRuntime();const [,tick]=useState(0);
  useEffect(()=>{const timer=setInterval(()=>tick(v=>v+1),1000);return()=>clearInterval(timer);},[]);
  const info=phaseDetails(workflow,projectId,run,hasManifest,explaining,pendingRevision);
  const elapsed=info.started?Math.max(0,Math.floor((Date.now()-Date.parse(info.started))/1000)):null;
  return <section className="workflowBar operationBar" aria-label="Current operation">
    <div className="workflowStatus" role="status" aria-live="polite">{info.busy&&<LoaderCircle size={14} className="spin"/>}<strong>{info.label}</strong>{elapsed!==null&&<span>{elapsed}s elapsed</span>}{info.busy&&onStop&&<button type="button" onClick={()=>onStop(info.run)}><Square size={12}/>Stop</button>}</div>
    {info.progress!=null&&info.busy&&<div className="workflowExecution"><progress value={info.progress} max="100"/><span>{Math.floor(info.progress)}% numerical work plan</span></div>}
    <p>{error?"Workflow connection interrupted; status may be stale. ":""}{info.detail}</p>
  </section>;
}
