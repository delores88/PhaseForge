/** A compatibility placeholder is not an installed OpenSSL library identity. */
export function cryptoRuntimeIdentity(versions){
  const observed=versions?.openssl;
  if(typeof observed==='string'&&/^\d+\.\d+\.\d+[a-z]?(?:[-+][A-Za-z0-9.-]+)?$/.test(observed)&&!/^0\.0\.0(?:$|[-+])/.test(observed)){
    return {status:'observed_version',component:'openssl',version:observed,evidence:'Actual Electron process.versions.openssl; exact native binary identity is retained separately'};
  }
  return {status:'unresolved',observed_openssl:observed??null,component:null,gap:'Electron reported no concrete OpenSSL library version. A placeholder does not identify OpenSSL or the exact BoringSSL revision. Official Electron source/archive and packaged-native-byte mapping is required before claiming crypto library coverage.'};
}
