import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {frontendModuleEvidence} from './frontend-map.mjs';
import {inside} from './common.mjs';

const hash=value=>crypto.createHash('sha256').update(value).digest('hex');
function fixture(t){
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-frontend-map-'));
  t.after(()=>fs.rmSync(inside(os.tmpdir(),root,{existing:true}),{recursive:true,force:true}));
  return root;
}
function write(root,name,content){const file=path.join(root,name);fs.mkdirSync(path.dirname(file),{recursive:true});fs.writeFileSync(file,content);}
function basic(){return {version:3,sources:['webpack:///src/app.js','webpack:///node_modules/lib/index.js','unused.js'],sourcesContent:['export const app = 1;','export const library = 2;','removed'],names:[],mappings:'AAAA;ACAA'};}
function pair(root,map=basic()){
  const js='/* fixture is parsed, never executed */\n//# sourceMappingURL=compiled.js.map\n';
  write(root,'chunks/compiled.js',js);write(root,'chunks/compiled.js.map',JSON.stringify(map));return js;
}

test('binds exact shipped bundle/map/source hashes and distinguishes unused map entries',async t=>{
  const root=fixture(t),map=basic(),js=pair(root,map),evidence=await frontendModuleEvidence(root);
  assert.deepEqual(evidence.summary,{bundles:1,mapped_bundles:1,unmapped_bundles:0,source_records:3,mapped_source_records:2,unique_mapped_sources:2,missing_sources_content:0});
  const bundle=evidence.bundles[0];
  assert.equal(bundle.sha256,hash(js));assert.equal(bundle.map.sha256,hash(JSON.stringify(map)));
  assert.equal(bundle.modules.find(module=>module.source==='unused.js').mapped_segments,0);
  assert.equal(bundle.modules.find(module=>module.source.endsWith('src/app.js')).sha256,hash(map.sourcesContent[0]));
  write(root,'chunks/compiled.js',js+'\n');
  assert.notEqual((await frontendModuleEvidence(root)).bundles[0].sha256,bundle.sha256);
});

test('installed copy at a different root produces identical evidence',async t=>{
  const source=fixture(t),destination=fixture(t);pair(source);
  fs.cpSync(source,path.join(destination,'installed-ui'),{recursive:true});
  assert.deepEqual(await frontendModuleEvidence(source),await frontendModuleEvidence(path.join(destination,'installed-ui')));
});

test('embedded indexed maps retain content identities and count references',async t=>{
  const root=fixture(t);pair(root,{version:3,sections:[{offset:{line:0,column:0},map:basic()},{offset:{line:2,column:0},map:{version:3,sourceRoot:'turbopack:///',sources:['entry.js'],sourcesContent:['entry'],names:[],mappings:'AAAA'}}]});
  const evidence=await frontendModuleEvidence(root);
  assert.equal(evidence.summary.mapped_source_records,3);
  assert.equal(evidence.bundles[0].modules.find(module=>module.source==='entry.js').source_root,'turbopack:///');
});

test('unmapped wrappers, missing maps and missing content are explicit evidence gaps',async t=>{
  const root=fixture(t),map=basic();map.sourcesContent[1]=null;pair(root,map);
  write(root,'wrapper.js','void 0;');write(root,'missing.js','//# sourceMappingURL=absent.map');
  const evidence=await frontendModuleEvidence(root);
  assert.equal(evidence.summary.unmapped_bundles,2);assert.equal(evidence.summary.missing_sources_content,1);assert.equal(evidence.gaps.length,3);
  assert.equal(evidence.bundles.find(bundle=>bundle.path==='wrapper.js').map,null);
});

test('source identities never cause filesystem reads and external map references are refused',async t=>{
  const root=fixture(t),map=basic();map.sources[0]='C:/private/nonexistent.env';pair(root,map);
  assert.equal((await frontendModuleEvidence(root)).bundles[0].modules.find(module=>module.source===map.sources[0]).sha256,hash(map.sourcesContent[0]));
  for(const reference of ['https://example.org/map','../../escape.map','//example.org/map','compiled.js.map?token=secret','..\\escape.map']){
    write(root,'bad.js',`//# sourceMappingURL=${reference}`);
    await assert.rejects(frontendModuleEvidence(root),/source-map|boundary/);
  }
});

test('unsupported external sections and malformed references disclose gaps without fetching',async t=>{
  const root=fixture(t);pair(root,{version:3,sections:[{offset:{line:0,column:0},url:'https://example.org/external.map'}]});
  let evidence=await frontendModuleEvidence(root);
  assert.match(evidence.gaps[0].reason,/External source-map sections/);assert.equal(evidence.bundles[0].modules.length,0);
  const map=basic();map.mappings='AIAA';pair(root,map);
  evidence=await frontendModuleEvidence(root);assert.match(evidence.gaps[0].reason,/Invalid source-map source location/);
  assert.equal(evidence.summary.mapped_source_records,0);
});
