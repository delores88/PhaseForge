"""Inspect completed ordinary-chat diffusion outputs using independent frozen formulas."""
import argparse,base64,hashlib,json,math,os,pathlib
from urllib.request import Request,urlopen
from check_field_worker import inspect_fourier,inspect_images

def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--session',required=True);parser.add_argument('--cold',required=True,help='D=0.2 source job');parser.add_argument('--hot',required=True,help='D=0.4 source job');parser.add_argument('--base-url',default='http://127.0.0.1:7332');parser.add_argument('--output',type=pathlib.Path,required=True);args=parser.parse_args()
    root=pathlib.Path(os.environ['LOCALAPPDATA'])/'PhaseForge/PhaseForge/data/artifacts/laboratory'
    def job(ident):
        with urlopen(Request(f'{args.base_url}/api/laboratory/jobs/{ident}',headers={'Origin':args.base_url})) as response:return json.load(response)
    session=job(args.session);runs=[]
    expected={'nx':64,'ny':64,'length_x_um':10,'length_y_um':10,'dt_s':.005,'steps':512,'record_interval':8,'boundary':'periodic','initial':{'kind':'fourier','baseline':1,'amplitude':.2,'mode_x':1,'mode_y':2}}
    for ident,diffusivity in [(args.cold,.2),(args.hot,.4)]:
        record=job(ident);folder=root/ident;manifest=read(folder/'manifest.json');parameters=manifest['input']['parameters'];numerics=inspect_fourier(folder,parameters);images=inspect_images(folder)
        probes=parameters.get('probes',[])
        checks={'completed_adapter':record['state']=='completed' and record['input']['engine']=='diffusion_2d',
            'same_parent_and_project':record['parent_id']==args.session and record['project_id']==session['project_id'],
            'requested_physics':all(parameters.get(k)==v for k,v in expected.items()) and parameters['diffusivity_um2_s']==diffusivity,
            'point_requested':any(p.get('kind')=='point' and p.get('x_um')==.2 and p.get('y_um')==2.4 for p in probes),
            'region_requested':any(p.get('kind')=='region' and all(p.get(k)==v for k,v in {'x_min_um':1.3,'x_max_um':3.8,'y_min_um':4.2,'y_max_um':7.1}.items()) for p in probes),
            'independent_numerics':numerics['passed'],'independent_images':images['passed']}
        runs.append({'job_id':ident,'diffusivity':diffusivity,'checks':checks,'numerics':numerics,'images':images,'result_sha256':sha(folder/'result.json'),'worker_sha256':sha(folder/'field_worker.py')})
    acknowledgements=[];observed_sources=set()
    for ack_path in (root/args.session).glob('native-vision-ack-*.json'):
        ack=read(ack_path);dispatch=read(root/args.session/ack['request_path'])
        assert ack['status']=='completed_response_received' and ack['provider_response_id']
        assert ack['usage_id']==dispatch['receipt']['usage_id'] and ack['images']==dispatch['receipt']['images']
        actual_images=[part['image_url'] for item in dispatch['input_images'] for part in item.get('content',[]) if part.get('type')=='input_image']
        for receipt in ack['images']:
            image_path=root/receipt['source_job_id']/receipt['path'];raw=image_path.read_bytes()
            assert sha(image_path)==receipt['sha256']
            assert 'data:image/png;base64,'+base64.b64encode(raw).decode() in actual_images
            observed_sources.add(receipt['source_job_id'])
        acknowledgements.append({'path':ack_path.name,'usage_id':ack['usage_id'],'provider_response_id':ack['provider_response_id'],'images':ack['images']})
    ratio=runs[1]['numerics']['frames'][-1]['independent']['mode_amplitude']/runs[0]['numerics']['frames'][-1]['independent']['mode_amplitude']
    expected_ratio=math.exp(-.2*((2*math.pi/10)**2+(4*math.pi/10)**2)*2.56)
    report={'session_id':args.session,'provider':session['input']['provider'],'model':session['input']['model'],'reasoning_effort':session['input']['reasoning_effort'],'runs':runs,'ratio':ratio,'expected_ratio':expected_ratio,'vision_acknowledgements':acknowledgements,'checks':{'all_runs':all(all(run['checks'].values()) for run in runs),'intervention':abs(ratio/expected_ratio-1)<.01,'session_completed':session['state']=='completed','both_actual_images_delivered':{args.cold,args.hot}<=observed_sources,'matched_fixed_color_scales':runs[0]['images']['color_scale']==runs[1]['images']['color_scale']},'scope':'Independent numerical and pixel verification plus native image request acknowledgement; installed viewer inspection and scientific interpretation are separately reviewed.'}
    report['passed']=all(report['checks'].values());args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({k:v for k,v in report.items() if k!='runs'},indent=2))
    if not report['passed']:raise SystemExit(1)
if __name__=='__main__':main()
