import {createContext, useContext, useEffect, useState} from 'react';

const SelectionContext = createContext(null);
export function ModelSelectionProvider({children}) {
  const [selection, setSelection] = useState({provider:'open_ai', model:'', reasoning_effort:null, research_mode:false});
  const [ready, setReady] = useState(false);
  useEffect(() => {
    try { const saved=JSON.parse(localStorage.getItem('phaseforge.modelSelection') || 'null'); if(saved && ['open_ai','anthropic'].includes(saved.provider))setSelection(saved); } catch {}
    setReady(true);
  }, []);
  useEffect(() => { if(ready)try {localStorage.setItem('phaseforge.modelSelection', JSON.stringify(selection));} catch {} },[selection,ready]);
  return <SelectionContext.Provider value={{selection,setSelection}}>{children}</SelectionContext.Provider>;
}
export const useModelSelection = () => useContext(SelectionContext);
