const {randomBytes,createHmac,timingSafeEqual}=require('node:crypto');

function createBackendSecret(){return randomBytes(32).toString('hex');}
const validPort=port=>Number.isInteger(port)&&port>0&&port<=65535;
function readBackendEndpoint(child,version,{timeoutMs=45000}={}){
  return new Promise((resolve,reject)=>{
    if(!child.stdout)return reject(new Error('The research engine has no private startup channel.'));
    let pending=Buffer.alloc(0),received=0,settled=false;
    const finish=(error,endpoint)=>{
      if(settled)return;settled=true;clearTimeout(timer);
      child.stdout.removeListener('data',onData);child.stdout.removeListener('end',onEnd);
      child.removeListener('error',onError);child.removeListener('exit',onExit);
      if(error)reject(error);else resolve(Object.freeze(endpoint));
    };
    const fail=message=>finish(new Error(message));
    const onError=()=>fail('The research engine could not start.');
    const onExit=()=>fail('The research engine exited before announcing its local endpoint.');
    const onEnd=()=>fail('The research engine closed its startup channel without an endpoint.');
    const onData=chunk=>{
      received+=chunk.length;
      if(received>65536)return fail('The research engine startup announcement exceeded its size limit.');
      pending=Buffer.concat([pending,chunk]);let endpoint;
      while(true){
        const newline=pending.indexOf(10);if(newline<0)break;
        if(newline>4096)return fail('The research engine startup line exceeded its size limit.');
        const line=pending.subarray(0,newline).toString('utf8').replace(/\r$/,'');pending=pending.subarray(newline+1);
        if(!line.startsWith('PHASEFORGE_DESKTOP_ENDPOINT '))continue;
        if(endpoint)return fail('The research engine announced more than one startup endpoint.');
        try{endpoint=JSON.parse(line.slice('PHASEFORGE_DESKTOP_ENDPOINT '.length));}catch{return fail('The research engine announced a malformed startup endpoint.');}
        if(!endpoint||Array.isArray(endpoint)||Object.keys(endpoint).length!==3||endpoint.address!=='127.0.0.1'||!validPort(endpoint.port)||endpoint.version!==version)
          return fail('The research engine announced an invalid local endpoint or version.');
      }
      if(pending.length>4096)return fail('The research engine startup line exceeded its size limit.');
      if(endpoint)finish(null,endpoint);
    };
    const timer=setTimeout(()=>fail('The research engine did not announce its local endpoint in time.'),timeoutMs);
    child.stdout.on('data',onData);child.stdout.once('end',onEnd);child.once('error',onError);child.once('exit',onExit);
    if(child.exitCode!==null||child.killed)onExit();
  });
}
async function verifyBackendReady(secret,version,{port,fetcher=fetch}={}){
  if(!/^[a-f0-9]{64}$/.test(secret)||!validPort(port))return false;
  const nonce=randomBytes(32).toString('hex');
  try{
    const response=await fetcher(`http://127.0.0.1:${port}/api/desktop/ready?nonce=${nonce}`,{redirect:'error',signal:AbortSignal.timeout(1500),cache:'no-store'});
    if(!response.ok){await response.body?.cancel();return false;}
    const reader=response.body?.getReader();if(!reader)return false;
    const chunks=[];let size=0;
    try{
      if(Number(response.headers.get('content-length'))>4096){await reader.cancel();return false;}
      while(true){const {done,value}=await reader.read();if(done)break;size+=value.byteLength;if(size>4096){await reader.cancel();return false;}chunks.push(Buffer.from(value));}
    }finally{reader.releaseLock();}
    const result=JSON.parse(Buffer.concat(chunks,size).toString('utf8'));
    if(result.algorithm!=='HMAC-SHA256'||result.version!==version||result.nonce!==nonce||typeof result.proof!=='string'||!/^[a-f0-9]{64}$/.test(result.proof))return false;
    const expected=createHmac('sha256',Buffer.from(secret,'hex')).update(nonce).digest();
    return timingSafeEqual(expected,Buffer.from(result.proof,'hex'));
  }catch{return false;}
}
module.exports={createBackendSecret,readBackendEndpoint,verifyBackendReady};
