"""Read-only verification of saved installed-app movies and retained source identity."""
import hashlib,json,os,pathlib,subprocess

ROOT=pathlib.Path(__file__).resolve().parents[1]
DATA=pathlib.Path(os.environ['LOCALAPPDATA'])/'PhaseForge/PhaseForge/data/artifacts/laboratory'
OUT=ROOT/'.local/science/installed-exports'
BIN=ROOT/'.local/render-tools/ffmpeg-9.0.1-essentials_build/bin'
SOURCE='cdff7bda-8a88-480a-9d40-0afe2f2cd67d'
CASES=[('a138b47b-d1dd-4e11-b5a3-1280afe18030',1280,720,3),('dc80dd4d-cb96-41dd-88ff-c0d2d66eb420',1920,1080,6)]
def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def sha(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
OUT.mkdir(parents=True,exist_ok=True)
source=DATA/SOURCE
index=read(source/'trajectory/index.json')
report={'source_job_id':SOURCE,'source_result_sha256':sha(source/'result.json'),'cases':[],'checks':{}}
for ident,width,height,seconds in CASES:
    folder=DATA/ident;result=read(folder/'result.json')
    movie=pathlib.Path.home()/'Downloads'/f'phaseforge-simulation-{ident}.mp4'
    decoded=json.loads(subprocess.check_output([str(BIN/'ffprobe.exe'),'-v','error','-count_frames','-show_entries','stream=codec_name,width,height,r_frame_rate,nb_read_frames,duration','-of','json',str(movie)],text=True))['streams'][0]
    checks={'saved_bytes_match_result':sha(movie)==result['sha256'],
        'dimensions':(decoded['width'],decoded['height'])==(width,height),
        'frames_and_duration':int(decoded['nb_read_frames'])==seconds*30 and float(decoded['duration'])==seconds and decoded['r_frame_rate']=='30/1',
        'source_index_unchanged':result['source_index_sha256']==sha(source/'trajectory/index.json'),
        'same_retained_horizon':result['start_time']==0 and result['end_time']==index['end_time'],
        'no_scientific_rerun':result['scientific_rerun'] is False,
        'exact_endpoint_states':result['endpoint_frames'][0]['source_frame']==0 and result['endpoint_frames'][-1]['source_frame']==5000}
    for name,frame in [('first',0),('last',seconds*30-1)]:
        target=OUT/f'{height}p-decoded-{name}.png'
        subprocess.run([str(BIN/'ffmpeg.exe'),'-hide_banner','-loglevel','error','-y','-i',str(movie),'-vf',f'select=eq(n\\,{frame})','-frames:v','1',str(target)],check=True)
    report['cases'].append({'job_id':ident,'saved_path':str(movie),'sha256':sha(movie),'decoder':decoded,'checks':checks,'render_elapsed_seconds':result['elapsed_seconds']})
report['checks']={'source_numerics_still_match_recovery':report['source_result_sha256']=='2c2d3e495d39a4c8753a34a6b57f36bef3acad222bfc9d1343463f86ff0c242e',
    'all_export_checks':all(all(case['checks'].values()) for case in report['cases'])}
report['passed']=all(report['checks'].values())
(OUT/'report.json').write_text(json.dumps(report,indent=2)+'\n',encoding='utf-8')
print(json.dumps(report,indent=2))
if not report['passed']:raise SystemExit(1)
