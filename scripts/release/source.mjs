import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {ROOT,command,readJSON,sha256,sourceIdentity,writeJSON} from './common.mjs';

const phase=process.argv[2];
if(!['before','after'].includes(phase))throw Error('Usage: source.mjs before|after');
const source=sourceIdentity(),directory=path.join(ROOT,'.local/marketplace/source');
const files=command('git',['ls-files','-z']).split('\0').filter(Boolean),materials=[];
for(const relative of files){const file=path.join(ROOT,relative);materials.push({path:relative,bytes:fs.statSync(file).size,sha256:await sha256(file)});}
const result={schema:'phaseforge.clean-source.v1',source,materials,toolchain:{node:process.version,npm:command(process.platform==='win32'?'npm.cmd':'npm',['--version'],{shell:process.platform==='win32'}),rustc:command('rustc',['--version']),cargo:command('cargo',['--version'])},workflow:{run_id:process.env.GITHUB_RUN_ID,run_attempt:process.env.GITHUB_RUN_ATTEMPT,workflow_ref:process.env.GITHUB_WORKFLOW_REF},recorded_at:new Date().toISOString()};
if(phase==='after')assert.deepEqual(result.materials,readJSON(path.join(directory,'before.json')).materials,'Source inputs changed during packaging');
writeJSON(path.join(directory,`${phase}.json`),result);console.log(`Verified ${phase}: ${source.commit}, ${materials.length} tracked inputs`);
