import {useEffect,useMemo,useRef,useState} from "react";
import {useRouter} from "next/router";
import Link from "next/link";
import {ArrowRight,BookOpen,Compass,Download,GitBranch,Pause,Play,Plus,RefreshCw,ShieldAlert,Sparkles,Square} from "lucide-react";
import {api} from "@/lib/api";
import {useModelSelection} from "@/lib/modelSelection";
import {downloadBlob,terminalStudy,fmt} from "@/lib/discovery";
import ResourceMonitor from "@/components/workspace/ResourceMonitor";
import ChatMarkdown from "@/components/workspace/ChatMarkdown";
import StudyBuilder from "./StudyBuilder";
import StudyResults from "./StudyResults";
import NotebookEditor from "./NotebookEditor";
import VerificationLab from "@/components/assurance/VerificationLab";

export default function DiscoveryView({backend,embeddedProjectId,embeddedManifestId}) {
  const {getSelection,getSelectionError}=useModelSelection();
  const router=useRouter();const [projects,setProjects]=useState([]),[projectId,setProjectId]=useState(""),[manifests,setManifests]=useState([]),[manifestId,setManifestId]=useState("");
  const [studies,setStudies]=useState([]),[studyId,setStudyId]=useState(""),[detail,setDetail]=useState(null),[builder,setBuilder]=useState(false),[tab,setTab]=useState("campaign");
  const [busy,setBusy]=useState(""),[error,setError]=useState(""),[notice,setNotice]=useState(""),[review,setReview]=useState(null),[refresh,setRefresh]=useState(0),[loading,setLoading]=useState(true);
  const designRequest=useRef(null);const notebookDirty=useRef(false);
  const project=projects.find(p=>p.id===projectId);const source=manifests.find(m=>m.id===manifestId);const s=detail?.study;
  const modelError=getSelectionError(projectId);
  const selectedModel=()=>{const problem=getSelectionError(projectId);if(problem)throw Error(problem);return {...getSelection(projectId)};};
  const bump=()=>setRefresh(v=>v+1);
  useEffect(()=>{
    if(!backend.connected||!router.isReady)return;
    let gone=false;setLoading(true);
    api.projects().then(r=>{if(gone)return;const rows=r.projects||[];setProjects(rows);const requested=String(embeddedProjectId||router.query.project||localStorage.getItem("phaseforge.activeProject")||"");setProjectId(rows.some(p=>p.id===requested)?requested:rows[0]?.id||"");}).catch(e=>{if(!gone)setError(e.message);}).finally(()=>{if(!gone)setLoading(false);});
    return()=>{gone=true;};
  },[backend.connected,router.isReady,router.query.project,embeddedProjectId]);
  useEffect(()=>{
    if(!projectId)return;let gone=false;setLoading(true);setDetail(null);setStudyId("");setReview(null);
    localStorage.setItem("phaseforge.activeProject",projectId);
    Promise.all([api.manifests(projectId,2000),api.studies(projectId)]).then(([m,r])=>{
      if(gone)return;const all=m.manifests||[],bases=all.filter(x=>!String(x.authored_by).startsWith("PhaseForge discovery"));setManifests(bases);
      const requested=String(embeddedManifestId||router.query.manifest||"");setManifestId(bases.some(x=>x.id===requested)?requested:bases[0]?.id||"");
      const rows=r.studies||[];setStudies(rows);const requestedStudy=String(router.query.study||"");setStudyId(rows.some(x=>x.id===requestedStudy)?requestedStudy:rows[0]?.id||"");
    }).catch(e=>{if(!gone)setError(e.message);}).finally(()=>{if(!gone)setLoading(false);});return()=>{gone=true;};
  },[projectId,router.query.manifest,router.query.study,embeddedManifestId]);
  useEffect(()=>{
    if(!studyId){setDetail(null);return;}let gone=false,timer,controller;
    const poll=async()=>{controller=new AbortController();try{const r=await api.study(studyId,controller.signal);if(gone)return;setDetail(r);setStudies(rows=>rows.map(x=>x.id===studyId?{...x,state:r.study.state,summary:r.summary,stage:r.study.stage}:x));timer=setTimeout(poll,document.hidden?8000:terminalStudy(r.study.state)?10000:1700);}catch(e){if(!gone&&e.name!=="AbortError"){setError(e.message);timer=setTimeout(poll,7000);}}};
    poll();return()=>{gone=true;clearTimeout(timer);controller?.abort();};
  },[studyId,refresh]);
  const action=async(label,fn)=>{setBusy(label);setError("");setNotice("");try{await fn();bump();}catch(e){setError(e.message);}finally{setBusy("");}};
  const created=study=>{setStudies(rows=>[{id:study.id,project_id:study.project_id,title:study.recipe.title,state:study.state,summary:{completed:0,planned:study.recipe.exploration_trials+2*study.recipe.validation_finalists}},...rows]);setStudyId(study.id);setBuilder(false);setTab("campaign");setReview(null);bump();};
  const designStudy=()=>action("design",async()=>{
    if(!source)return;
    const selection=selectedModel();
    const requestId=crypto.randomUUID();designRequest.current=requestId;
    try {
      const runs=await api.runs(1000,projectId);
      const baseline=(runs.runs||[]).find(r=>r.manifest_id===source.id&&r.status==="completed");
      const response=await api.sendMessage(projectId,{
        request_id:requestId,study_intent:"discovery",source_run_id:baseline?.id||null,...selection,agent_role:"explorer",auto_run:false,attachments:[],structure_id:null,
        content:`Design an explicitly bounded discovery study based on the exact source manifest below. Return a complete revised manifest, not a simulation run. Propose physically justified search variables and ranges; define 1–3 meaningful objectives and corresponding observable names, units, constraints excluding trivial or invalid solutions, and falsification rules. Preserve the scientific question and capability limitations. Do not claim novelty, fabricate literature, or insert pre-specified solutions. Explain suggested behavioral descriptors and useful bounds for a MAP-Elites archive, distinguishing suggestions from facts. Human approval is required before execution. Exact source: ${JSON.stringify(source)}`
      });
      setReview(response);
      if(response.manifest){setManifests(rows=>[response.manifest,...rows.filter(m=>m.id!==response.manifest.id)]);setManifestId(response.manifest.id);setBuilder(true);}
      setNotice(response.manifest?"AI prepared a new experiment revision and suggested bounds. Review the study form and freeze it before authorizing compute. No simulation was launched.":response.assistant_message?.content||response.notice||"No executable proposal was produced. Check provider configuration and capability limits in the Laboratory.");
    } finally {designRequest.current=null;}
  });
  const stopDesign=async()=>{if(designRequest.current){await api.cancelChatRequest(designRequest.current).catch(e=>setError(e.message));setNotice("Cancellation requested. Tokens already processed remotely may still be billed.");}};
  const exportStudy=()=>action("export",async()=>{const blob=await api.exportStudy(studyId);downloadBlob(`PhaseForge-study-${studyId}.zip`,blob);setNotice("Research bundle prepared. It contains saved author notes, references and project-level model usage, not credentials or chat history. Review data rights before sharing.");});
  const ask=next=>action("review",async()=>{const selection=selectedModel();const r=await (next?api.nextStudyProposal(studyId,selection):api.reviewStudy(studyId,selection));setReview(r);setNotice(next?"A metered proposal was prepared. No new simulation was started.":"Advisory review recorded in the research-world chat. Numerical data and pass/fail checks remain authoritative.");});
  if(!backend.connected)return <div className="discEmpty"><Compass size={30}/><h2>Connect the local laboratory</h2><p>{backend.error||"Start the PhaseForge backend to access discovery campaigns."}</p><button className="button button--secondary" onClick={backend.refresh}>Retry connection</button></div>;
  return <div className={`discoveryShell ${embeddedProjectId?"advancedEmbedded":""}`}>
    {!embeddedProjectId&&<ResourceMonitor/>}
    {projectId&&modelError&&<p className="discNotice">AI actions use this world's conversation model. {modelError} <Link href={`/?project=${projectId}`}>Choose in chat</Link></p>}
    <header className="discoveryHero"><div><span className="eyebrow">OPTIONAL TOOLS · NOT REQUIRED FOR A SIMULATION</span><h2>Batch runs & independent checks</h2><p>Approve a bounded search, let the lab explore and challenge candidates, then decide which claims deserve independent scrutiny.</p></div><div className="discWorld"><label>Research world<select value={projectId} disabled={!!busy||!!embeddedProjectId} onChange={e=>{if(notebookDirty.current&&!window.confirm("Switch worlds without saving the research record?"))return;notebookDirty.current=false;setProjectId(e.target.value);setBuilder(false);setReview(null);}}><option value="">Select a research world</option>{projects.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label><Link href="/" className="discTextButton">Return to the laboratory <ArrowRight size={14}/></Link></div></header>
    {error&&<div className="discError" role="alert"><ShieldAlert size={16}/><span>{error}</span><button onClick={()=>setError("")} aria-label="Dismiss error">×</button></div>}{notice&&<p className="discSuccess" role="status">{notice}</p>}
    {!projectId?<section className="discoveryCard discEmpty"><BookOpen size={27}/><h3>Start with a question and an executable baseline</h3><p>Create a research world in the Laboratory, ask the Builder to design an experiment, and examine its baseline evidence. Then launch a campaign here.</p><Link href="/" className="button button--primary">Open Laboratory</Link></section>:<>
      <nav className="discoveryTabs" aria-label="Advanced experiment tools">{[["campaign","Explore & validate"],["verify","Verify & compare"],["record","Research record"],["publish","Publication bundle"]].map(([id,title])=><button key={id} className={tab===id?"active":""} onClick={()=>setTab(id)}>{title}</button>)}</nav>
      <div hidden={tab!=="campaign"}>
        <section className="discLaunch discoveryCard"><div><h3>Choose an experiment, not a canned problem</h3><p>Start from any supported immutable manifest. AI study design is an optional metered call; local drafting is free.</p></div><label>Source revision<select value={manifestId} onChange={e=>setManifestId(e.target.value)} disabled={builder}><option value="">No authored manifest yet</option>{manifests.map(m=><option key={m.id} value={m.id}>Revision {m.revision} · {m.title}</option>)}</select></label><div className="discActions"><button className="button button--secondary" disabled={!source||!!busy||!!modelError} onClick={designStudy}><Sparkles size={15}/>{busy==="design"?"Agent designing…":"AI study design"}</button>{busy==="design"&&<button className="button button--danger" onClick={stopDesign}><Square size={13}/>Stop model</button>}<button className="button button--primary" disabled={!source||!!busy} onClick={()=>setBuilder(v=>!v)}><Plus size={15}/>{builder?"Hide draft":"New campaign"}</button></div></section>
        {builder&&source&&<StudyBuilder key={source.id} manifest={source} onCreated={created} onCancel={()=>setBuilder(false)}/>}
        <div className="discMainGrid"><aside className="discStudyRail discoveryCard"><header><h3>Campaign history</h3><button className="discIcon" aria-label="Refresh campaigns" onClick={()=>action("refresh",async()=>{const r=await api.studies(projectId);setStudies(r.studies||[]);})}><RefreshCw size={15}/></button></header>{studies.map(row=><button className={studyId===row.id?"selected":""} key={row.id} onClick={()=>{setStudyId(row.id);setReview(null);}}><strong>{row.title}</strong><span>{row.state.replaceAll("_"," ")} · {row.summary?.completed??0}/{row.summary?.planned??"—"} trials</span></button>)}{!studies.length&&<p>{loading?"Reading campaigns…":"No campaigns yet. Define a bounded search from an authored experiment above."}</p>}</aside>
        <div className="discStudyContent">{s?<>
          <section className="discoveryCard discStudyHeader"><div><span className="eyebrow">{s.recipe.strategy.replaceAll("_"," ")} · PROTOCOL {s.recipe_hash.slice(0,12)}</span><h2>{s.recipe.title}</h2><p>{s.recipe.hypothesis}</p><span className={`discBadge discBadge--${s.state}`}>{s.state.replaceAll("_"," ")}</span></div><div className="discActions">
            {["draft","paused","failed"].includes(s.state)&&<button className="button button--primary" disabled={!!busy} onClick={()=>action("start",async()=>{await api.startStudy(s.id);})}><Play size={15}/>{s.state==="draft"?"Approve & start budget":"Resume remaining budget"}</button>}
            {s.state==="running"&&<button className="button button--secondary" disabled={!!busy} onClick={()=>action("pause",async()=>{await api.pauseStudy(s.id);})}><Pause size={15}/>Pause campaign</button>}
            {["running","pausing","paused"].includes(s.state)&&<button className="button button--danger" disabled={!!busy} onClick={()=>action("cancel",async()=>{await api.cancelStudy(s.id);})}><Square size={14}/>End campaign</button>}
            {terminalStudy(s.state)&&s.trials.length>0&&<><button className="button button--secondary" disabled={!!busy||!!modelError} onClick={()=>ask(false)}><Sparkles size={15}/>{busy==="review"?"Agent working…":"Request AI review"}</button><button className="button button--secondary" disabled={!!busy||!!modelError} onClick={()=>ask(true)}><GitBranch size={15}/>Propose next experiment</button></>}
            <Link href="/usage/" className="discTextButton">Token budget / stop paid agents</Link>
          </div></section>
          {s.state==="draft"&&<div className="discNotice">Approval authorizes up to {s.recipe.exploration_trials+2*s.recipe.validation_finalists} local runs, {s.recipe.wall_seconds} wall seconds, and {s.recipe.auto_review?"one metered AI review at completion":"no automatic paid AI calls"}. Pause/end stops the active numerical run; completed results remain. Resume never resets the budget. Cancelling an individual trial from Runs does not end the whole campaign.</div>}
          {review&&<section className="discoveryCard discReview"><h3>AI scientific advisory</h3><ChatMarkdown>{review.assistant_message?.content||review.notice}</ChatMarkdown>{review.manifest&&<Link className="button button--primary" href={`/?panel=advanced&project=${projectId}&manifest=${review.manifest.id}`}>Use proposed revision {review.manifest.revision} for a new draft</Link>}<p className="discCaption">Advisory only. No discovery certification and no new simulation automatically launched. Full message is retained in the project chat.</p></section>}
          <StudyResults detail={detail} onRecord={()=>setTab("record")}/><section className="discoveryCard"><h3>Do not stop at an unusual score</h3><p>Freeze a promising candidate for an independent solver, new perturbations, known-result comparisons and a traceable literature review.</p><button className="button button--primary" onClick={()=>setTab("verify")}>Verify & compare this study</button></section>
        </>:<section className="discoveryCard discEmpty"><Compass size={28}/><h3>{studyId?"Reading campaign evidence…":"Your discovery record starts here"}</h3><p>Campaigns retain attempted candidates, diverse outcomes, numerical challenges and decisions rather than only the winning score.</p></section>}</div></div>
      </div>
      {tab==="verify"&&(s?<VerificationLab key={s.id} study={s}/>:<section className="discoveryCard discEmpty"><h3>Select a discovery campaign first</h3><p>Choose a completed or paused study in Explore & validate to build a candidate verification dossier.</p><button className="button button--primary" onClick={()=>setTab("campaign")}>Select campaign</button></section>)}
      <div hidden={tab!=="record"}>{projectId&&<NotebookEditor key={projectId} projectId={projectId} study={s} onSaved={bump} onDirty={value=>{notebookDirty.current=value;}}/>}</div>
      <div hidden={tab!=="publish"}><section className="discoveryCard discPublication"><header><div><span className="eyebrow">REPRODUCIBLE HANDOFF · NO AUTOMATIC SUBMISSION</span><h2>Take the work out of the chat</h2><p>A publication bundle is the beginning of expert review, not the end of it.</p></div><button className="button button--primary" disabled={!s||!terminalStudy(s.state)||!!busy} onClick={exportStudy}><Download size={16}/>{busy==="export"?"Building bundle…":"Export research ZIP"}</button></header>
        {!s?<p className="discNotice">Choose a campaign in Explore & validate first.</p>:<><p>Selected campaign: <strong>{s.recipe.title}</strong>. The export uses the latest <em>saved</em> research-record revision, including author notes and references. It excludes private keys, endpoint settings, chat history and rate cards.</p><div className="discPublicationGrid"><article><h3>Defensible results</h3><p>All attempts, raw metrics, exact manifests, run records, parameters, seeds, constraints and numerical challenge outcomes. CSV and SVG figures are derived from retained data.</p></article><article><h3>Independent checking</h3><p>A separately implemented Python standard-library ODE integrator replays Euler/RK4 systems without PhaseForge or an API key. Integrity verifier checks every payload hash. Use Verify & compare to actually execute independently implemented checks through a bounded Python worker. Only completed records are evidence; a supplied script alone is not.</p></article><article><h3>Author workspace</h3><p>A methods/results manuscript draft, claim-to-evidence ledger, independent verification dossiers, frozen numerical reference sets, literature search records, human reviews, BibTeX/CSL references, RO-Crate-style metadata, usage disclosure and explicit unresolved publication gates.</p></article></div>
        <h3>Unresolved review gates</h3><div className="discGaps">{detail.readiness?.gaps?.map((gap,i)=><p key={i}><ShieldAlert size={14}/>{gap}</p>)}</div><p className="discNotice">Same-engine refinement is not independent validation. DOI metadata is not a literature review. No automated tool can certify novelty, ethical approvals, scientific truth, authorship eligibility or journal acceptance.</p>
        <button className="button button--secondary" onClick={()=>setTab("record")}><BookOpen size={15}/>Edit research record and disclosures</button></>}
      </section></div>
    </>}
  </div>;
}
