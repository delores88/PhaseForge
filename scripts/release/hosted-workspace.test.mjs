/** Owned directory fixtures only; no hosted identity, installer or app execution. */
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {execFileSync} from 'node:child_process';
import {hostedTemporaryDirectory} from './common.mjs';

function fixture(t){
  const parent=fs.realpathSync.native(os.tmpdir());
  const root=fs.realpathSync.native(fs.mkdtempSync(path.join(parent,'phaseforge-hosted-path-')));
  t.after(()=>{assert.equal(path.dirname(root),parent);assert.match(path.basename(root),/^phaseforge-hosted-path-/);assert.equal(fs.realpathSync.native(root),root);fs.rmSync(root,{recursive:true});});
  return root;
}

test('hosted temporary resolver requires an existing absolute directory',t=>{
  const root=fixture(t);assert.equal(hostedTemporaryDirectory(root),root);
  assert.throws(()=>hostedTemporaryDirectory(undefined));assert.throws(()=>hostedTemporaryDirectory('relative'));
  assert.throws(()=>hostedTemporaryDirectory(path.join(root,'missing')));
  const file=path.join(root,'file.txt');fs.writeFileSync(file,'fixture');assert.throws(()=>hostedTemporaryDirectory(file));
});

test('owned Windows 8.3 temporary root expands before installed runtime paths are derived',{skip:process.platform!=='win32'},t=>{
  const root=fixture(t);
  const script=`Add-Type -TypeDefinition 'using System; using System.Text; using System.Runtime.InteropServices; public static class HostedPathFixture { [DllImport("kernel32.dll", CharSet=CharSet.Unicode, SetLastError=true)] public static extern uint GetShortPathName(string input, StringBuilder output, uint length); }'; $buffer=New-Object System.Text.StringBuilder 32768; $count=[HostedPathFixture]::GetShortPathName($env:PHASEFORGE_OWNED_PATH_FIXTURE,$buffer,32768); if($count -eq 0 -or $count -ge 32768){throw 'GetShortPathName failed'}; $buffer.ToString()`;
  const short=execFileSync('powershell.exe',['-NoProfile','-NonInteractive','-Command',script],{env:{...process.env,PHASEFORGE_OWNED_PATH_FIXTURE:root},encoding:'utf8',windowsHide:true,timeout:15000}).trim();
  if(!short.includes('~')){t.skip('The fixture volume does not provide 8.3 aliases');return;}
  assert.notEqual(short,root);assert.equal(fs.realpathSync.native(short),root);
  assert.equal(hostedTemporaryDirectory(short),root);
  const workspace=path.join(hostedTemporaryDirectory(short),'fixture-data');fs.mkdirSync(workspace);
  assert.equal(workspace,fs.realpathSync.native(workspace));
  t.diagnostic('Actual GetShortPathName alias resolved to its long native identity; no app or runtime launched.');
});
