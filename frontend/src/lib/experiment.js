// Shared by the actual UI and behavioral tests. No provider/model interpretation
// of execution consent, no fictitious numerical results, no domain presets.
export const DEFAULT_BUILD = {assumptions_allowed:true,max_wall_seconds:180,max_memory_mb:1024,max_candidates:64};
export const RUNNING = new Set(['queued','running']);
export function validateBudget(value) {
  for (const [key,lo,hi] of [['max_wall_seconds',1,86400],['max_memory_mb',128,262144],['max_candidates',1,131072]]) {
    if(!Number.isInteger(value[key])||value[key]<lo||value[key]>hi)throw new Error(`${key.replaceAll('_',' ')} must be an integer between ${lo} and ${hi}.`);
  }
  return {...value,assumptions_allowed:value.assumptions_allowed===true};
}
export function buildRequest({project,manifest,instruction='',options=DEFAULT_BUILD,run=false,sourceRun=null}) {
  if(!project?.id)throw new Error('Create a project before building an experiment.');
  const budget=validateBudget(options);
  const goal=instruction.trim()||project.question;
  if(!goal?.trim())throw new Error('Describe what this experiment should test.');
  return {content:`${goal}\n\nBuild ${manifest?'a new revision of the selected experiment':'an executable experiment'} for this subquestion, not another research plan. ${budget.assumptions_allowed?'Use clearly declared hypothetical/scaled inputs where needed for exploratory computation; do not claim measured, fitted or validated data.':'Use supplied inputs; identify the minimum missing input rather than inventing empirical values.'} Preserve meaningful measurements and explicit limits. Do not mark any prior research task completed.`,
    study_intent:'experiment',context_manifest_id:manifest?.id||null,source_run_id:sourceRun?.id||null,
    experiment_options:budget,agent_role:'builder',auto_run:run===true,attachments:[]};
}
export function checkedResponse(response) {
  if(!response?.assistant_message)throw new Error('Backend returned no assistant response.');
  if(response.assistant_message.kind==='error'){
    const error=new Error(response.assistant_message.content||'Request did not produce an experiment.');error.savedResponse=response;throw error;
  }
  return {manifest:response.manifest||null,runId:response.submitted_run_id||null,
    gap:response.capability_gap||null,queueError:response.assistant_message.metadata?.execution_error||null,
    planned:!!response.assistant_message.metadata?.research_plan_id};
}
// A synchronous lock, not a render-lagging React state flag. Always releases after
// failures and Stop, and never enqueues or retries a second request automatically.
export function createSingleFlight() {let active=false;return async task=>{if(active)throw new Error('A request is already in progress. Wait or press Stop.');active=true;try{return await task();}finally{active=false;}};}
export function legacyDestination(query={}) {
  const out={panel:'advanced'};
  for(const key of ['project','manifest','study','run'])if(typeof query[key]==='string'&&/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(query[key]))out[key]=query[key];
  return {pathname:'/',query:out};
}
export function localDraft({project,title,states,constants,start=0,end=10,step=.01,budget=DEFAULT_BUILD}) {
  validateBudget(budget);
  if(!states.length||states.length>128)throw new Error('Provide 1–128 state variables.');
  const used=new Set(['t']);
  for(const row of [...states,...constants]) {
    if(!/^[A-Za-z_][A-Za-z0-9_]*$/.test(row.name)||used.has(row.name))throw new Error('Use unique variable and constant names; t is reserved.');used.add(row.name);
    if(!Number.isFinite(Number(row.initial??row.value))||String(row.initial??row.value).trim()==='')throw new Error('Every starting value and constant must be a finite number.');
  }
  if(states.some(s=>!s.derivative.trim()))throw new Error('Every state needs a derivative expression.');
  const count=Math.ceil((end-start)/step);
  if(![start,end,step].every(Number.isFinite)||end<=start||step<=0||count>2e6)throw new Error('Choose an increasing time interval and positive step (at most 2,000,000 steps).');
  return {title:title.trim()||'User-authored experiment',question:project.question,scientific_boundary:'Exploratory user-authored ODE; numerical output is not empirical validation.',hypothesis:'Evaluate the supplied equations and starting values over the stated horizon.',
    model:{kind:'state_vector_ode',variables:states.map(s=>({name:s.name,initial:Number(s.initial),unit:s.unit||''})),derivatives:states.map(s=>({variable:s.name,expression:s.derivative}))},
    constants:constants.map(c=>({name:c.name,value:Number(c.value),unit:c.unit||'',description:'User supplied'})),trajectory:[],
    integration:{method:'rk4',start_time:start,end_time:end,time_step:step,output_stride:Math.max(1,Math.ceil(count/1000)),max_steps:Math.min(2000000,Math.max(1,count+1))},
    search:{enabled:false,algorithm:'none',variables:[],objectives:[],population:1,generations:1,elite_fraction:.2,mutation_scale:.1,seed:42},
    observables:states.map(s=>({name:`observed_${s.name}`,expression:s.name,unit:s.unit||''})),constraints:[],falsification:[],
    visualization:{kind:'trajectory',x:states[0].name,y:states[1]?.name||'0',z:states[2]?.name||'0',point_size:.05,max_frames:1000,trails:true,entities:[]},
    compute:{preference:'cpu',policy:'interactive',candidate_count:1,batch_size:1,max_wall_seconds:budget.max_wall_seconds,max_memory_mb:budget.max_memory_mb},
    limitations:['No empirical calibration or physical correctness is inferred. State-space rendering is not a literal physical scene. No acceptance tests have yet been authored.']};
}
