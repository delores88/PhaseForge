const LEGACY_PREFIX='Build the next concrete experiment for this researcher-authorized bounded task.';
const LEGACY_MARKER='TASK AND SHARED ARTIFACTS:\n';

// Presentation only: saved prompts and numerical provenance remain unchanged.
export function chatPresentation(message) {
  const meta=message?.metadata||{};
  let session=meta.source==='research_session'&&typeof meta.origin_task_id==='string';
  let objective=meta.session_objective,cycle=meta.origin_task_cycle;
  if(!session&&!meta.source&&message?.role==='user'&&message.content?.startsWith(LEGACY_PREFIX)) {
    const at=message.content.indexOf(LEGACY_MARKER);
    if(at>=0)try {
      const data=JSON.parse(message.content.slice(at+LEGACY_MARKER.length));
      if(typeof data.objective==='string'&&Number.isInteger(data.cycle)&&data.cycle>0) {
        session=true;objective=data.objective;cycle=data.cycle;
      }
    } catch { /* An ordinary or malformed message keeps its original presentation. */ }
  }
  return {session,label:session?`RESEARCH TEAM${Number.isInteger(cycle)?` · CYCLE ${cycle}`:''}`:'YOU',
    content:session&&message?.role==='user'&&typeof objective==='string'?objective:message?.content||''};
}
