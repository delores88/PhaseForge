"""Native verification integration acceptance; requires Cargo-built binary and Python. No AI/network.
This is NOT executed in the packaging container. Windows/Linux CI executes it.
"""
import argparse,hashlib,io,json,os,pathlib,subprocess,tempfile,time,urllib.request,urllib.error,zipfile
from test_proposal_schema import draft

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',required=True);p.add_argument('--port',type=int,default=17435);args=p.parse_args()
    binary=pathlib.Path(args.binary).resolve()
    if not binary.is_file():raise SystemExit('Build the backend first; this is a native integration test, not a source check.')
    with tempfile.TemporaryDirectory(prefix='phaseforge-discovery-') as folder:
        root=pathlib.Path(folder);config=root/'isolated.toml';config.write_text('bind_address = "127.0.0.1"\nport = '+str(args.port)+'\ndata_directory = '+json.dumps(str(root/'data'))+'\ngpu_enabled = false\n',encoding='utf-8')
        log=(root/'backend.log').open('w');environment=os.environ.copy();environment["PHASEFORGE_VERIFIER_PYTHON"]=str(pathlib.Path(os.sys.executable).resolve());proc=subprocess.Popen([str(binary),'--cpu-only','--config',str(config)],stdout=log,stderr=subprocess.STDOUT,env=environment)
        def call(path,payload=None,method=None,raw=False):
            req=urllib.request.Request(f'http://127.0.0.1:{args.port}'+path,data=json.dumps(payload).encode() if payload is not None else None,method=method,headers={'Content-Type':'application/json'})
            with urllib.request.urlopen(req,timeout=30) as response:return response.read() if raw else json.load(response)
        try:
            for _ in range(100):
                try:
                    if call('/api/health')['status']=='ok':break
                except Exception:
                    if proc.poll() is not None:raise RuntimeError((root/'backend.log').read_text())
                    time.sleep(.2)
            else:raise RuntimeError('Backend did not start')
            world=call('/api/projects',{'question':'Can a finite discovery campaign retain reproducible evidence?','name':'Discovery CI only'})
            m=draft();m['model']['variables'][0]['initial']=1;m['model']['derivatives'][0]['expression']='-x';m['integration']['max_steps']=101
            imported=call(f"/api/projects/{world['id']}/manifests",{'manifest':m,'auto_run':False})['manifest']
            recipe={'title':'Bounded CI campaign','hypothesis':'No scientific claim; contract test only','base_manifest_id':imported['id'],'strategy':'novelty_search',
                'parameters':[{'name':'x0','target':'initial:x','minimum':.5,'maximum':1.5}],
                'objectives':[{'metric':'state','goal':'minimize'}],'descriptors':[{'metric':'state','minimum':0,'maximum':1,'bins':4}],
                'exploration_trials':4,'validation_finalists':1,'wall_seconds':60,'per_trial_seconds':10,'seed':7,'absolute_tolerance':1e-6,'relative_tolerance':.001,'auto_review':False}
            study=call('/api/discovery/studies',recipe)['study'];sid=study['id'];assert study['state']=='draft' and study['trials']==[]
            call(f'/api/discovery/studies/{sid}/start',{})
            for _ in range(600):
                detail=call(f'/api/discovery/studies/{sid}');study=detail['study']
                if study['state'] not in ('running','pausing'):break
                time.sleep(.2)
            assert study['state']=='completed',study
            assert len(study['trials'])==6,study['trials'];assert detail['summary']['eligible']==4
            assert detail['summary']['validation'][0]['status']=='agreement',detail
            runid=study['trials'][0]['run_id'];assert call('/api/runs/'+runid+'/signals')['series']
            candidate=next(t for t in study['trials'] if t['id']==study['finalist_ids'][0])
            source=call('/api/runs/'+candidate['run_id'])
            # A second actually executed ODE run calibrates the independent solver.
            ctrl=call('/api/manifests/'+imported['id']+'/run',{})
            for _ in range(200):
                ctrl=call('/api/runs/'+ctrl['id'])
                if ctrl['status'] not in ('queued','running'):break
                time.sleep(.1)
            assert ctrl['status']=='completed',ctrl
            rules=[{'metric':'state','scale':1.,'absolute_tolerance':1e-6,'relative_tolerance':.001,'robustness_absolute':.1}]
            ref=call('/api/verification/catalog',{'study_id':sid,'metric_rules':rules,'title':'Explicit test-only reference',
                'source':'Synthetic source for API contract testing, not published evidence','license':'Test fixture data','run_id':None,'measurements':{'state':.95}})['reference']
            protocol={'study_id':sid,'trial_id':candidate['id'],'question':'Does the selected test candidate reproduce independently?',
                'metrics':rules,'holdout_replicates':4,'perturb_fraction':.01,'holdout_seed':123456,'wall_seconds':30,'max_rhs_evaluations':100000,
                'solver_absolute_tolerance':1e-10,'solver_relative_tolerance':1e-8,'duplicate_distance':.001,
                'controls':[{'run_id':ctrl['id'],'metric':'state','minimum':.3678,'maximum':.3680,'rationale':'Analytic scalar decay, test fixture only'}],
                'reference_ids':[ref['id']]}
            dossier=call('/api/verification/dossiers',protocol)['dossier'];did=dossier['id']
            assert dossier['state']=='draft' and dossier['result'] is None
            assert call('/api/verification/probe',{})['available']
            call('/api/verification/dossiers/'+did+'/start',{})
            for _ in range(500):
                evidence=call('/api/verification/dossiers/'+did);dossier=evidence['dossier']
                if dossier['state'] not in ('running','stopping'):break
                time.sleep(.1)
            assert dossier['state']=='completed',dossier
            assert dossier['result']['independent_agreement'],dossier['result']
            assert dossier['result']['controls_passed'],dossier['result']
            assert dossier['result']['robustness']=='within_declared_limits'
            assert dossier['completed_tasks']==7
            assert evidence['assessment']['novelty']=='not_certified'
            assert evidence['assessment']['status']=='evidence_gaps_remain' # No search or source-reading claim.
            assert evidence['assessment']['catalog']['compared']==1
            window=call('/api/verification/runs/'+source['id']+'/window?max_points=3')
            assert all(s['returned_points']<=3 for s in window['series'])
            review=call('/api/verification/dossiers/'+did+'/review',{'reviewer':'Native CI test', 'disposition':'inconclusive',
                'comparison_notes':'This is a synthetic integration test, not scientific novelty evidence.',
                'limitations':'No source texts searched or read.', 'examined_sources':[], 'full_text_reviewed':False})
            assert review['review']['evidence_hash']
            raw=call(f'/api/discovery/studies/{sid}/export',raw=True)
            with zipfile.ZipFile(io.BytesIO(raw)) as z:
                assert z.testzip() is None;checksums=json.loads(z.read('checksums.json'))
                for name,wanted in checksums.items():assert hashlib.sha256(z.read(name)).hexdigest()==wanted,name
                assert 'manuscript.md' in z.namelist();assert 'reference_ode.py' in z.namelist()
                assert len([n for n in z.namelist() if n.startswith('runs/')])==6
                assert f'verification/{did}/dossier.json' in z.namelist()
                assert f'verification/{did}/input.json' in z.namelist()
                assert 'verification_worker.py' in z.namelist()
                out=root/'bundle';z.extractall(out)
            result=subprocess.run([os.sys.executable,str(root/'bundle/verify_bundle.py'),str(root/'bundle')],capture_output=True,text=True);assert result.returncode==0,result.stderr+result.stdout
            first=study['trials'][0]
            replay=subprocess.run([os.sys.executable,str(root/'bundle/reference_ode.py'),str(root/'bundle/manifests'/f"{first['manifest_id']}.json"),'--run',str(root/'bundle/runs'/f"{runid}.json")],capture_output=True,text=True)
            assert replay.returncode==0,replay.stderr+replay.stdout
            # No budget was silently consumed by AI.
            assert call('/api/usage')['totals']['total_tokens']==0
            print('PASS native integration: novelty campaign, frozen verification, 2 independent solvers, declared control, 4 holdouts, comparison corpus, review, evidence window, export hashes, zero paid calls.')
        finally:
            proc.terminate()
            try:proc.wait(timeout=10)
            except subprocess.TimeoutExpired:proc.kill();proc.wait()
            log.close()
if __name__=='__main__':main()
