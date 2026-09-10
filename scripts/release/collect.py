"""Create a compact reviewable candidate set. Never creates a GitHub release."""
import datetime, hashlib, json, os, pathlib, platform, shutil, zipfile
ROOT=pathlib.Path(__file__).resolve().parents[2]

def load(file):return json.loads(file.read_text(encoding='utf-8-sig'))
def digest(file):
    with file.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def write(file,value):file.write_text(json.dumps(value,indent=2)+'\n',encoding='utf8')

def main():
    system='windows' if platform.system()=='Windows' else 'linux'
    folder=ROOT/'.local/marketplace'/f'{system}-x64'
    acceptance=load(folder/'NATIVE_ACCEPTANCE.json');review=load(folder/'checks/SCAN_REVIEW.json')
    if not acceptance.get('passed') or not review.get('tools_completed'):raise ValueError('Incomplete native/scanner execution cannot produce a final candidate')
    version=load(ROOT/'desktop/package.json')['version'];commit=os.environ['GITHUB_SHA']
    if acceptance['source_commit']!=commit or acceptance['version']!=version or review['source_commit']!=commit:raise ValueError('Candidate source or version mismatch')
    final=folder/'candidate';final.mkdir()
    prefix=f'PhaseForge_{version}_{system}_x64'
    installer=ROOT/'desktop/dist'/acceptance['artifact']['name']
    if installer.name!=acceptance['artifact']['name'] or installer.is_symlink() or not installer.is_file():raise ValueError('Invalid installer')
    if not 0<installer.stat().st_size<=2_000_000_000 or digest(installer)!=acceptance['artifact']['sha256']:raise ValueError('Installer bytes changed after native acceptance')
    shutil.copyfile(installer,final/installer.name)
    shutil.copyfile(folder/'checks/source.cdx.json',final/f'{prefix}_source.cdx.json')
    # Preserve each inventory's scope. No transitive completeness or CVE waiver is inferred by merging.
    components=[]
    for scope,file in [('payload',folder/'checks/payload.cdx.json'),('asar',folder/'checks/asar.cdx.json'),('observed-runtime',folder/'runtime-observations.cdx.json')]:
        for index,component in enumerate(load(file).get('components',[])):
            item=dict(component);item['bom-ref']=f'{scope}:{index}:'+item.get('bom-ref',item.get('name','component'))
            item['properties']=[*item.get('properties',[]),{'name':'phaseforge:inventory-scope','value':scope}];components.append(item)
    write(final/f'{prefix}_payload.cdx.json',{'bomFormat':'CycloneDX','specVersion':'1.6','version':1,'metadata':{'timestamp':datetime.datetime.now(datetime.timezone.utc).isoformat(),'component':{'type':'application','name':'PhaseForge','version':version},'properties':[{'name':'phaseforge:source-commit','value':commit},{'name':'phaseforge:scope','value':'Combined scanner observations and observed runtimes. Raw original BOMs, .dep-v0 compiler inventory and scope explanations remain in checks ZIP. No complete coverage claim.'}]},'components':components})
    checks=final/f'{prefix}_checks.zip'
    with zipfile.ZipFile(checks,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=6) as archive:
        files=[file for file in (folder/'checks').rglob('*') if file.is_file()]
        files.extend(file for file in folder.glob('*.json') if file.is_file())
        files.extend(file for file in folder.glob('launch-*/*') if file.is_file() and file.suffix in ('.json','.png'))
        for file in sorted(files):
            if file.is_symlink() or not file.resolve().is_relative_to(folder.resolve()):raise ValueError('Linked evidence file refused')
            archive.write(file,'evidence/'+file.relative_to(folder).as_posix())
        for name in ('before.json','after.json'):
            archive.write(ROOT/'.local/marketplace/source'/name,'source/'+name)
        archive.write(ROOT/'scripts/release/tool-pins.json','source/tool-pins.json')
    observed=load(folder/'launch-1/runtime.json');installed=load(folder/'installed-payload.json')
    files=[{'name':file.name,'bytes':file.stat().st_size,'sha256':digest(file)} for file in sorted(final.iterdir())]
    manifest={'schema':'phaseforge.candidate-evidence.v1','version':version,'channel':'beta','intended_prerelease':True,'repository':'delores88/PhaseForge','source_commit':commit,'workflow':{'path':'.github/workflows/marketplace-candidates.yml','commit':commit,'run_id':os.environ['GITHUB_RUN_ID'],'attempt':os.environ['GITHUB_RUN_ATTEMPT'],'url':f'https://github.com/delores88/PhaseForge/actions/runs/{os.environ["GITHUB_RUN_ID"]}'},'platform':system,'architecture':'x64','native_acceptance':acceptance,'runtime_versions':observed['runtime']['versions'],'backend_sqlite':observed['health']['sqlite'],'installed_payload_bytes':installed['bytes'],'backend_sha256':installed['backend']['sha256'],'files':files,'security':{'scanner_tools_completed':True,'security_review_required':True,'unreviewed_high_critical_count':len(review['unreviewed_high_critical']),'secret_findings_count':len(review['secret_findings']),'marketplace_admitted':False},'os_code_signing':'Unsigned; no OS signing credential is used by this workflow. Detached GitHub OIDC authenticates build origin, not a security verdict.','limits':['This is build/native evidence, not the DeloresAI submission manifest or signed marketplace review.','Only the recorded hosted OS version, architecture and disposable workflows were tested.','Runtime RAM/disk observations do not certify minimum requirements.','No release is published by this workflow.']}
    manifest['security']['runtime_identity_gaps']=review.get('runtime_identity_gaps',[])
    manifest['security']['inventory_complete']=False
    frontend_modules=load(folder/'frontend-modules.json')
    manifest['frontend_module_evidence']={'summary':frontend_modules['summary'],'gaps':frontend_modules['gaps'],'evidence':'frontend-modules.json in the checks archive; identical evidence is packaged in resources/runtime'}
    write(final/f'{prefix}_BUILD_MANIFEST.json',manifest)
    subjects=sorted(file.name for file in final.iterdir() if file.is_file())
    checksum=final/f'{prefix}_SHA256SUMS.txt'
    checksum.write_text(''.join(f'{digest(final/name)}  {name}\n' for name in subjects),encoding='utf8')
    subjects.append(checksum.name)
    write(folder/'attestation-subjects.json',subjects)
    print(json.dumps({'candidate_directory':str(final),'subject_count':len(subjects),'security_review_required':True}))

if __name__=='__main__':main()
