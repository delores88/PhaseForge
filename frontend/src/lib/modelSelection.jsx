import {createContext,useCallback,useContext,useEffect,useMemo,useState} from 'react';
import {EMPTY_SELECTION,MODEL_SELECTION_KEY,normalizeSelection,readSelections,selectionProblem} from './modelChoices.mjs';

const SelectionContext=createContext(null);
export function ModelSelectionProvider({children}) {
  const [state,setState]=useState({version:2,projects:{},legacy:null});
  const [projectId,setSelectionProject]=useState(null);
  const [ready,setReady]=useState(false);
  const [catalogs,setCatalogs]=useState({});
  useEffect(()=>{
    try {setState(readSelections(localStorage.getItem(MODEL_SELECTION_KEY),localStorage.getItem('phaseforge.modelSelection')));} catch {}
    setReady(true);
  },[]);
  useEffect(()=>{
    const invalidate=()=>setCatalogs({});
    window.addEventListener('phaseforge:providers-changed',invalidate);
    return()=>window.removeEventListener('phaseforge:providers-changed',invalidate);
  },[]);
  useEffect(()=>{
    if(!ready)return;
    try {localStorage.setItem(MODEL_SELECTION_KEY,JSON.stringify(state));} catch {}
  },[state,ready]);
  // Migrate a previous explicit choice to the first opened conversation only.
  useEffect(()=>{
    if(!ready||!projectId)return;
    setState(current=>current.legacy&&!current.projects[projectId]
      ? {...current,projects:{...current.projects,[projectId]:current.legacy},legacy:null}
      : current);
  },[projectId,ready]);
  const key=projectId||'__new_conversation__';
  const selection=state.projects[key]||EMPTY_SELECTION;
  const setSelection=useCallback(update=>{
    setState(current=>{
      const prior=current.projects[key]||EMPTY_SELECTION;
      const next=normalizeSelection(typeof update==='function'?update(prior):update);
      return {...current,projects:{...current.projects,[key]:next}};
    });
  },[key]);
  const getSelection=useCallback(id=>state.projects[id]||EMPTY_SELECTION,[state.projects]);
  const getSelectionError=useCallback(id=>{
    if(!ready)return 'Loading the conversation model selection.';
    const saved=state.projects[id]||EMPTY_SELECTION;
    const account=catalogs[saved.provider];
    return account?.error||selectionProblem(saved,account?.models,!!account?.ready);
  },[state.projects,catalogs,ready]);
  const setModelCatalog=useCallback((provider,models,error='')=>setCatalogs(current=>({...current,[provider]:{models,error,ready:!error}})),[]);
  const catalog=catalogs[selection.provider];
  const selectionError=catalog?.error||selectionProblem(selection,catalog?.models,!!catalog?.ready);
  const value=useMemo(()=>({selection,setSelection,setSelectionProject,getSelection,getSelectionError,setModelCatalog,selectionError,selectionValid:ready&&!selectionError,selectionProjectId:projectId,selectionReady:ready}),[selection,setSelection,getSelection,getSelectionError,setModelCatalog,selectionError,projectId,ready]);
  return <SelectionContext.Provider value={value}>{children}</SelectionContext.Provider>;
}
export const useModelSelection=()=>useContext(SelectionContext);
