const API_BASE =
  (typeof window !== 'undefined' && window.__PHASEFORGE_DESKTOP__ ? window.location.origin : null) || process.env.NEXT_PUBLIC_API_BASE_URL || "http://127.0.0.1:7331";

async function request(path, options = {}) {
  const response = await fetch(`${API_BASE}${path}`, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...(options.headers || {}),
    },
  });
  if (response.status === 204) return null;
  const body = await response.json().catch(() => null);
  if (!response.ok) {
    const message =
      body?.error?.message ||
      body?.message ||
      `${response.status} ${response.statusText}`;
    const error = new Error(message);
    error.status = response.status;
    error.body = body;
    throw error;
  }
  return body;
}

function json(method, payload = {}) {
  return { method, body: JSON.stringify(payload) };
}

export const api = {
  studio: (path, payload, signal) => request(`/api/${path}`, {...(payload===undefined?{}:json('POST',payload)),signal}),
  tasks: (projectId, signal) => request(`/api/projects/${projectId}/tasks`, {signal}),
  task: (id, signal) => request(`/api/tasks/${id}`, {signal}),
  createTask: (projectId, payload) => request(`/api/projects/${projectId}/tasks`, json('POST',payload)),
  controlTask: (id, payload) => request(`/api/tasks/${id}/control`, json('POST',payload)),
  nextExperiment: (project,payload,signal)=>request(`/api/projects/${project}/experiments/next`,{...json("POST",payload),signal}),
  researchWorkspace: (id, signal) => request(`/api/projects/${id}/research`, {signal}),
  researchSearch: (id, query, consent, signal) => request(`/api/projects/${id}/research/search`, {...json("POST", {query,consent}),signal}),
  researchTask: (id, payload, signal) => request(`/api/projects/${id}/research/tasks`, {...json("POST",payload),signal}),
  researchData: (id, payload, signal) => request(`/api/projects/${id}/research/data`, {...json("POST",payload),signal}),
  computeAdvice: (id, signal) => request(`/api/manifests/${id}/compute-advice`, {signal}),
  studies: (projectId, signal) => request(`/api/discovery/studies${projectId ? `?project_id=${projectId}` : ""}`, {signal}),
  study: (id, signal) => request(`/api/discovery/studies/${id}`, {signal}),
  createStudy: recipe => request("/api/discovery/studies", json("POST", recipe)),
  startStudy: id => request(`/api/discovery/studies/${id}/start`, json("POST")),
  pauseStudy: id => request(`/api/discovery/studies/${id}/pause`, json("POST")),
  cancelStudy: id => request(`/api/discovery/studies/${id}/cancel`, json("POST")),
  nextStudyProposal: id => request(`/api/discovery/studies/${id}/next-proposal`, json("POST")),
  reviewStudy: id => request(`/api/discovery/studies/${id}/review`, json("POST")),
  forkStudy: id => request(`/api/discovery/studies/${id}/fork`, json("POST")),
  notebook: (id, signal) => request(`/api/research/notebooks/${id}`, {signal}),
  saveNotebook: (id, value) => request(`/api/research/notebooks/${id}`, json("PUT", value)),
  literature: query => request("/api/research/literature", json("POST", {query, allow_public_query: true})),
  signals: (id, signal) => request(`/api/runs/${id}/signals`, {signal}),
  exportStudy: async id => {
    const response = await fetch(`${API_BASE}/api/discovery/studies/${id}/export`);
    if (!response.ok) { const body = await response.json().catch(() => ({})); throw new Error(body?.error?.message || "Export failed"); }
    return response.blob();
  },
  baseUrl: API_BASE,
  workflow: signal => request("/api/workflow", {signal}),
  telemetry: signal => request("/api/telemetry", {signal}),
  findings: (id, signal) => request(`/api/runs/${id}/findings`, {signal}),
  explainRun: (id, payload, signal) => request(`/api/runs/${id}/explain`, {...json("POST",payload), signal}),
  usage: () => request("/api/usage"),
  usageSettings: () => request("/api/usage/settings"),
  saveUsageSettings: (payload) => request("/api/usage/settings", {method: "PUT", body: JSON.stringify(payload)}),
  cancelAllAgents: () => request("/api/chat/cancel-all", {method: "POST", body: "{}"}),
  health: () => request("/api/health"),
  verificationList: (studyId, signal) => request(`/api/verification/dossiers?study_id=${studyId}`, {signal}),
  verification: (id, signal) => request(`/api/verification/dossiers/${id}`, {signal}),
  createVerification: payload => request("/api/verification/dossiers", json("POST",payload)),
  startVerification: id => request(`/api/verification/dossiers/${id}/start`,json("POST")),
  cancelVerification: id => request(`/api/verification/dossiers/${id}/cancel`,json("POST")),
  probeVerification: () => request("/api/verification/probe",json("POST")),
  verificationCatalog: project => request(`/api/verification/catalog?project_id=${project}`),
  addVerificationReference: payload => request("/api/verification/catalog",json("POST",payload)),
  verificationSearch: (id, query, consent) => request(`/api/verification/dossiers/${id}/search`,json("POST",{query,allow_public_query:consent})),
  saveVerificationReview: (id, payload) => request(`/api/verification/dossiers/${id}/review`,json("POST",payload)),
  verificationAgentReview: id => request(`/api/verification/dossiers/${id}/agent-review`,json("POST")),
  evidenceWindow: (id, {series,start,end,max_points=512}={}, signal) => {
    const q=new URLSearchParams({max_points:String(max_points)});
    if(series)q.set("series",series);if(Number.isFinite(start))q.set("start",String(start));if(Number.isFinite(end))q.set("end",String(end));
    return request(`/api/verification/runs/${id}/window?${q}`,{signal});
  },

  hardware: () => request("/api/hardware"),
  capabilities: () => request("/api/capabilities"),
  scientificEngines: () => request("/api/scientific/engines"),

  projects: () => request("/api/projects"),
  project: (id) => request(`/api/projects/${id}`),
  createProject: (payload) => request("/api/projects", json("POST", payload)),
  updateProject: (id, payload) =>
    request(`/api/projects/${id}`, json("PATCH", payload)),
  deleteProject: (id) => request(`/api/projects/${id}`, { method: "DELETE" }),

  messages: (projectId, limit = 300) =>
    request(`/api/projects/${projectId}/messages?limit=${limit}`),
  sendMessage: (projectId, payload, signal) =>
    request(`/api/projects/${projectId}/messages`, {
      ...json("POST", payload),
      signal,
    }),
  cancelChatRequest: (requestId) =>
    request(`/api/chat/requests/${requestId}/cancel`, json("POST")),

  manifests: (projectId, limit = 100) =>
    request(`/api/projects/${projectId}/manifests?limit=${limit}`),
  manifest: (id) => request(`/api/manifests/${id}`),
  importManifest: (projectId, payload) =>
    request(`/api/projects/${projectId}/manifests`, json("POST", payload)),
  runManifest: (id) => request(`/api/manifests/${id}/run`, json("POST")),

  runs: (limit = 100, projectId = null) =>
    request(
      `/api/runs?limit=${limit}${projectId ? `&project_id=${projectId}` : ""}`,
    ),
  run: (id) => request(`/api/runs/${id}`),
  submitRun: (payload) => request("/api/runs", json("POST", payload)),
  cancelRun: (id) => request(`/api/runs/${id}/cancel`, json("POST")),

  providers: () => request("/api/providers"),
  saveProviderKey: (provider, payload) =>
    request(`/api/providers/${provider}/key`, json("PUT", payload)),
  deleteProviderKey: (provider) =>
    request(`/api/providers/${provider}/key`, { method: "DELETE" }),
  providerModels: (provider) => request(`/api/providers/${provider}/models`),
  saveProviderModel: (provider, model) =>
    request(`/api/providers/${provider}/model`, json("PUT", { model })),
  testProvider: (provider) =>
    request(`/api/providers/${provider}/test`, json("POST")),
  deleteProvider: (provider) =>
    request(`/api/providers/${provider}`, { method: "DELETE" }),

  molecules: (projectId = null, limit = 250) =>
    request(
      `/api/molecules?limit=${limit}${projectId ? `&project_id=${projectId}` : ""}`,
    ),
  molecule: (id) => request(`/api/molecules/${id}`),
  importMolecule: (payload) => request("/api/molecules", json("POST", payload)),
  deleteMolecule: (id) => request(`/api/molecules/${id}`, { method: "DELETE" }),
  qmmmPlans: (id) => request(`/api/molecules/${id}/qmmm-plans`),
  createQmmmPlan: (id, payload) =>
    request(`/api/molecules/${id}/qmmm-plans`, json("POST", payload)),
  campaigns: (id) => request(`/api/molecules/${id}/campaigns`),
  createCampaign: (id, payload) =>
    request(`/api/molecules/${id}/campaigns`, json("POST", payload)),
};

export function eventsUrl() {
  return `${API_BASE.replace(/^http/, "ws")}/api/events/ws`;
}
