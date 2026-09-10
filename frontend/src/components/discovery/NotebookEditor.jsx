import {useEffect,useState} from "react";
import {BookOpen,Plus,Save,Search,Trash2} from "lucide-react";
import {api} from "@/lib/api";
import {fmt} from "@/lib/discovery";

export default function NotebookEditor({projectId,study,onSaved,onDirty}) {
  const [book,setBook]=useState(null),[dirty,setDirty]=useState(false),[busy,setBusy]=useState(false),[error,setError]=useState("");
  const [query,setQuery]=useState(""),[hits,setHits]=useState([]),[searching,setSearching]=useState(false),[message,setMessage]=useState("");
  const load=()=>{setBusy(true);api.notebook(projectId).then(r=>{setBook(r.notebook);setDirty(false);setError("");}).catch(e=>setError(e.message)).finally(()=>setBusy(false));};
  useEffect(()=>{load();},[projectId]);
  useEffect(()=>{const warn=e=>{if(dirty){e.preventDefault();e.returnValue="";}};window.addEventListener("beforeunload",warn);return()=>window.removeEventListener("beforeunload",warn);},[dirty]);
  useEffect(()=>{onDirty?.(dirty);},[dirty,onDirty]);
  useEffect(()=>{
    const guard=e=>{
      if(!dirty||e.ctrlKey||e.metaKey||e.shiftKey||e.altKey)return;
      const link=e.target.closest?.("a[href]");if(!link||link.target==="_blank")return;
      const target=new URL(link.href,window.location.href);
      if(target.href!==window.location.href&&!window.confirm("Leave without saving the research record?")){e.preventDefault();e.stopPropagation();}
    };
    document.addEventListener("click",guard,true);return()=>document.removeEventListener("click",guard,true);
  },[dirty]);
  const patch=(key,value)=>{setBook(b=>({...b,[key]:value}));setDirty(true);};
  const row=(key,index,field,value)=>patch(key,book[key].map((r,i)=>i===index?{...r,[field]:value}:r));
  const save=async()=>{setBusy(true);setError("");try{const r=await api.saveNotebook(projectId,book);setBook(r.notebook);setDirty(false);setMessage(`Saved revision ${r.notebook.revision}. Older revisions are retained.`);onSaved?.();}catch(e){setError(e.message);}finally{setBusy(false);}};
  const search=async()=>{setSearching(true);setError("");try{const r=await api.literature(query);setHits(r.references||[]);}catch(e){setError(e.message);}finally{setSearching(false);}};
  const addReference=ref=>{if(book.references.some(r=>r.doi.toLowerCase()===ref.doi.toLowerCase()))return;patch("references",[...book.references,ref]);};
  const runOptions=(study?.trials||[]).filter(t=>t.state==="completed"&&t.run_id);
  if(!book)return <section className="discoveryCard"><p>{error||"Loading the research record…"}</p>{error&&<button className="button button--secondary" onClick={load}>Retry</button>}</section>;
  return <section className="discoveryNotebook discoveryCard">
    <header><div><span className="eyebrow">AUTHOR-OWNED RESEARCH RECORD · REVISION {book.revision}</span><h2>Make every claim traceable</h2><p>Observations, interpretation, counter-evidence and unresolved questions remain distinct. AI is an assistant, not an author of record.</p></div><button className="button button--primary" disabled={busy||!dirty} onClick={save}><Save size={15}/>{busy?"Saving…":"Save record"}</button></header>
    {dirty&&<p className="discNotice">Unsaved changes. Save before exporting or switching research worlds.</p>}
    {error&&<p className="discError" role="alert">{error}<button className="button button--secondary" onClick={()=>{if(!dirty||window.confirm("Discard local edits and load the latest saved revision?"))load();}}>Reload saved version</button></p>}
    {message&&!dirty&&<p role="status" className="discSuccess">{message}</p>}
    <label>Working paper title<input value={book.title} onChange={e=>patch("title",e.target.value)}/></label>
    <h3>People and responsibilities</h3>
    {book.authors.map((a,i)=><div key={i} className="discParameter"><label>Name<input value={a.name} onChange={e=>row("authors",i,"name",e.target.value)}/></label><label>ORCID (optional)<input value={a.orcid} onChange={e=>row("authors",i,"orcid",e.target.value)}/></label><label>Contributions<input value={a.contributions} onChange={e=>row("authors",i,"contributions",e.target.value)}/></label><button className="discIcon" aria-label="Remove author" onClick={()=>patch("authors",book.authors.filter((_,j)=>j!==i))}><Trash2 size={15}/></button></div>)}
    <button className="button button--secondary" onClick={()=>patch("authors",[...book.authors,{name:"",orcid:"",contributions:""}])}><Plus size={14}/>Add human author</button>
    <div className="discFields"><label>Hypothesis and alternatives<textarea rows={4} value={book.hypothesis} onChange={e=>patch("hypothesis",e.target.value)}/></label><label>Protocol, controls and inclusion rules<textarea rows={4} value={book.protocol} onChange={e=>patch("protocol",e.target.value)}/></label></div>
    <h3>Claim-to-evidence ledger</h3><p className="discCaption">Link completed runs from this world. No amount of successful simulation automatically establishes novelty, clinical efficacy, or the truth of the physical model.</p>
    {book.claims.map((c,i)=><article className="discClaim" key={c.id}>
      <header><strong>Claim {i+1}</strong><select aria-label="Claim class" value={c.kind} onChange={e=>row("claims",i,"kind",e.target.value)}><option value="observation">Numerical observation</option><option value="interpretation">Interpretation</option><option value="hypothesis">Hypothesis to test</option></select><button className="discIcon" aria-label="Remove claim" onClick={()=>patch("claims",book.claims.filter((_,j)=>i!==j))}><Trash2 size={16}/></button></header>
      <label>Precisely scoped statement<textarea rows={2} value={c.statement} onChange={e=>row("claims",i,"statement",e.target.value)}/></label>
      <div className="discFields">{[["supporting_runs","Supporting evidence"],["contradicting_runs","Counter-evidence"]].map(([key,label])=><label key={key}>{label}<select multiple size={4} value={c[key]} onChange={e=>row("claims",i,key,[...e.target.selectedOptions].map(o=>o.value))}>{c[key].filter(id=>!runOptions.some(t=>t.run_id===id)).map(id=><option key={id} value={id}>Previously linked · {id}</option>)}{runOptions.map(t=><option key={t.id} value={t.run_id}>Trial {t.index} · {t.phase} · {t.run_id.slice(0,8)} {t.eligible?"":"· infeasible"}</option>)}</select><small>Ctrl/Cmd-select multiple runs.</small></label>)}</div>
      <label>Limitations and confounders<textarea rows={2} value={c.limitations} onChange={e=>row("claims",i,"limitations",e.target.value)}/></label>
      <div className="discFields"><label>Comparison with prior work<textarea rows={2} value={c.literature_comparison} onChange={e=>row("claims",i,"literature_comparison",e.target.value)}/></label><label>Human reviewer<input value={c.reviewed_by} onChange={e=>row("claims",i,"reviewed_by",e.target.value)}/></label></div>
    </article>)}
    <button className="button button--secondary" onClick={()=>patch("claims",[...book.claims,{id:crypto.randomUUID(),statement:"",kind:"observation",supporting_runs:[],contradicting_runs:[],limitations:"",literature_comparison:"",reviewed_by:""}])}><Plus size={14}/>Add evidence-linked claim</button>
    <h3>Literature and novelty triage</h3><div className="discNotice"><BookOpen size={18}/><span>Search sends only the text below to the public Crossref service, without provider keys. Results are bibliographic metadata, not a full-paper review. A missing search match is not evidence of novelty.</span></div>
    <div className="discSearch"><input aria-label="Public literature query" value={query} onChange={e=>setQuery(e.target.value)} placeholder="Topic, method, title or DOI (public query)"/><button className="button button--secondary" onClick={search} disabled={searching||query.trim().length<3}><Search size={15}/>{searching?"Searching metadata…":"Search Crossref"}</button></div>
    {!!hits.length&&<div className="discReferences">{hits.map((r,i)=><article key={`${r.doi}-${i}`}><a href={r.url} target="_blank" rel="noreferrer">{r.title}</a><small>{r.year||"Year unavailable"} · {r.authors.slice(0,3).join(", ")} · {r.doi}</small><button className="button button--secondary" disabled={book.references.some(x=>x.doi===r.doi)} onClick={()=>addReference(r)}>Add reference</button></article>)}</div>}
    {book.references.map((r,i)=><article className="discSavedRef" key={`${r.doi}-${i}`}><header><a href={r.url} target="_blank" rel="noreferrer">[{i+1}] {r.title}</a><button className="discIcon" aria-label="Remove reference" onClick={()=>patch("references",book.references.filter((_,j)=>i!==j))}><Trash2 size={14}/></button></header><small>{r.doi} · {r.source} · retrieved {new Date(r.retrieved_at).toLocaleDateString()}</small><label>What you actually read; relevance, differences and caveats<textarea rows={2} value={r.reading_notes} onChange={e=>row("references",i,"reading_notes",e.target.value)}/></label></article>)}
    <h3>Reproducibility and publication disclosures</h3>
    <div className="discFields">{[["independent_validation_notes","Independent implementation checks and results"],["data_availability","Data availability"],["code_availability","Code availability and exact code version"],["ai_disclosure","Actual AI assistance and author responsibility"]].map(([key,label])=><label key={key}>{label}<textarea rows={3} value={book[key]} onChange={e=>patch(key,e.target.value)}/></label>)}</div>
    <label>Data license / permissions (not inferred from the software license)<input value={book.data_license} onChange={e=>patch("data_license",e.target.value)}/></label>
    <label>Research notes / discussion draft<textarea rows={5} value={book.notes} onChange={e=>patch("notes",e.target.value)}/></label>
    <footer><span>Save creates a retained revision; concurrent stale writes are rejected.</span><button className="button button--primary" disabled={busy||!dirty} onClick={save}><Save size={15}/>Save research record</button></footer>
  </section>;
}
