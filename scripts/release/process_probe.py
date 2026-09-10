"""Bounded process identity/resource receipts for the disposable native release job."""
import argparse, json, os, pathlib, platform, sys
import psutil

def record(process):
    with process.oneshot():
        return {'pid':process.pid,'created':process.create_time(),'exe':process.exe(),
                'rss_bytes':process.memory_info().rss}

def same_process(item):
    try:
        process=psutil.Process(item['pid'])
        if process.create_time()!=item['created'] or process.status()==psutil.STATUS_ZOMBIE:return None
        return process
    except psutil.NoSuchProcess:return None

def main():
    parser=argparse.ArgumentParser();parser.add_argument('mode',choices=['snapshot','alive','cleanup','host'])
    parser.add_argument('--pid',type=int);parser.add_argument('--created',type=float);parser.add_argument('--identities',type=pathlib.Path);args=parser.parse_args()
    if args.mode=='host':
        result={'system':platform.system(),'release':platform.release(),'version':platform.version(),
                'machine':platform.machine(),'total_ram_bytes':psutil.virtual_memory().total,
                'glibc':platform.libc_ver(),'python':sys.version,'psutil':psutil.__version__}
        if pathlib.Path('/etc/os-release').is_file():result['os_release']=pathlib.Path('/etc/os-release').read_text()
    elif args.mode=='snapshot':
        result=[]
        try:
            owner=psutil.Process(args.pid)
            if args.created is not None and owner.create_time()!=args.created:
                print('[]');return
            processes=[owner,*owner.children(recursive=True)]
        except psutil.NoSuchProcess:processes=[]
        for process in processes:
            try:result.append(record(process))
            except psutil.NoSuchProcess:pass
    else:
        records=json.loads(args.identities.read_text());result=[]
        if not isinstance(records,list) or len(records)>256:raise ValueError('Invalid owned process inventory')
        for item in records:
            process=same_process(item)
            if process is None:continue
            if args.mode=='cleanup':
                if os.environ.get('GITHUB_ACTIONS')!='true' or os.environ.get('PHASEFORGE_RELEASE_ACCEPTANCE')!='1':
                    raise ValueError('Emergency cleanup requires the disposable hosted acceptance job')
                try:
                    result.append(record(process))
                    process.kill()
                except psutil.NoSuchProcess:pass
            else:
                try:result.append(record(process))
                except psutil.NoSuchProcess:pass
    print(json.dumps(result))

if __name__=='__main__':main()
