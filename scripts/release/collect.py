"""Create a compact reviewable candidate set. Never creates a GitHub release."""
import datetime, hashlib, json, os, pathlib, platform, re, shutil, zipfile
ROOT=pathlib.Path(__file__).resolve().parents[2]

def load(file):return json.loads(file.read_text(encoding='utf-8-sig'))
def digest(file):
    with file.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def write(file,value):file.write_text(json.dumps(value,indent=2)+'\n',encoding='utf8')

def release_target(system=None,architecture=None):
    system=platform.system() if system is None else system
    architecture=(platform.machine() if architecture is None else architecture).lower()
    name={'Windows':'windows','Linux':'linux','Darwin':'macos'}.get(system)
    arch={'amd64':'x64','x86_64':'x64','arm64':'arm64','aarch64':'arm64'}.get(architecture)
    if name is None or arch!=('arm64' if name=='macos' else 'x64'):
        raise ValueError('Candidate evidence requires native Windows/Linux x64 or macOS ARM64')
    return {'platform':name,'architecture':arch,'folder':f'{name}-{arch}',
            'node_platform':{'windows':'win32','linux':'linux','macos':'darwin'}[name],
            'installer_format':{'windows':'windows-nsis','linux':'linux-appimage','macos':'macos-dmg'}[name],
            'installer_suffix':{'windows':'-setup.exe','linux':'.AppImage','macos':'.dmg'}[name]}

def validated_inputs(folder,target,version,commit):
    if not re.fullmatch(r'[a-f0-9]{40}',commit):raise ValueError('Expected one full immutable source commit')
    acceptance=load(folder/'NATIVE_ACCEPTANCE.json');review=load(folder/'checks/SCAN_REVIEW.json')
    if acceptance.get('passed') is not True or review.get('tools_completed') is not True:raise ValueError('Incomplete native/scanner execution cannot produce a final candidate')
    if acceptance['source_commit']!=commit or acceptance['version']!=version or review['source_commit']!=commit:raise ValueError('Candidate source or version mismatch')
    if acceptance['platform']!=target['platform'] or acceptance['architecture']!=target['architecture']:raise ValueError('Native acceptance target mismatch')
    host=acceptance['host']
    if release_target(host['system'],host['machine'])['folder']!=target['folder']:raise ValueError('Observed native host target mismatch')
    before=load(ROOT/'.local/marketplace/source/before.json');after=load(ROOT/'.local/marketplace/source/after.json')
    for record in (before,after):
        if record['source']['commit']!=commit or record['source']['dirty'] is not False:raise ValueError('Source material receipt is stale or dirty')
        if record['workflow']['run_id']!=os.environ['GITHUB_RUN_ID'] or record['workflow']['run_attempt']!=os.environ['GITHUB_RUN_ATTEMPT']:raise ValueError('Source material receipt belongs to another workflow run')
    if before['materials']!=after['materials']:raise ValueError('Source materials changed during packaging')
    installed=load(folder/'installed-payload.json');compiled=load(folder/'compiled-dependencies.json');frontend=load(folder/'frontend-modules.json')
    for record in (installed,compiled,frontend):
        if record['source_commit']!=commit:raise ValueError('Installed material receipt belongs to another source')
    if installed['version']!=version or installed['backend']['machine']['architecture']!=target['architecture']:raise ValueError('Installed backend target or version mismatch')
    expected_format={'windows':'pe','linux':'elf','macos':'macho'}[target['platform']]
    if installed['backend']['machine']['format']!=expected_format or installed['backend']['sha256']!=compiled['binary_sha256']:raise ValueError('Installed compiler/binary identity mismatch')
    expected_resources='Contents/Resources' if target['platform']=='macos' else 'resources'
    if installed['resources_relative']!=expected_resources:raise ValueError('Installed resources layout mismatch')
    observed=None
    for index in range(1,4):
        launch=load(folder/f'launch-{index}/runtime.json');result=load(folder/f'launch-{index}/result.json');build=launch['build'];runtime=launch['runtime']
        if build['sourceCommit']!=commit or build['sourceDirty'] is not False or build['version']!=version:raise ValueError('Launch build identity is stale or dirty')
        if build['platform']!=target['node_platform'] or build['architecture']!=target['architecture'] or runtime['platform']!=target['node_platform'] or runtime['architecture']!=target['architecture']:raise ValueError('Actual launch target mismatch')
        if runtime['appVersion']!=version or launch['health']['version']!=version or result['source_commit']!=commit or result['version']!=version or result['passed'] is not True:raise ValueError('Actual launch evidence mismatch')
        if index==1:observed=launch
    artifact=acceptance['artifact'];name=artifact['name']
    if not isinstance(name,str) or '/' in name or '\\' in name or pathlib.Path(name).name!=name or not name.endswith(target['installer_suffix']):raise ValueError('Invalid native installer name or format')
    if target['platform']=='windows':
        wrapper=load(folder/'installer-wrapper.json')
        if wrapper.get('completed') is not True or wrapper['source_commit']!=commit or wrapper['installer']!=artifact:raise ValueError('Installer wrapper coverage is incomplete or stale')
        if review.get('installer_wrapper',{}).get('completed') is not True or review['installer_wrapper']['installer_sha256']!=artifact['sha256']:raise ValueError('Installer wrapper raw scans are missing')
        for name in ('installer-wrapper','installer-native'):
            raw=load(folder/f'checks/grype-{name}.stdout')
            if not isinstance(raw.get('matches'),list) or load(folder/f'checks/grype-{name}.command.json')['exit_code']!=0:raise ValueError('Installer wrapper scan did not complete')
    if target['platform']=='macos':
        backend_signing=load(folder/'backend-signing.json')
        if backend_signing['source_commit']!=commit or backend_signing['staged_signed_sha256']!=installed['backend']['sha256'] or backend_signing['compiler_dependency_section_unchanged'] is not True or backend_signing['verification']['status']!=0:raise ValueError('Signed backend provenance is stale or unverified')
        mac=acceptance['macos_installation']
        if mac.get('copy_exact') is not True or mac.get('detached') is not True:raise ValueError('DMG copy/detach evidence is incomplete')
        signature=mac['signature']
        if signature['signature_observation'] not in ('ad-hoc','unsigned','signature-present-see-raw-observation','undetermined'):raise ValueError('Unknown code-signing observation')
        for key in ('display','verify','gatekeeper_assessment','quarantine_attribute'):
            command=signature[key]
            if type(command.get('status')) is not int or not isinstance(command.get('stdout'),str) or not isinstance(command.get('stderr'),str):raise ValueError('Missing raw code-signing observation')
        if signature['signature_observation']!='ad-hoc' or signature['display']['status']!=0 or signature['verify']['status']!=0:raise ValueError('Completed macOS candidate requires a verified ad-hoc application seal')
    return acceptance,review,observed,installed,frontend

