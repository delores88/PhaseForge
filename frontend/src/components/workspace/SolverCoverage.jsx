import {useEffect,useState} from 'react';
import {api} from '@/lib/api';
import styles from './SolverCoverage.module.css';

const names={openmm_argon:'Molecular dynamics',diffusion_2d:'Diffusion',newtonian_nbody:'Newtonian mechanics',heat_conduction_2d:'Heat conduction',navier_stokes_2d:'Incompressible fluid flow'};
export default function SolverCoverage(){
  const [catalog,setCatalog]=useState(null),[error,setError]=useState('');
  useEffect(()=>{const controller=new AbortController();api.labCapabilities(controller.signal).then(setCatalog).catch(e=>{if(e.name!=='AbortError')setError(e.message);});return()=>controller.abort();},[]);
  return <section className={styles.coverage} aria-label="Executable solver coverage">
    <header><span>SCIENTIFIC ENGINES</span><h2>Choose a question the model can actually test</h2><p>These engines compute and retain numerical states. Chat can design a compatible experiment or explain the missing capability before proceeding.</p></header>
    {error&&<p role="alert">Could not load solver coverage: {error}</p>}
    {!catalog&&!error&&<p role="status">Loading installed capabilities…</p>}
    <div className={styles.grid}>{(catalog?.engines||[]).map(engine=><article key={engine.id}><h3>{names[engine.id]||engine.id}</h3><p>{engine.scope}</p>{engine.equations&&<code>{engine.equations}</code>}<p><strong>Measurements:</strong> {(engine.instruments||[]).join(', ')}</p>{engine.validation&&<p className={styles.limit}>{engine.validation}</p>}<details><summary>Parameters, units and limits</summary><pre>{JSON.stringify({parameters:engine.parameters,units:engine.units,limits:engine.limits},null,2)}</pre></details></article>)}</div>
    <h2>Where another model or engine is needed</h2>
    <div className={styles.gaps}>{(catalog?.gaps||[]).map(gap=><article key={gap.domain}><h3>{gap.domain}</h3><p>{gap.reason}</p>{gap.required&&<p><strong>Required:</strong> {gap.required}</p>}{gap.available_alternative&&<p>{gap.available_alternative}</p>}</article>)}</div>
    {catalog&&<p>{catalog.generated_experiments}</p>}
  </section>;
}
