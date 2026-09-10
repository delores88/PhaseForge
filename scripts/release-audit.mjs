// Source/contract audit only. Native compilation and provider tests are separate gates.
import fs from 'node:fs';import path from 'node:path';import {fileURLToPath} from 'node:url';import {execFileSync} from 'node:child_process';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const read=p=>fs.readFileSync(path.join(root,p),'utf8');let passed=0,failed=0;
function check(name,ok){console.log(`${ok?'PASS':'FAIL'} ${name}`);ok?passed++:failed++;}
function tokens(name,s,values){check(name,values.every(x=>s.includes(x)));}
function walk(p){return fs.readdirSync(p,{withFileTypes:true}).flatMap(x=>{if(x.name==='.git')return[];const f=path.join(p,x.name);return x.isDirectory()?walk(f):[f];});}
function shippingFiles(){
 try{
  // Include tracked files even when they match an ignore rule. Only genuinely
  // gitignored, untracked local build/QA artifacts are outside the source audit.
  const names=execFileSync('git',['ls-files','--cached','--others','--exclude-standard','-z'],{cwd:root,encoding:'utf8',maxBuffer:16*1024*1024});
  return [...new Set(names.split('\0').filter(Boolean))].map(name=>path.resolve(root,name)).filter(file=>fs.existsSync(file)&&fs.statSync(file).isFile());
 }catch{
  // An exported source archive has no Git metadata; inspect every included file
  // rather than exempting artifacts merely because their folder name is familiar.
  console.log('Git file inventory unavailable; auditing every source-archive file.');return walk(root);
 }
}
const files=shippingFiles();const rel=p=>path.relative(root,p).split(path.sep).join('/');
const required=['README.md','README.windows.md','README.linux.md','RELEASE_NOTES_0.6.0.md','RELEASE_NOTES_0.7.0.md','backend/Cargo.toml',
 'backend/src/usage/mod.rs','backend/src/agent/proposal.schema.json','backend/src/agent/schema.rs','backend/src/agent/manifest-guide.md',
 'frontend/package.json','frontend/pages/_document.jsx','frontend/pages/usage/index.jsx','frontend/src/lib/theme.jsx',
 'frontend/src/components/usage/UsageView.jsx','frontend/styles/workspace.css','docs/USAGE_AND_COST.md','docs/RELEASE_VALIDATION.md',
 'tests/test_proposal_schema.py','tests/frontend-api.mjs','INSTALL_ALL.txt','START_ALL.txt'];
check('Complete incremental project files',required.every(x=>fs.existsSync(path.join(root,x))));
const pkg=JSON.parse(read('frontend/package.json')),cargo=read('backend/Cargo.toml'),desktop=JSON.parse(read('desktop/package.json')),expectedVersion=desktop.version;
check(`Backend release matches desktop ${expectedVersion}`,cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1]===expectedVersion);
check(`Frontend release matches desktop ${expectedVersion}`,pkg.version===expectedVersion);check('Working Node floor retained',pkg.engines.node==='>=20.9.0');
check('Working Next.js frontend retained',pkg.scripts.build==='next build'&&pkg.dependencies.next==='16.3.3');
check('No Windows wrapper files',!files.some(x=>x.endsWith('.ps1')));
const helpers=['INSTALL_ALL','INSTALL_BACKEND','INSTALL_FRONTEND','START_ALL','START_BACKEND','START_FRONTEND','STOP_ALL','CHECK_ENVIRONMENT'];
check('Complete self-contained PowerShell text helpers',helpers.every(x=>{const s=read(x+'.txt');return s.includes('$ErrorActionPreference')&&!/\.ps1\b/i.test(s)&&s.length>500;}));
tokens('Native backend install gates',read('INSTALL_BACKEND.txt'),['cargo','"build"','"test"','--all-targets','Cargo.lock']);
tokens('Actual frontend production-build gate',read('INSTALL_FRONTEND.txt'),['npm','"run", "build"','out\\index.html']);
check('Private dependency metadata probe absent',!/require\.resolve\([^)]*package\.json/.test(read('INSTALL_FRONTEND.txt')));
tokens('Upgrade rejects obsolete running services',read('START_ALL.txt'),[expectedVersion,'PFExpectedVersion','already in use','PFVerifiedHealth']);
const agent=read('backend/src/agent/mod.rs'),providers=read('backend/src/agent/providers.rs'),schema=JSON.parse(read('backend/src/agent/proposal.schema.json')),
 api=read('backend/src/api/mod.rs'),frontApi=read('frontend/src/lib/api.js'),usage=read('backend/src/usage/mod.rs'),db=read('backend/src/persistence/database.rs');
