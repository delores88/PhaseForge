#!/usr/bin/env python3
"""Verify all exported payload bytes before using a PhaseForge research bundle."""
from pathlib import Path
import argparse,hashlib,json

def verify(root:Path)->list[str]:
    root=root.resolve();document=json.loads((root/'checksums.json').read_text(encoding='utf-8'))
    failures=[]
    for name,wanted in document.items():
        path=(root/name).resolve()
        if not path.is_relative_to(root) or not path.is_file():failures.append(name+': missing or unsafe');continue
        if hashlib.sha256(path.read_bytes()).hexdigest()!=wanted:failures.append(name+': checksum mismatch')
    for path in root.rglob('*'):
        if path.is_file() and path.relative_to(root).as_posix() not in document and path.name!='checksums.json':failures.append(path.relative_to(root).as_posix()+': unlisted file')
    return failures
if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('folder',nargs='?',default='.');args=parser.parse_args()
    errors=verify(Path(args.folder))
    print('\n'.join(errors) if errors else 'PASS: all listed payloads match their SHA-256 digests; no unlisted files. This is integrity checking, not scientific validation.')
    raise SystemExit(1 if errors else 0)
