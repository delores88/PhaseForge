"""Passive NSIS outer-member evidence, including temporary installer plugins."""
import ctypes, hashlib, json, os, pathlib, re, shutil, struct, subprocess, time

MAX_FILE=1_000_000_000
MAX_TOTAL=2_000_000_000
MAX_ENTRIES=10000
READER_HASHES={'7z.exe':'c7245e21a7553d9e52d434002a401c77a7ca7d0f245f2311b0ddf16f8f946c6f',
               '7z.dll':'9ed007aa82e440ceb39a6e105bb1d602a9bc59a4946267ba8de2f220aa15bc06'}

def digest(file):
    with file.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()

def member_path(value):
    value=value.replace('\\','/')
    parts=value.split('/')
    if not value or len(value)>4096 or any(ord(c)<32 for c in value) or ':' in value:
        raise ValueError('Invalid installer member path')
    for part in parts:
        if part in ('','.','..') or part[-1:] in (' ','.') or re.fullmatch(r'(?i)(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?',part):
            raise ValueError('Unsafe installer member path')
    return '/'.join(parts)

def parse_listing(text):
    header,separator,body=text.partition('----------')
    if not separator or not re.search(r'^Type = Nsis\s*$',header,re.M):raise ValueError('Expected an actual NSIS archive listing')
    entries=[];seen=set()
    for block in re.split(r'\r?\n\s*\r?\n',body.strip()):
        row={}
        for line in block.splitlines():
            key,sep,value=line.partition(' = ')
            if sep:
                if key in row:raise ValueError('Duplicate installer listing field')
                row[key]=value
        if not row:continue
        original=row['Path'];relative=member_path(original)
        if relative.casefold() in seen:raise ValueError('Duplicate or aliased installer member')
        seen.add(relative.casefold())
        if any(row.get(key) for key in ('Symbolic Link','Hard Link')) or 'L' in row.get('Attributes',''):
            raise ValueError('Linked installer members are refused')
        if row.get('Folder')=='+' or 'D' in row.get('Attributes',''):continue
        size=int(row['Size']) if row.get('Size') else None
        if size is not None and not 0<=size<=MAX_FILE:raise ValueError('Installer member size exceeds budget')
        entries.append({'member':original,'path':relative,'declared_bytes':size})
        if len(seen)>MAX_ENTRIES:raise ValueError('Installer member count exceeds budget')
    if not entries:raise ValueError('Empty installer wrapper inventory')
    return entries

def run_reader(executable,args,output,error,limit):
    """Never allow the archive reader to create paths; stream stdout to our file."""
    with output.open('xb') as stdout,error.open('xb') as stderr:
        child=subprocess.Popen([str(executable),*args],stdin=subprocess.DEVNULL,stdout=stdout,stderr=stderr,
                               creationflags=0x08000000 if os.name=='nt' else 0)
        try:
            deadline=time.monotonic()+180
            while True:
                if output.stat().st_size>limit or error.stat().st_size>1_000_000:raise ValueError('Installer extraction output exceeds budget')
                if time.monotonic()>deadline:raise TimeoutError('Installer archive reader deadline exceeded')
                try:code=child.wait(timeout=.1);break
                except subprocess.TimeoutExpired:pass
            if code or output.stat().st_size>limit:raise ValueError('Installer archive read failed; diagnostics retained')
        finally:
            if child.poll() is None:child.kill()
            child.wait(timeout=10)

