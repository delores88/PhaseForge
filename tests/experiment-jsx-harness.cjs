/* Actual component functions with deterministic hooks. No React DOM, Next
 * hydration, network service or WebGL is emulated or claimed by this harness. */
const fs=require('fs'),path=require('path');
const root=path.resolve(__dirname,'..');
function typescript(){
 if(process.env.TYPESCRIPT_PATH)return require(process.env.TYPESCRIPT_PATH);
 // Match normal dependency resolution first, then the optional local test setup
 // and standard Windows/Unix global installs. No network or installation here.
 for(const base of[path.join(root,'frontend'),root,path.join(root,'.local/validation')]){
  try{return require(require.resolve('typescript',{paths:[base]}));}catch(error){if(error.code!=='MODULE_NOT_FOUND')throw error;}
 }
 const globalRoots=[path.resolve(path.dirname(process.execPath),'../lib/node_modules'),process.env.APPDATA&&path.join(process.env.APPDATA,'npm/node_modules')].filter(Boolean);
 for(const base of globalRoots){const candidate=path.join(base,'typescript/lib/typescript.js');if(fs.existsSync(candidate))return require(candidate);}
 throw Error('JSX tests need TypeScript 5.8.3. Install it as documented in CI, locally under .local/validation, or set TYPESCRIPT_PATH to its module.');
}
const ts=typescript();
function harness(){
 let owner='',index=0;const slots=new Map(),cache=new Map();
 const bucket=()=>{if(!slots.has(owner))slots.set(owner,[]);return slots.get(owner);};
 const React={Fragment:Symbol.for('react.fragment'),createElement:(type,props,...children)=>({type,props:{...(props||{}),...(children.length?{children}: {})}}),
  useState:initial=>{const rows=bucket(),i=index++;if(!(i in rows))rows[i]=typeof initial==='function'?initial():initial;return[rows[i],v=>{rows[i]=typeof v==='function'?v(rows[i]):v;}];},
  useRef:initial=>{const rows=bucket(),i=index++;if(!(i in rows))rows[i]={current:initial};return rows[i];},useEffect:()=>{},useMemo:fn=>fn(),useCallback:fn=>fn};
 const icons=new Proxy({}, {get:(_,name)=>props=>React.createElement('span',{'aria-hidden':true,className:'fixtureIcon','data-icon':String(name)},'◇')});
 function resolve(p){for(const file of[p,p+'.jsx',p+'.js',p+'.mjs',p+'.cjs',p+'.json'])if(fs.existsSync(file)&&fs.statSync(file).isFile())return file;throw Error('Unresolved source '+p);}
 function load(file){file=resolve(path.resolve(file));if(cache.has(file))return cache.get(file).exports;if(file.endsWith('.json'))return JSON.parse(fs.readFileSync(file));const module={exports:{}};cache.set(file,module);
  const out=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{module:ts.ModuleKind.CommonJS,jsx:ts.JsxEmit.React,target:ts.ScriptTarget.ES2022},reportDiagnostics:true});
  if(out.diagnostics?.length)throw Error(out.diagnostics.map(d=>ts.flattenDiagnosticMessageText(d.messageText,' ')).join('\n'));
  function req(id){if(id==='react')return React;if(id==='lucide-react')return icons;
   // Keep portal children in the virtual tree; browser tests verify placement.
   if(id==='react-dom')return{createPortal:children=>children};
   if(id==='next/link')return{__esModule:true,default:p=>React.createElement('a',{...p,href:typeof p.href==='string'?p.href:'/'},p.children)};
   if(id==='next/router')return{useRouter:()=>({pathname:'/',isReady:true,query:{},push:async()=>{},replace:async()=>{}})};
   if(id==='@/lib/api')return{api:{}};
   // These workflow tests exercise local button handlers, not provider catalog
   // requests or persistence of the global model-selection context.
   if(id==='@/lib/modelSelection')return{useModelSelection:()=>({selection:{provider:'open_ai',model:'',reasoning_effort:null},setSelection:()=>{}})};
   if(id==='@/lib/theme')return{ThemeToggle:()=>React.createElement('button',{'aria-label':'Theme fixture'},'◐')};
   if(id.includes('GenericViewport'))return{__esModule:true,default:p=>React.createElement('section',{className:'viewportPanel fixtureViewport','aria-label':'Viewport placeholder'},React.createElement('strong',null,p.run?'Recorded scene area':'Simulation will appear here'),React.createElement('small',null,'Viewport placeholder: this test does not execute WebGL.'))};
   if(id.includes('CommandPalette'))return{__esModule:true,default:()=>null};
   if(id.startsWith('@/'))return load(path.join(root,'frontend/src',id.slice(2)));
   if(id.startsWith('.'))return load(path.resolve(path.dirname(file),id));return require(id);
  }
  new Function('require','module','exports','React',out.outputText)(req,module,module.exports,React);return module.exports;
 }
 function expand(node){if(node==null||typeof node==='boolean')return null;if(Array.isArray(node))return node.map(expand);if(typeof node!=='object')return node;let {type,props}=node;
  if(typeof type==='function'){const old=[owner,index];owner=type.name||'Anonymous';index=0;const child=expand(type(props));[owner,index]=old;return child;}
  if(type===React.Fragment)return expand(props.children);return{type,props:{...props,children:expand(props.children)}};
 }
 function render(Component,props){return expand(React.createElement(Component,props));}
 function walk(node,predicate,out=[]){if(Array.isArray(node)){node.forEach(n=>walk(n,predicate,out));return out;}if(!node||typeof node!=='object')return out;if(predicate(node))out.push(node);walk(node.props?.children,predicate,out);return out;}
 const text=node=>node==null?'':Array.isArray(node)?node.map(text).join(''):typeof node==='object'?text(node.props.children):String(node);
 const escape=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
 function html(node){if(node==null||typeof node==='boolean')return'';if(Array.isArray(node))return node.map(html).join('');if(typeof node!=='object')return escape(node);const {type,props}=node;
  const attrs=Object.entries(props).filter(([k,v])=>!['children','key','ref','dangerouslySetInnerHTML'].includes(k)&&!k.startsWith('on')&&v!=null&&v!==false).map(([k,v])=>{if(k==='className')k='class';if(k==='htmlFor')k='for';if(k==='style')v=Object.entries(v).map(([a,b])=>a.replace(/[A-Z]/g,m=>'-'+m.toLowerCase())+':'+(typeof b==='number'&&!['opacity','flex','flexGrow'].includes(a)?b+'px':b)).join(';');return typeof v==='boolean'&&!k.startsWith('aria-')?` ${k}`:` ${k}="${escape(v)}"`;}).join('');
  return `<${type}${attrs}>`+(['input','br','hr','img','meta','link'].includes(type)?'':html(props.children)+`</${type}>`);
 }
 return{load,render,walk,text,html,React,seed:(name,i,v)=>{if(!slots.has(name))slots.set(name,[]);slots.get(name)[i]=v;}};
}
module.exports={harness,root};