schema.$defs.scene=JSON.parse(read('docs/scene.schema.json'));
const sceneContract=schema.$defs.manifest.properties.visualization;
sceneContract.properties.scene={anyOf:[{$ref:'#/$defs/scene'},{type:'null'}]};sceneContract.required.push('scene');
tokens('Runtime composes the canonical procedural scene contract',read('backend/src/agent/schema.rs'),['../../../docs/scene.schema.json','#/$defs/scene','visualization["required"]']);
check('Proposal uses nested manifest, not JSON string',schema.properties.manifest&&schema.properties.manifest.anyOf&&!schema.properties.manifest_json&&!providers.includes('pub manifest_json'));
check('All integration fields required', ['method','start_time','end_time','time_step','output_stride','max_steps'].every(x=>schema.$defs.integration.required.includes(x)));
let closed=true;function walkSchema(v){if(!v||typeof v!=='object')return;if(v.type==='object'&&(v.additionalProperties!==false||Object.keys(v.properties).sort().join()!=[...v.required].sort().join()))closed=false;Object.values(v).forEach(walkSchema);}walkSchema(schema);check('Every provider object closed and fully required',closed);
tokens('Same strict schema used by both providers',providers,['proposal_schema()','json_schema','strict','output_config']);
tokens('Local shape plus scientific validation',providers+read('backend/src/agent/schema.rs'),['validate_proposal_shape','validate_draft','validate_manifest']);
const draftGate=read('backend/src/agent/schema.rs').split('pub fn validate_draft(')[1]?.split('#[cfg(test)]')[0]||'';
check('Validator adapts warning-list success to unit and propagates errors',
 /->\s*anyhow::Result<\(\)>/.test(draftGate)&&
 /validate_manifest\(&manifest\)\.context\("runtime rejected the proposed experiment"\)\?;\s*Ok\(\(\)\)/.test(draftGate));

