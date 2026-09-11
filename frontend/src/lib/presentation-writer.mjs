const equal=(a,b)=>JSON.stringify(a)===JSON.stringify(b);
export function presentationPatch(previous,next){
  const patch={};for(const key of Object.keys(next))if(!equal(previous?.[key],next[key]))patch[key]=next[key];return patch;
}

/** Serialize this viewer's changed fields; rebase only those fields after a server CAS conflict. */
export class PresentationWriter {
  constructor({revision=0,settings={},send,onState,onError,uuid=()=>crypto.randomUUID()}){
    this.revision=revision;this.settings=settings;this.send=send;this.onState=onState;this.onError=onError;this.uuid=uuid;
    this.pending={};this.inFlight=null;this.running=null;this.detached=false;
  }
  queue(patch){this.pending={...this.pending,...patch};}
  projected(){return {...this.settings,...this.inFlight?.patch,...this.pending};}
  acceptRemote(value){if(value.revision>this.revision&&!this.inFlight){const keys=Object.keys(presentationPatch(this.settings,value.settings||{})).filter(key=>!(key in this.pending));this.revision=value.revision;this.settings=value.settings||{};if(!this.detached)this.onState?.(this.projected(),this.revision,{remoteKeys:keys});}}
  flush(){
    if(this.running)return this.running;
    this.running=this.drain().finally(()=>{this.running=null;});return this.running;
  }
  async drain(){
    while(Object.keys(this.pending).length||this.inFlight){
      if(!this.inFlight){this.inFlight={patch:this.pending,operation_id:this.uuid()};this.pending={};}
      let acknowledged=false;
      for(let attempt=0;attempt<4;attempt++){
        try{
          const value=await this.send({base_revision:this.revision,...this.inFlight});
          this.revision=value.revision;this.settings=value.settings||{};this.inFlight=null;acknowledged=true;
          if(!this.detached)this.onState?.(this.projected(),this.revision);break;
        }catch(error){
          const current=error.body?.error?.current||error.body?.current;
          if(error.status===409&&current&&Number.isFinite(current.revision)){
            const keys=Object.keys(presentationPatch(this.settings,current.settings||{})).filter(key=>!(key in this.inFlight.patch)&&!(key in this.pending));
            this.revision=current.revision;this.settings=current.settings||{};
            if(!this.detached)this.onState?.(this.projected(),this.revision,{remoteKeys:keys});
            continue;
          }
          if(!this.detached)this.onError?.(error);return;
        }
      }
      if(!acknowledged){if(!this.detached)this.onError?.(new Error('Presentation changed repeatedly. Your edits remain queued; retry saving.'));return;}
    }
  }
  retry(){return this.flush();}
  detach(){this.detached=true;this.onState=null;this.onError=null;return this.flush();}
}
