import dynamic from "next/dynamic";
import ExperimentWorkspace from "./ExperimentWorkspace";
import ResearchSessions from './ResearchSessions';
import StudioWorkbench from './StudioWorkbench';
import {useModelSelection} from '@/lib/modelSelection';
import EquationEditor from "./EquationEditor";
import {buildRequest,checkedResponse,DEFAULT_BUILD,RUNNING,createSingleFlight} from "@/lib/experiment";
const AdvancedExperiments=dynamic(()=>import("../discovery/DiscoveryView"),{ssr:false,loading:()=> <p className="expNotice">Loading optional batch and verification tools…</p>});
import ResearchPlan from "./ResearchPlan";
import {useLiveRuntime} from "@/lib/liveRuntime";
import FindingsView from "./FindingsView";
import ResourceMonitor from "./ResourceMonitor";
import WorkflowBar from "./WorkflowBar";
import Link from "next/link";
import {
  BookOpen,
  Braces,
  ChevronDown,
  Dna,
  FlaskConical,
  MessageSquareText,
  Plus,
  RefreshCw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useRouter } from "next/router";
import { api } from "@/lib/api";
import { useServerEvents } from "@/lib/useBackend";
import ChatDrawer from "./ChatDrawer";
import EvidenceView from "./EvidenceView";
import ImportManifestDialog from "./ImportManifestDialog";
import ManifestInspector from "./ManifestInspector";
import MoleculeWorkbench from "./MoleculeWorkbench";
import NewWorldDialog from "./NewWorldDialog";
import RunStrip from "./RunStrip";

const PROJECT_KEY = "phaseforge.activeProject";
const STRUCTURE_KEY = "phaseforge.activeStructure";