tokens('No execution before validated proposal',agent,['generate_valid_proposal','validating_proposal','apply_proposal','validate_manifest']);
tokens('Bounded separately metered repair loop',agent+usage,['0..=settings.repair_attempts','settings.repair_attempts','repair_attempts > 2','"repair"','audited_call']);
check('Usage persisted before response interpretation',agent.indexOf('Some(&response.body)')<agent.indexOf('response.text()'));
tokens('Cancellation retained through validation and commit',agent,['RequestGuard','cancelled_requests','cancellation.is_cancelled()','self.active_requests.lock()','self.usage.settings()?.paused']);
tokens('Paid test calls use the usage gate',agent,['"connection_test"','self.audited_call','self.usage.begin']);
tokens('All-agent stop and persistent pause',api+agent,['/api/chat/cancel-all','save_usage_settings','updated.paused','cancel_all']);
tokens('Usage endpoints connected end-to-end',api+frontApi,['/api/usage','/api/usage/settings','saveUsageSettings','cancelAllAgents']);
tokens('Provider counts with cache and reasoning semantics',usage,['input_tokens_details/cached_tokens','output_tokens_details/reasoning_tokens','cache_read_input_tokens','cache_creation_input_tokens','saturating_add(creation)']);
tokens('Unreported usage keeps a reservation',usage,['if self.usage.reported','reserved_input_tokens.saturating_add(self.reserved_output_tokens)','interrupted']);
check('Lifetime accounting query is not truncated',/SELECT json FROM objects WHERE kind = 'usage_call'"/.test(db));
tokens('Admission is serialized and occurs before network',usage+agent,['gate: Arc<Mutex<()>>','let _guard = self.gate.lock()','self.usage.begin','client.request_completion']);
tokens('No guessed rates; current and historical unpriced calls block cost enforcement',usage,['rates: Vec::new()','model has no rate card','historical calls are unpriced']);
tokens('Model/key setup still independent',api+frontApi,['/api/providers/:provider/key','/api/providers/:provider/model','providerModels','saveProviderKey','saveProviderModel']);
const settings=read('frontend/src/components/settings/SettingsView.jsx');
check('Key save button not gated on model',settings.includes('!form.apiKey?.trim()')||settings.includes('!form.api_key?.trim()'));
tokens('Actual live model discovery retained',providers,['list_models','openai_models','anthropic_models']);
const shell=read('frontend/src/components/shared/AppShell.jsx'),workbench=read('frontend/src/components/workspace/ResearchWorkbench.jsx'),chat=read('frontend/src/components/workspace/ChatDrawer.jsx');
check('Exactly one navigation aside', (shell.match(/<aside\b/g)||[]).length===1&&!shell.includes('activityRail')&&!shell.includes('contextSidebar'));
check('No forced question modal',workbench.includes('open={newWorldOpen}')&&!workbench.includes('open={newWorldOpen || noProjects}')&&!workbench.includes('first={noProjects}'));
const createBlock=workbench.slice(workbench.indexOf('const createWorld ='),workbench.indexOf('const renameProject ='));
check('World creation is local-only',createBlock.includes('api.createProject')&&!createBlock.includes('api.sendMessage'));
tokens('Non-modal experiment landing actions',read('frontend/src/components/workspace/ExperimentWorkspace.jsx'),['onClick={onNew}','Start researching','Build experiment','Build & run','onClick={onEquations}','Enter equations']);
tokens('Chat has controls, history, attachments, retry and branch',chat,['Research history','Edit & branch','Regenerate','addFiles','onCancel','draftOwner','branch_from_message_id','reply_to_message_id']);
tokens('Real branch ancestry used in model context',agent,['conversation_ancestry','parent_message_id','context_manifest_id']);
tokens('Desktop split width 10 to 60 percent',chat,['Math.max(10','Math.min(60','role="separator"','researchChatReveal']);
tokens('Theme persistence and pre-render bootstrap',read('frontend/src/lib/theme.jsx')+read('frontend/pages/_document.jsx'),['phaseforge.theme','dataset.theme','localStorage','ThemeToggle']);
tokens('Readable dark and light workspace styles',read('frontend/styles/workspace.css'),['data-theme="light"','.primarySidebar','.researchMessage .chatMarkdown','.usagePage']);
tokens('Usage view actual controls and unknown-cost labels',read('frontend/src/components/usage/UsageView.jsx'),['Unpriced','Pause paid calls','Stop all agents','Output tokens per call','Repair attempts','rate','Unknown']);
const molecular=read('backend/src/science/molecular.rs'),engines=read('backend/src/science/engines.rs');
tokens('Molecular parser and planner retained',molecular,['parse_pdb','parse_mol_v2000','parse_xyz','plan_qmmm_region','plan_campaign']);
tokens('Scientific-engine registry retained',engines,['gromacs','openmm','cp2k','xtb','lammps','openbabel']);
tokens('Molecular viewport delegates coordinate-aware rendering and retains research controls',read('frontend/src/components/workspace/MoleculeViewport.jsx')+read('frontend/src/components/workspace/MoleculeWorkbench.jsx')+read('frontend/src/components/workspace/ScientificModelViewer.jsx')+read('frontend/src/lib/model-viewer.mjs'),['ScientificModelViewer','createScientificModelViewer','createMolecularSurface','prepareMolecularCoordinates','THREE.InstancedMesh','selectedAtomIds','onAtomSelect','Create region plan','Import structure']);
let unresolved=[];const frontendFiles=files.filter(x=>x.includes(path.sep+'frontend'+path.sep)&&/\.(?:[cm]?js|jsx)$/.test(x));
for(const file of frontendFiles){for(const m of read(rel(file)).matchAll(/(?:\bfrom\s+|\bimport\s*(?:\(\s*)?)["']([^"']+)["']/g)){
 const spec=m[1];let b;if(spec.startsWith('.'))b=path.resolve(path.dirname(file),spec);else if(spec.startsWith('@/'))b=path.join(root,'frontend',spec.startsWith('@/styles/')?spec.slice(2):'src/'+spec.slice(2));else continue;
 if(![b,b+'.js',b+'.jsx',b+'.mjs',b+'.cjs',path.join(b,'index.js'),path.join(b,'index.jsx'),path.join(b,'index.mjs')].some(p=>fs.existsSync(p)&&fs.statSync(p).isFile()))unresolved.push(`${rel(file)}: ${spec}`);
}}
check('Frontend relative/alias imports resolve',!unresolved.length);if(unresolved.length)console.error(unresolved.join('\n'));
const runtime=files.filter(p=>/\.(rs|jsx|[cm]?js)$/.test(p)&&(rel(p).startsWith('backend/src/')||rel(p).startsWith('frontend/'))).map(p=>fs.readFileSync(p,'utf8')).join('\n');
check('No named canned scientific cases added',!/(three[-_ ]body|figure[-_ ]eight|electron[-_ ]collapse|known[-_ ]answer[-_ ]seed)/i.test(runtime));
const secretHits=files.filter(f=>/\.(rs|[cm]?js|jsx|json|toml|md|txt|sh|css|ya?ml)$/.test(f)).filter(f=>/sk-(?:ant-)?[a-zA-Z0-9_-]{24,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/.test(fs.readFileSync(f,'utf8')));
check('No embedded credentials detected',secretHits.length===0);
const state=files.filter(f=>/\.(?:sqlite3|db)(?:-wal|-shm)?$|(?:^|[/\\])\.env(?:$|\.(?!example$))|\.pyc$/.test(f));check('No runtime databases or local secret files',state.length===0);
const evidence=read('backend/src/science/evidence.rs'),findings=read('backend/src/science/findings.rs');
tokens('Metric-based challenge records, not a score-only pass',evidence,['absolute_tolerance','relative_tolerance','maximum_absolute_change','reached_end_time','No explicit comparison rule','values.iter().all']);
tokens('Actual constraint results returned by both solvers',read('backend/src/science/ode.rs')+read('backend/src/science/particles.rs'),['constraint_results: evidence::constraints','evidence_version: 2','baseline_metrics']);
tokens('Structured findings and immutable continuation',api+agent+workbench,['/api/runs/:id/findings','/api/runs/:id/explain','source_run_id','auto_run:false','runManifestSnapshot']);
tokens('Paid explanations audited, cached, and serialized',agent,['evidence_explanation','get_run_analysis','evidence_hash','analysis_gate.lock()','max_output_tokens.min(3000)']);
tokens('GenAI does not decide numerical status or novelty',findings,['checks_failed','checks_passed','inconclusive','"novelty":"not_assessed"']);
tokens('Default-off auto explanation limited to user-started runs',workbench,['useState(false)','ownedRuns.current.has','ownedRuns.current.delete','phaseforge.autoExplainEvidence']);
tokens('Live telemetry has independent GPU worker, nullable counters',read('backend/src/compute/telemetry.rs'),['gpu_state','gpu_target','cpu_percent','ram_available_bytes','memory_used_bytes','memory_total_bytes','nvidia-smi','Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine','mem_info_vram_used']);
tokens('Non-overlapping read-only polling',read('frontend/src/lib/liveRuntime.jsx'),['api.telemetry','api.workflow','AbortController','document.hidden','setTimeout(poll']);
const viewer=read('frontend/src/components/workspace/GenericViewport.jsx'),sceneViewer=read('frontend/src/lib/scene-viewer.mjs');
tokens('Viewer interpolation is display-only and time-based',viewer+sceneViewer,['createSceneViewer','OrbitControls','sampleFrames','interpolatePosition','requestAnimationFrame','ResizeObserver','webglcontextlost','poseStamp','releaseRenderer(renderer)','Playback interpolation affects the display only.']);
tokens('Viewer lifetime follows source identity and explicitly releases graphics contexts',viewer+sceneViewer+read('frontend/src/lib/viewer-resources.mjs'),['sceneSourceFromEvidence','normalizeScene(sceneSource),[sceneSource]','if(disposed)return','cancelAnimationFrame(raf)','observer?.disconnect()','renderer.dispose()','forceContextLoss','object.dispose()']);
tokens('Multi-entity explicit mappings supported',read('backend/src/science/ode.rs')+read('backend/src/domain/mod.rs'),['compile_visual_entities','VisualMapping','velocities','visual_entity_mapping']);
check('New provider schema includes entity mappings and metric checks',!!schema.$defs.manifest.properties.visualization.properties.entities&&!!schema.$defs.manifest.properties.falsification.items.properties.checks);
tokens('Findings panel has user approval and export workflow',read('frontend/src/components/workspace/FindingsView.jsx'),['labApprovalDialog','source','onContinue','onReplay','Export research bundle','AI prose is advisory']);
const studyService=read('backend/src/discovery/service.rs'),studyMath=read('backend/src/discovery/math.rs'),publication=read('backend/src/discovery/publication.rs');
const discoveryView=read('frontend/src/components/discovery/DiscoveryView.jsx'),notebook=read('backend/src/discovery/notebook.rs');
tokens('Discovery routes integrated with existing application',api+read('backend/src/app.rs'),['discovery::api::routes','DiscoveryService::new']);
tokens('Frozen protocol and explicit campaign admission',studyService,['recipe_hash','Awaiting compute approval','protocol_frozen','User approved bounded numerical execution']);
tokens('Budget and pause do not reset prior compute',studyService,['elapsed_before','study.recipe.wall_seconds','budget_exhausted','token.cancel()','Backend restarted. No compute or paid calls resume automatically']);
tokens('Every candidate has independent retained identity',studyService,['trial.manifest_id=Some','trial.run_id=Some','current.trials.push(trial)','Candidate rejected before execution']);
tokens('Diverse archive and descriptive Pareto ranking',studyMath,['pub fn archive','pub fn pareto','Strategy::LatinHypercube','cells.insert','15']);
tokens('Challenges distinguish refinement from replication',studyMath+studyService,['half_step','quarter_step','absolute_tolerance','inconclusive','NOT independent replication']);
tokens('Actual native differential evolution selection wired',read('backend/src/science/search.rs')+read('backend/src/science/ode.rs')+read('backend/src/science/particles.rs'),['DifferentialEvolution','self.targets','engine.select','population_size < 4']);
check('New algorithm choices in model contract',schema.$defs.manifest.properties.search.properties.algorithm.enum.includes('differential_evolution')&&schema.$defs.manifest.properties.search.properties.algorithm.enum.includes('latin_hypercube'));
tokens('AI designs, reviews and proposes without hidden reruns',discoveryView+studyService,['AI study design','request_id:requestId','auto_run:false','review_count>=3','Author-recorded','Usage & cost']);
tokens('Transactional immutable notebooks and atomic manifest reservations',db,['manifest_revision_counter','let tx = connection.transaction()','notebook_revision','notebook changed in another window','tx.commit()']);
tokens('Scoped evidence-linked human claims',notebook,['claim evidence belongs to another research world','reviewed_by','literature_comparison','author_review_required']);
tokens('Explicit public metadata query only',notebook+read('backend/src/discovery/api.rs'),['https://api.crossref.org/works','allow_public_query','redirect(reqwest::redirect::Policy::none())','Bibliographic metadata only']);
tokens('Offline research export includes raw evidence and review disclosures',publication,['tables/all-trials.csv','manuscript.md','references.bib','ro-crate-metadata.json','ai-usage-disclosure.json','checksums.json']);
tokens('Export excludes hidden uploads and detects incomplete evidence',publication,['incomplete exports are not silently accepted','Nothing has been uploaded','MAX_BUNDLE','a study run is missing']);
tokens('Independent bounded ODE implementation included',read('tools/reference_ode.py'),['ast.parse','Forbidden expression node','max_steps','reached_end_time','provenance mismatch']);
tokens('Signal inspection has explicit sampling caveats',read('backend/src/discovery/signals.rs'),['uniform_timestamps','sample_stride','numerical_range_exceeded','not full-resolution state']);
tokens('Campaign UI exposes measured candidates and stopping',discoveryView+read('frontend/src/components/discovery/StudyResults.jsx'),['Pause campaign','End campaign','Propose next experiment','Every attempted trial','No discovery certification']);
check('Research regression suites retained in complete package',['tests/test_reference_tools.py','tests/discovery-model.mjs','tests/discovery_runtime_smoke.py','backend/src/discovery/tests.rs','docs/DISCOVERY_CAMPAIGNS.md','docs/RESEARCH_EXPORT.md'].every(x=>fs.existsSync(path.join(root,x))));
// v0.5 checks are source contracts only. Executed worker and native integration tests are separate.
const assurance=read('backend/src/assurance/service.rs'),verifyApi=read('backend/src/assurance/api.rs'),comparison=read('backend/src/assurance/comparison.rs'),worker=read('tools/verification_worker.py');
check('Verification module, UI and native tests included', ['backend/src/assurance/mod.rs','backend/src/assurance/tests.rs','frontend/src/components/assurance/VerificationLab.jsx','frontend/src/lib/verification.js','tests/test_verification_worker.py','tests/verification_runtime_smoke.py','docs/VERIFICATION_DOSSIERS.md'].every(x=>fs.existsSync(path.join(root,x))));
tokens('Novelty search uses actual feasible descriptor distances',studyMath+studyService,['novelty_scores','novelty_values','diverse_finalists','Strategy::NoveltySearch']);
tokens('Independent attempt freezes exact candidate provenance',assurance,['protocol_hash','source_run_hash','recipe_hash','worker_source_hash','reference_source_hash','cmp::signature','Protocol frozen']);
tokens('New controls and holdouts do not self-validate the winner',assurance+worker,['spec.run_id==run.id','holdout_seed==study.recipe.seed','No known calibration/control interval','controls_passed','New perturbations']);
tokens('Reference matching is namespace-scoped and incomplete by design',comparison,['comparison_space','source_run_id==Some(d.source_run.id)','no_comparable_reference','near_match','not novelty to science']);
tokens('Independent solver is executable, not a generated proposal',worker,['adaptive_replay','Dormand','ref.replay','Budget','max_rhs_evaluations','endpoint_ok','holdout_replicates']);
tokens('Worker lifecycle bounded and stoppable',assurance,['one independent worker at a time','kill_on_drop(true)','child.kill().await','wall_seconds+3']);
tokens('Interrupted independent work never silently resumes',assurance,['interrupted','no work automatically resumed','stopping']);
tokens('Cancelled work retains bounded verified checkpoints',assurance,['checkpoint.json','checkpoint_notice','input_sha256','16*1024*1024']);
tokens('Verification status never certifies scientific novelty',assurance,['ready_for_external_review','not_certified','evidence_gaps_remain','current human review']);
tokens('Recorded source search and review evidence hashes are explicit',verifyApi+assurance,['allow_public_query','put_literature_search','list_literature_searches','evidence_hash','examined_sources','full_text_reviewed']);
tokens('New agent review is read-only and usage metered',agent+verifyApi,['review_verification','verification_review','audited_call','false, max_tokens','advisory','packet["evidence_hash"]']);
tokens('Recorded evidence windows are bounded, not new measurements',verifyApi,['/api/verification/runs/:id/window','max_points','2048','Recorded samples only']);
tokens('Publication includes verifier inputs, attempts and assessments',publication,['verification/','verification_worker.py','assurance::service::assessment','source_run','protocol_hash']);
check('Standalone optional verifier installers have no wrappers or packages',!read('INSTALL_VERIFIER.txt').match(/\.ps1\b/i)&&read('INSTALL_VERIFIER.txt').includes('--without-pip')&&read('INSTALL_VERIFIER.txt').includes('test_verification_worker.py')&&read('scripts/install-verifier.sh').includes('--without-pip'));
tokens('Native CI exercises actual verification API when binary exists',read('.github/workflows/ci.yml'),['verification_runtime_smoke.py','test_verification_worker.py','verification-model.mjs']);
// v0.6 additive source contracts. Numerical/native execution is independently reported.
const research=read('backend/src/research/mod.rs'),researchApi=read('backend/src/research/api.rs'),trajectory=read('backend/src/science/trajectory.rs');
tokens('Research API and persistent records are integrated',api+db,['research::api::routes','put_research_plan','research_searches','put_research_task','put_research_data']);
check('New closed proposal schema supports research programmes',schema.properties.research_plan.anyOf&&schema.properties.action.enum.includes('research_plan')&&schema.$defs.research_plan.properties.tasks);
tokens('Explicit study intent is enforced during bounded repair',agent,['study_intent','intent=="plan"','intent=="discovery"','research::validate']);
tokens('Citable sources are scoped and gaps are not dead ends',research+providers,['source ID not retrieved','depends_on','ResearchPlan','research_plan']);
tokens('Public source queries are bounded and consented',research+researchApi,['SEARCH_GATE','q.consent','redirect(reqwest::redirect::Policy::none())','response_sha256','4*1024*1024']);
tokens('Local CSV retains descriptive profiles not raw records',read('backend/src/research/data.rs'),['sha256','finite_numeric','nonfinite','sample_std_dev','Raw rows are NOT retained']);
tokens('Trajectory observations are native streaming reductions',trajectory+read('backend/src/science/ode.rs'),['Tracker::new','tracker.observe','tracker.extend','Kind::FirstBelow','_observed']);
tokens('Resolution ladder is distinct without rewriting StepHalving',read('backend/src/science/ode.rs')+evidence,['FalsificationKind::ResolutionLadder','FalsificationKind::StepHalving','pub fn refinement','level+1']);
const scheduler=read('backend/src/compute/scheduler.rs');
tokens('Compute is admitted again and actual allocation retained',scheduler,['advice.admissible','Available memory changed while queued','execute_checkpointed','compute_advice']);
check('Admitted allocation is written to the run record',/record\.compute\s*=\s*allocation\.clone\(\)/.test(scheduler));
tokens('Legacy frozen sources are selected by exact hash',assurance,['LEGACY_WORKER','LEGACY_REFERENCE','frozen_sources','worker_source_hash','reference_source_hash']);
tokens('Recorded radius glyphs do not claim contact mechanics',viewer+sceneViewer,['radiusFor','entity.radius','SphereGeometry(1','matrix.makeScale(r,r,r)','not contact or material physics']);
tokens('Optional notes preserve sources/data without stage gates',read('frontend/src/components/workspace/ResearchPlan.jsx'),['Notes, sources & research history','Build an experiment now','consent','Profile local CSV','not block']);
// v0.7 direct workflow contracts; not substitutes for native/UI execution tests.
const direct=read('backend/src/experiment/mod.rs'),directApi=read('backend/src/experiment/api.rs'),workspace=read('frontend/src/components/workspace/ResearchWorkbench.jsx');
check('No standalone Discovery navigation entry',!read('frontend/src/components/shared/AppShell.jsx').includes('href:"/discovery/"')&&!read('frontend/src/components/shared/CommandPalette.jsx').includes('href: "/discovery/"'));
tokens('Old Discovery links redirect to optional laboratory tools',read('frontend/pages/discovery/index.jsx')+read('frontend/src/lib/experiment.js'),['legacyDestination','router.replace',"panel:'advanced'","pathname:'/'"]);
tokens('Explicit direct and review routing prevent another plan',direct+agent,['validate_action','intent=="experiment"','intent=="review"','has_plan','options.validate_draft']);
tokens('Direct builds keep typed user caps and assumptions',direct+agent,['assumptions_allowed','max_candidates','population.saturating_mul','auto_run_requested','explicit_user_action']);
tokens('User consent replaces only the model run veto',agent,['auto_run_requested && (direct || proposal.should_run)','execution_error','Some(project_id)','This project already has an active model request']);
tokens('Repeat/refine/longer are scoped and idempotent',directApi+direct,['request_hash','reserved','put_experiment_receipt','Source run belongs to another project','FinerSteps','LongerHorizon','saturating_mul(2)']);
tokens('Manual mathematics follows the real import/validation route',read('frontend/src/components/workspace/EquationEditor.jsx')+read('frontend/src/lib/experiment.js'),['localDraft','auto_run:false','state_vector_ode','derivatives','max_steps','No acceptance tests']);
tokens('Main UI exposes direct controls with same-flight protection',workspace+read('frontend/src/components/workspace/ExperimentWorkspace.jsx'),['messageFlight.current','checkedResponse','Run saved setup','Build & run',"onNext('finer_steps')",'onStop']);
check('The old numbered progress ribbon is absent',!read('frontend/src/components/workspace/WorkflowBar.jsx').includes('<ol>'));
tokens('Direct Results actions do not require provider keys',read('frontend/src/components/workspace/FindingsView.jsx'),['["run","direct"]','Run this changed copy','research-plan completion'.replace('research-plan','Research-plan')]);
tokens('New integration and local-provider tests are supplied',read('backend/src/agent/mod.rs')+read('tests/experiment_runtime_smoke.py'),['repeated_plan_is_repaired_into_executable_setup','safety_refusal_is_not_repaired_or_bypassed','explicit_build_and_run_uses_user_authority_not_model_veto','finer_steps','zero paid tokens']);
tokens('New regressions run in CI',read('.github/workflows/ci.yml'),['experiment_runtime_smoke.py','experiment-model.mjs','experiment-ui.cjs','test_experiment_editor.py']);
console.log(`\nSource audit: ${passed} passed, ${failed} failed. This is not a native build.`);if(failed)process.exit(1);
