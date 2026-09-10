const {test}=require('node:test');
const assert=require('node:assert/strict');
const {installWorkbenchPermissions}=require('../permissions.cjs');

function fixture(){
  const origin='http://127.0.0.1:7332',handlers={};
  const contents={getURL:()=>origin+'/workspace/',isDestroyed:()=>false,session:{setPermissionRequestHandler(handler){handlers.request=handler;},setPermissionCheckHandler(handler){handlers.check=handler;}}};
  installWorkbenchPermissions(contents,origin);
  const details={isMainFrame:true,requestingUrl:origin+'/workspace/?model=1'};
  const request=(permission='fullscreen',wc=contents,info=details)=>{let answer,called=0;handlers.request(wc,permission,result=>{answer=result;called++;},info);assert.equal(called,1);return answer;};
  const check=(permission='fullscreen',wc=contents,info=details,requestOrigin=origin)=>handlers.check(wc,permission,requestOrigin,info);
  return {origin,contents,details,request,check};
}

test('both Electron permission paths grant fullscreen to the exact workbench main frame',()=>{
  const f=fixture();assert.equal(f.request(),true);assert.equal(f.check(),true);
  f.contents.getURL=()=>f.origin+'/another-project/';assert.equal(f.request(),true);assert.equal(f.check(),true);
});

test('automatic fullscreen and every unrelated permission remain denied',()=>{
  const f=fixture();
  for(const permission of ['automatic-fullscreen','media','display-capture','clipboard-read','geolocation','notifications','pointerLock','openExternal','unknown']){
    assert.equal(f.request(permission),false);assert.equal(f.check(permission),false);
  }
});

test('fullscreen denies other contents, subframes, opaque contexts and mismatched origins',()=>{
  const f=fixture();
  for(const contents of [null,{...f.contents}]){assert.equal(f.request('fullscreen',contents),false);assert.equal(f.check('fullscreen',contents),false);}
  for(const details of [null,{}, {...f.details,isMainFrame:false},{...f.details,isMainFrame:undefined},...['https://example.com/','http://127.0.0.1:7331/','http://localhost:7332/','http://127.0.0.1:7332.evil.test/','file:///private','about:blank','blob:'+f.origin+'/token',f.origin.replace('://','://user@'),'invalid'].map(requestingUrl=>({...f.details,requestingUrl}))]){
    assert.equal(f.request('fullscreen',f.contents,details),false);assert.equal(f.check('fullscreen',f.contents,details),false);
  }
  assert.equal(f.check('fullscreen',f.contents,f.details,'https://example.com'),false);
  f.contents.getURL=()=> 'https://example.com';assert.equal(f.request(),false);assert.equal(f.check(),false);
  f.contents.getURL=()=>f.origin;f.contents.isDestroyed=()=>true;assert.equal(f.request(),false);assert.equal(f.check(),false);
});

test('desktop package includes its permission policy',()=>{
  assert.ok(require('../package.json').build.files.includes('permissions.cjs'));
});
