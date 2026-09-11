const LEGACY_PREFIX='Build the next concrete experiment for this researcher-authorized bounded task.';
const LEGACY_MARKER='TASK AND SHARED ARTIFACTS:\n';
const VIEWPORT_MARKER='\n\nCurrent viewport inspection (visual context, not new empirical evidence): ';

function userFacingContent(message) {
  if(message?.role!=='user')return message?.content||'';
  if(typeof message.metadata?.display_content==='string')return message.metadata.display_content;
  const content=message.content||'',at=content.lastIndexOf(VIEWPORT_MARKER);
  if(at<0)return content;
  // Old versions appended a serialized selection to the researcher's sentence.
  // Recognize only that exact valid suffix, leaving ordinary prose untouched.
  try{
    const context=JSON.parse(content.slice(at+VIEWPORT_MARKER.length));
    if(context&&typeof context==='object'&&!Array.isArray(context)&&('camera' in context||'inspection' in context))return content.slice(0,at);
  }catch{}
  return content;
}

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
    content:session&&message?.role==='user'&&typeof objective==='string'?objective:userFacingContent(message)};
}
