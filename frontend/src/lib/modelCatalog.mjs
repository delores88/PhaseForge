import {api} from './api';
const cache=new Map();
export function invalidateModelCatalog(){cache.clear();}
export function loadModelCatalog(provider,baseUrl=''){
  const key=`${provider}:${baseUrl}`,saved=cache.get(key);
  if(saved&&Date.now()-saved.at<300000)return saved.promise;
  const promise=api.providerModels(provider).catch(error=>{cache.delete(key);throw error;});
  cache.set(key,{promise,at:Date.now()});
  return promise;
}
