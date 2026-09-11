import {test} from 'node:test';
import assert from 'node:assert/strict';
import {webcrypto} from 'node:crypto';
import {artifactPath,fitPlot,laboratoryViewable,plotSourceId,studyDownloads,verifyPlotPng,zoomPlot} from '../src/lib/laboratory-plot.mjs';
import {selectedLaboratoryContext} from '../src/lib/workbenchRequests.mjs';

test('study results resolve charts without treating them as particle sources or crossing project context',()=>{
  const study={id:'study',kind:'ml_study',project_id:'a',result:{plot_job_id:'plot'}},plot={id:'plot',kind:'study_plot',project_id:'a'};
  assert.equal(plotSourceId(study),'plot');assert.equal(plotSourceId(plot),'plot');assert.equal(plotSourceId({kind:'solver',id:'solver'}),null);
  assert.equal(laboratoryViewable(study),true);assert.equal(laboratoryViewable(plot),true);assert.equal(laboratoryViewable({kind:'generated'}),false);
  assert.equal(selectedLaboratoryContext('a','laboratory','plot',[plot]),plot);
  assert.equal(selectedLaboratoryContext('b','laboratory','plot',[plot]),null);
  assert.equal(selectedLaboratoryContext('a','sessions','plot',[plot]),null);
});
test('downloads require completed charts and preserve full numerical source links without unsafe paths',()=>{
  const plot={id:'plot',kind:'study_plot',state:'completed',project_id:'a'},study={id:'study',kind:'ml_study',state:'completed',project_id:'a',result:{dataset_export_job_id:'export',evaluation_job_id:'evaluate',freeze_job_id:'split',fit_job_id:'fit'}};
  assert.equal(studyDownloads({...plot,state:'running'},study).length,0);
  const links=studyDownloads(plot,study);assert.equal(links.length,9);
  assert.ok(links.some(item=>item.href.endsWith('/export/artifacts/work/dataset.npz')));
  assert.ok(links.some(item=>item.href.endsWith('/evaluate/artifacts/work/evaluation.json')));
  assert.equal(studyDownloads(plot,{...study,project_id:'b'}).length,2);
  for(const path of ['../x','a/../x','a//x','a\\x','https://bad','/x','a?x'])assert.throws(()=>artifactPath('plot',path));
  assert.throws(()=>artifactPath('../other','plot.png'));
});
test('chart fit stays within narrow and wide viewports; zoom preserves the exact pixel under the cursor',()=>{
  for(const [width,height] of [[320,320],[520,430],[1200,720]]){
    const view=fitPlot(width,height);assert.ok(view.x>=0&&view.y>=0);assert.ok(view.x+1600*view.scale<=width);assert.ok(view.y+1000*view.scale<=height);
    const anchor={x:width*.7,y:height*.2},before=[(anchor.x-view.x)/view.scale,(anchor.y-view.y)/view.scale],zoomed=zoomPlot(view,2,anchor);
    assert.ok(Math.abs((anchor.x-zoomed.x)/zoomed.scale-before[0])<1e-10);assert.ok(Math.abs((anchor.y-zoomed.y)/zoomed.scale-before[1])<1e-10);
    assert.equal(zoomPlot(view,1e9,anchor).scale,8);assert.equal(zoomPlot(view,1e-9,anchor).scale,.02);
  }
});
test('saved PNG bytes need the completed receipt digest and correct original dimensions',async()=>{
  const bytes=new Uint8Array(24);bytes.set([137,80,78,71,13,10,26,10]);const data=new DataView(bytes.buffer);data.setUint32(16,1600);data.setUint32(20,1000);
  const hash=Buffer.from(await webcrypto.subtle.digest('SHA-256',bytes)).toString('hex');
  assert.equal(await verifyPlotPng(bytes.buffer,hash,webcrypto.subtle),hash);
  await assert.rejects(verifyPlotPng(bytes.buffer,'0'.repeat(64),webcrypto.subtle),/does not match/);
  await assert.rejects(verifyPlotPng(bytes.buffer,null,webcrypto.subtle),/no verifiable/);
  bytes[0]=0;await assert.rejects(verifyPlotPng(bytes.buffer,hash,webcrypto.subtle),/not a bounded PNG/);
});