def extract_members(archive,destination,reader,evidence,runner=run_reader):
    if destination.exists() or destination.is_symlink():raise ValueError('Installer scan directory must be new')
    destination.mkdir(parents=True);evidence.mkdir(parents=True,exist_ok=True)
    listing=evidence/'installer-wrapper-listing.txt'
    runner(reader,['l','-slt','-sccUTF-8','--',str(archive)],listing,evidence/'installer-wrapper-listing.stderr',8_000_000)
    entries=parse_listing(listing.read_text(encoding='utf-8-sig'));records=[];total=0
    for index,entry in enumerate(entries):
        target=destination/'members'/entry['path'];target.parent.mkdir(parents=True,exist_ok=True)
        runner(reader,['x','-so','-y','-spd','--',str(archive),entry['member']],target,evidence/f'installer-member-{index}.stderr',min(MAX_FILE,MAX_TOTAL-total))
        size=target.stat().st_size;total+=size
        if total>MAX_TOTAL or entry['declared_bytes'] is not None and size!=entry['declared_bytes']:raise ValueError('Installer member size differs from listing')
        records.append({**entry,'path':target.relative_to(destination).as_posix(),'bytes':size,'sha256':digest(target)})
    # The executable wrapper itself is a distinct shipped PE component too.
    container=destination/'container'/archive.name;container.parent.mkdir()
    if archive.stat().st_size+total>MAX_TOTAL:raise ValueError('Full installer scan exceeds budget')
    shutil.copyfile(archive,container)
    records.append({'member':None,'path':container.relative_to(destination).as_posix(),'bytes':container.stat().st_size,'sha256':digest(container)})
    return records

def pe_version(file):
    """Windows version-resource API reads data only; never load/execute a member."""
    if os.name!='nt':raise ValueError('PE version observations require the native Windows evidence host')
    with file.open('rb') as stream:
        if stream.read(2)!=b'MZ':return None
    api=ctypes.WinDLL('version',use_last_error=True);size_fn=api.GetFileVersionInfoSizeW
    size_fn.argtypes=[ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_uint32)];size_fn.restype=ctypes.c_uint32
    ignored=ctypes.c_uint32();size=size_fn(str(file),ctypes.byref(ignored))
    if not size:return {'status':'no_version_resource'}
    if size>16*1024*1024:raise ValueError('PE version resource exceeds budget')
    buffer=ctypes.create_string_buffer(size);get=api.GetFileVersionInfoW
    get.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p];get.restype=ctypes.c_int
    if not get(str(file),0,size,buffer):raise OSError('Unable to read PE version resource')
    query=api.VerQueryValueW;query.argtypes=[ctypes.c_void_p,ctypes.c_wchar_p,ctypes.POINTER(ctypes.c_void_p),ctypes.POINTER(ctypes.c_uint32)];query.restype=ctypes.c_int
    def value(key):
        pointer=ctypes.c_void_p();length=ctypes.c_uint32()
        if not query(buffer,key,ctypes.byref(pointer),ctypes.byref(length)):return None,0
        start=ctypes.addressof(buffer);offset=pointer.value-start
        if offset<0 or offset>=size:raise ValueError('PE version query exceeds its resource')
        return pointer.value,min(length.value,size-offset)
    pointer,length=value('\\');fixed={}
    if pointer and length>=52:
        words=struct.unpack('<13I',ctypes.string_at(pointer,52))
        if words[0]!=0xFEEF04BD:raise ValueError('Invalid PE version signature')
        fixed={'file_version_parts':[words[2]>>16,words[2]&65535,words[3]>>16,words[3]&65535],
               'product_version_parts':[words[4]>>16,words[4]&65535,words[5]>>16,words[5]&65535]}
    pointer,length=value('\\VarFileInfo\\Translation');translations=[]
    if pointer:
        if length%4:raise ValueError('Invalid PE version translation')
        translations=list(struct.iter_unpack('<HH',ctypes.string_at(pointer,length)))
    strings=[]
    for language,codepage in translations[:64]:
        row={'language':language,'codepage':codepage}
        for key in ['ProductName','ProductVersion','FileVersion','FileDescription','OriginalFilename','CompanyName']:
            pointer,length=value(f'\\StringFileInfo\\{language:04x}{codepage:04x}\\{key}')
            if pointer:
                # String query lengths count UTF-16 code units, including NUL.
                offset=pointer-ctypes.addressof(buffer)
                if length*2>size-offset:raise ValueError('PE version string exceeds its resource')
                row[key]=ctypes.wstring_at(pointer,length).rstrip('\0')
        strings.append(row)
    return {'status':'observed_version_resource',**fixed,'strings':strings}

