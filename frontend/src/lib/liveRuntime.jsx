import {createContext,useCallback,useContext,useEffect,useRef,useState} from "react";
import {api} from "./api";
import {loadObservedCompletedJobs,mergeAcknowledgedJobs,observedCompletedJobs} from './laboratory-read-state.mjs';
const LiveContext=createContext({telemetry:null,workflow:null,laboratoryJobs:[],error:null,receivedAt:0});
export function LiveRuntimeProvider({children}){
  const [state,setState]=useState({telemetry:null,workflow:null,laboratoryJobs:[],error:null,receivedAt:0});
  const jobsRef=useRef([]),jobsLoaded=useRef(false),watermarks=useRef(new Map());jobsRef.current=state.laboratoryJobs;
  const retainAcknowledgements=useCallback(jobs=>{
    for(const job of jobs)if(job.seen_at){const previous=watermarks.current.get(job.id);if(!previous||Date.parse(job.seen_at)>Date.parse(previous))watermarks.current.set(job.id,job.seen_at);}
    setState(current=>({...current,laboratoryJobs:mergeAcknowledgedJobs(current.laboratoryJobs,watermarks.current)}));
  },[]);
  const acknowledgeProject=useCallback(async (projectId,viewedAt=Date.now())=>{
    // Opening the first workspace can beat the initial global poll. Fetch that
    // snapshot explicitly, but leave completions after the click unread.
    const observed=await loadObservedCompletedJobs({projectId,viewedAt,jobs:jobsLoaded.current?jobsRef.current:null,fetchJobs:api.labJobs});
    for(let offset=0;offset<observed.length;offset+=1000){const result=await api.labProjectSeen(projectId,observed.slice(offset,offset+1000));retainAcknowledgements(result.jobs||[]);}
  },[retainAcknowledgements]);
  const acknowledgeJob=useCallback(async job=>{
    const observed=observedCompletedJobs([job],job.project_id)[0];
    if(observed)retainAcknowledgements([await api.labSeen(job.id,observed.completed_at)]);
  },[retainAcknowledgements]);
  useEffect(()=>{
    let stopped=false,timer,controller;
    async function poll(){
      controller=new AbortController();const timeout=setTimeout(()=>controller.abort(),7000);
      try{
        const [telemetry,workflow,laboratory]=await Promise.all([api.telemetry(controller.signal),api.workflow(controller.signal),api.labJobs(null,controller.signal)]);
        if(!stopped){jobsLoaded.current=true;setState({telemetry,workflow,laboratoryJobs:mergeAcknowledgedJobs(laboratory.jobs||[],watermarks.current),error:null,receivedAt:Date.now()});}
      }catch(error){if(!stopped)setState(v=>({...v,error:error.name==="AbortError"?"Resource monitor timed out":error.message}));}
      finally{clearTimeout(timeout);if(!stopped)timer=setTimeout(poll,document.hidden?7000:1500);}
    }
    poll();return()=>{stopped=true;clearTimeout(timer);controller?.abort();};
  },[]);
  return <LiveContext.Provider value={{...state,acknowledgeProject,acknowledgeJob}}>{children}</LiveContext.Provider>;
}
export const useLiveRuntime=()=>useContext(LiveContext);
