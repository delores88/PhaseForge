"""Retain raw findings and inventory coverage. This is not marketplace admission."""
import datetime, hashlib, json, os, pathlib, platform, shutil, subprocess
from collect import release_target
from installer_wrapper import prepare_windows_installer

ROOT=pathlib.Path(__file__).resolve().parents[2]
TARGET=release_target();SYSTEM=TARGET['platform'];ARCHITECTURE=TARGET['architecture']
FOLDER=ROOT/'.local/marketplace'/TARGET['folder']
TOOLS=ROOT/'.local/marketplace/tools'
REPORTS=FOLDER/'checks'

def write(name,value):
    (REPORTS/name).write_text(json.dumps(value,indent=2)+'\n',encoding='utf8')

def invoke(executable,args,name,allowed=(0,),env=None,cwd=ROOT):
    result=subprocess.run([str(executable),*map(str,args)],cwd=cwd,env=env,capture_output=True,timeout=1200)
    (REPORTS/(name+'.stdout')).write_bytes(result.stdout)
    (REPORTS/(name+'.stderr')).write_bytes(result.stderr)
    write(name+'.command.json',{'tool':pathlib.Path(executable).name,'arguments':list(map(str,args)),'exit_code':result.returncode})
    if result.returncode not in allowed:raise RuntimeError(f'{name} tool failed with exit {result.returncode}; raw output retained')
    return result

def tool(name):return TOOLS/(name+('.exe' if SYSTEM=='windows' else ''))

