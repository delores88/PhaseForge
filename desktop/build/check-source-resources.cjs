module.exports=async function beforePack(context){
  if(context.electronPlatformName!=='win32')return;
  const {stagedOpenmmSources,verifyOpenmmSourceDelivery}=await import('../../scripts/release/openmm-sources.mjs');
  await verifyOpenmmSourceDelivery(stagedOpenmmSources);
  const {verifyMsvcNotices}=await import('../../scripts/release/msvc-notices.mjs');
  await verifyMsvcNotices();
};
