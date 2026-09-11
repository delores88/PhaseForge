/** Read-only SQLite execution checks for the exact managed runtime copies. */
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {ROOT,command,readJSON,sha256} from './common.mjs';
import {verifySeed} from './stage-seeds.mjs';

export const SQLITE_VERSION='3.53.4';
export const MANAGED_SQLITE_LAYOUT=Object.freeze({'science-v4':'environments/science-v4','python-numpy-v3':'environments/python-numpy-v3/runtime'});
const samePath=(left,right)=>path.resolve(left).toLowerCase()===path.resolve(right).toLowerCase();
export const SQLITE_PROBE=String.raw`
import ctypes,json,sqlite3,sys
from ctypes import wintypes
database=sqlite3.connect(':memory:')
version,source_id=database.execute('SELECT sqlite_version(),sqlite_source_id()').fetchone()
compile_options=sorted(row[0] for row in database.execute('PRAGMA compile_options'))
database.execute('CREATE VIRTUAL TABLE documents USING fts5(title, body)')
database.executemany('INSERT INTO documents VALUES (?,?)',[('retained result','argon numerical evidence'),('other record','diffusion field')])
rows=database.execute("SELECT rowid,title FROM documents WHERE documents MATCH ? ORDER BY rowid",('argon',)).fetchall()
database.close()
kernel=ctypes.WinDLL('kernel32',use_last_error=True)
kernel.GetModuleHandleW.argtypes=[wintypes.LPCWSTR]
kernel.GetModuleHandleW.restype=wintypes.HMODULE
kernel.GetModuleFileNameW.argtypes=[wintypes.HMODULE,wintypes.LPWSTR,wintypes.DWORD]
kernel.GetModuleFileNameW.restype=wintypes.DWORD
handle=kernel.GetModuleHandleW('sqlite3.dll')
if not handle: raise ctypes.WinError(ctypes.get_last_error())
buffer=ctypes.create_unicode_buffer(32768)
count=kernel.GetModuleFileNameW(handle,buffer,len(buffer))
if not 0<count<len(buffer): raise RuntimeError('SQLite loaded path is unavailable')
print(json.dumps({'sqlite_version':version,'sqlite_source_id':source_id,'compile_options':compile_options,'module_version':sqlite3.sqlite_version,'loaded_sqlite_path':buffer.value,'executable':sys.executable,'pointer_bits':ctypes.sizeof(ctypes.c_void_p)*8,'fts5_rows':rows}))
`;

export function validateSqliteObservation(value,{executable,dll}){
  assert.equal(value.sqlite_version,SQLITE_VERSION,'Managed SQLite must be the exact reviewed fixed version');
  assert.equal(value.module_version,SQLITE_VERSION,'Python wrapper and SQL connection versions differ');
  assert.match(value.sqlite_source_id,/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [a-f0-9]{64}$/,'Actual SQLite source ID missing');
  assert.equal(value.pointer_bits,64);
  assert.ok(Array.isArray(value.compile_options)&&value.compile_options.every(option=>typeof option==='string'&&option.length>0)&&value.compile_options.includes('ENABLE_FTS5'),'Actual SQLite compile options/FTS5 capability are missing');
  assert.equal(new Set(value.compile_options).size,value.compile_options.length);
  assert.ok(samePath(value.executable,executable),'Probe used another Python executable');
  assert.ok(samePath(value.loaded_sqlite_path,dll),'SQLite resolved outside this exact managed runtime');
  assert.deepEqual(value.fts5_rows,[[1,'retained result']],'Ordinary FTS5 indexed query did not return the expected row');
  return true;
}

