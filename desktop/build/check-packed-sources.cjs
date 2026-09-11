const path=require('node:path');
module.exports=async function afterPack(context){
  if(context.electronPlatformName!=='win32')return;
  const {OPENMM_RESOURCE,verifyOpenmmSourceDelivery}=await import('../../scripts/release/openmm-sources.mjs');
  await verifyOpenmmSourceDelivery(path.join(context.appOutDir,'resources',OPENMM_RESOURCE));
};
