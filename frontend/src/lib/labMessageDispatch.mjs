export async function dispatchLabMessage({projectId,activeSession,payload,snapshotSelection,submitSession,submitUpdate}){
  let selection,selectionError;
  try{selection=Object.freeze({...snapshotSelection(projectId)});}catch(error){selectionError=error;}
  const target=activeSession?.project_id===projectId&&activeSession.kind==='session'&&['queued','running','waiting','provisioning'].includes(activeSession.state)?activeSession:null;
  let attachments=payload.attachments||[];
  if(target){
    try{return{kind:'update',value:await submitUpdate(target.id,{request_id:payload.request_id,content:payload.content,attachments})};}
    catch(error){
      if(error.status!==409||error.body?.error?.code!=='session_finished')throw error;
      const retained=error.body.error.retained_files;
      if(Array.isArray(retained)&&retained.length===attachments.length)attachments=retained;
    }
  }
  if(selectionError)throw selectionError;
  return{kind:'session',value:await submitSession(projectId,{...payload,...selection,attachments})};
}
