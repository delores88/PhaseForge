import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import {stageSeeds,verifySeed} from './stage-seeds.mjs';
test('staging verifies exact inputs before replacement and refuses extra or altered files',async()=>{
  const root=fs.mkdtempSync(path.join(os.tmpdir(),'phaseforge-seed-stage-'));
  try{
    const sourceRoot=path.join(root,'source'),destination=path.join(root,'stage'),manifestRoot=path.join(root,'frozen');fs.mkdirSync(manifestRoot);
    for(const kind of ['science-v3','python-numpy-v2']){
      const folder=path.join(sourceRoot,kind);fs.mkdirSync(folder,{recursive:true});const raw=Buffer.from(`fixture bytes ${kind}`);
      fs.writeFileSync(path.join(folder,'python.exe'),raw);
      const manifest=JSON.stringify({files:{'python.exe':crypto.createHash('sha256').update(raw).digest('hex')},file_bytes:{'python.exe':raw.length}});
      fs.writeFileSync(path.join(folder,'phaseforge-runtime-seed.json'),manifest);fs.writeFileSync(path.join(manifestRoot,`${kind}.manifest.json`),manifest);
    }
    const options={sourceRoot,destination,manifestRoot};await stageSeeds(options);await stageSeeds(options);
    const before=fs.readFileSync(path.join(destination,'runtime-seeds/science-v3/python.exe'));
    fs.writeFileSync(path.join(sourceRoot,'science-v3/extra.py'),'unexpected');await assert.rejects(stageSeeds(options),/differs/);
    assert.deepEqual(fs.readFileSync(path.join(destination,'runtime-seeds/science-v3/python.exe')),before);
    fs.unlinkSync(path.join(sourceRoot,'science-v3/extra.py'));fs.writeFileSync(path.join(sourceRoot,'science-v3/python.exe'),'modified');
    await assert.rejects(verifySeed(path.join(sourceRoot,'science-v3'),path.join(manifestRoot,'science-v3.manifest.json')),/differs/);
    assert.deepEqual(fs.readFileSync(path.join(destination,'runtime-seeds/science-v3/python.exe')),before);
  }finally{const target=path.resolve(root);assert.ok(target.startsWith(path.resolve(os.tmpdir())+path.sep)&&path.basename(target).startsWith('phaseforge-seed-stage-'));fs.rmSync(target,{recursive:true});}
});
