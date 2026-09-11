import {createContext,useContext,useEffect,useState} from "react";
import {api} from "./api";
const LiveContext=createContext({telemetry:null,workflow:null,laboratoryJobs:[],error:null,receivedAt:0});
export function LiveRuntimeProvider({children}){
  const [state,setState]=useState({telemetry:null,workflow:null,laboratoryJobs:[],error:null,receivedAt:0});
  useEffect(()=>{
    let stopped=false,timer,controller;
    async function poll(){
      controller=new AbortController();const timeout=setTimeout(()=>controller.abort(),7000);
      try{
        const [telemetry,workflow,laboratory]=await Promise.all([api.telemetry(controller.signal),api.workflow(controller.signal),api.labJobs(null,controller.signal)]);
        if(!stopped)setState({telemetry,workflow,laboratoryJobs:laboratory.jobs||[],error:null,receivedAt:Date.now()});
      }catch(error){if(!stopped)setState(v=>({...v,error:error.name==="AbortError"?"Resource monitor timed out":error.message}));}
      finally{clearTimeout(timeout);if(!stopped)timer=setTimeout(poll,document.hidden?7000:1500);}
    }
    poll();return()=>{stopped=true;clearTimeout(timer);controller?.abort();};
  },[]);
  return <LiveContext.Provider value={state}>{children}</LiveContext.Provider>;
}
export const useLiveRuntime=()=>useContext(LiveContext);
