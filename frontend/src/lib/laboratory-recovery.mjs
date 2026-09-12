// Recovery is background file I/O; it never replaces the scientific job state,
// completion timestamp, unread marker, or numerical progress.
export function laboratoryRecovery(job) {
  const event=(job?.events||[]).findLast(row=>row.kind==='nr_recovery');
  const value=job?.recovery||event?.data;
  if(!value?.operation_id||!['started','cancel_requested','completed','failed','cancelled'].includes(value.status))return null;
  const active=['started','cancel_requested'].includes(value.status);
  const label=value.status==='started'?(value.phase==='queued'?'Recovery queued':'Verifying saved files'):
    {cancel_requested:'Stopping recovery…',completed:'Recovery completed',failed:'Recovery failed',cancelled:'Recovery cancelled'}[value.status];
  return {...value,active,label,error:typeof value.error==='string'?value.error:null};
}

export function laboratoryAttention(job) {
  const value=job?.result?.attention;
  if(value?.schema!=='phaseforge.agent-attention.v1'||value.status!=='needs_direction')return null;
  return {...value,actions:(Array.isArray(value.actions)?value.actions:[]).filter(action=>
    action&&['revise_request','inspect_capabilities','inspect_evidence','resume'].includes(action.id)&&typeof action.label==='string')};
}
