import net from 'node:net';

/** Occupied legacy loopback ports must receive no traffic from the installed app. */
export async function startInertListeners(targets){
  const entries=[];
  const close=async()=>{for(const entry of entries){for(const socket of entry.sockets)socket.destroy();if(entry.server.listening)await new Promise(resolve=>entry.server.close(resolve));}};
  try{
    for(const {address,port} of targets){
      if(!['127.0.0.1','::1'].includes(address)||!Number.isInteger(port)||port<0||port>65535)throw Error('Fixture listeners must use explicit loopback addresses and valid ports');
      const entry={server:null,sockets:new Set(),address,port,connections:0,received_bytes:0};
      entry.server=net.createServer(socket=>{
        entry.connections++;entry.sockets.add(socket);socket.setTimeout(1000,()=>socket.destroy());
        socket.on('data',bytes=>{entry.received_bytes+=bytes.length;socket.end('HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n');});
        socket.on('error',()=>{});socket.on('close',()=>entry.sockets.delete(socket));
      });
      entries.push(entry);
      await new Promise((resolve,reject)=>{entry.server.once('error',reject);entry.server.listen({host:address,port,ipv6Only:address==='::1'},()=>{entry.server.removeListener('error',reject);resolve();});});
      entry.port=entry.server.address().port;
    }
    return {close,receipts:()=>entries.map(({address,port,connections,received_bytes})=>({address,port,connections,received_bytes}))};
  }catch(error){await close();throw error;}
}
