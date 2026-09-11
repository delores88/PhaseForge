const path=require('node:path');
module.exports=async function afterPack(context){
  if(context.electronPlatformName!=='win32')return;
  const {OPENMM_RESOURCE,verifyOpenmmSourceDelivery}=await import('../../scripts/release/openmm-sources.mjs');
  await verifyOpenmmSourceDelivery(path.join(context.appOutDir,'resources',OPENMM_RESOURCE));
  const {MSVC_RESOURCE,verifyMsvcNotices}=await import('../../scripts/release/msvc-notices.mjs');
  await verifyMsvcNotices(path.join(context.appOutDir,'resources',MSVC_RESOURCE));
};
