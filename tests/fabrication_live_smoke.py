"""Opt-in CAD/PCB acceptance against a running local PhaseForge backend.

Creates one clearly named validation project and retains its jobs/artifacts for
inspection. Does not call models or install software. Run only after the intended
backend is ready: python tests/fabrication_live_smoke.py --base http://127.0.0.1:7331
"""
import argparse
import io
import json
import math
from pathlib import Path
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
import zipfile

from test_fabrication_worker import plate, board


def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--base',default='http://127.0.0.1:7331')
    parser.add_argument('--output',default='.local/fabrication-http-validation')
    parser.add_argument('--case',choices=['all','cad_plate','connected_pcb','disconnected_pcb'],default='all')
    parser.add_argument('--project-id',help='Reuse an existing validation project instead of creating another')
    args=parser.parse_args()
    parsed=urllib.parse.urlparse(args.base)
    if parsed.scheme!='http' or parsed.hostname not in ('127.0.0.1','localhost','::1') or parsed.path not in ('','/') or parsed.query or parsed.fragment or parsed.username:
        raise SystemExit('Use the intended local backend HTTP origin.')
    base=args.base.rstrip('/')
    out=Path(args.output).resolve();out.mkdir(parents=True,exist_ok=True)

    def request(path,payload=None,raw=False):
        req=urllib.request.Request(base+path,data=json.dumps(payload).encode() if payload is not None else None,headers={'Content-Type':'application/json'})
        try:
            with urllib.request.urlopen(req,timeout=30) as response:
                value=response.read(32*1024*1024+1)
                if len(value)>32*1024*1024:raise AssertionError('Validation artifact exceeded 32 MiB')
                return value if raw else json.loads(value)
        except urllib.error.HTTPError as error:
            raise AssertionError(f'{path}: HTTP {error.code}: {error.read(4000).decode(errors="replace")}') from error

    health=request('/api/health')
    assert health.get('name')=='PhaseForge' and health.get('status')=='ok',health
    engines=request('/api/studio/fabrication-engines')
    assert engines.get('cadquery_available') and engines.get('kicad_available'),engines
    project=request('/api/projects/'+str(uuid.UUID(args.project_id))) if args.project_id else request('/api/projects',{'name':'CAD / PCB native validation · '+str(uuid.uuid4())[:8],'question':'Software acceptance only: native CAD geometry, KiCad library consistency and manufacturing gates. No scientific or clinical hypothesis.'})
    report={'backend':health,'project_id':project['id'],'engines':engines,'jobs':[]}
    jobs=[]
    try:
        for label,recipe in [('cad_plate',plate()),('connected_pcb',board()),('disconnected_pcb',board())]:
            if args.case not in ('all',label):continue
            if label=='disconnected_pcb':recipe['pcb']['tracks']=[]
            job=request('/api/studio/fabrications',{'project_id':project['id'],'design':recipe,'max_seconds':180})
            jobs.append(job['id']);deadline=time.monotonic()+200
            while job['status'] in ('queued','running','cancelling') and time.monotonic()<deadline:
                time.sleep(.4);job=request('/api/studio/fabrications/'+job['id'])
            assert job['status']=='completed',job
            result=job['result'];artifacts={};folder=out/label;folder.mkdir(exist_ok=True)
            def download(name):
                data=request('/api/studio/fabrications/'+job['id']+'/artifacts/'+name,raw=True)
                (folder/name).write_bytes(data);artifacts[name]=len(data);return data
            if label=='cad_plate':
                assert result['valid_solid'] and result['solid_count']==1,result
                expected=40*30*3-4*math.pi*1.5**2*3
                assert math.isclose(result['volume_mm3'],expected,abs_tol=1e-6),result
                assert b'ISO-10303-21' in download('model.step')[:100]
                assert len(download('model.stl'))>1000
            else:
                drc=json.loads(download('drc.json'))
                assert download('board.glb')[:4]==b'glTF'
                with zipfile.ZipFile(io.BytesIO(download('editable-project.zip'))) as editable:
                    assert 'PhaseForge.pretty/J1.kicad_mod' in editable.namelist()
                    assert 'board.kicad_pro' in editable.namelist()
                    assert 'fp-lib-table' in editable.namelist()
                assert not any(v['type']=='lib_footprint_mismatch' for v in drc['violations']),drc
                if label=='connected_pcb':
                    assert result['drc_status']=='passed' and result['manufacturing_files'],result
                    with zipfile.ZipFile(io.BytesIO(download('manufacturing.zip'))) as manufacturing:
                        names=manufacturing.namelist()
                        assert any(n.lower().endswith('.gbr') for n in names),names
                        assert any(n.lower().endswith('.drl') for n in names),names
                else:
                    assert result['drc_status']=='issues_found' and not result['manufacturing_files'],result
                    assert drc['unconnected_items'],drc
                    assert 'manufacturing.zip' not in result['files'],result
            # Every advertised UI download must actually be served by the API.
            for name in result['files']:
                assert Path(name).name==name and '\\' not in name,name
                if name not in artifacts:download(name)
            report['jobs'].append({'kind':label,'id':job['id'],'status':job['status'],'result':result,'downloaded_bytes':artifacts})
            (out/'validation.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
            print(f'PASS {label}: {job["id"]}',flush=True)
    finally:
        for identifier in jobs:
            current=request('/api/studio/fabrications/'+identifier)
            if current['status'] in ('queued','running'):
                request('/api/studio/fabrications/'+identifier+'/cancel',{})
    print(json.dumps({'project_id':project['id'],'passed':len(report['jobs']),'report':str(out/'validation.json')}))


if __name__=='__main__':main()