def version_components(files,observations):
    components=[]
    for file,version in zip(files,observations):
        if not version or version.get('status')!='observed_version_resource':continue
        for strings in version['strings']:
            identity=' '.join(strings.get(key,'') for key in ['ProductName','FileDescription'])
            if not re.search(r'(?i)\b7-zip\b',identity):continue
            match=re.fullmatch(r'(\d+)\.(\d{1,2})(?:\.0\.0)?',strings.get('ProductVersion','').strip())
            if not match:continue
            parts=version.get('product_version_parts');numeric=[int(match[1]),int(match[2])]
            if not parts or parts[:2]!=numeric:raise ValueError('7-Zip textual and fixed product versions disagree')
            normalized=f'{numeric[0]}.{numeric[1]:02d}'
            components.append({'type':'library','name':'7-Zip','version':normalized,'bom-ref':'installer-member:'+file['path'],
                               'cpe':f'cpe:2.3:a:7-zip:7-zip:{normalized}:*:*:*:*:*:*:*',
                               'hashes':[{'alg':'SHA-256','content':file['sha256']}],
                               'properties':[{'name':'phaseforge:installer-member','value':file['path']},
                                             {'name':'syft:location:0:path','value':file['path']},
                                             {'name':'phaseforge:scope','value':'Actual outer-installer PE version resource, including temporary NSIS plugins; not an installed application file.'}]})
            break
    return components

def prepare_windows_installer(root,folder,commit):
    acceptance=json.loads((folder/'NATIVE_ACCEPTANCE.json').read_bytes());artifact=acceptance['artifact'];name=artifact['name']
    if acceptance['source_commit']!=commit or pathlib.Path(name).name!=name or '/' in name or '\\' in name:raise ValueError('Installer identity mismatch')
    archive=root/'desktop/dist'/name
    if archive.is_symlink() or archive.stat().st_size!=artifact['bytes'] or digest(archive)!=artifact['sha256']:raise ValueError('Installer changed after native acceptance')
    vendor=root/'desktop/node_modules/electron-winstaller';lock=json.loads((root/'desktop/package-lock.json').read_bytes())['packages']['node_modules/electron-winstaller']
    if lock['version']!='5.4.0' or json.loads((vendor/'package.json').read_bytes())['version']!='5.4.0':raise ValueError('Unexpected pinned archive-reader package')
    for name,expected in READER_HASHES.items():
        if (vendor/'vendor'/name).is_symlink() or digest(vendor/'vendor'/name)!=expected:raise ValueError('Pinned archive-reader bytes differ')
    destination=folder/'installer-wrapper';files=extract_members(archive,destination,vendor/'vendor/7z.exe',folder/'checks')
    versions=[pe_version(destination/file['path']) for file in files]
    for file,version in zip(files,versions):file['pe_version']=version
    plugins=[file for file in files if pathlib.PurePosixPath(file['path']).name.casefold()=='nsis7z.dll']
    components=version_components(files,versions)
    if any(not any(c['bom-ref']=='installer-member:'+plugin['path'] for c in components) for plugin in plugins):raise ValueError('Present NSIS decompression plugin lacks explicit version coverage')
    scope='Full original NSIS container and all listed outer members, including temporary plugins and directly embedded application files. Installed payload/ASAR bytes are also scanned separately; no installer code executed. No nested archive or decompression plugin is required to exist.'
    receipt={'schema':'phaseforge.installer-wrapper.v1','source_commit':commit,'installer':artifact,'completed':True,'scope':scope,'files':files,
             'reader':{'package':'electron-winstaller','version':'5.4.0','lock_integrity':lock['integrity'],'executable_version':'7-Zip16.04','file_sha256':READER_HASHES},
             'limits':['Version resources do not identify every third-party library inside every plugin. Unknown identities remain visible in inventory; this is not a complete native-component certification.']}
    (folder/'installer-wrapper.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8')
    bom={'bomFormat':'CycloneDX','specVersion':'1.6','version':1,'metadata':{'properties':[{'name':'phaseforge:source-commit','value':commit},{'name':'phaseforge:scope','value':scope}]},'components':components}
    (folder/'installer-native.cdx.json').write_text(json.dumps(bom,indent=2)+'\n',encoding='utf-8')
    return destination
