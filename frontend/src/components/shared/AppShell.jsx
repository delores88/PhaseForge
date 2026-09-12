import {version as appVersion} from "../../../package.json";
import Link from "next/link";
import { useRouter } from "next/router";
import { Activity, Command, Cpu, Gauge, House, PanelLeftClose, PanelLeftOpen, Settings, Wallet, Plus, Folder, LoaderCircle, CircleAlert } from "lucide-react";
import { useEffect, useState } from "react";
import CommandPalette from "./CommandPalette";
import { ThemeToggle } from "@/lib/theme";
import {api} from '@/lib/api';
import {useLiveRuntime} from '@/lib/liveRuntime';
import {labJobPresentation} from '@/lib/laboratoryPresentation.mjs';
import jobStyles from './SidebarJobs.module.css';
const navigation = [
  { href:"/", label:"Laboratory", icon:House }, { href:"/runs/", label:"Runs", icon:Activity },
  { href:"/hardware/", label:"Compute", icon:Cpu }, { href:"/usage/", label:"Usage & cost", icon:Wallet },
  { href:"/settings/", label:"Settings", icon:Settings },
];
export default function AppShell({ backend, hardware, eventState, title, subtitle, actions, children }) {
  const router = useRouter();
  const {laboratoryJobs=[]}=useLiveRuntime();
  const jobActive=job=>['queued','running','provisioning','waiting'].includes(job.state);
  const jobUnread=job=>labJobPresentation(job).unread;
  const openJob=job=>{
    if(router.pathname==='/')window.dispatchEvent(new CustomEvent('phaseforge:open-job',{detail:{job}}));
    else router.push({pathname:'/',query:{project:job.project_id,lab_job:job.id}});
  };
  const [collapsed, setCollapsed] = useState(false);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [projects,setProjects]=useState([]), [activeProject,setActiveProject]=useState(null);
  useEffect(()=>{
    if(!backend?.connected)return;
    let alive=true;
    const refresh=()=>api.projects().then(r=>{if(alive){setProjects(r.projects||[]);try{setActiveProject(localStorage.getItem('phaseforge.activeProject'));}catch{}}}).catch(()=>{});
    const selected=e=>setActiveProject(e.detail?.id);
    refresh();const timer=setInterval(refresh,10000);
    window.addEventListener('phaseforge:projects-changed',refresh);window.addEventListener('phaseforge:project-active',selected);
    return()=>{alive=false;clearInterval(timer);window.removeEventListener('phaseforge:projects-changed',refresh);window.removeEventListener('phaseforge:project-active',selected);};
  },[backend?.connected]);
  const newProject=()=>{if(router.pathname==='/')window.dispatchEvent(new CustomEvent('phaseforge:new-question'));else router.push('/').then(()=>window.dispatchEvent(new CustomEvent('phaseforge:new-question')));};
  const chooseProject=id=>{if(router.pathname==='/')window.dispatchEvent(new CustomEvent('phaseforge:select-project',{detail:{id}}));else router.push({pathname:'/',query:{project:id}});};
  useEffect(() => {
    try { setCollapsed(localStorage.getItem("phaseforge.nav.collapsed.v2") === "true"); } catch {}
    const key = (event) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault(); setPaletteOpen((value) => !value);
      }
      if (event.key === "Escape") setPaletteOpen(false);
    };
    window.addEventListener("keydown", key); return () => window.removeEventListener("keydown", key);
  }, []);
  const toggle = () => setCollapsed((current) => {
    try { localStorage.setItem("phaseforge.nav.collapsed.v2", String(!current)); } catch {}
    return !current;
  });
  const gpu = hardware?.adapters?.find((adapter) => adapter.selected);
  return <div className={`appFrame ${collapsed ? "appFrame--compact" : ""}`}>
    <aside className="primarySidebar" aria-label="Primary navigation">
      <Link href="/" className="primaryBrand" title="PhaseForge · Scientific workbench" aria-label="PhaseForge · Scientific workbench">
        <img className="primaryBrandImage brandDark" src="/brand/phaseforge-horizontal-dark.svg?v=1.1" alt="" />
        <img className="primaryBrandImage brandLight" src="/brand/phaseforge-horizontal-light.svg?v=1.1" alt="" />
        <img className="primaryBrandSymbol brandDark" src="/brand/phaseforge-symbol-dark.svg?v=1.1" alt="" />
        <img className="primaryBrandSymbol brandLight" src="/brand/phaseforge-symbol-light.svg?v=1.1" alt="" />
        <span><small>SCIENTIFIC WORKBENCH</small></span>
      </Link>
      <button type="button" className="sidebarNew" onClick={newProject} title="New research project"><Plus size={16}/><span>New research</span></button>
      <div className="navSectionLabel">WORKSPACE</div>
      <nav>{navigation.map(({ href, label, icon: Icon }) => {
        const active = router.pathname.replace(/\/$/, "") === href.replace(/\/$/, "");
        return <Link key={href} href={href} title={label} aria-label={label} aria-current={active ? "page" : undefined} className={`primaryNavItem ${active ? "active" : ""}`}><Icon size={19} /><span>{label}</span></Link>;
      })}</nav>
      <div className="navSectionLabel">YOUR PROJECTS <span>{projects.length||''}</span></div>
      <div className="sidebarProjects">{projects.map(p=>{
        const jobs=laboratoryJobs.filter(j=>j.project_id===p.id),working=jobs.some(jobActive),unread=jobs.some(jobUnread);
        return <button type="button" key={p.id} title={p.name} className={`sidebarProject ${activeProject===p.id?'active':''}`} onClick={()=>chooseProject(p.id)}>{working?<LoaderCircle className={jobStyles.spin} size={14} aria-label="Work running"/>:<Folder size={14}/>}<span>{p.name}</span>{!working&&unread&&<i className={jobStyles.dot} aria-label="Unread completed work"/>}</button>;
      })}{!projects.length&&<p className="sidebarProjectEmpty">Your experiments and conversations live here.</p>}</div>
      {!!laboratoryJobs.length&&<div className={jobStyles.jobs} aria-label="Background jobs"><div className="navSectionLabel">JOBS</div>{[...laboratoryJobs].sort((a,b)=>Number(jobActive(b))-Number(jobActive(a))||Date.parse(b.created_at)-Date.parse(a.created_at)).slice(0,5).map(job=><button key={job.id} onClick={()=>openJob(job)} title={`${job.title} · ${job.state}`}>{jobActive(job)?<LoaderCircle size={14} className={jobStyles.spin}/>:jobUnread(job)?<i className={jobStyles.dot}/>:<Activity size={14}/>}<span>{job.title}<small>{job.state.replaceAll('_',' ')}</small></span></button>)}</div>}
      <div className="navBottom">
        <div className="navCompute" title={gpu?.name || "CPU fallback"}><Gauge size={16} /><span><small>DETECTED ACCELERATOR</small><strong>{gpu?.name || "CPU fallback"}</strong><em>{gpu ? `${gpu.vendor} · ${gpu.backend}` : `${hardware?.cpu?.logical_cores || "—"} CPU cores`}</em></span></div>
        <button type="button" className="navCollapse" onClick={toggle} aria-label={collapsed ? "Expand navigation" : "Collapse navigation"} title={collapsed ? "Expand navigation" : "Collapse navigation"}>
          {collapsed ? <PanelLeftOpen size={18} /> : <PanelLeftClose size={18} />}<span>Collapse navigation</span>
        </button>
      </div>
    </aside>
    <main className="appMain">
      <header className="appHeader"><div><h1>{title}</h1>{subtitle && <p>{subtitle}</p>}</div><div className="appHeaderActions">
        <span className={`connectionPill ${backend?.connected ? "connectionPill--online" : "connectionPill--offline"}`} title={`Backend ${backend?.connected ? "connected" : "unavailable"}; events ${eventState || "inactive"}`}><span className="connectionPill__dot" />{backend?.checking ? "Checking" : backend?.connected ? "Connected" : "Offline"}</span>
        <ThemeToggle /><button type="button" className="commandHint" onClick={() => setPaletteOpen(true)} aria-label="Open command palette"><Command size={14} /><span>Ctrl K</span></button>{actions}
      </div></header>
      <div className="pageContent">{children}</div>
    </main>
    <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} onNewQuestion={() => { setPaletteOpen(false); router.push("/").then(() => window.dispatchEvent(new CustomEvent("phaseforge:new-question"))); }} />
  </div>;
}
