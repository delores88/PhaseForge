import styles from './DeliverableStatus.module.css';
const names={analysis:'analysis',simulation:'simulation',illustration:'illustration',explanation:'explanation',presentation_edit:'presentation edit'};
export default function DeliverableStatus({value,onInspect}){
  if(!value||value.schema!=='phaseforge.deliverable.v1')return null;
  const fulfilled=value.status==='fulfilled',kind=names[value.output_intent]||'output';
  const title=fulfilled?`${kind[0].toUpperCase()+kind.slice(1)} saved`:value.status==='capability_gap'?'This request needs another scientific capability':value.status==='intent_unresolved'?'Requested output needs clarification':`Requested ${kind} was not completed`;
  return <aside className={`${styles.status} ${fulfilled?styles.fulfilled:styles.attention}`} aria-label="Requested output status">
    <strong>{title}</strong>
    {!fulfilled&&<p>{value.capability_gap||value.reason||value.scope||'The saved work does not yet meet the requested deliverable. Continue in chat to resolve the missing requirement.'}</p>}
    {fulfilled&&value.output_intent==='simulation'&&<p>Playback is backed by retained numerical states. Model assumptions and scientific validation remain separate.</p>}
    {!fulfilled&&onInspect&&<button type="button" onClick={onInspect}>Inspect work and evidence</button>}
  </aside>;
}