export function sqliteComponentEvidence(kind,observed,identity,manifest){
  assert.ok(Object.hasOwn(MANAGED_SQLITE_LAYOUT,kind));
  assert.equal(observed.passed,true);assert.equal(observed.sqlite_version,SQLITE_VERSION);
  assert.equal(observed.module_version,SQLITE_VERSION);assert.equal(manifest.sqlite,SQLITE_VERSION);
  assert.deepEqual(observed.fts5_rows,[[1,'retained result']]);
  assert.ok(observed.compile_options?.includes('ENABLE_FTS5'),'Scanner identity lacks observed compile options');
  assert.match(observed.sqlite_source_id,/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} [a-f0-9]{64}$/);
  assert.equal(identity.all_native_archive_members_verified,true,'Every native member requires archive provenance');
  const matches=identity.files.filter(row=>row.path==='sqlite3.dll');assert.equal(matches.length,1);
  const row=matches[0],replacement=manifest.transformations.native_replacements['sqlite3.dll'];
  assert.equal(row.sha256,observed.dll_sha256);assert.equal(row.bytes,observed.dll_bytes);
  assert.equal(row.sha256,manifest.files['sqlite3.dll']);assert.equal(row.sha256,replacement.replacement.sha256);
  assert.equal(row.origin?.archive,replacement.replacement.archive_name);
  assert.equal(row.origin?.archive_sha256,replacement.replacement.archive_sha256);
  assert.equal(row.origin?.component,'SQLite');assert.equal(row.origin?.component_version,observed.sqlite_version);
  assert.deepEqual(row.origin?.replaces,replacement.original);
  return {name:`sqlite-${kind}`,version:observed.sqlite_version,purl:`pkg:generic/sqlite@${observed.sqlite_version}?consumer=${kind}`,
    cpe:`cpe:2.3:a:sqlite:sqlite:${observed.sqlite_version}:*:*:*:*:*:*:*`,hashes:[{alg:'SHA-256',content:row.sha256}],properties:[
      {name:'sqlite:source-id',value:observed.sqlite_source_id},{name:'phaseforge:evidence:seed-kind',value:kind},
      {name:'phaseforge:evidence:archive',value:row.origin.archive},{name:'phaseforge:evidence:archive-sha256',value:row.origin.archive_sha256},
      {name:'phaseforge:evidence:member',value:row.origin.member},{name:'phaseforge:evidence:scope',value:'Actual managed Python SQLite/FTS5 connection and loaded DLL path, unchanged frozen DLL hash, exact official replacement archive member. The original CPython SQLite member is explicitly recorded as replaced, not shipped.'},
      {name:'phaseforge:evidence:observations',value:'managed-sqlite-runtime.json; managed-runtime-materials.json'}]};
}

export async function observeManagedSqlite({workspace,sourceCommit,execute=command}){
  assert.equal(process.platform,'win32','Managed SQLite execution acceptance requires Windows');
  const runtimes={};
  for(const [kind,relative] of Object.entries(MANAGED_SQLITE_LAYOUT)){
    const directory=path.join(workspace,relative),frozen=path.join(ROOT,'tools/runtime-seeds',`${kind}.manifest.json`);
    await verifySeed(directory,frozen);
    const manifest=readJSON(frozen),executable=path.join(directory,'python.exe'),dll=path.join(directory,'sqlite3.dll');
    assert.ok(fs.statSync(executable).isFile()&&fs.statSync(dll).isFile());
    const before=await sha256(dll);
    assert.equal(before,manifest.files['sqlite3.dll']);
    const observed=JSON.parse(execute(executable,['-I','-B','-c',SQLITE_PROBE],{timeout:15000,maxBuffer:64*1024}));
    validateSqliteObservation(observed,{executable,dll});
    assert.equal(await sha256(dll),before,'Loaded SQLite bytes changed during observation');
    await verifySeed(directory,frozen);
    runtimes[kind]={...observed,passed:true,dll_sha256:before,dll_bytes:fs.statSync(dll).size,seed_manifest_sha256:await sha256(frozen)};
  }
  return {schema:'phaseforge.managed-sqlite-execution.v1',source_commit:sourceCommit,passed:true,runtimes,
    scope:'Actual bundled managed-copy Python imports and in-memory SQLite/FTS5 queries, with loaded DLL paths and unchanged frozen file pins. No user database, provider, solver or exploit payload was accessed; this is not a general security certification.'};
}
