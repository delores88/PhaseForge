export const ATTACHMENT_FORMATS=['txt','md','json','csv','toml','yaml','yml','pdb','sdf','mol','xyz'];
const encoder=new TextEncoder();
export async function readAttachmentBatch(files,existing=[]){
  const selected=Array.from(files||[]);
  if(existing.length+selected.length>8)throw Error('At most eight attachments per message.');
  let total=existing.reduce((sum,file)=>sum+(file.size_bytes||0),0);
  const next=[];
  for(const file of selected){
    if(!file.name||encoder.encode(file.name).length>180||/[<>:"/\\|?*\u0000-\u001f\u007f]/u.test(file.name)||/[ .]$/u.test(file.name))throw Error('Use a plain filename of at most 180 UTF-8 bytes.');
    const extension=file.name.split('.').at(-1)?.toLowerCase();
    if(!ATTACHMENT_FORMATS.includes(extension))throw Error(`${file.name}: use a supported text, data, or molecular structure file.`);
    const limit=(extension==='csv'?1:2)*1024*1024;
    if(!file.size||file.size>limit||total+file.size>8*1024*1024)throw Error('Attachment limits: CSV 1 MiB; other files 2 MiB; 8 MiB per message.');
    const bytes=new Uint8Array(await file.arrayBuffer());
    if(bytes.length!==file.size)throw Error(`${file.name}: the file changed while it was being read.`);
    let content;
    try{content=new TextDecoder('utf-8',{fatal:true,ignoreBOM:true}).decode(bytes);}catch{throw Error(`${file.name}: save valid UTF-8 text before attaching it. Binary or damaged text cannot be imported.`);}
    if(content.includes('\0'))throw Error(`${file.name}: binary NUL bytes are not supported.`);
    next.push({name:file.name,mime_type:file.type||'text/plain',size_bytes:bytes.length,content});total+=bytes.length;
  }
  return next;
}
