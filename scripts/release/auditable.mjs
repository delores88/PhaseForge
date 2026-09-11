import zlib from 'node:zlib';

/** Read the compiler-emitted dependency section, never infer it from a lockfile. */
export function auditableSection(bytes){
  const range=(offset,size)=>{if(!Number.isSafeInteger(offset)||!Number.isSafeInteger(size)||offset<0||size<0||offset+size>bytes.length)throw Error('Native dependency section exceeds binary bounds');return bytes.subarray(offset,offset+size);};
  let sections=[];
  if(bytes.subarray(0,2).toString()==='MZ'){
    range(0,64);const pe=bytes.readUInt32LE(0x3c);range(pe,24);
    if(range(pe,4).toString('binary')!=='PE\0\0')throw Error('Invalid PE signature');
    const count=bytes.readUInt16LE(pe+6),start=pe+24+bytes.readUInt16LE(pe+20);
    if(count>4096)throw Error('Excessive PE section count');
    for(let i=0;i<count;i++){const row=range(start+i*40,40),name=row.subarray(0,8).toString().replace(/\0.*$/,'');sections.push({name,offset:row.readUInt32LE(20),size:row.readUInt32LE(16)});}
  }else if(bytes.subarray(0,4).equals(Buffer.from([0x7f,69,76,70]))){
    range(0,64);if(bytes[4]!==2||bytes[5]!==1)throw Error('Release ELF must be little-endian 64-bit');
    const start=Number(bytes.readBigUInt64LE(40)),width=bytes.readUInt16LE(58),count=bytes.readUInt16LE(60),namesIndex=bytes.readUInt16LE(62);
    if(width<64||!count||count>4096||namesIndex>=count)throw Error('Unsupported ELF section table');
    const nameRow=range(start+width*namesIndex,64),names=range(Number(nameRow.readBigUInt64LE(24)),Number(nameRow.readBigUInt64LE(32)));
    for(let i=0;i<count;i++){const row=range(start+width*i,64),index=row.readUInt32LE(0);if(index>=names.length)throw Error('Invalid ELF section name');const end=names.indexOf(0,index);if(end<0)throw Error('Unterminated ELF section name');sections.push({name:names.subarray(index,end).toString(),offset:Number(row.readBigUInt64LE(24)),size:Number(row.readBigUInt64LE(32))});}
  }else if(bytes.subarray(0,4).equals(Buffer.from([0xcf,0xfa,0xed,0xfe]))){
    range(0,32);const count=bytes.readUInt32LE(16),width=bytes.readUInt32LE(20);range(32,width);
    if(!count||count>4096)throw Error('Unsupported Mach-O load command count');
    let cursor=32;const end=32+width;
    for(let i=0;i<count;i++){
      if(cursor+8>end)throw Error('Mach-O load command exceeds declared bounds');
      const command=bytes.readUInt32LE(cursor),size=bytes.readUInt32LE(cursor+4);
      if(size<8||size%8!==0||cursor+size>end)throw Error('Invalid Mach-O load command bounds');
      if(command===0x19){
        if(size<72)throw Error('Truncated Mach-O segment');
        const count=bytes.readUInt32LE(cursor+64);
        if(count>4096||72+count*80>size)throw Error('Mach-O sections exceed segment bounds');
        for(let index=0;index<count;index++){
          const row=range(cursor+72+index*80,80),name=row.subarray(0,16).toString().replace(/\0.*$/,'');
          sections.push({name,offset:row.readUInt32LE(48),size:Number(row.readBigUInt64LE(40))});
        }
      }
      cursor+=size;
    }
    if(cursor!==end)throw Error('Mach-O load commands do not cover their declared table');
  }else throw Error('Expected an actual PE, ELF or little-endian 64-bit Mach-O backend');
  const selected=sections.filter(row=>row.name==='.dep-v0');if(selected.length!==1)throw Error('Missing compiler-emitted .dep-v0 metadata; rebuild with cargo auditable');
  const {offset,size}=selected[0];if(size>16*1024*1024)throw Error('Compressed dependency metadata exceeds budget');
  return range(offset,size);
}

export function auditableDependencies(bytes){
  const value=JSON.parse(zlib.inflateSync(auditableSection(bytes),{maxOutputLength:16*1024*1024}).toString('utf8'));
  if(!Array.isArray(value.packages)||!value.packages.length||value.packages.length>100000||value.packages.some(item=>typeof item.name!=='string'||typeof item.version!=='string'))throw Error('Malformed compiler dependency metadata');
  return value;
}
