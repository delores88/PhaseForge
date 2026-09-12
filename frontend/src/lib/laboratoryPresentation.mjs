export const ACTIVE_LAB_STATES=new Set(['queued','running','provisioning','waiting']);
export const RESUMABLE_LAB_STATES=new Set(['paused','failed','timed_out']);
export function labCompletedAt(job) {
  return (job.events||[]).findLast(event=>event.kind==='completed')?.at||job.completed_at||job.updated_at;
}
export function labJobPresentation(job) {
  const state=job.state||'unknown';
  const completed=Date.parse(labCompletedAt(job)),seen=Date.parse(job.seen_at);
  const unread=state==='completed'&&(!Number.isFinite(seen)||(Number.isFinite(completed)&&seen<completed));
  const labels={queued:'Queued',running:'Running',provisioning:'Preparing environment',waiting:'Waiting',completed:'Completed',paused:'Paused',failed:'Failed',timed_out:'Time limit reached',cancelled:'Cancelled'};
  const fraction=job.progress?.fraction;
  return {state,label:labels[state]||state.replaceAll('_',' '),active:ACTIVE_LAB_STATES.has(state),unread,resumable:RESUMABLE_LAB_STATES.has(state)&&job.kind!=='published_simulation',
    percent:Number.isFinite(fraction)?Math.round(Math.max(0,Math.min(1,fraction))*100):null};
}
export function labElapsed(job,now=Date.now()) {
  const start=Date.parse(job.created_at),end=ACTIVE_LAB_STATES.has(job.state)?now:Date.parse(job.state==='completed'?labCompletedAt(job):job.updated_at);
  if(!Number.isFinite(start)||!Number.isFinite(end))return null;
  return Math.max(0,Math.floor((end-start)/1000));
}
export function labDuration(seconds) {
  const value=Math.max(0,Math.floor(seconds));
  return value>=3600?`${Math.floor(value/3600)}h ${Math.floor(value%3600/60)}m`:value>=60?`${Math.floor(value/60)}m ${value%60}s`:`${value}s`;
}
