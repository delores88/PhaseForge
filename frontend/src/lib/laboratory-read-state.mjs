import {labCompletedAt,labJobPresentation} from './laboratoryPresentation.mjs';

export function observedCompletedJobs(jobs,projectId,viewedAt=Infinity){
  return jobs.filter(job=>job.project_id===projectId&&labJobPresentation(job).unread)
    .map(job=>({id:job.id,completed_at:labCompletedAt(job)}))
    .filter(job=>Number.isFinite(Date.parse(job.completed_at))&&Date.parse(job.completed_at)<=viewedAt);
}

// The opening action owns this timestamp. Loading a project or its first job
// snapshot must not advance the read boundary to a later completion.
export async function loadObservedCompletedJobs({projectId,viewedAt=Date.now(),jobs,fetchJobs}){
  const snapshot=jobs??(await fetchJobs(projectId)).jobs??[];
  return observedCompletedJobs(snapshot,projectId,viewedAt);
}

// A late polling response may predate the acknowledgement response. Merge only
// its seen watermark; never hide a newly completed result or overwrite its data.
export function mergeAcknowledgedJobs(jobs,watermarks){
  return jobs.map(job=>{
    const seen=watermarks.get(job.id);
    return seen&&(!job.seen_at||Date.parse(seen)>Date.parse(job.seen_at))?{...job,seen_at:seen}:job;
  });
}
