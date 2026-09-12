export const laboratoryViewable=job=>['solver','illustration','ml_study','study_plot','published_simulation'].includes(job?.kind)||(job?.kind==='generated'&&job.state==='completed');
export const isStudyPlot=job=>['ml_study','study_plot'].includes(job?.kind);

export function plotSourceId(job){
  if(job?.kind==='study_plot')return job.id;
  return job?.kind==='ml_study'&&typeof job.result?.plot_job_id==='string'?job.result.plot_job_id:null;
}

export function artifactPath(jobId,path){
  if(typeof jobId!=='string'||!jobId||/[/?#\\]/.test(jobId))throw Error('Invalid artifact job identity.');
  if(typeof path!=='string'||!path||path.split('/').some(part=>!part||part==='.'||part==='..')||/[\\:?#]/.test(path))throw Error('Invalid artifact path.');
  return `/api/laboratory/jobs/${encodeURIComponent(jobId)}/artifacts/${path.split('/').map(encodeURIComponent).join('/')}`;
}

export function studyDownloads(plot,study){
  if(plot?.kind!=='study_plot'||plot.state!=='completed')return [];
  const links=[['Save PNG',plot.id,'plot/plot.png'],['Plot data & provenance',plot.id,'plot/plot.json']];
  if(study?.kind==='ml_study'&&study.project_id===plot.project_id){
    const result=study.result||{};
    for(const [label,key,path] of [
      ['Full numerical dataset (NPZ)','dataset_export_job_id','work/dataset.npz'],
      ['Dataset metadata & all 99 seeds','dataset_export_job_id','work/dataset.json'],
      ['Evaluation & gates','evaluation_job_id','work/evaluation.json'],
      ['Frozen split & protocol','freeze_job_id','work/split.json'],
      ['Frozen model','fit_job_id','work/model.npz'],
      ['Model card','fit_job_id','work/model-card.json'],
    ])if(typeof result[key]==='string')links.push([label,result[key],path]);
    if(study.state==='completed')links.push(['Full study receipt',study.id,'result.json']);
  }
  return links.map(([label,id,path])=>({label,href:artifactPath(id,path),filename:`phaseforge-${id}-${path.split('/').at(-1)}`}));
}

export function fitPlot(width,height,imageWidth=1600,imageHeight=1000){
  const scale=Math.min(Math.max(1,width-24)/imageWidth,Math.max(1,height-24)/imageHeight,1);
  return {scale,x:(width-imageWidth*scale)/2,y:(height-imageHeight*scale)/2};
}

/** Zoom around a screen-space anchor without changing the image pixel beneath it. */
export function zoomPlot(view,factor,anchor,minScale=.02,maxScale=8){
  const scale=Math.min(maxScale,Math.max(minScale,view.scale*factor)),ratio=scale/view.scale;
  return {scale,x:anchor.x-(anchor.x-view.x)*ratio,y:anchor.y-(anchor.y-view.y)*ratio};
}

export async function verifyPlotPng(bytes,expected,subtle=globalThis.crypto?.subtle){
  if(!/^[a-f0-9]{64}$/.test(expected||'')||!subtle)throw Error('The saved plot has no verifiable image digest.');
  const raw=new Uint8Array(bytes);
  if(raw.length>16*1024*1024||raw.length<24||[137,80,78,71,13,10,26,10].some((byte,i)=>raw[i]!==byte))throw Error('The saved plot is not a bounded PNG.');
  const hash=Array.from(new Uint8Array(await subtle.digest('SHA-256',bytes)),byte=>byte.toString(16).padStart(2,'0')).join('');
  if(hash!==expected)throw Error('The saved plot image does not match its completed receipt.');
  const size=new DataView(raw.buffer,raw.byteOffset,raw.byteLength);
  if(size.getUint32(16)!==1600||size.getUint32(20)!==1000)throw Error('Unexpected plot image dimensions.');
  return hash;
}