def signing_observation(acceptance,target):
    if target['platform']!='macos':return 'Unsigned; no OS signing credential is used by this workflow. Detached GitHub OIDC authenticates build origin, not a security verdict.',None
    signature=acceptance['macos_installation']['signature'];kind=signature['signature_observation']
    observed={'signature_observation':kind,'ad_hoc_code_integrity_verified':kind=='ad-hoc' and signature['verify']['status']==0,
              'codesign_verify_status':signature['verify']['status'],'gatekeeper_assessment_status':signature['gatekeeper_assessment']['status'],
              'developer_id_signing_performed':False,'notarization_performed':False,
              'evidence':'NATIVE_ACCEPTANCE.json macos_installation.signature; raw codesign, spctl and quarantine observations',
              'scope':'Ad-hoc signing checks code integrity, not publisher identity. Direct instrumented launch does not certify Finder/Gatekeeper handling of quarantined internet downloads.'}
    return f'Observed macOS signature: {kind}; codesign verification status {signature["verify"]["status"]}. No Developer ID signing or notarization was performed. Detached GitHub OIDC authenticates build origin, not OS trust or security admission.',observed

def main():
    target=release_target();system=target['platform'];architecture=target['architecture']
    folder=ROOT/'.local/marketplace'/target['folder']
    version=load(ROOT/'desktop/package.json')['version'];commit=os.environ['GITHUB_SHA']
    acceptance,review,observed,installed,frontend_modules=validated_inputs(folder,target,version,commit)
    final=folder/'candidate';final.mkdir()
    prefix=f'PhaseForge_{version}_{system}_{architecture}'
    installer=ROOT/'desktop/dist'/acceptance['artifact']['name']
    if installer.name!=acceptance['artifact']['name'] or installer.is_symlink() or not installer.is_file():raise ValueError('Invalid installer')
    if not 0<installer.stat().st_size<=2_000_000_000 or installer.stat().st_size!=acceptance['artifact']['bytes'] or digest(installer)!=acceptance['artifact']['sha256']:raise ValueError('Installer bytes changed after native acceptance')
    shutil.copyfile(installer,final/installer.name)
    shutil.copyfile(folder/'checks/source.cdx.json',final/f'{prefix}_source.cdx.json')
    # Preserve each inventory's scope. No transitive completeness or CVE waiver is inferred by merging.
    components=[]
    inventories=[('payload',folder/'checks/payload.cdx.json'),('asar',folder/'checks/asar.cdx.json'),('observed-runtime',folder/'runtime-observations.cdx.json')]
    if system=='windows':inventories.extend([('installer-wrapper',folder/'checks/installer-wrapper.cdx.json'),('installer-native',folder/'installer-native.cdx.json')])
    for scope,file in inventories:
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
    files=[{'name':file.name,'bytes':file.stat().st_size,'sha256':digest(file)} for file in sorted(final.iterdir())]
    manifest={'schema':'phaseforge.candidate-evidence.v1','version':version,'channel':'beta','intended_prerelease':True,'repository':'delores88/PhaseForge','source_commit':commit,'workflow':{'path':'.github/workflows/marketplace-candidates.yml','commit':commit,'run_id':os.environ['GITHUB_RUN_ID'],'attempt':os.environ['GITHUB_RUN_ATTEMPT'],'url':f'https://github.com/delores88/PhaseForge/actions/runs/{os.environ["GITHUB_RUN_ID"]}'},'platform':system,'architecture':architecture,'installer_format':target['installer_format'],'native_acceptance':acceptance,'runtime_versions':observed['runtime']['versions'],'backend_sqlite':observed['health']['sqlite'],'installed_payload_bytes':installed['bytes'],'backend_sha256':installed['backend']['sha256'],'files':files,'security':{'scanner_tools_completed':True,'security_review_required':True,'unreviewed_high_critical_count':len(review['unreviewed_high_critical']),'secret_findings_count':len(review['secret_findings']),'marketplace_admitted':False},'os_code_signing':'Unsigned; no OS signing credential is used by this workflow. Detached GitHub OIDC authenticates build origin, not a security verdict.','limits':['This is build/native evidence, not the DeloresAI submission manifest or signed marketplace review.','Only the recorded hosted OS version, architecture and disposable workflows were tested.','Runtime RAM/disk observations do not certify minimum requirements.','No release is published by this workflow.']}
    # The marketplace accepts beta as its prerelease channel; alpha remains in the version.
    manifest['os_code_signing'],mac_signing=signing_observation(acceptance,target)
    if mac_signing is not None:manifest['macos_code_signing']=mac_signing
    manifest['observed_native_host']=acceptance['host']
    manifest['security']['runtime_identity_gaps']=review.get('runtime_identity_gaps',[])
    manifest['security']['inventory_complete']=False
    if system=='windows':manifest['installer_wrapper_evidence']=review['installer_wrapper']
    manifest['frontend_module_evidence']={'summary':frontend_modules['summary'],'gaps':frontend_modules['gaps'],'evidence':'frontend-modules.json in the checks archive; identical evidence is packaged in resources/runtime'}
    write(final/f'{prefix}_BUILD_MANIFEST.json',manifest)
    subjects=sorted(file.name for file in final.iterdir() if file.is_file())
    checksum=final/f'{prefix}_SHA256SUMS.txt'
    checksum.write_text(''.join(f'{digest(final/name)}  {name}\n' for name in subjects),encoding='utf8')
    subjects.append(checksum.name)
    write(folder/'attestation-subjects.json',subjects)
    print(json.dumps({'candidate_directory':str(final),'subject_count':len(subjects),'security_review_required':True}))

if __name__=='__main__':main()
