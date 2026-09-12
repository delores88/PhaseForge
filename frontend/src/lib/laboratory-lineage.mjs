const KINDS=new Set(['solver','illustration','generated','published_simulation','ml_study','study_plot']);
const sourceValue=value=>typeof value==='string'&&value?value:null;

export function laboratoryDerivation(job){
  const input=job.input||{},result=job.result||{};
  const restart=input.restarted_from_illustration_id||input.restarted_from_job_id||input.restarted_from;
  if(sourceValue(restart))return {label:'Restart of',sourceId:restart};
  const illustration=input.source_illustration_id||result.source_illustration_id;
  if(sourceValue(illustration))return {label:'Render from',sourceId:illustration};
  const source=input.source_job_id||result.source_job_id||input.source?.job_id||input.source_pin?.job_id;
  if(sourceValue(source))return {label:job.kind==='published_simulation'?'Published from':'Derived from',sourceId:source};
  if(job.kind==='study_plot'&&sourceValue(job.parent_id))return {label:'Chart from',sourceId:job.parent_id};
  return null;
}

/** Primary scientific outputs, never each orchestration/tool helper. No selection side effects. */
export function laboratoryLineage(jobs=[]){
  const all=jobs.filter(job=>job&&typeof job.id==='string'),byId=new Map(all.map(job=>[job.id,job]));
  const represented=new Set();
  for(const job of all){
    if(job.kind==='published_simulation'){
      const source=laboratoryDerivation(job)?.sourceId;
      if(byId.get(source)?.kind==='generated')represented.add(source);
    }
    if(job.kind==='ml_study'&&job.result?.plot_job_id)represented.add(job.result.plot_job_id);
  }
  return all.filter(job=>KINDS.has(job.kind)&&!represented.has(job.id)&&!(job.kind==='generated'&&(job.state!=='completed'||job.input?.ml_stage))).sort((a,b)=>{
    const time=value=>{const t=Date.parse(value.created_at||value.queued_at||'');return Number.isFinite(t)?t:0;};
    return time(a)-time(b)||a.id.localeCompare(b.id);
  }).map(job=>({job,derivation:laboratoryDerivation(job),type:job.kind==='illustration'?'Illustration':job.kind==='published_simulation'?'Retained simulation':job.kind==='solver'?'Numerical experiment':job.kind==='ml_study'?'ML study':job.kind==='study_plot'?'Evaluation chart':'Saved output'}));
}
