"""Fetch only hash-pinned official release executables, without running installer scripts."""
import argparse, hashlib, io, json, os, pathlib, platform, subprocess, tarfile, urllib.request, zipfile

ROOT=pathlib.Path(__file__).resolve().parents[2]
LIMIT=256*1024*1024

def binary_member(archive,filename,content):
    if archive.endswith('.zip'):
        with zipfile.ZipFile(io.BytesIO(content)) as package:
            matches=[item for item in package.infolist() if pathlib.PurePosixPath(item.filename).name==filename and not item.is_dir()]
            if len(matches)!=1 or matches[0].file_size>LIMIT:raise ValueError('Expected one bounded tool executable')
            return package.read(matches[0])
    with tarfile.open(fileobj=io.BytesIO(content),mode='r:*') as package:
        matches=[item for item in package.getmembers() if item.isfile() and pathlib.PurePosixPath(item.name).name==filename]
        if len(matches)!=1 or matches[0].size>LIMIT:raise ValueError('Expected one bounded tool executable')
        return package.extractfile(matches[0]).read()

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--only',default='syft,grype,gh,cargo-auditable,gitleaks');parser.add_argument('--github-path',action='store_true');args=parser.parse_args()
    system={'Windows':'windows','Linux':'linux'}.get(platform.system())
    if system is None or platform.machine().lower() not in ('amd64','x86_64'):raise ValueError('ALPHA release tools support native Windows/Linux x64 only')
    pins=json.loads((ROOT/'scripts/release/tool-pins.json').read_text());destination=ROOT/'.local/marketplace/tools';destination.mkdir(parents=True,exist_ok=True)
    if destination.is_symlink() or destination.resolve()!=destination.absolute():raise ValueError('Linked tool directory is refused')
    records=[]
    for name in args.only.split(','):
        if name not in pins:raise ValueError('Unknown pinned release tool')
        specification=pins[name];target=specification[system]
        url=f'https://github.com/{specification["repository"]}/releases/download/{specification["tag"]}/{target["archive"]}'
        with urllib.request.urlopen(url,timeout=90) as response:content=response.read(LIMIT+1)
        if len(content)>LIMIT or hashlib.sha256(content).hexdigest()!=target['sha256']:raise ValueError(f'Official archive digest mismatch: {name}')
        filename=name+('.exe' if system=='windows' else '');binary=destination/filename
        if binary.is_symlink():raise ValueError('Linked tool executable is refused')
        binary.write_bytes(binary_member(target['archive'],filename,content));binary.chmod(0o755)
        arguments=['auditable','--version'] if name=='cargo-auditable' else ['version'] if name=='gitleaks' else ['--version']
        version=subprocess.check_output([str(binary),*arguments],text=True,timeout=30).strip()
        # cargo-auditable forwards --version to Cargo; it has no own version switch.
        # Its exact tool version is bound by the official release archive digest.
        if name!='cargo-auditable' and specification['version'] not in version:raise ValueError(f'Unexpected tool version: {version}')
        if name=='cargo-auditable' and not version.startswith('cargo '):raise ValueError('cargo-auditable forwarding probe failed')
        records.append({'name':name,'version':specification['version'],'platform':system,'url':url,'archive_sha256':target['sha256'],'executable_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'version_output':version,'version_binding':'Official release archive SHA-256; cargo-auditable probe reports Cargo, not its own version' if name=='cargo-auditable' else 'Pinned archive SHA-256 and executable version output'})
    (destination/'tool-receipts.json').write_text(json.dumps(records,indent=2)+'\n')
    if args.github_path:
        if os.environ.get('GITHUB_ACTIONS')!='true' or not os.environ.get('GITHUB_PATH'):raise ValueError('GITHUB_PATH is available only in the hosted workflow')
        with open(os.environ['GITHUB_PATH'],'a',encoding='utf8') as stream:stream.write(str(destination)+'\n')
    print(json.dumps({'installed':[record['name'] for record in records]}))

if __name__=='__main__':main()
