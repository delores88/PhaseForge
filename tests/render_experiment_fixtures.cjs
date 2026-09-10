// Real JSX static rendering. Viewport is an explicitly labeled placeholder.
const fs=require('fs'),path=require('path');const {harness,root}=require('./experiment-jsx-harness.cjs');
const out=process.argv[2]||path.join(root,'build/experiment-layouts');fs.mkdirSync(out,{recursive:true});
const project={id:'fixture-world',name:'Investigate a numerical hypothesis',question:'Explore the equations and compare outcomes under changed starting conditions.'};
const manifest={id:'fixture-manifest',title:'Explore a dynamical hypothesis',revision:3,compute:{max_wall_seconds:180,max_memory_mb:1024}};
const run={id:'fixture-run',name:'Earlier recorded experiment',manifest_id:'fixture-manifest',manifest_revision:3,status:'completed'};
for(const scenario of ['empty','request','ready','running','results','error','manual'])for(const theme of ['dark','light']){
 const h=harness();const Workspace=h.load(path.join(root,'frontend/src/components/workspace/ExperimentWorkspace.jsx')).default;
 const Editor=h.load(path.join(root,'frontend/src/components/workspace/EquationEditor.jsx')).default;
 const Shell=h.load(path.join(root,'frontend/src/components/shared/AppShell.jsx')).default;
 const noop=async()=>{};const props={project:scenario==='empty'?null:project,manifest:['ready','running','results'].includes(scenario)?manifest:null,run:scenario==='running'?{...run,status:'running'}:scenario==='results'?run:null,
   busy:scenario==='running',providerReady:scenario!=='ready',build:noop,onRun:noop,onNext:noop,onStop:noop,onSetup:noop,onEquations:noop,onNew:noop,onResults:noop};
 if(scenario==='ready'||scenario==='running'||scenario==='results')h.seed('ExperimentWorkspace',3,false);
 if(scenario==='error')h.seed('ExperimentWorkspace',2,'The proposal exceeded the selected budget. The existing experiment is unchanged. Review the limits and retry.');
 let content=scenario==='manual'?h.render(Editor,{project,busy:false,onImport:noop,onBack:noop}):h.render(Workspace,props);
 const e=h.React.createElement;
 content=e('div',{className:'fixtureHost'},e('div',{className:'fixtureBanner'},'UI layout fixture · real JSX · no live model, solver or WebGL'),e('nav',{className:'workspaceTabs','aria-label':'Fixture workspace tabs'},e('button',{className:'active'},'Experiment'),e('button',null,'Results'),e('button',null,'Setup'),e('button',null,'More')),content);
 const tree=h.render(Shell,{backend:{connected:true},hardware:{cpu:{logical_cores:8},adapters:[]},title:'Laboratory',subtitle:'Build, run, inspect, continue.',children:content});
 const css=['globals.css','workspace.css','research.css','lab.css','experiment.css'].map(f=>fs.readFileSync(path.join(root,'frontend/styles',f),'utf8')).join('\n');
 const doc=`<!doctype html><html data-theme="${theme}"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}\n.fixtureHost{width:100%;max-width:var(--fixture-width,100%);height:100%;min-width:0;margin:auto;display:flex;flex-direction:column}.fixtureBanner{font-size:10px;padding:6px 14px;line-height:1.4;color:var(--faint);flex-shrink:0}.fixtureIcon{width:14px;display:inline-block;flex-shrink:0}.fixtureViewport{display:flex;flex-direction:column;justify-content:center;align-items:center;gap:10px;background:var(--surface-2);color:var(--muted);border:0;border-radius:0}.fixtureViewport small{font-size:10px}.experimentWorkspace,.equationEditor{flex:1;min-height:0}.pageContent{padding:0!important}.workspaceTabs{flex:0 0 auto}.expNext{display:block}</style><body>${h.html(tree)}</body></html>`;
 fs.writeFileSync(path.join(out,`${scenario}-${theme}.html`),doc);
}
console.log('Actual experiment JSX fixture HTML:',out);