def main():
    REPORTS.mkdir(parents=True,exist_ok=True)
    if os.environ.get('GITHUB_ACTIONS')!='true':raise RuntimeError('Candidate scans require the isolated hosted source and payload')
    source=FOLDER/'source-materials';source.mkdir()
    # Scan exact tracked inputs; never accidentally scan credentials or local developer files.
    paths=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().split('\0')
    for relative in filter(None,paths):
        original=ROOT/relative;target=source/relative
        if original.is_symlink() or '..' in pathlib.PurePosixPath(relative).parts:raise RuntimeError('Linked source input refused')
        target.parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(original,target)
    syft_env={**os.environ,'SYFT_JAVASCRIPT_INCLUDE_DEV_DEPENDENCIES':'true','SYFT_CHECK_FOR_APP_UPDATE':'false'}
    targets={'source':source,'payload':FOLDER/'payload','asar':FOLDER/'expanded-asar'}
    if SYSTEM=='windows':
        targets['installer-wrapper']=prepare_windows_installer(ROOT,FOLDER,os.environ['GITHUB_SHA'])
    for scope,target in targets.items():
        exclusions=[]
        if scope=='payload':
            # Retain these files in payload hashes, but do not mislabel build inputs or optional engines as installed packages.
            for pattern in ['**/runtime/build-inputs/**','**/runtime/Cargo.lock','**/tools/requirements-cad.txt']:
                exclusions.extend(['--exclude',pattern])
        invoke(tool('syft'),[f'dir:{target}',*exclusions,'-o',f'cyclonedx-json={REPORTS/f"{scope}.cdx.json"}'],'syft-'+scope,env=syft_env)
        document=json.loads((REPORTS/f'{scope}.cdx.json').read_text())
        if scope in ('source','payload') and not document.get('components'):raise RuntimeError(f'Missing {scope} inventory coverage')
        if scope=='payload' and not any(item.get('name')=='rusqlite' for item in document.get('components',[])):
            raise RuntimeError('Syft did not detect compiled Rust rusqlite metadata in the actual installed backend')
        if scope=='payload' and SYSTEM=='windows':
            names={str(item.get('name','')).lower() for item in document.get('components',[])}
            if not {'numpy','openmm','pillow'}.issubset(names):
                raise RuntimeError('Payload inventory omitted bundled scientific package metadata; preserve raw evidence and resolve coverage before candidate assembly')
        if scope=='source' and not any(item.get('name')=='electron' for item in document.get('components',[])):
            raise RuntimeError('Source inventory omitted the Electron build input that becomes shipped runtime')
    grype_env={**os.environ,'GRYPE_DB_CACHE_DIR':str(FOLDER/'grype-db'),'GRYPE_CHECK_FOR_APP_UPDATE':'false','GRYPE_DB_VALIDATE_AGE':'true','GRYPE_DB_MAX_ALLOWED_BUILT_AGE':'120h'}
    invoke(tool('grype'),['db','update'],'grype-db-update',env=grype_env)
    invoke(tool('grype'),['db','status','-o','json'],'grype-db-status',env=grype_env)
    all_matches=[]
    scan_boms=[(scope,REPORTS/f'{scope}.cdx.json') for scope in targets]
    scan_boms.extend([('runtime',FOLDER/'runtime-observations.cdx.json'),('runtime-screening-aliases',FOLDER/'runtime-screening-aliases.cdx.json')])
    if SYSTEM=='windows':
        scan_boms.append(('installer-native',FOLDER/'installer-native.cdx.json'))
        components=json.loads((FOLDER/'runtime-observations.cdx.json').read_text())['components']
        for kind in ('science-v4','python-numpy-v3'):
            rows=[item for item in components if item.get('name')=='sqlite-'+kind]
            if (len(rows)!=1 or rows[0].get('version')!='3.53.4'
                    or rows[0].get('cpe')!='cpe:2.3:a:sqlite:sqlite:3.53.4:*:*:*:*:*:*:*'):
                raise RuntimeError('Managed SQLite DLL lacks exact observed-version scanner coverage: '+kind)
    for scope,bom in scan_boms:
        result=invoke(tool('grype'),[f'sbom:{bom}','-o','json'],'grype-'+scope,env=grype_env)
        data=json.loads(result.stdout);matches=data.get('matches')
        if not isinstance(matches,list):raise RuntimeError('Missing vulnerability matches array')
        all_matches.extend({'scope':scope,'id':m['vulnerability']['id'],'severity':m['vulnerability']['severity'],'artifact':m.get('artifact',{}).get('name'),'version':m.get('artifact',{}).get('version'),'locations':m.get('artifact',{}).get('locations',[])} for m in matches)
    secrets=[]
    for scope,target in targets.items():
        report=REPORTS/f'gitleaks-{scope}.json'
        invoke(tool('gitleaks'),['dir',str(target),'--no-banner','--redact=100','--report-format','json','--report-path',str(report)],'gitleaks-'+scope,allowed=(0,1))
        findings=json.loads(report.read_text())
        secrets.extend({'scope':scope,'rule':item.get('RuleID'),'file':item.get('File'),'line':item.get('StartLine')} for item in findings)
    # Keep build-only and production lock audits distinct. npm.cmd needs a Windows shell only for this fixed command.
    for label,directory in [('frontend',ROOT/'frontend'),('desktop',ROOT/'desktop'),('release-tools',ROOT/'scripts/release')]:
        for scope in ('all','production'):
            args=['npm','audit','--json']+(['--omit=dev'] if scope=='production' else [])
            result=subprocess.run(args,cwd=directory,capture_output=True,timeout=180,shell=SYSTEM=='windows')
            (REPORTS/f'npm-{label}-{scope}.json').write_bytes(result.stdout)
            (REPORTS/f'npm-{label}-{scope}.stderr').write_bytes(result.stderr)
            if result.returncode not in (0,1) or 'metadata' not in json.loads(result.stdout):raise RuntimeError(f'npm audit incomplete for {label}/{scope}')
    high=[m for m in all_matches if m['severity'].lower() in ('high','critical')]
    write('SCAN_REVIEW.json',{'schema':'phaseforge.publisher-scan.v1','source_commit':os.environ['GITHUB_SHA'],'tools_completed':True,'marketplace_admission':False,'security_review_required':True,'unreviewed_high_critical':high,'secret_findings':secrets,'raw_match_count':len(all_matches),'observed_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'limits':['All raw findings retained. No CVE or secret allowlist is applied.','Source SBOM includes development/build inputs, including the Electron runtime declared as a devDependency.','Payload and ASAR inventories are scanner observations, supplemented by actual runtime versions and compiler metadata; not a complete scientific or security certification.','ClamAV and signed marketplace policy admission are required separately before publication.','Exact Electron backport and wrong-platform reviews must bind to these bytes; old package waivers do not transfer.']})
    shutil.copyfile(TOOLS/'tool-receipts.json',REPORTS/'tool-receipts.json')
    review=json.loads((REPORTS/'SCAN_REVIEW.json').read_text())
    review['runtime_identity_gaps']=json.loads((FOLDER/'runtime-identity-gaps.json').read_text())['gaps']
    review['inventory_complete']=False
    if SYSTEM=='windows':
        wrapper=json.loads((FOLDER/'installer-wrapper.json').read_text())
        review['installer_wrapper']={'completed':wrapper['completed'],'installer_sha256':wrapper['installer']['sha256'],'members_including_container':len(wrapper['files']),'inventory':'installer-wrapper.json','native_version_bom':'installer-native.cdx.json','scope':wrapper['scope']}
    write('SCAN_REVIEW.json',review)
    print(json.dumps({'tools_completed':True,'security_review_required':True,'high_critical_findings':len(high),'secret_findings':len(secrets)}))

if __name__=='__main__':
    try:main()
    except Exception as error:
        REPORTS.mkdir(parents=True,exist_ok=True);write('SCAN_FAILURE.json',{'tools_completed':False,'error':str(error)})
        raise
