import {api} from './api';
export const laboratoryUrl=path=>/^https?:\/\//.test(path)?path:`${api.baseUrl}${path}`;
export async function laboratoryJson(path,{signal,method='GET',body,keepalive=false}={}){
  const response=await fetch(laboratoryUrl(path),{signal,method,keepalive,headers:body?{'Content-Type':'application/json'}:undefined,body:body?JSON.stringify(body):undefined});
  const value=await response.json().catch(()=>null);
  if(!response.ok){const error=new Error(value?.error?.message||value?.message||`Request failed (${response.status})`);error.status=response.status;error.body=value;throw error;}return value;
}
