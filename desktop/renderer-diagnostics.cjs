const fs=require('node:fs');
const path=require('node:path');
const MAX_BYTES=1024*1024;
function clean(value,limit=12000){return String(value??'').replace(/\bBearer\s+\S+/gi,'Bearer [redacted]').replace(/\bsk-[A-Za-z0-9_-]{12,}/g,'[redacted]').replace(/([?&](?:api_?key|token|secret|authorization)=)[^&\s)]+/gi,'$1[redacted]').slice(0,limit);}
function installRendererDiagnostics(contents,directory){
  const filename=path.join(directory,'renderer.log');
  const write=(kind,details)=>{try{
    fs.mkdirSync(directory,{recursive:true});
    if(fs.existsSync(filename)&&fs.statSync(filename).size>=MAX_BYTES){fs.renameSync(filename,path.join(directory,'renderer.previous.log'));}
    fs.appendFileSync(filename,JSON.stringify({at:new Date().toISOString(),kind,...details})+'\n');
  }catch{/* Diagnostics must never crash the application. */}};
  const consoleMessage=(details,legacyLevel,legacyMessage,legacyLine,legacySource)=>{
    const level=details.level??['debug','info','warning','error'][legacyLevel];
    if(!['warning','error'].includes(level))return;
    write('console',{level,message:clean(details.message??legacyMessage),source:clean(details.sourceId??legacySource,2000),line:details.lineNumber??legacyLine??null});
  };
  const gone=(_event,details)=>write('render-process-gone',{reason:clean(details.reason,120),exitCode:details.exitCode});
  const failed=(_event,code,description,url,isMainFrame)=>{if(isMainFrame&&code!==-3)write('did-fail-load',{code,description:clean(description),url:clean(url,2000)});};
  const preload=(_event,file,error)=>write('preload-error',{file:clean(file,2000),message:clean(error?.stack||error)});
  contents.on('console-message',consoleMessage);contents.on('render-process-gone',gone);contents.on('did-fail-load',failed);contents.on('preload-error',preload);
  write('renderer-attached',{pid:contents.getOSProcessId?.()||null});
  return {filename,dispose(){contents.removeListener('console-message',consoleMessage);contents.removeListener('render-process-gone',gone);contents.removeListener('did-fail-load',failed);contents.removeListener('preload-error',preload);}};
}
module.exports={installRendererDiagnostics,clean};
