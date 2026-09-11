import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {ROOT} from './common.mjs';
import {verifyMsvcNotices,MSVC_NOTICES} from './msvc-notices.mjs';

test('configured Microsoft agreement has exact packaged bytes and preserves MIT scope',async()=>{
  const config=JSON.parse(fs.readFileSync(path.join(ROOT,'desktop/package.json'),'utf8')).build;
  assert.equal(config.nsis.oneClick,false);
  assert.equal(config.nsis.license,'../tools/third-party/msvc-runtime/END-USER-TERMS.txt');
  const file=path.resolve(ROOT,'desktop',config.nsis.license),text=fs.readFileSync(file,'utf8');
  assert.match(text,/PhaseForge remains licensed under the MIT License/);
  assert.match(text,/terms do not limit your MIT rights/);
  assert.match(text,/MSVC-2026-09-11/);
  const receipt=await verifyMsvcNotices();
  assert.ok(receipt.files.some(row=>row.path.endsWith('/END-USER-TERMS.txt')&&row.sha256===MSVC_NOTICES['END-USER-TERMS.txt'].sha256));
});

test('agreement hook consumes the page before the stock update skip and silent installation requires explicit assent',()=>{
  const custom=fs.readFileSync(path.join(ROOT,'desktop/build/installer.nsh'),'utf8');
  assert.ok(custom.indexOf('!macroundef licensePage')<custom.indexOf('!macro customWelcomePage'));
  assert.match(custom,/!macro customWelcomePage[\s\S]*!define MUI_LICENSEPAGE_CHECKBOX[\s\S]*!insertmacro MUI_PAGE_LICENSE.*END-USER-TERMS.txt/);
  assert.equal((custom.match(/!insertmacro MUI_PAGE_LICENSE/g)||[]).length,1);
  assert.match(custom,/\$\{If\} \$\{Silent\}[\s\S]*\/ACCEPT_MSVC_TERMS=[\s\S]*\$R1 != "MSVC-2026-09-11"[\s\S]*SetErrorLevel 2[\s\S]*Quit/);
  const harness=fs.readFileSync(path.join(ROOT,'scripts/release/native.mjs'),'utf8');
  assert.ok(harness.includes("['/S','/ACCEPT_MSVC_TERMS=MSVC-2026-09-11',`/D=${install}`]"));
});
