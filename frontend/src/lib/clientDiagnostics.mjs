const STORAGE_KEY='phaseforge.clientDiagnostics.v1';
export function diagnosticText(value,limit=6000){
  return String(value??'').replace(/\bBearer\s+\S+/gi,'Bearer [redacted]').replace(/\bsk-[A-Za-z0-9_-]{12,}/g,'[redacted]').replace(/([?&](?:api_?key|token|secret|authorization)=)[^&\s)]+/gi,'$1[redacted]').slice(0,limit);
}
export function recordClientError(error,{scope='client',componentStack=''}={}){
  const entry={at:new Date().toISOString(),scope:diagnosticText(scope,120),name:diagnosticText(error?.name||'Error',120),message:diagnosticText(error?.message||error,2000),stack:diagnosticText(error?.stack||''),componentStack:diagnosticText(componentStack)};
  try{const stored=JSON.parse(localStorage.getItem(STORAGE_KEY)||'[]'),entries=Array.isArray(stored)?stored:[];localStorage.setItem(STORAGE_KEY,JSON.stringify([...entries.slice(-19),entry]));}catch{}
  console.error('[PhaseForge client diagnostic]',JSON.stringify(entry));
  return entry;
}
export function installClientDiagnostics(target=window){
  const error=event=>recordClientError(event.error||new Error(event.message||'Uncaught client error'),{scope:'window.error'});
  const rejection=event=>recordClientError(event.reason,{scope:'unhandledrejection'});
  target.addEventListener('error',error);target.addEventListener('unhandledrejection',rejection);
  return()=>{target.removeEventListener('error',error);target.removeEventListener('unhandledrejection',rejection);};
}
