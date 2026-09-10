/* Static render fixture of the ACTUAL new JSX, with minimal React-hook doubles.
 * Not a React/Next production or hydration test. For layout review without npm networking.
 * Requires a TypeScript compiler installed by the developer; not required by the application.
 */
const fs=require('fs'),path=require('path');let ts;
try {ts=require('typescript');} catch {ts=require(process.env.TYPESCRIPT_PATH||path.resolve(path.dirname(process.execPath),'../lib/node_modules/typescript/lib/typescript.js'));}
const root=path.resolve(__dirname,'..'),out=process.argv[2]||path.join(root,'build/layout-fixtures');fs.mkdirSync(out,{recursive:true});
let owner='',index=0,tab='campaign';const now='2026-09-06T18:00:00Z';
const manifest=JSON.parse(fs.readFileSync(path.join(root,'backend/tests/fixtures/discovery-control.json')));manifest.id='m';manifest.project_id='p';manifest.revision=1;manifest.authored_by='Builder';
const recipe={title:'Behavioral diversity investigation',hypothesis:'Assess distinct observed regimes without claiming a new physical law.',base_manifest_id:'m',strategy:'map_elites',parameters:[{name:'p',target:'initial:x',minimum:.5,maximum:1.5}],objectives:[{metric:'state',goal:'minimize'},{metric:'behavior',goal:'maximize'}],descriptors:[{metric:'state',minimum:0,maximum:1,bins:8},{metric:'behavior',minimum:0,maximum:1,bins:6}],exploration_trials:16,validation_finalists:1,wall_seconds:600,per_trial_seconds:60,seed:42,absolute_tolerance:1e-6,relative_tolerance:.001,auto_review:false};
const trials=Array.from({length:8},(_,i)=>({id:'t'+i,index:i+1,phase:'explore',parent_trial_id:null,run_id:'r'+i,manifest_id:'m'+i,values:[.5+i/8],state:'completed',metrics:{state:.1+i*.1,behavior:.8-i*.1},eligible:i!==5,reasons:i===5?['A baseline constraint failed']:[],cell:`${i}:2`,created_at:now}));
const study={id:'s',project_id:'p',recipe,recipe_hash:'0123456789abcdef'.repeat(4),base_manifest:manifest,state:'paused',stage:'Resolution challenges',trials,finalist_ids:['t0'],finalists_frozen:true,elapsed_seconds:58,review_count:0,events:[{at:now,kind:'protocol_frozen',message:'Protocol frozen before compute.'},{at:now,kind:'pause',message:'User paused the campaign; evidence retained.'}],created_at:now,updated_at:now};
const summary={eligible:7,rejected:1,completed:8,planned:18,archive:trials.filter(t=>t.eligible).map(t=>({cell:t.cell,trial_id:t.id})),coverage:.15,pareto_trial_ids:['t0','t2'],validation:[{trial_id:'t0',status:'inconclusive',checks:[{metric:'state',baseline:.1,half_step:.100000001,quarter_step:null,half_quarter_difference:null,threshold:.000101}]}]};
const detail={study,summary,readiness:{gaps:['Independent verification has not been documented.','Literature comparison and author review remain required.','Finalist refinement is inconclusive.']}};
const project={id:'p',name:'Dynamical model investigation',question:'How does a bounded authored model respond to initial conditions?'};
const notebook={project_id:'p',revision:1,title:'Working paper on distinct modeled behaviors',authors:[{name:'Researcher',orcid:'',contributions:'Methods and validation'}],hypothesis:recipe.hypothesis,protocol:'Bounds and inclusion rules were declared before search.',notes:'',claims:[{id:'c',statement:'The selected numerical observations occupy multiple descriptor regions.',kind:'observation',supporting_runs:['r0'],contradicting_runs:[],limitations:'Sampling and model assumptions require review.',literature_comparison:'',reviewed_by:''}],references:[],data_availability:'',code_availability:'',ai_disclosure:'Describe actual assistance before submission.',data_license:'Author decision required',independent_validation_notes:'',updated_at:now};
const overrides=()=>({DiscoveryView:[[project],'p',[manifest],'m',[{id:'s',title:recipe.title,state:'paused',summary}], 's', detail,false,tab,'','','',null,0,false],NotebookEditor:[notebook,false,false,'','',[],false,''],StudyResults:['t0',0,null,'']});
const h=(type,props,...children)=>({type,props:{...props,children:children.length?children:props?.children}});
const hooks={useState(initial){const n=index++;return [overrides()[owner]?.[n]??(typeof initial==='function'?initial():initial),()=>{}];},useEffect(){},useMemo:f=>f(),useCallback:f=>f,useRef:v=>({current:v})};
const cache={};function load(file){file=path.resolve(file);if(cache[file])return cache[file].exports;const module={exports:{}};cache[file]=module;
 const source=ts.transpileModule(fs.readFileSync(file,'utf8'),{fileName:file,compilerOptions:{jsx:ts.JsxEmit.ReactJSX,module:ts.ModuleKind.CommonJS,target:ts.ScriptTarget.ES2022,esModuleInterop:true}}).outputText;
 function req(id){if(id==='react')return {...hooks,Fragment:'fragment'};if(id==='react/jsx-runtime')return {jsx:(t,p)=>h(t,p),jsxs:(t,p)=>h(t,p),Fragment:'fragment'};
 if(id==='next/router')return {useRouter:()=>({isReady:true,query:{}})};if(id==='next/link')return {__esModule:true,default:p=>h('a',{...p,href:typeof p.href==='string'?p.href:'#'})};
 if(id==='lucide-react')return new Proxy({}, {get:(_,name)=>p=>h('svg',{...p,width:p?.size||16,height:p?.size||16,'aria-hidden':'true',viewBox:'0 0 24 24'},h('circle',{cx:12,cy:12,r:8,stroke:'currentColor',fill:'none'}))});
 if(id==='@/lib/liveRuntime')return {useLiveRuntime:()=>({workflow:{runs:[]}})};
 if(id==='@/components/workspace/ResourceMonitor')return {__esModule:true,default:()=>h('div',{className:'discNotice'},'Live resources · fixture presentation only; no hardware counters queried.')};
 if(id==='@/components/workspace/ChatMarkdown')return {__esModule:true,default:p=>h('p',{},p.children)};
 if(id==='@/lib/api')return {api:{}};
 let target=id.startsWith('@/')?path.join(root,'frontend/src',id.slice(2)):path.resolve(path.dirname(file),id);for(const ext of ['','.js','.jsx'])if(fs.existsSync(target+ext))return load(target+ext);throw Error(id);}
 new Function('require','module','exports',source)(req,module,module.exports);return module.exports;}
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
function render(v){if(v==null||typeof v==='boolean')return '';if(Array.isArray(v))return v.map(render).join('');if(typeof v!=='object')return esc(v);
 if(typeof v.type==='function'){const prev=[owner,index];owner=v.type.name;index=0;const result=v.type(v.props);[owner,index]=prev;return render(result);}
 if(v.type==='fragment')return render(v.props.children);
 const props=v.props||{};const attrs=Object.entries(props).filter(([k,val])=>!['children','key','ref','dangerouslySetInnerHTML'].includes(k)&&!k.startsWith('on')&&val!=null&&val!==false).map(([k,val])=>{
  if(k==='className')k='class';if(k==='htmlFor')k='for';if(k==='style')val=Object.entries(val).map(([a,b])=>`${a.replace(/[A-Z]/g,c=>'-'+c.toLowerCase())}:${typeof b==='number'?b+'px':b}`).join(';');
  return `${k}="${val===true?'':esc(val)}"`;
 }).join(' ');const voids=['input','br','hr','img'];let content=v.type==='textarea'?props.value:props.children;
 return `<${v.type} ${attrs}>${voids.includes(v.type)?'':render(content)+`</${v.type}>`}`;}
const View=load(path.join(root,'frontend/src/components/discovery/DiscoveryView.jsx')).default;
const css=['globals.css','workspace.css','discovery.css'].map(f=>fs.readFileSync(path.join(root,'frontend/styles',f),'utf8')).join('\n');
for(tab of ['campaign','record','publish'])for(const theme of ['dark','light']){
 const html=`<!doctype html><html data-theme="${theme}"><head><meta name="viewport" content="width=device-width,initial-scale=1"><style>${css}\nbody{overflow:auto!important;padding:22px;margin:0}.discoveryShell{max-width:1500px}body>[role=note]{font:12px system-ui;color:var(--muted);margin-bottom:12px}</style></head><body><div role="note">Static JSX layout fixture — synthetic values, not scientific evidence or a live application.</div>${render(h(View,{backend:{connected:true}}))}</body></html>`;
 fs.writeFileSync(path.join(out,`${tab}-${theme}.html`),html);
}
console.log('Rendered 6 real-JSX static layouts with hook doubles to',out);
