"""Real native API acceptance: requires the compiled backend; no paid model/public search.
Tests reducers, true h/2+h/4, per-body radii, preflight, local CSV and consent rejection.
A missing binary is a failed prerequisite, never a passed test or a mock server.
"""
import argparse,hashlib,json,os,pathlib,subprocess,tempfile,time,urllib.error,urllib.request
from test_proposal_schema import draft
from runtime_smoke import backend_process

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--port',type=int,default=17436);a=p.parse_args()
    binary=pathlib.Path(a.binary).resolve()
    if not binary.is_file():raise SystemExit('Compile backend first; this test launches the real native application.')
    with tempfile.TemporaryDirectory(prefix='phaseforge-research-test-') as temp:
        root=pathlib.Path(temp);cfg=root/'test.toml';cfg.write_text('bind_address = "127.0.0.1"\nport = '+str(a.port)+'\ndata_directory = '+json.dumps(str(root/'data'))+'\ngpu_enabled = false\n',encoding='utf-8')
        def call(route,payload=None,method=None):
            req=urllib.request.Request(f'http://127.0.0.1:{a.port}'+route,data=json.dumps(payload).encode() if payload is not None else None,headers={'Content-Type':'application/json'},method=method)
            with urllib.request.urlopen(req,timeout=20) as r:return json.load(r)
        def reject(route,payload):
            try:call(route,payload)
            except urllib.error.HTTPError as error:
                assert error.code==422,(route,error.code,error.read());return
            raise AssertionError('Invalid request accepted: '+route)
        with backend_process([str(binary),'--cpu-only','--config',str(cfg)], root/'backend.log') as proc:
            for _ in range(100):
                try:
                    if call('/api/health')['status']=='ok':break
                except Exception:
                    if proc.poll() is not None:raise RuntimeError((root/'backend.log').read_text())
                    time.sleep(.2)
            else:raise RuntimeError('Native backend did not start')
            world=call('/api/projects',{'question':'Can this implementation retain whole-trajectory measurements?','name':'Numerical contract test'})
            route='/api/projects/'+world['id']+'/research'
            assert call(route)=={'plans':[],'searches':[],'tasks':[],'data':[]}
            reject(route+'/search',{'query':'synthetic test query','consent':False})
            csv='time,measurement\r\n0,1\r\n1,3\r\n2,NA\r\n'
            profile=call(route+'/data',{'name':'synthetic.csv','content':csv,'provenance':'Software test only'})
            assert profile['rows']==3 and profile['columns'][1]['missing']==1
            assert profile['columns'][1]['mean']==2 and profile['sha256']==hashlib.sha256(csv.encode()).hexdigest()
            reject(route+'/data',{'name':'ragged.csv','content':'a,b\n1\n','provenance':'test'})
            m=draft();m['model']['derivatives'][0]['expression']='1';m['integration'].update(end_time=2.,max_steps=201,output_stride=200)
            m['trajectory']=[dict(name='peak',expression='1-abs(t-1)',reducer='maximum',unit='1',threshold=0.,hysteresis=0.),dict(name='first_entry',expression='x',reducer='first_above',unit='time',threshold=.5,hysteresis=0.)]
            m['observables']=[dict(name='peak_readout',expression='peak',unit='1')]
            m['constraints']=[dict(name='peak_bound',expression='peak',tolerance=1.01)]
            m['visualization']['entities']=[dict(id='a',x='x',y='0',z='0',vx='1',vy='0',vz='0',radius='0.2'),dict(id='b',x='x+2',y='0',z='0',vx='1',vy='0',vz='0',radius='0.7')]
            m['falsification']=[dict(name='real_resolution',kind='resolution_ladder',target=None,magnitude=0,repetitions=2,checks=[dict(metric='peak',expectation='stable',absolute_tolerance=1e-8,relative_tolerance=0)])]
            saved=call('/api/projects/'+world['id']+'/manifests',{'manifest':m,'auto_run':False})['manifest']
            advice=call('/api/manifests/'+saved['id']+'/compute-advice');assert advice['admissible'] and not advice['gpu_eligible'];assert advice['candidate_evaluations']==1
            run=call('/api/manifests/'+saved['id']+'/run',{})
            for _ in range(300):
                run=call('/api/runs/'+run['id'])
                if run['status'] not in ('queued','running'):break
                time.sleep(.1)
            assert run['status']=='completed',run.get('error')
            result=run['result'];assert abs(result['metrics']['peak']-1)<1e-9
            assert result['metrics']['first_entry_observed']==1
            assert .49<result['metrics']['first_entry']<.52
            assert [t['time_step'] for t in result['falsification'][0]['trials']]==[.005,.0025]
            assert result['falsification'][0]['status']=='passed'
            entities=result['visualization']['frames'][0]['entities'];assert len(entities)==2
            assert [e['radius'] for e in entities]==[.2,.7]
            assert result['numerical']['compute_advice']['recommended_compute']==run['compute']
            findings=call('/api/runs/'+run['id']+'/findings');assert findings['novelty']=='not_assessed'
            assert any(x['name']=='peak' for x in findings['primary_metrics'])
            assert len(call(route)['data'])==1 and call(route)['searches']==[]
            assert call('/api/usage')['totals']['total_tokens']==0
            print('PASS real native API: admission, reducers, censored-event metadata, distinct h/2+h/4, explicit radii, local CSV provenance, public-query refusal without consent, zero paid tokens.')
if __name__=='__main__':main()