function requestId() {
  if (typeof crypto !== "undefined" && crypto.randomUUID) {
    return crypto.randomUUID();
  }
  const bytes = new Uint8Array(16);
  if (typeof crypto !== "undefined" && crypto.getRandomValues) {
    crypto.getRandomValues(bytes);
  } else {
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Math.floor(Math.random() * 256);
    }
  }
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export default function ResearchWorkbench({ backend, onHardware, onEventState }) {
  const {selection}=useModelSelection();
  const [sceneContext,setSceneContext]=useState(null);
  const router = useRouter();
  const studioRef = useRef(null);
  const sendingRef=useRef(false);
  const messageFlight=useRef(createSingleFlight());
  const nextReceipts=useRef(new Map());
  const loadGeneration=useRef(0);
  const chatAbortRef = useRef(null);
  const {workflow} = useLiveRuntime();
  const ownedRuns = useRef(new Set());
  const fetchingRuns = useRef(new Set());
  const runsRef = useRef([]);
  const projectRef = useRef(null);
  const [explaining, setExplaining] = useState(false);
  const [findingsRefresh, setFindingsRefresh] = useState(0);
  const [autoExplain, setAutoExplain] = useState(false);
  useEffect(() => { try { setAutoExplain(localStorage.getItem("phaseforge.autoExplainEvidence") === "true"); } catch {} }, []);
  const changeAutoExplain = value => { setAutoExplain(value); try { localStorage.setItem("phaseforge.autoExplainEvidence",String(value)); } catch {} };
  const [projects, setProjects] = useState([]);
  const [activeProjectId, setActiveProjectId] = useState(null);
  const [messages, setMessages] = useState([]);
  const [manifests, setManifests] = useState([]);
  const [selectedManifestId, setSelectedManifestId] = useState(null);
  const [runs, setRuns] = useState([]);
  const [selectedRunId, setSelectedRunId] = useState(null);
  const [providers, setProviders] = useState([]);
  const [capabilities, setCapabilities] = useState([]);
  const [structures, setStructures] = useState([]);
  const [selectedStructureId, setSelectedStructureId] = useState(null);
  const [engines, setEngines] = useState([]);
  const [tab, setTab] = useState("world");
  const [moreOpen,setMoreOpen]=useState(false);
  const [notice,setNotice]=useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [activeRequestId, setActiveRequestId] = useState(null);
  const [error, setError] = useState(null);
  const [newWorldOpen, setNewWorldOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);

  const activeProject = useMemo(
    () => projects.find((item) => item.id === activeProjectId) || null,
    [activeProjectId, projects],
  );
  const activeManifest = useMemo(
    () => manifests.find((item) => item.id === selectedManifestId)
      || manifests.find((item) => item.id === activeProject?.active_manifest_id)
      || manifests[0]
      || null,
    [activeProject?.active_manifest_id, manifests, selectedManifestId],
  );
  const selectedRun = useMemo(
    () => runs.find((item) => item.id === selectedRunId) || runs[0] || null,
    [runs, selectedRunId],
  );
  runsRef.current = runs;
  projectRef.current = activeProjectId;
  const runManifestSnapshot = useMemo(() => manifests.find(m => m.id === selectedRun?.manifest_id) || null, [manifests,selectedRun?.manifest_id]);
  useEffect(() => { setSceneContext(null); }, [selectedRun?.id, activeManifest?.id]);
  const pendingRevision = activeManifest && activeManifest.id !== selectedRun?.manifest_id && !runs.some(r => r.manifest_id === activeManifest.id);
  const selectedStructure = useMemo(
    () => structures.find((item) => item.id === selectedStructureId) || structures[0] || null,
    [selectedStructureId, structures],
  );

  const selectStructure = useCallback((id) => {
    const next = id || null;
    setSelectedStructureId(next);
    if (typeof window === "undefined") return;
    if (next) window.localStorage.setItem(STRUCTURE_KEY, next);
    else window.localStorage.removeItem(STRUCTURE_KEY);
  }, []);

  const loadProjectData = useCallback(async (projectId, preferredRunId = null) => {
    const generation=++loadGeneration.current;
    if (!projectId) {
      setMessages([]);
      setManifests([]);
      setRuns([]);
      setStructures([]);
      setSelectedRunId(null);
      setSelectedStructureId(null);
      return;
    }

    const [messageResponse, manifestResponse, runResponse, moleculeResponse] = await Promise.all([
      api.messages(projectId, 500),
      api.manifests(projectId, 250),
      api.runs(400, projectId),
      api.molecules(projectId, 250),
    ]);
    if(generation!==loadGeneration.current)return;
    const nextManifests = manifestResponse.manifests || [];
    const nextRuns = runResponse.runs || [];
    const nextStructures = moleculeResponse.structures || [];

    setMessages(messageResponse.messages || []);
    setManifests(nextManifests);
    setRuns(nextRuns);
    setStructures(nextStructures);
    setSelectedManifestId((current) =>
      nextManifests.some((item) => item.id === current)
        ? current
        : nextManifests[0]?.id || null,
    );
    setSelectedRunId((current) => {
      const desired = preferredRunId || current;
      return nextRuns.some((item) => item.id === desired)
        ? desired
        : nextRuns[0]?.id || null;
    });
    setSelectedStructureId((current) => {
      const remembered = typeof window !== "undefined"
        ? window.localStorage.getItem(STRUCTURE_KEY)
        : null;
      const desired = current || remembered;
      return nextStructures.some((item) => item.id === desired)
        ? desired
        : nextStructures[0]?.id || null;
    });
  }, []);

  const refreshEngines = useCallback(async () => {
    try {
      const response = await api.scientificEngines();
      setEngines(response.engines || []);
    } catch (value) {
      setError(`Scientific engine discovery failed: ${value.message}`);
    }
  }, []);

  const bootstrap = useCallback(async () => {
    if (!backend.connected) return;
    setLoading(true);
    try {
      setError(null);
      const [projectResponse, hardware, providerResponse, capabilityResponse] = await Promise.all([
        api.projects(),
        api.hardware(),
        api.providers(),
        api.capabilities(),
      ]);
      const nextProjects = projectResponse.projects || [];
      setProjects(nextProjects);
      setProviders(providerResponse.providers || []);
      setCapabilities(capabilityResponse.capabilities || []);
      onHardware?.(hardware);

      const queryRun = typeof router.query.run === "string" ? router.query.run : null;
      let projectId = null;
      if (queryRun) {
        try {
          const run = await api.run(queryRun);
          projectId = run.project_id;
        } catch {
          projectId = null;
        }
      }
      const requestedProject=typeof router.query.project==="string"?router.query.project:null;
      const remembered = window.localStorage.getItem(PROJECT_KEY);
      projectId = projectId
        || (nextProjects.some(p=>p.id===requestedProject)?requestedProject:null)
        || (nextProjects.some((item) => item.id === remembered) ? remembered : null)
        || nextProjects[0]?.id
        || null;
      setActiveProjectId(projectId);
      if (projectId) {
        window.localStorage.setItem(PROJECT_KEY, projectId);
        await loadProjectData(projectId, queryRun);
        if(typeof router.query.manifest==="string")setSelectedManifestId(router.query.manifest);
      }
      await refreshEngines();
    } catch (value) {
      setError(value.message);
    } finally {
      setLoading(false);
    }
  }, [backend.connected, loadProjectData, onHardware, refreshEngines, router.query.run,router.query.project,router.query.manifest]);

  useEffect(()=>{if(router.isReady&&['advanced','sessions','research','molecules','equations','evidence'].includes(router.query.panel))setTab(router.query.panel);},[router.isReady,router.query.panel,router.query.study]);
  useEffect(() => {
    bootstrap();
  }, [bootstrap]);

  const eventState = useServerEvents(useCallback(async (event) => {
    if (!event?.run_id || !activeProjectId) return;
    if (["run_progress","run_started","run_queued"].includes(event.kind)) {
      setRuns(current => current.map(r => r.id === event.run_id && !["completed","failed","cancelled"].includes(r.status) ? {...r,...event.payload} : r));
      return;
    }
    try {
      const updated = await api.run(event.run_id);
      if (updated.project_id !== activeProjectId) return;
      setRuns((current) => {
        const present = current.some((item) => item.id === updated.id);
        return present
          ? current.map((item) => item.id === updated.id ? updated : item)
          : [updated, ...current];
      });
      if (event.kind === "run_completed") { setSelectedRunId(updated.id); setTab("findings"); }
    } catch {
      // HTTP polling and manual refresh remain authoritative.
    }
  }, [activeProjectId]));

  useEffect(() => {
    if (!workflow || !activeProjectId) return;
    const items=(workflow.runs||[]).filter(r=>r.project_id===activeProjectId);
    const previous=runsRef.current;
    setRuns(current => current.map(run => {
      const update=items.find(r=>r.id===run.id);
      if(!update || (["completed","failed","cancelled"].includes(run.status) && ["running","queued"].includes(update.status)))return run;
      return {...run,...update};
    }));
    for(const item of items){
      const existing=previous.find(r=>r.id===item.id);
      if(["completed","failed","cancelled"].includes(item.status) && (!existing || !existing.result && item.status==="completed") && !fetchingRuns.current.has(item.id)){
        fetchingRuns.current.add(item.id);
        api.run(item.id).then(full=>{
          if(projectRef.current!==full.project_id)return;
          setRuns(current=>[full,...current.filter(r=>r.id!==full.id)]);
          if(ownedRuns.current.has(full.id)||selectedRunId===full.id){setSelectedRunId(full.id);if(full.status==="completed")setTab("findings");}
        }).catch(()=>{}).finally(()=>fetchingRuns.current.delete(item.id));
      }
    }
  },[workflow,activeProjectId,selectedRunId]);

  useEffect(() => {
    onEventState?.(backend.connected ? eventState : "disconnected");
  }, [backend.connected, eventState, onEventState]);

  useEffect(() => {
    const open = () => setNewWorldOpen(true);
    const key = (event) => {
      if (event.ctrlKey && event.key === "\\") {
        event.preventDefault();
        window.dispatchEvent(new CustomEvent("phaseforge:toggle-chat"));
      }
    };
    window.addEventListener("phaseforge:new-question", open);
    window.addEventListener("keydown", key);
    return () => {
      window.removeEventListener("phaseforge:new-question", open);
      window.removeEventListener("keydown", key);
    };
  }, []);

  const switchProject = async (projectId) => {
    if(sendingRef.current||busy||!projectId||projectId===activeProjectId)return;
    setMessages([]);setManifests([]);setRuns([]);setStructures([]);setSelectedManifestId(null);setSelectedRunId(null);setNotice("");setTab("world");
    setActiveProjectId(projectId);
    setSceneContext(null);
    window.localStorage.setItem(PROJECT_KEY, projectId);
    setLoading(true);
    setError(null);
    try {
      await loadProjectData(projectId);
    } catch (value) {
      setError(value.message);
    } finally {
      setLoading(false);
    }
  };

  useEffect(()=>{
    const select=e=>switchProject(e.detail?.id);
    window.addEventListener('phaseforge:select-project',select);
    return()=>window.removeEventListener('phaseforge:select-project',select);
  });
  useEffect(()=>{window.dispatchEvent(new CustomEvent('phaseforge:project-active',{detail:{id:activeProjectId}}));},[activeProjectId]);
  useEffect(()=>{window.dispatchEvent(new CustomEvent('phaseforge:projects-changed'));},[projects.length]);
  const refreshTaskData=useCallback(()=>{if(projectRef.current)loadProjectData(projectRef.current).catch(e=>setError(e.message));},[loadProjectData]);

  const createWorld = async ({ question, name }) => {
    if(sendingRef.current||busy)throw new Error("Wait for the current request or press Stop before creating a project.");
    sendingRef.current=true;
    setBusy(true); setError(null);
    try {
      const project = await api.createProject({ question, name });
      // Creating a world is local only. No implicit provider call or token spend.
      try {
        localStorage.setItem(PROJECT_KEY, project.id);
        localStorage.setItem(`phaseforge.chat.draft:${project.id}`, question);
      } catch {}
      setProjects((current) => [project, ...current]); setActiveProjectId(project.id);projectRef.current=project.id;
      setMessages([]); setManifests([]); setRuns([]); setStructures([]);
      setSelectedRunId(null); setSelectedStructureId(null); setSelectedManifestId(null);
      setNewWorldOpen(false); setTab("world");
    } catch (value) { setError(value.message); throw value; }
    finally { sendingRef.current=false;setBusy(false); }
  };

  const renameProject = async (id, name) => {
    if(sendingRef.current||busy){setError("Wait for the current operation or press Stop before renaming a project.");return;}
    try {
      const updated = await api.updateProject(id, { name });
      setProjects((current) =>
        current.map((item) => item.id === id ? updated : item),
      );
    } catch (value) {
      setError(value.message);
    }
  };

  const deleteProject = async (id) => {
    if(sendingRef.current||busy){setError("Stop current work before deleting a project and its evidence.");return;}
    const target = projects.find((item) => item.id === id);
    if (
      !target ||
      !window.confirm(`Delete the research world “${target.name}” and all local evidence?`)
    ) {
      return;
    }
    try {
      await api.deleteProject(id);
      const remaining = projects.filter((item) => item.id !== id);
      setProjects(remaining);
      if (id === activeProjectId) {
        const next = remaining[0]?.id || null;
        setActiveProjectId(next);
        if (next) {
          window.localStorage.setItem(PROJECT_KEY, next);
          await loadProjectData(next);
        } else {
          window.localStorage.removeItem(PROJECT_KEY);
          await loadProjectData(null);
        }
      }
    } catch (value) {
      setError(value.message);
    }
  };

  const sendWithinFlight = async (payload) => {
    payload={...payload,provider:payload.provider||selection.provider,model:payload.model||selection.model||null,reasoning_effort:payload.reasoning_effort||selection.reasoning_effort||null,research_mode:payload.research_mode??!!selection.research_mode};
    if(sceneContext)payload={...payload,content:`${payload.content}\n\nCurrent viewport inspection (visual context, not new empirical evidence): ${JSON.stringify(sceneContext)}`};
    if(!activeProjectId)throw new Error("Create a project first.");
    if(sendingRef.current||busy)throw new Error("One request is already running. Wait or press Stop.");
    sendingRef.current=true;
    const world=activeProjectId;
    const id = payload.request_id || requestId();
    const controller = new AbortController();
    chatAbortRef.current = controller;
    setActiveRequestId(id);
    setBusy(true);
    setNotice("");
    setError(null);

    const optimistic = {
      id: `pending-${id}`,
      project_id: activeProjectId,
      role: "user",
      kind: "chat",
      content: payload.content,
      agent_role: payload.agent_role,
      metadata: {
        request_id: id,
        attachments: (payload.attachments || []).map((attachment, index) => ({
          id: `pending-attachment-${index}`,
          name: attachment.name,
          size_bytes: attachment.size_bytes,
        })),
      },
      created_at: new Date().toISOString(),
    };
    setMessages((current) => [...current, optimistic]);

    try {
      const response = await api.sendMessage(
        activeProjectId,
        { ...payload, context_manifest_id:payload.context_manifest_id??(payload.source_run_id||payload.branch_from_message_id?null:activeManifest?.id||null), request_id: id },
        controller.signal,
      );
      if(projectRef.current!==world)return response;
      setMessages((current) => [
        ...current.filter((item) => item.id !== optimistic.id),
        response.user_message,
        response.assistant_message,
      ]);
      const outcome=checkedResponse(response);
      if(outcome.planned)setNotice("Notes saved. They are optional; return to Experiment to build a simulation without completing this plan.");
      if(outcome.gap)setNotice(`${outcome.gap.reason} ${outcome.gap.suggested_extension||""}`);
      if(outcome.queueError)setError(outcome.queueError);
      if (response.manifest) {
        setManifests((current) => [
          response.manifest,
          ...current.filter((item) => item.id !== response.manifest.id),
        ]);
        setSelectedManifestId(response.manifest.id);
        setTab("world");
      }
      if (response.submitted_run_id) {
        ownedRuns.current.add(response.submitted_run_id);
        const run = await api.run(response.submitted_run_id);
        setRuns((current) => [run, ...current.filter((item) => item.id !== run.id)]);
        setSelectedRunId(run.id);
        setTab("world");
      }
      if (response.imported_structure_ids?.length) {
        const moleculeResponse = await api.molecules(activeProjectId, 250);
        const nextStructures = moleculeResponse.structures || [];
        setStructures(nextStructures);
        selectStructure(response.imported_structure_ids[0]);
        setTab("molecules");
      }
      const refreshed = await api.project(activeProjectId);
      setProjects((current) =>
        current.map((item) => item.id === refreshed.id ? refreshed : item),
      );
      return response;
    } catch (value) {
      if(projectRef.current!==world)throw value;
      setMessages((current) => current.filter((item) => item.id !== optimistic.id));
      if (value.name !== "AbortError") setError(value.message);
      api.messages(world, 500).then((r) => {if(projectRef.current===world)setMessages(r.messages || []);}).catch(() => {});
      throw value;
    } finally {
      if (chatAbortRef.current === controller) chatAbortRef.current = null;
      sendingRef.current=false;
      setActiveRequestId(null);
      setBusy(false);
    }
  };

  const sendMessage = payload=>messageFlight.current(()=>sendWithinFlight(payload));

  const cancelChat = async () => {
    const id = activeRequestId;
    if (!id) return;
    try {
      await api.cancelChatRequest(id);
      chatAbortRef.current?.abort();
      // The in-flight handler owns the busy flag until it settles.
    } catch (value) { setError(`Could not confirm cancellation: ${value.message}. Use Usage & cost to retry stopping the request.`); }
  };

  const explainRun = async (run) => {
    if (!run || busy || sendingRef.current) return;
    sendingRef.current=true;
    const id=requestId(), controller=new AbortController();
    chatAbortRef.current=controller;setActiveRequestId(id);setBusy(true);setExplaining(true);setError(null);
    try {
      await api.explainRun(run.id,{request_id:id,provider:null},controller.signal);
      if(projectRef.current===run.project_id){
        const response=await api.messages(run.project_id,500);setMessages(response.messages||[]);
        setFindingsRefresh(n=>n+1);setSelectedRunId(run.id);setTab("findings");
      }
    } catch(value) { if(value.name!=="AbortError")setError(`AI explanation did not complete: ${value.message}. No further work was started.`); }
    finally { if(chatAbortRef.current===controller)chatAbortRef.current=null;sendingRef.current=false;setExplaining(false);setBusy(false);setActiveRequestId(null); }
  };
  const explainRef=useRef(explainRun);explainRef.current=explainRun;
  useEffect(()=>{
    if(!autoExplain||busy)return;
    const ready=runs.find(r=>r.project_id===activeProjectId&&r.status==="completed"&&r.result&&ownedRuns.current.has(r.id));
    if(ready){ownedRuns.current.delete(ready.id);explainRef.current(ready);}
  },[runs,autoExplain,busy,activeProjectId]);

  const directNext = async (source,operation) => {
    if(!source||busy||sendingRef.current)return;
    sendingRef.current=true;setBusy(true);setError(null);setNotice("");
    const world=activeProjectId;
    try {
      const key=`${world}:${source.id}:${operation}`;
      const request=nextReceipts.current.get(key)||requestId();nextReceipts.current.set(key,request);
      const response=await api.nextExperiment(world,{request_id:request,source_run_id:source.id,operation,run:true});
      // Retain the ID across a transport failure; retry checks the same receipt.
      if(response.status!=="reserved")nextReceipts.current.delete(key);
      if(projectRef.current!==world)return;
      if(response.manifest){setManifests(rows=>[response.manifest,...rows.filter(m=>m.id!==response.manifest.id)]);setSelectedManifestId(response.manifest.id);}
      if(response.run){setRuns(rows=>[response.run,...rows.filter(r=>r.id!==response.run.id)]);setSelectedRunId(response.run.id);ownedRuns.current.add(response.run.id);}
      if(response.execution_error)setError(response.execution_error);
      if(response.status==="reserved")setNotice("This request was reserved but its final queue receipt is unavailable. Inspect Runs before starting another pass; this request will not enqueue twice.");
      setTab("world");
    }catch(e){if(projectRef.current===world)setError(e.message);throw e;}
    finally{sendingRef.current=false;setBusy(false);}
  };
  const replayRun = async source=>{try{await directNext(source,"replay");}catch{}};
  const continueRun = async (source,action) => {
    try {
      if(action.kind==="direct") {await directNext(source,action.operation);return;}
      await sendMessage({content:action.prompt,study_intent:action.id==="research_plan"?"plan":"experiment",
        experiment_options:DEFAULT_BUILD,request_id:requestId(),provider:null,model:null,agent_role:action.role,
        source_run_id:source.id,auto_run:false,attachments:[],structure_id:null});
      window.dispatchEvent(new CustomEvent("phaseforge:focus-chat"));
    }catch{ /* persisted error remains visible */ }
  };
  const buildExperiment = async(instruction,options,run)=>sendMessage(buildRequest({project:activeProject,manifest:activeManifest,instruction,options,run}));
  const stopWorkflow = async (run) => {
    if(activeRequestId){await cancelChat();return;}
    // A page reload can leave a server-side request running without a local ID.
    const remote=(workflow?.requests||[]).find(r=>r.project_id===activeProjectId);
    if(remote?.request_id){try{await api.cancelChatRequest(remote.request_id);}catch(e){setError(`Could not confirm cancellation: ${e.message}`);}return;}
    const running=(workflow?.runs||[]).find(r=>r.project_id===activeProjectId&&RUNNING.has(r.status));
    if(running)await cancelRun(running.id);else if(run&&RUNNING.has(run.status))await cancelRun(run.id);
  };

  const runManifest = async () => {
    if(!activeManifest||busy||sendingRef.current)return;
    sendingRef.current=true;
    setBusy(true);
    setError(null);
    try {
      const run = await api.runManifest(activeManifest.id);
      ownedRuns.current.add(run.id);
      setRuns((current) => [run, ...current]);
      setSelectedRunId(run.id);
      setTab("world");
    } catch (value) {
      setError(value.message);
    } finally {
      sendingRef.current=false;
      setBusy(false);
    }
  };

  const importManifest = async (payload) => {
    if (!activeProjectId) throw new Error("Create a project before importing equations.");
    if(sendingRef.current||busy)throw new Error("Wait for the current request or press Stop before saving another setup.");
    sendingRef.current=true;
    setBusy(true);
    setError(null);
    try {
      const response = await api.importManifest(activeProjectId, payload);
      setManifests((current) => [response.manifest, ...current]);
      setSelectedManifestId(response.manifest.id);setTab("world");
      if (response.run) {
        ownedRuns.current.add(response.run.id);
        setRuns((current) => [response.run, ...current]);
        setSelectedRunId(response.run.id);
      }
      const refreshed = await api.project(activeProjectId);
      setProjects((current) =>
        current.map((item) => item.id === refreshed.id ? refreshed : item),
      );
    } catch (value) {
      setError(value.message);
      throw value;
    } finally {
      sendingRef.current=false;setBusy(false);
    }
  };

  const cancelRun = async (id) => {
    try {
      const updated = await api.cancelRun(id);
      setRuns((current) =>
        current.map((item) => item.id === id ? updated : item),
      );
    } catch (value) {
      setError(value.message);
    }
  };

  const handleMoleculeImported = async (imported) => {
    setStructures((current) => [
      imported,
      ...current.filter((item) => item.id !== imported.id),
    ]);
    selectStructure(imported.id);
    setTab("molecules");
  };

  const handleMoleculeDeleted = async (id) => {
    const remaining = structures.filter((item) => item.id !== id);
    setStructures(remaining);
    selectStructure(remaining[0]?.id || null);
  };

  if (!backend.connected) {
    return (
      <div className="backendOffline">
        <FlaskConical size={28} />
        <strong>Connecting to your local research engine</strong>
        <p>{backend.error || "PhaseForge cannot reach http://127.0.0.1:7331."}</p>
        <button className="button button--secondary" onClick={backend.refresh}>
          <RefreshCw size={14} /> Retry
        </button>
      </div>
    );
  }

  const noProjects = !loading && projects.length === 0;

  return (
    <div className="researchStudio" ref={studioRef}>
      <section className="laboratoryPane">
        <header className="laboratoryToolbar">
          <div className="projectPicker">
            <span className="projectPicker__mark"><FlaskConical size={16} /></span>
            <label>
              <span>Project</span>
              <div>
                <select
                  value={activeProjectId || ""}
                  onChange={(event) => switchProject(event.target.value)}
                  disabled={!projects.length || busy}
                >
                  {!projects.length && <option value="">No research world</option>}
                  {projects.map((project) => (
                    <option key={project.id} value={project.id}>{project.name}</option>
                  ))}
                </select>
                <ChevronDown size={14} />
              </div>
            </label>
          </div>
          <div className="laboratoryToolbar__right">
            <div
              className="capabilityBadge"
              title={capabilities.map((item) => item.name).join(", ")}
            >
              <ShieldCheck size={14} />
              {capabilities.length} numerical capabilities · {structures.length} molecular structures
            </div>
            <button
              className="button button--secondary"
              onClick={() => setImportOpen(true)}
              disabled={!activeProject}
            >
              <Braces size={14} /> Import manifest
            </button>
            <button className="button button--primary" onClick={() => setNewWorldOpen(true)} disabled={busy}>
              <Plus size={14} /> New project
            </button>
          </div>
        </header>


        <ResourceMonitor />
        {error&&<div className="workspaceAlert" role="alert"><span>{error}</span><button type="button" onClick={()=>setError(null)}>Dismiss</button></div>}
        {notice&&<div className="expNotice" role="status">{notice}<button type="button" onClick={()=>setNotice("")} aria-label="Dismiss notice">×</button></div>}
        <nav className="workspaceTabs" aria-label="Experiment workspace">
          <button type="button" className={tab==="world"?"active":""} onClick={()=>setTab("world")}><FlaskConical size={14}/>Experiment</button>
          <button type="button" className={tab==="studio"?"active":""} onClick={()=>setTab("studio")}><Dna size={14}/>3D & fabrication</button>
          <button type="button" className={tab==="findings"?"active":""} onClick={()=>setTab("findings")}><BookOpen size={14}/>Results</button>
          <button type="button" className={tab==="manifest"?"active":""} onClick={()=>setTab("manifest")}><Braces size={14}/>Setup</button>
          <button type="button" className={tab==="sessions"?"active":""} onClick={()=>setTab("sessions")}><Sparkles size={14}/>Agents</button>
          <button type="button" className={tab==="research"?"active":""} onClick={()=>setTab("research")}><BookOpen size={14}/>Research</button>
          <div className="expMore"><button type="button" onClick={()=>setMoreOpen(v=>!v)} aria-expanded={moreOpen}>More <ChevronDown size={14}/></button>{moreOpen&&<div className="expMoreMenu">{[["research","Notes & sources"],["molecules","Molecular files"],["evidence","Detailed evidence"],["equations","Equation editor"],["advanced","Batch runs & verification"]].map(([id,label])=><button type="button" key={id} onClick={()=>{setTab(id);setMoreOpen(false);}}>{label}</button>)}</div>}</div>
          <button type="button" className="workspaceTabs__chat" onClick={()=>window.dispatchEvent(new CustomEvent("phaseforge:focus-chat"))}><MessageSquareText size={14}/>Chat</button>
        </nav>

        <div className="workspaceStage">
          {tab==='studio'&&<StudioWorkbench project={activeProject} manifest={activeManifest} providers={providers} onInspect={value=>setSceneContext(v=>({...v,inspection:value}))} onCameraChange={value=>setSceneContext(v=>({...v,camera:value}))}/>}
          {tab==='sessions'&&<ResearchSessions project={activeProject} providers={providers} onRefresh={refreshTaskData} onInspectRun={async id=>{try{const run=await api.run(id);setRuns(rows=>[run,...rows.filter(r=>r.id!==id)]);setSelectedRunId(id);setTab('world');}catch(e){setError(e.message);}}}/>}
          {tab==="research"&&<ResearchPlan project={activeProject} manifest={activeManifest} refreshKey={messages.length} busy={busy}
            onExperiment={content=>{setTab("world");return buildExperiment(content,DEFAULT_BUILD,false);}}
            onAdvanced={()=>setTab("advanced")}
            onPropose={async(content,intent="review")=>{await sendMessage({content,study_intent:intent,experiment_options:intent==="experiment"?DEFAULT_BUILD:null,auto_run:false,agent_role:"builder",attachments:[]});window.dispatchEvent(new CustomEvent("phaseforge:focus-chat"));}}/>}
          {tab==="advanced"&&<AdvancedExperiments backend={backend} embeddedProjectId={activeProjectId} embeddedManifestId={activeManifest?.id}/>}
          {tab==="equations"&&<EquationEditor key={activeProjectId||"none"} project={activeProject} busy={busy} onImport={importManifest} onBack={()=>setTab("world")}/>}
          {tab==="world"&&<ExperimentWorkspace project={activeProject} manifest={activeManifest} run={selectedRun} busy={busy} providerReady={providers.some(p=>p.configured)} providers={providers} onSession={()=>setTab('sessions')} onInspect={value=>setSceneContext(v=>({...v,inspection:value}))} onCameraChange={value=>setSceneContext(v=>({...v,camera:value}))}
            build={buildExperiment} onRun={runManifest} onNext={op=>directNext(selectedRun,op)} onStop={()=>stopWorkflow((workflow?.runs||[]).find(r=>r.project_id===activeProjectId&&RUNNING.has(r.status))||selectedRun)}
            onSetup={()=>setTab("manifest")} onEquations={()=>setTab("equations")} onNew={()=>setNewWorldOpen(true)} onResults={()=>setTab("findings")}/>}
          {tab === "molecules" && (
            <MoleculeWorkbench
              project={activeProject}
              structures={structures}
              selectedStructureId={selectedStructure?.id || null}
              onSelectStructure={selectStructure}
              onImported={handleMoleculeImported}
              onDeleted={handleMoleculeDeleted}
              engines={engines}
              onRefreshEngines={refreshEngines}
            />
          )}
          {tab === "manifest" && (
            <ManifestInspector
              manifest={activeManifest}
              manifests={manifests}
              onSelectManifest={setSelectedManifestId}
              onRun={runManifest}
              running={busy}
              onOpenImport={() => setImportOpen(true)}
            />
          )}
          {tab === "evidence" && <EvidenceView run={selectedRun} manifest={runManifestSnapshot} />}
          {tab === "findings" && <FindingsView run={selectedRun} manifest={runManifestSnapshot} runs={runs} manifests={manifests}
            onExplain={explainRun} onContinue={continueRun} onReplay={replayRun} onInspect={()=>setTab("evidence")}
            busy={busy} explaining={explaining} refreshToken={findingsRefresh} autoExplain={autoExplain} onAutoExplain={changeAutoExplain}
            providerReady={providers.some(p=>p.configured)} />}
        </div>

        {tab!=="studio"&&<RunStrip
          runs={runs}
          selectedRunId={selectedRun?.id}
          onSelect={(run) => {
            setSelectedRunId(run.id);
            setTab("world");
          }}
          onCancel={cancelRun}
        />}
      </section>

      <ChatDrawer
        containerRef={studioRef}
        messages={messages}
        providers={providers}
        busy={busy}
        error={error}
        project={activeProject}
        projects={projects}
        structures={structures}
        selectedStructure={selectedStructure}
        onStructureSelect={selectStructure}
        onProjectSelect={switchProject}
        onNewProject={() => setNewWorldOpen(true)}
        onRenameProject={renameProject}
        onDeleteProject={deleteProject}
        onSend={sendMessage}
        onCancel={cancelChat}
        activeRequestId={activeRequestId}
        findingsRun={selectedRun?.status==="completed"?selectedRun:null}
        onOpenFindings={()=>setTab("findings")}
      />

      <NewWorldDialog
        open={newWorldOpen}
        onClose={() => setNewWorldOpen(false)}
        onCreate={createWorld}
        busy={busy}
      />
      <ImportManifestDialog
        open={importOpen}
        onClose={() => setImportOpen(false)}
        onImport={importManifest}
        busy={busy}
      />
    </div>
  );
}
