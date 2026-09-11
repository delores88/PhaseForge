/** Detached private GitHub provenance; no repository write or public-good disclosure. */
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {attestProvenance} from '@actions/attest';
import {ROOT,REPOSITORY,WORKFLOW,command,hostedWorkspace,inside,readJSON,releaseTarget,sha256,writeJSON} from './common.mjs';

hostedWorkspace();assert.equal(process.env.GITHUB_REPOSITORY,REPOSITORY);
assert.equal(process.env.GITHUB_WORKFLOW_REF?.split('@')[0],WORKFLOW);
const {platform,architecture,folder:targetFolder}=releaseTarget();
const folder=path.join(ROOT,'.local/marketplace',targetFolder),final=path.join(folder,'candidate');
const names=readJSON(path.join(folder,'attestation-subjects.json'));
assert.ok(Array.isArray(names)&&names.length>0&&names.length<=20);
assert.equal(new Set(names).size,names.length);
const subjects=[];
for(const name of names){assert.equal(path.basename(name),name);const file=inside(final,path.join(final,name),{existing:true});subjects.push({name,digest:{sha256:await sha256(file)}});}
const prefix=`PhaseForge_${readJSON(path.join(ROOT,'desktop/package.json')).version}_${platform}_${architecture}`;
const bundle=path.join(final,`${prefix}_ATTESTATION.jsonl`);
const result=await attestProvenance({subjects,token:'',sigstore:'github',skipWrite:true});
fs.writeFileSync(bundle,JSON.stringify(result.bundle)+'\n',{flag:'wx'});

// Authenticate freshly obtained GitHub TUF roots through the pinned gh verifier.
const verifier=path.join(ROOT,'.local/marketplace/tools',process.platform==='win32'?'gh.exe':'gh');
const receipt=readJSON(path.join(ROOT,'.local/marketplace/tools/tool-receipts.json')).find(row=>row.name==='gh');
assert.ok(receipt);assert.equal(receipt.executable_sha256,await sha256(verifier));
const allRoots=command(verifier,['attestation','trusted-root','--hostname','github.com'],{timeout:120000});
const roots=allRoots.trim().split(/\r?\n/).filter(line=>{const row=JSON.parse(line);return row.certificateAuthorities?.length&&row.certificateAuthorities.every(ca=>['fulcio.githubapp.com','https://fulcio.githubapp.com'].includes(ca.uri));});
assert.equal(roots.length,1,'Expected exactly one freshly authenticated complete GitHub trust root');
const trusted=path.join(final,`${prefix}_GITHUB_TRUSTED_ROOT.jsonl`);fs.writeFileSync(trusted,roots[0]+'\n',{flag:'wx'});
const verified=[];
for(const subject of subjects){
  const stdout=command(verifier,['attestation','verify',inside(final,path.join(final,subject.name),{existing:true}),'--repo',REPOSITORY,'--source-digest',process.env.GITHUB_SHA,'--signer-workflow',WORKFLOW,'--signer-digest',process.env.GITHUB_SHA,'--predicate-type','https://slsa.dev/provenance/v1','--cert-oidc-issuer','https://token.actions.githubusercontent.com','--deny-self-hosted-runners','--no-public-good','--bundle',bundle,'--custom-trusted-root',trusted,'--format=json'],{timeout:120000});
  const statements=JSON.parse(stdout);assert.ok(Array.isArray(statements)&&statements.length>0,'Verifier returned no authenticated statements');
  verified.push({...subject,verification:statements});
}
writeJSON(path.join(final,`${prefix}_ATTESTATION_VERIFICATION.json`),{passed:true,source_commit:process.env.GITHUB_SHA,workflow:WORKFLOW,workflow_commit:process.env.GITHUB_SHA,gh:receipt,bundle_sha256:await sha256(bundle),trusted_root_sha256:await sha256(trusted),verified,notice:'Consumers must apply their own builder policy and obtain fresh authenticated roots. Provenance is not an OS publisher certificate or security admission.'});
// This transfer checksum also covers the detached bundle and local verification, without circular self-signing.
const finalFiles=fs.readdirSync(final).sort();let checksums='';
for(const name of finalFiles)checksums+=`${await sha256(path.join(final,name))}  ${name}\n`;
fs.writeFileSync(path.join(final,`${prefix}_TRANSFER_SHA256SUMS.txt`),checksums,{flag:'wx'});
console.log(JSON.stringify({verified:true,subjects:subjects.length,source_commit:process.env.GITHUB_SHA}));
