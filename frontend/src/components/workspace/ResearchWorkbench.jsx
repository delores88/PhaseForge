import dynamic from "next/dynamic";
import ExperimentWorkspace from "./ExperimentWorkspace";
import ResearchSessions from './ResearchSessions';
import LaboratoryActivity from './LaboratoryActivity';
import WorkspaceErrorBoundary from './WorkspaceErrorBoundary';
const LaboratoryViewer=dynamic(()=>import('./LaboratoryViewer'),{ssr:false});
import LaboratoryLineage from './LaboratoryLineage';
import WorkspaceResults from './WorkspaceResults';
import LaboratoryResultActions from './LaboratoryResultActions';
import {activeResultReviewSession,captureLaboratoryReview,pendingResultReview,retainResultReview,releaseResultReview,uncertainReviewDelivery} from '@/lib/laboratory-result-review.mjs';
import {completedWorkspaceResults,currentWorkspaceResult,unhandledRunLink,WORKSPACE_PANELS} from '@/lib/workspace-results.mjs';
import SolverCoverage from './SolverCoverage';
import DeliverableStatus from './DeliverableStatus';
import {initialLaboratorySelection,workspaceTools,latestDeliverable} from '@/lib/workspace-navigation.mjs';
import StudioWorkbench from './StudioWorkbench';
import {useModelSelection} from '@/lib/modelSelection';
import {snapshotConversationModel,selectedLaboratoryContext,conversationTimeLimit,resolveRunInspection,createViewTracker,createProjectRequestGate,mergeBackgroundRun} from '@/lib/workbenchRequests.mjs';
import {laboratoryViewable} from '@/lib/laboratory-plot.mjs';
import {dispatchLabMessage} from '@/lib/labMessageDispatch.mjs';
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
  const {getSelection,getSelectionError,setSelectionProject}=useModelSelection();
  const selectionFor=projectId=>snapshotConversationModel(projectId,getSelection,getSelectionError);
  const timeDrafts=useRef(new Map());
  const rememberTimeDraft=useCallback((projectId,draft)=>{if(projectId)timeDrafts.current.set(projectId,{...draft});},[]);
  const timeLimitFor=projectId=>conversationTimeLimit(projectId,window.localStorage,timeDrafts.current.get(projectId));
  const viewTracker=useRef(createViewTracker());
  const labRequestGate=useRef(createProjectRequestGate());
  const [,refreshLabRequests]=useState(0);
  const [sceneContext,setSceneContext]=useState(null);
  const router = useRouter();
  const studioRef = useRef(null);
  const sendingRef=useRef(false);
  const messageFlight=useRef(createSingleFlight());
  const nextReceipts=useRef(new Map());
  const loadGeneration=useRef(0);
  const handledRunLink=useRef(null),handledJobLink=useRef(null),bootstrapGeneration=useRef(0),resultsRef=useRef([]),resultReviewContext=useRef(null);
  const chatAbortRef = useRef(null);
  const {workflow,laboratoryJobs=[],acknowledgeProject,acknowledgeJob} = useLiveRuntime();
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
  const [selectedLabId,setSelectedLabId]=useState(null);
  const [resolvedLabLink,setResolvedLabLink]=useState(null);
  const [admittedReviews,setAdmittedReviews]=useState([]);
  const [reviewDeliveries,setReviewDeliveries]=useState({});
  useEffect(()=>{if(!activeProjectId)return;try{const saved=pendingResultReview(window.localStorage,activeProjectId);setReviewDeliveries(previous=>({...previous,[activeProjectId]:saved}));}catch(value){setError(value.message);}},[activeProjectId]);
  const availableLaboratoryJobs=useMemo(()=>resolvedLabLink&&!laboratoryJobs.some(job=>job.id===resolvedLabLink.id)?[...laboratoryJobs,resolvedLabLink]:laboratoryJobs,[laboratoryJobs,resolvedLabLink]);
  const reviewJobs=useMemo(()=>[...availableLaboratoryJobs,...admittedReviews.filter(local=>!availableLaboratoryJobs.some(job=>job.id===local.id))],[availableLaboratoryJobs,admittedReviews]);
  useEffect(()=>{setAdmittedReviews(previous=>previous.some(local=>availableLaboratoryJobs.some(job=>job.id===local.id))?previous.filter(local=>!availableLaboratoryJobs.some(job=>job.id===local.id)):previous);},[availableLaboratoryJobs]);
  useEffect(()=>{
    for(const [project,pending] of Object.entries(reviewDeliveries)){
      if(pending&&availableLaboratoryJobs.some(job=>job.id===pending.payload.request_id&&job.project_id===project&&job.kind==='session')){
        try{releaseResultReview(window.localStorage,pending);setReviewDeliveries(previous=>({...previous,[project]:null}));}catch{}
      }
    }
  },[availableLaboratoryJobs,reviewDeliveries]);
  const labSubmitting=labRequestGate.current.pending(activeProjectId);
  useEffect(()=>{setSelectionProject(activeProjectId);},[activeProjectId,setSelectionProject]);
  const projectLabJobs=useMemo(()=>availableLaboratoryJobs.filter(j=>j.project_id===activeProjectId),[availableLaboratoryJobs,activeProjectId]);
  const labSolvers=projectLabJobs.filter(laboratoryViewable);
  const selectedLab=labSolvers.find(j=>j.id===selectedLabId)||null;
  const deliverableSession=latestDeliverable(projectLabJobs);
  useEffect(()=>{setSelectedLabId(current=>initialLaboratorySelection(availableLaboratoryJobs,activeProjectId,current));},[availableLaboratoryJobs,activeProjectId]);
  const activeLabSession=projectLabJobs.find(j=>j.kind==='session'&&['queued','running','provisioning','waiting'].includes(j.state));
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
  const [tab, setTab] = useState("laboratory");
  const [moreOpen,setMoreOpen]=useState(false);
  const [notice,setNotice]=useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [activeRequestId, setActiveRequestId] = useState(null);
  const [requestProjectId,setRequestProjectId]=useState(null);
  const [error, setError] = useState(null);
  const [newWorldOpen, setNewWorldOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const projectBusy=busy&&requestProjectId===activeProjectId;
  const projectRequestId=requestProjectId===activeProjectId?activeRequestId:null;
  const targetModelError=getSelectionError(activeProjectId);
  viewTracker.current.observe([activeProjectId,tab,selectedLabId,selectedRunId,selectedManifestId,selectedStructureId]);

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
  const completedResults=useMemo(()=>completedWorkspaceResults({jobs:availableLaboratoryJobs,runs,projectId:activeProjectId}),[availableLaboratoryJobs,runs,activeProjectId]);
  const latestEquationResult=completedResults.findLast(result=>result.source==='run');
  const visibleResultKey=tab==='laboratory'&&selectedLab?`laboratory:${selectedLab.id}`:['world','findings','evidence'].includes(tab)&&selectedRun?`run:${selectedRun.id}`:null;
  resultsRef.current=completedResults;
  const reviewBusy=projectBusy||labSubmitting||!!activeResultReviewSession(reviewJobs,activeProjectId);
  resultReviewContext.current={view:{projectId:activeProjectId,jobId:selectedLabId,runId:selectedRun?.id,tab,busy:projectBusy||labSubmitting},jobs:reviewJobs,snapshotSelection:selectionFor};
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
    if(generation!==loadGeneration.current||projectRef.current!==projectId)return;
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
    const viewedAt=Date.now();
    const generation=++bootstrapGeneration.current;
    setLoading(true);
    try {
      setError(null);
      const [projectResponse, hardware, providerResponse, capabilityResponse] = await Promise.all([
        api.projects(),
        api.hardware(),
        api.providers(),
        api.capabilities(),
      ]);
      if(generation!==bootstrapGeneration.current)return;
      const nextProjects = projectResponse.projects || [];
      setProjects(nextProjects);
      setProviders(providerResponse.providers || []);
      setCapabilities(capabilityResponse.capabilities || []);
      onHardware?.(hardware);

      const runIntent=unhandledRunLink(router.query.run,router.query.panel,handledRunLink.current);
      const queryRun=runIntent?.id||null;
      let projectId = null,linkedRun=null;
      if (queryRun) {
        try {
          const run = await api.run(queryRun);
          if(run.id===queryRun&&nextProjects.some(project=>project.id===run.project_id)){projectId=run.project_id;linkedRun=run;}
        } catch {
          projectId = null;
        }
      }
      const queryLab=typeof router.query.lab_job==='string'?router.query.lab_job:null;
      let linkedLab=null;
      if(queryLab&&handledJobLink.current!==queryLab&&!linkedRun){try{const job=await api.labJob(queryLab);if(job.id===queryLab&&nextProjects.some(project=>project.id===job.project_id)){linkedLab=job;projectId=job.project_id;}}catch{}}
      if(generation!==bootstrapGeneration.current)return;
      const requestedProject=typeof router.query.project==="string"?router.query.project:null;
      const remembered = window.localStorage.getItem(PROJECT_KEY);
      projectId = projectId
        || (nextProjects.some(p=>p.id===requestedProject)?requestedProject:null)
        || (nextProjects.some((item) => item.id === remembered) ? remembered : null)
        || nextProjects[0]?.id
        || null;
      setActiveProjectId(projectId);
      projectRef.current=projectId;
      if(linkedLab&&handledJobLink.current!==linkedLab.id){handledJobLink.current=linkedLab.id;setResolvedLabLink(linkedLab);setSelectedLabId(laboratoryViewable(linkedLab)?linkedLab.id:null);setTab(laboratoryViewable(linkedLab)?'laboratory':'sessions');}
      if(linkedRun&&runIntent){handledRunLink.current=runIntent.key;setTab(runIntent.panel);}
      if (projectId) {
        window.localStorage.setItem(PROJECT_KEY, projectId);
        await loadProjectData(projectId, linkedRun?.id||null);
        if(generation!==bootstrapGeneration.current||projectRef.current!==projectId)return;
        await acknowledgeProject(projectId,viewedAt);
        if(generation!==bootstrapGeneration.current||projectRef.current!==projectId)return;
        if(typeof router.query.manifest==="string")setSelectedManifestId(router.query.manifest);
      }
      await refreshEngines();
    } catch (value) {
      if(generation===bootstrapGeneration.current)setError(value.message);
    } finally {
      if(generation===bootstrapGeneration.current)setLoading(false);
    }
  }, [backend.connected, loadProjectData, onHardware, refreshEngines, router.query.run,router.query.project,router.query.manifest,router.query.panel,router.query.lab_job,acknowledgeProject]);

  useEffect(()=>{if(router.isReady&&WORKSPACE_PANELS.has(router.query.panel))setTab(router.query.panel);},[router.isReady,router.query.panel,router.query.study]);
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
      if (updated.project_id !== projectRef.current) return;
      setRuns(current=>mergeBackgroundRun(current,updated,projectRef.current));
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
          setRuns(current=>mergeBackgroundRun(current,full,projectRef.current));
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

  const switchProject = async (projectId,destination={}) => {
    if(!projectId)return;
    const viewedAt=Date.now();
    bootstrapGeneration.current++;setLoading(false);
    if(projectId===activeProjectId){try{await acknowledgeProject(projectId,viewedAt);}catch(value){setError(value.message);}return;}
    setMessages([]);setManifests([]);setRuns([]);setStructures([]);setSelectedManifestId(null);setSelectedRunId(null);setNotice("");setTab(destination.tab||"laboratory");
    setActiveProjectId(projectId);
    projectRef.current=projectId;
    setSelectedLabId(destination.labId||null);
    setSceneContext(null);
    window.localStorage.setItem(PROJECT_KEY, projectId);
    setLoading(true);
    setError(null);
    try {
      await loadProjectData(projectId);
      await acknowledgeProject(projectId,viewedAt);
    } catch (value) {
      if(projectRef.current===projectId)setError(value.message);
    } finally {
      if(projectRef.current===projectId)setLoading(false);
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
    bootstrapGeneration.current++;setLoading(false);
    sendingRef.current=true;
    setRequestProjectId(activeProjectId);setBusy(true); setError(null);
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
      setNewWorldOpen(false); setSelectedLabId(null); setTab("laboratory");
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
        projectRef.current=next;
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
    if(!activeProjectId)throw new Error("Create a project first.");
    const world=activeProjectId;
    payload={...payload,...selectionFor(world)};
    if(sendingRef.current||busy)throw new Error("One request is already running. Wait or press Stop.");
    sendingRef.current=true;
    const view=viewTracker.current.capture();
    const id = payload.request_id || requestId();
    const controller = new AbortController();
    chatAbortRef.current = controller;
    setActiveRequestId(id);
    setRequestProjectId(world);
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
        if(viewTracker.current.isCurrent(view))setSelectedManifestId(response.manifest.id);
      }
      if (response.submitted_run_id) {
        ownedRuns.current.add(response.submitted_run_id);
        const run = await api.run(response.submitted_run_id);
        if(projectRef.current!==world)return response;
        setRuns(current=>mergeBackgroundRun(current,run,world));
        if(viewTracker.current.isCurrent(view))setSelectedRunId(run.id);
      }
      if (response.imported_structure_ids?.length) {
        const moleculeResponse = await api.molecules(world, 250);
        if(projectRef.current!==world)return response;
        const nextStructures = moleculeResponse.structures || [];
        setStructures(nextStructures);
        if(viewTracker.current.isCurrent(view))selectStructure(response.imported_structure_ids[0]);
      }
      const refreshed = await api.project(world);
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
  const sendLaboratoryMessage=async payload=>{
    if(!activeProjectId)throw new Error("Create a project first.");
    const project=activeProjectId;
    const context=selectedLaboratoryContext(project,tab,selectedLabId,availableLaboratoryJobs);
    const id=payload.request_id||requestId();
    const ticket=labRequestGate.current.begin(project,id);refreshLabRequests(value=>value+1);setError(null);
    try{
      const sent=await dispatchLabMessage({projectId:project,activeSession:activeLabSession,payload:{...payload,request_id:id,time_limit_seconds:payload.time_limit_seconds===undefined?timeLimitFor(project):payload.time_limit_seconds,context_job_id:context?.id||null},snapshotSelection:selectionFor,submitSession:api.labChat,submitUpdate:api.labSteer});
      if(projectRef.current===project){
        const response=await api.messages(project,500);if(projectRef.current===project){setMessages(response.messages||[]);if(sent.kind==='session')setNotice("Your agent is working in the background. Open Agents for its tools and progress.");}
      }
      return sent.value;
    }catch(value){if(projectRef.current===project)setError(value.message);throw value;}
    finally{labRequestGate.current.finish(ticket);refreshLabRequests(value=>value+1);}
  };
  const reviewLabResult=async (source,action)=>{
    const context=resultReviewContext.current,saved=pendingResultReview(window.localStorage,source.project_id);
    if(saved&&(saved.payload.result_review.source_job_id!==source.id||saved.payload.result_review.action!==action))throw Error(`A prior review delivery is unconfirmed. Open result ${saved.payload.result_review.source_job_id} and retry the same action, or check Agents.`);
    const captured=captureLaboratoryReview({job:source,action,...context,
      snapshotSelection:saved?()=>({provider:saved.payload.provider,model:saved.payload.model,reasoning_effort:saved.payload.reasoning_effort,research_mode:saved.payload.research_mode}):context.snapshotSelection,
      timeLimit:saved?saved.payload.time_limit_seconds:timeLimitFor(source.project_id),requestId:saved?.payload.request_id||requestId()});
    const request=saved||captured,project=request.projectId,ticket=labRequestGate.current.begin(project,request.payload.request_id);refreshLabRequests(value=>value+1);setError(null);
    try{
      // Persist before delivery so an explicit retry, including after reload, reuses this identity.
      retainResultReview(window.localStorage,request);setReviewDeliveries(previous=>({...previous,[project]:request}));
      // Dedicated route: an older backend returns 404 and cannot start ordinary agent chat.
      const session=await api.labReview(source.id,request.payload);
      if(session.id!==request.payload.request_id||session.project_id!==project||session.kind!=='session')throw Error('The review response did not match its requested conversation.');
      releaseResultReview(window.localStorage,request);setReviewDeliveries(previous=>({...previous,[project]:null}));
      setAdmittedReviews(previous=>[...previous.filter(job=>job.id!==session.id),session]);
      if(projectRef.current===project){
        setNotice(`Review accepted for saved result ${source.id}. Follow its reply in chat and progress in Agents.`);
        try{const response=await api.messages(project,500);if(projectRef.current===project)setMessages(response.messages||[]);}catch{}
      }
      return session;
    }catch(value){
      const confirmed=resultReviewContext.current.jobs.find(job=>job.id===request.payload.request_id&&job.project_id===project&&job.kind==='session');
      if(confirmed){releaseResultReview(window.localStorage,request);setReviewDeliveries(previous=>({...previous,[project]:null}));return confirmed;}
      if(!uncertainReviewDelivery(value)){releaseResultReview(window.localStorage,request);setReviewDeliveries(previous=>({...previous,[project]:null}));}
      if(value.status===404)value.message='This backend does not support result reviews. Install the matching app update; no ordinary chat fallback was attempted.';
      else if(uncertainReviewDelivery(value))value.message=`Review delivery is unconfirmed. Retry the same action to reuse request ${request.payload.request_id}, or check Agents. ${value.message}`;
      if(projectRef.current===project)setError(value.message);throw value;
    }
    finally{labRequestGate.current.finish(ticket);refreshLabRequests(value=>value+1);}
  };
  const labMessageRevision=projectLabJobs.filter(j=>j.kind==='session').map(j=>`${j.id}:${j.updated_at}`).join('|');
  useEffect(()=>{
    if(!activeProjectId||!labMessageRevision)return;
    let live=true;const project=activeProjectId;
    api.messages(project,500).then(response=>{if(live&&projectRef.current===project)setMessages(response.messages||[]);}).catch(()=>{});
    return()=>{live=false;};
  },[activeProjectId,labMessageRevision]);
  const selectLabJob=async job=>{
    bootstrapGeneration.current++;setLoading(false);
    if(typeof router.query.lab_job==='string')handledJobLink.current=router.query.lab_job;
    const destination=laboratoryViewable(job)?{tab:'laboratory',labId:job.id}:{tab:'sessions'};
    if(job.project_id!==activeProjectId)await switchProject(job.project_id,destination);
    else{if(destination.labId)setSelectedLabId(destination.labId);setTab(destination.tab);}
    await acknowledgeJob(job);
  };
  const openCompletedResult=async result=>{
    const current=currentWorkspaceResult(resultsRef.current,result?.key,projectRef.current);
    if(!current)return;
    bootstrapGeneration.current++;setLoading(false);
    const project=current.projectId;
    try{
      if(current.source==='laboratory'){await selectLabJob(current.record);return;}
      setSelectedRunId(current.id);setTab('findings');
      if(!current.record.result){
        const full=await api.run(current.id);
        if(full.id!==current.id||full.project_id!==project)throw Error('The requested result identity did not match its saved run.');
        if(projectRef.current===project)setRuns(rows=>mergeBackgroundRun(rows,full,project));
      }
    }catch(value){if(projectRef.current===project)setError(value.message);}
  };
  const controlLabJob=async (job,action,options={})=>{
    try{return await api.labControl(typeof job==='string'?job:job.id,action,options);}catch(value){setError(value.message);throw value;}
  };
  const inspectSessionRun=async id=>{
    const project=projectRef.current,view=viewTracker.current.capture(),generation=loadGeneration.current;
    const isCurrent=()=>projectRef.current===project&&loadGeneration.current===generation&&viewTracker.current.isCurrent(view);
    try{
      const run=await resolveRunInspection({id,projectId:project,loadRun:api.run,isCurrent});
      if(!run)return;
      setRuns(rows=>mergeBackgroundRun(rows,run,project));setSelectedRunId(run.id);setTab('world');
    }catch(value){if(isCurrent())setError(value.message);}
  };
  useEffect(()=>{
    const handler=event=>{if(event.detail?.job)selectLabJob(event.detail.job).catch(e=>setError(e.message));};
    window.addEventListener('phaseforge:open-job',handler);
    return()=>window.removeEventListener('phaseforge:open-job',handler);
  });
  useEffect(()=>{
    const id=router.query.lab_job;
    if(!id||handledJobLink.current===id)return;
    const job=availableLaboratoryJobs.find(j=>j.id===id);
    if(job&&activeProjectId===job.project_id){handledJobLink.current=id;selectLabJob(job).catch(e=>setError(e.message));}
  },[router.query.lab_job,availableLaboratoryJobs,activeProjectId]);
  const observeLabFrame=async capture=>{
    const source=availableLaboratoryJobs.find(job=>job.id===(capture.sourceJobId||selectedLabId)&&job.project_id===activeProjectId&&['solver','published_simulation'].includes(job.kind));
    if(!source)throw Error("Select this project's saved laboratory run before observing a frame.");
    const selection=selectionFor(source.project_id);
    const job=await api.labObserve(source.id,{...capture,...selection,time_limit_seconds:timeLimitFor(source.project_id)});
    if(projectRef.current===source.project_id)setNotice("The captured image has been sent for evidence-based observation. Follow its progress in Agents.");
    return job;
  };

  const cancelChat = async () => {
    if(activeLabSession){try{await controlLabJob(activeLabSession,'cancel');}catch{}return;}
    const id = projectRequestId;
    if (!id) return;
    try {
      await api.cancelChatRequest(id);
      chatAbortRef.current?.abort();
      // The in-flight handler owns the busy flag until it settles.
    } catch (value) { setError(`Could not confirm cancellation: ${value.message}. Use Usage & cost to retry stopping the request.`); }
  };

  const explainRun = async (run) => {
    if (!run || run.project_id!==projectRef.current || busy || sendingRef.current) return;
    let selection;try{selection=selectionFor(run.project_id);}catch(value){if(projectRef.current===run.project_id)setError(value.message);return;}
    sendingRef.current=true;
    const id=requestId(), controller=new AbortController();
    chatAbortRef.current=controller;setActiveRequestId(id);setRequestProjectId(run.project_id);setBusy(true);setExplaining(true);setError(null);
    try {
      await api.explainRun(run.id,{request_id:id,provider:selection.provider,model:selection.model,reasoning_effort:selection.reasoning_effort},controller.signal);
      if(projectRef.current===run.project_id){
        const response=await api.messages(run.project_id,500);if(projectRef.current===run.project_id){setMessages(response.messages||[]);setFindingsRefresh(n=>n+1);}
      }
    } catch(value) { if(value.name!=="AbortError"&&projectRef.current===run.project_id)setError(`AI explanation did not complete: ${value.message}. No further work was started.`); }
    finally { if(chatAbortRef.current===controller)chatAbortRef.current=null;sendingRef.current=false;setExplaining(false);setBusy(false);setActiveRequestId(null); }
  };
  const explainRef=useRef(explainRun);explainRef.current=explainRun;
  useEffect(()=>{
    if(!autoExplain||busy||targetModelError)return;
    const ready=runs.find(r=>r.project_id===activeProjectId&&r.status==="completed"&&r.result&&ownedRuns.current.has(r.id));
    if(ready){ownedRuns.current.delete(ready.id);explainRef.current(ready);}
  },[runs,autoExplain,busy,activeProjectId,targetModelError]);

  const directNext = async (source,operation) => {
    if(!source||busy||sendingRef.current)return;
    if(source.project_id!==activeProjectId)throw Error("Select a source run in this project.");
    sendingRef.current=true;setRequestProjectId(activeProjectId);setBusy(true);setError(null);setNotice("");
    const world=activeProjectId;
    const view=viewTracker.current.capture();
    try {
      const key=`${world}:${source.id}:${operation}`;
      const request=nextReceipts.current.get(key)||requestId();nextReceipts.current.set(key,request);
      const response=await api.nextExperiment(world,{request_id:request,source_run_id:source.id,operation,run:true});
      // Retain the ID across a transport failure; retry checks the same receipt.
      if(response.status!=="reserved")nextReceipts.current.delete(key);
      if(projectRef.current!==world)return;
      if(response.manifest){setManifests(rows=>[response.manifest,...rows.filter(m=>m.id!==response.manifest.id)]);if(viewTracker.current.isCurrent(view))setSelectedManifestId(response.manifest.id);}
      if(response.run){setRuns(rows=>mergeBackgroundRun(rows,response.run,world));if(viewTracker.current.isCurrent(view))setSelectedRunId(response.run.id);ownedRuns.current.add(response.run.id);}
      if(response.execution_error)setError(response.execution_error);
      if(response.status==="reserved")setNotice("This request was reserved but its final queue receipt is unavailable. Inspect Runs before starting another pass; this request will not enqueue twice.");
    }catch(e){if(projectRef.current===world)setError(e.message);throw e;}
    finally{sendingRef.current=false;setBusy(false);}
  };
  const replayRun = async source=>{try{await directNext(source,"replay");}catch{}};
  const continueRun = async (source,action) => {
    try {
      if(action.kind==="direct") {await directNext(source,action.operation);return;}
      window.dispatchEvent(new CustomEvent("phaseforge:focus-chat"));
      await sendMessage({content:action.prompt,study_intent:action.id==="research_plan"?"plan":"experiment",
        experiment_options:DEFAULT_BUILD,request_id:requestId(),provider:null,model:null,agent_role:action.role,
        source_run_id:source.id,auto_run:false,attachments:[],structure_id:null});
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
    const world=activeProjectId,view=viewTracker.current.capture();setRequestProjectId(world);
    setBusy(true);
    setError(null);
    try {
      const run = await api.runManifest(activeManifest.id);
      ownedRuns.current.add(run.id);
      if(projectRef.current!==world)return;
      setRuns(current=>mergeBackgroundRun(current,run,world));
      if(viewTracker.current.isCurrent(view))setSelectedRunId(run.id);
    } catch (value) {
      if(projectRef.current===world)setError(value.message);
    } finally {
      sendingRef.current=false;
      setBusy(false);
    }
  };

  const importManifest = async (payload) => {
    if (!activeProjectId) throw new Error("Create a project before importing equations.");
    if(sendingRef.current||busy)throw new Error("Wait for the current request or press Stop before saving another setup.");
    sendingRef.current=true;
    const world=activeProjectId,view=viewTracker.current.capture();setRequestProjectId(world);
    setBusy(true);
    setError(null);
    try {
      const response = await api.importManifest(world, payload);
      if(projectRef.current!==world)return response;
      setManifests((current) => [response.manifest, ...current]);
      if(viewTracker.current.isCurrent(view))setSelectedManifestId(response.manifest.id);
      if (response.run) {
        ownedRuns.current.add(response.run.id);
        setRuns(current=>mergeBackgroundRun(current,response.run,world));
        if(viewTracker.current.isCurrent(view))setSelectedRunId(response.run.id);
      }
      const refreshed = await api.project(world);
      setProjects((current) =>
        current.map((item) => item.id === refreshed.id ? refreshed : item),
      );
    } catch (value) {
      if(projectRef.current===world)setError(value.message);
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
    if(imported.project_id&&projectRef.current!==imported.project_id)return;
    setStructures((current) => [
      imported,
      ...current.filter((item) => item.id !== imported.id),
    ]);
    selectStructure(imported.id);
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
        <p>{backend.error || "Waiting for your local research engine. Reopen PhaseForge if it stays disconnected."}</p>
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
                  disabled={!projects.length}
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
        {notice&&(notice!=="Your agent is working in the background. Open Agents for its tools and progress."||activeLabSession)&&<div className="expNotice" role="status">{notice}<button type="button" onClick={()=>setNotice("")} aria-label="Dismiss notice">×</button></div>}
        <nav className="workspaceTabs" aria-label="Experiment workspace">
          <button type="button" className={tab==="laboratory"?"active":""} onClick={()=>setTab("laboratory")}><Dna size={14}/>Scientific lab {labSolvers.length||''}</button>
          <button type="button" className={tab==="sessions"?"active":""} onClick={()=>setTab("sessions")}><Sparkles size={14}/>Agents</button>
          <button type="button" className={tab==="research"?"active":""} onClick={()=>setTab("research")}><BookOpen size={14}/>Notes & sources</button>
          <div className="expMore"><button type="button" onClick={()=>setMoreOpen(v=>!v)} aria-expanded={moreOpen}>Tools <ChevronDown size={14}/></button>{moreOpen&&<div className="expMoreMenu">{workspaceTools({runs,manifests,structures}).map(([id,label])=><button type="button" key={id} onClick={()=>{setTab(id);setMoreOpen(false);}}>{label}</button>)}</div>}</div>
          <button type="button" className="workspaceTabs__chat" onClick={()=>window.dispatchEvent(new CustomEvent("phaseforge:focus-chat"))}><MessageSquareText size={14}/>Chat</button>
        </nav>

        <WorkspaceResults results={completedResults} selectedKey={visibleResultKey} scopeKey={activeProjectId} onOpen={openCompletedResult}/>
        <div className="workspaceStage">
          <WorkspaceErrorBoundary scope="workspace-view" resetKey={`${activeProjectId}:${tab}:${selectedLab?.id||''}`}>
          {tab==='laboratory'&&<div style={{height:'100%',display:'flex',flexDirection:'column',minHeight:0,overflowY:'auto',scrollbarGutter:'stable'}}>
            {deliverableSession?.result?.deliverable?.status!=='fulfilled'&&<DeliverableStatus value={deliverableSession?.result?.deliverable} onInspect={()=>setTab('sessions')}/>}
            <div style={{padding:'8px 14px',display:'flex',gap:12,alignItems:'center',flexWrap:'wrap'}}>
              <strong title={selectedLab?.title||'Scientific workbench'} style={{flex:'1 1 260px',minWidth:0,display:'-webkit-box',WebkitLineClamp:2,WebkitBoxOrient:'vertical',overflow:'hidden'}}>{selectedLab?.title||'Scientific workbench'}</strong>
              <button type="button" className="button button--secondary" onClick={()=>setTab('capabilities')}>Solver coverage</button>
              {selectedLab&&<button type="button" className="button button--secondary" onClick={()=>setTab('sessions')}>Job history & evidence</button>}
            </div>
            {selectedLab?<LaboratoryViewer key={`viewer:${selectedLab.id}`} job={selectedLab} onCapture={['solver','published_simulation'].includes(selectedLab.kind)?observeLabFrame:undefined}/>:completedResults.length?<div className="expNotice"><strong>{completedResults.length} completed result{completedResults.length===1?'':'s'} retained in this project.</strong><p>Open the measured output below or browse Results above. Your equation measurements and newer laboratory artifacts share that results list.</p><button type="button" className="button button--secondary" onClick={()=>openCompletedResult(completedResults.at(-1))}>Open latest completed result</button></div>:<div className="expNotice"><strong>Describe the experiment or illustration you want in chat.</strong><p>Simulations retain computed states for playback. Scientific illustrations retain a rendered still and editable 3D geometry. The requested output determines the tools; this view holds both.</p><button type="button" className="button button--secondary" onClick={()=>setTab('capabilities')}>Explore supported physics and limitations</button></div>}
            {selectedLab&&<LaboratoryResultActions key={`review:${selectedLab.id}`} job={selectedLab} busy={reviewBusy} modelError={targetModelError} pendingReview={reviewDeliveries[activeProjectId]||null} onReview={reviewLabResult}/>}
            <LaboratoryLineage jobs={projectLabJobs} selectedJobId={selectedLab?.id} scopeKey={activeProjectId} onSelect={selectLabJob}/>

          </div>}
          {tab==='capabilities'&&<SolverCoverage/>}
          {tab==='studio'&&<StudioWorkbench project={activeProject} manifest={activeManifest} providers={providers} onInspect={value=>setSceneContext(v=>({...v,inspection:value}))} onCameraChange={value=>setSceneContext(v=>({...v,camera:value}))}/>}
          {tab==='sessions'&&<div style={{overflow:'auto',height:'100%'}}><LaboratoryActivity jobs={projectLabJobs} onSelect={selectLabJob} onControl={controlLabJob}/><ResearchSessions project={activeProject} providers={providers} onRefresh={refreshTaskData} onInspectRun={inspectSessionRun}/></div>}
          {tab==="research"&&<ResearchPlan project={activeProject} manifest={activeManifest} refreshKey={messages.length} busy={busy}
            onExperiment={content=>{setTab("world");return buildExperiment(content,DEFAULT_BUILD,false);}}
            onAdvanced={()=>setTab("advanced")}
            onPropose={async(content,intent="review")=>{window.dispatchEvent(new CustomEvent("phaseforge:focus-chat"));await sendMessage({content,study_intent:intent,experiment_options:intent==="experiment"?DEFAULT_BUILD:null,auto_run:false,agent_role:"builder",attachments:[]});}}/>}
          {tab==="advanced"&&<AdvancedExperiments backend={backend} embeddedProjectId={activeProjectId} embeddedManifestId={activeManifest?.id}/>}
          {tab==="equations"&&<EquationEditor key={activeProjectId||"none"} project={activeProject} busy={busy} onImport={importManifest} onBack={()=>setTab("world")}/>}
          {tab==="world"&&<ExperimentWorkspace project={activeProject} manifest={activeManifest} run={selectedRun} busy={busy} providerReady={!targetModelError} providers={providers} onSession={()=>setTab('sessions')} onInspect={value=>setSceneContext(v=>({...v,inspection:value}))} onCameraChange={value=>setSceneContext(v=>({...v,camera:value}))}
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
            providerReady={!targetModelError} />}
          </WorkspaceErrorBoundary>
        </div>

        {!['studio','laboratory','sessions','capabilities'].includes(tab)&&<RunStrip
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
        busy={projectBusy||labSubmitting||!!activeLabSession}
        activeJob={activeLabSession}
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
        onSend={sendLaboratoryMessage}
        onTimeLimitChange={rememberTimeDraft}
        onCancel={cancelChat}
        activeRequestId={activeLabSession?.id||projectRequestId}
        findingsRun={latestEquationResult?.record||null}
        onOpenFindings={()=>openCompletedResult(latestEquationResult)}
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
