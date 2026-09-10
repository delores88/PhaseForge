"""Native v0.7 acceptance: no model, paper review or task gates required.
Launches the actual built Rust backend. Missing binaries fail, never skip/pass.
"""
import argparse,copy,json,pathlib,subprocess,tempfile,time,urllib.request,urllib.error,uuid
from test_proposal_schema import draft
from runtime_smoke import backend_process

def main():
 p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--port',type=int,default=17437);a=p.parse_args();binary=pathlib.Path(a.binary).resolve()
 if not binary.is_file():raise SystemExit('Compile the Rust backend first. Native acceptance was not executed.')
 with tempfile.TemporaryDirectory(prefix='phaseforge-v07-') as d:
  root=pathlib.Path(d);config=root/'test.toml';config.write_text('bind_address="127.0.0.1"\nport='+str(a.port)+'\ndata_directory='+json.dumps(str(root/'data'))+'\ngpu_enabled=false\n',encoding='utf8')
  def call(route,body=None,method=None):
   req=urllib.request.Request('http://127.0.0.1:'+str(a.port)+route,data=json.dumps(body).encode() if body is not None else None,headers={'Content-Type':'application/json'},method=method)
   with urllib.request.urlopen(req,timeout=25) as r:return json.load(r) if r.status!=204 else None
  def rejected(route,body):
   try:call(route,body)
   except urllib.error.HTTPError as e:assert e.code in (400,404,409,422),e.read();return
   raise AssertionError('Invalid action was accepted')
  def finished(run):
   for _ in range(250):
    run=call('/api/runs/'+run['id'])
    if run['status'] not in ('queued','running'):break
    time.sleep(.05)
   assert run['status']=='completed',run.get('error');return run
  with backend_process([str(binary),'--cpu-only','--config',str(config)], root/'backend.log') as proc:
   for _ in range(100):
    try:
     if call('/api/health')['status']=='ok':break
    except Exception:
     if proc.poll() is not None:raise RuntimeError((root/'backend.log').read_text())
     time.sleep(.15)
   else:raise RuntimeError('Backend failed to start')
   world=call('/api/projects',{'name':'Direct-workflow software test','question':'Evaluate a user-authored numerical model without research stage gates'})
   m=draft();m['model']['derivatives'][0]['expression']='1';m['compute'].update(max_wall_seconds=30,max_memory_mb=1024,candidate_count=1);m['search']['enabled']=False
   saved=call('/api/projects/'+world['id']+'/manifests',{'manifest':m,'auto_run':False})['manifest']
   original=copy.deepcopy(saved)
   initial=finished(call('/api/manifests/'+saved['id']+'/run',{}));assert abs(initial['result']['metrics']['final_x']-1)<1e-8
   route='/api/projects/'+world['id']+'/experiments/next'
   q={'request_id':str(uuid.uuid4()),'source_run_id':initial['id'],'operation':'finer_steps','run':True}
   result=call(route,q);assert result['status']=='queued';assert result['manifest']['integration']['time_step']==.005
   repeated=call(route,q);assert repeated['run']['id']==result['run']['id'];assert repeated['manifest']['id']==result['manifest']['id']
   finer=finished(result['run']);assert abs(finer['result']['metrics']['final_x']-1)<1e-8
   conflict=dict(q,operation='longer_horizon');rejected(route,conflict)
   longer=call(route,dict(q,request_id=str(uuid.uuid4()),operation='longer_horizon'));r=finished(longer['run']);assert abs(r['result']['metrics']['final_x']-2)<1e-8
   assert longer['manifest']['compute']['max_wall_seconds']==original['compute']['max_wall_seconds']
   assert call('/api/manifests/'+original['id'])==original
   prepared=call(route,dict(q,request_id=str(uuid.uuid4()),operation='finer_steps',run=False));assert prepared['status']=='prepared';assert prepared['run'] is None
   other=call('/api/projects',{'name':'Other','question':'Project isolation test'})
   rejected('/api/projects/'+other['id']+'/experiments/next',dict(q,request_id=str(uuid.uuid4())))
   rejected(route,dict(q,request_id=str(uuid.uuid4()),operation='arbitrary_code'))
   no_consent=dict(q,request_id=str(uuid.uuid4()));del no_consent['run'];rejected(route,no_consent)
   workspace=call('/api/projects/'+world['id']+'/research');assert workspace['plans']==[] and workspace['tasks']==[]
   usage=call('/api/usage');assert usage['totals']['total_tokens']==0,usage['totals']
   print('PASS native direct workflow: local setup, real numeric results, h/2 replay, doubled horizon, idempotency, immutable history, no task gates, project isolation and zero paid tokens.')
if __name__=='__main__':main()
