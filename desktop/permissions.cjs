// Fullscreen has two Electron permission paths. Keep both scoped to the exact
// workbench document; automatic-fullscreen (without user activation) stays denied.
function installWorkbenchPermissions(workbench,workbenchUrl){
  const origin=new URL(workbenchUrl).origin;
  const sameOrigin=value=>{
    try{const url=new URL(value);return url.protocol==='http:'&&url.origin===origin&&!url.username&&!url.password;}
    catch{return false;}
  };
  const allowed=(contents,permission,details,requestingOrigin)=>{
    if(permission!=='fullscreen'||contents!==workbench||!contents||contents.isDestroyed()||details?.isMainFrame!==true)return false;
    return sameOrigin(contents.getURL())&&sameOrigin(details.requestingUrl)&&sameOrigin(requestingOrigin);
  };
  workbench.session.setPermissionRequestHandler((contents,permission,callback,details)=>{
    callback(allowed(contents,permission,details,details?.requestingUrl));
  });
  workbench.session.setPermissionCheckHandler((contents,permission,requestingOrigin,details)=>{
    return allowed(contents,permission,details,requestingOrigin);
  });
}
module.exports={installWorkbenchPermissions};
