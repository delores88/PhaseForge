import test from 'node:test';
import assert from 'node:assert/strict';
import path from 'node:path';
import {SQLITE_VERSION,MANAGED_SQLITE_LAYOUT,SQLITE_PROBE,validateSqliteObservation,sqliteComponentEvidence} from './sqlite-runtime.mjs';

const expected={executable:path.resolve('fixture/science-v5/python.exe'),dll:path.resolve('fixture/science-v5/sqlite3.dll')};
const fixture=()=>({sqlite_version:SQLITE_VERSION,module_version:SQLITE_VERSION,sqlite_source_id:`2026-08-26 00:00:00 ${'a'.repeat(64)}`,compile_options:['ENABLE_FTS5','THREADSAFE=1'],executable:expected.executable,loaded_sqlite_path:expected.dll,pointer_bits:64,fts5_rows:[[1,'retained result']]});
test('SQLite gate validates exact reviewed runtime and ordinary FTS5 query',()=>{
  assert.equal(validateSqliteObservation(fixture(),expected),true);
  assert.deepEqual(Object.keys(MANAGED_SQLITE_LAYOUT),['science-v5','python-numpy-v4']);
  assert.match(SQLITE_PROBE,/connect\(':memory:'\)/);assert.match(SQLITE_PROBE,/GetModuleFileNameW/);
});
test('SQLite gate rejects vulnerable version, host DLL resolution, missing source identity and wrong FTS5 result',()=>{
  for(const mutate of [value=>value.sqlite_version='3.53.1',value=>value.module_version='3.53.2',value=>value.loaded_sqlite_path=path.resolve('host/sqlite3.dll'),value=>value.executable=path.resolve('host/python.exe'),value=>value.sqlite_source_id='unknown',value=>value.compile_options=[],value=>value.pointer_bits=32,value=>value.fts5_rows=[],value=>value.fts5_rows=[[2,'other record']]]){
    const value=fixture();mutate(value);assert.throws(()=>validateSqliteObservation(value,expected));
  }
});

test('scanner SQLite identity binds executed version and DLL hash to the replacement archive, never CPython ownership',()=>{
  const replacement={original:{archive_name:'python.zip',sha256:'a'.repeat(64)},replacement:{archive_name:'sqlite.zip',archive_sha256:'b'.repeat(64),sha256:'c'.repeat(64)}};
  const observed={...fixture(),passed:true,dll_sha256:'c'.repeat(64),dll_bytes:100};
  const identity={all_native_archive_members_verified:true,files:[{path:'sqlite3.dll',sha256:'c'.repeat(64),bytes:100,origin:{archive:'sqlite.zip',archive_sha256:'b'.repeat(64),member:'sqlite3.dll',component:'SQLite',component_version:SQLITE_VERSION,replaces:replacement.original}}]};
  const manifest={sqlite:SQLITE_VERSION,files:{'sqlite3.dll':'c'.repeat(64)},transformations:{native_replacements:{'sqlite3.dll':replacement}}};
  const result=sqliteComponentEvidence('science-v5',observed,identity,manifest);
  assert.equal(result.version,'3.53.4');assert.equal(result.cpe,'cpe:2.3:a:sqlite:sqlite:3.53.4:*:*:*:*:*:*:*');
  assert.equal(result.hashes[0].content,'c'.repeat(64));
  for(const mutate of [value=>value.files[0].origin.archive='python.zip',value=>value.files[0].sha256='d'.repeat(64),value=>value.all_native_archive_members_verified=false,value=>value.files[0].origin.component_version='3.50.4']){
    const bad=structuredClone(identity);mutate(bad);assert.throws(()=>sqliteComponentEvidence('science-v5',observed,bad,manifest));
  }
});
