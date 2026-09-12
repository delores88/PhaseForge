import {fitPlot,zoomPlot} from './laboratory-plot.mjs';

export const fitImage=(viewport,image)=>fitPlot(viewport.width,viewport.height,image.width,image.height);
/** Direction means moving the inspection window, matching 3D camera panning. */
export function navigateImage(view,command,viewport,image){
  if(command.fit)return fitImage(viewport,image);
  if(command.actual)return {scale:1,x:(viewport.width-image.width)/2,y:(viewport.height-image.height)/2};
  if(command.zoom)return zoomPlot(view,command.zoom,command.anchor||{x:viewport.width/2,y:viewport.height/2},Math.min(.02,fitImage(viewport,image).scale),16);
  if(command.pan)return {...view,x:view.x-command.pan[0],y:view.y-command.pan[1]};
  return view;
}
export function imageKeyCommand(event){
  if(event.ctrlKey||event.metaKey||event.altKey)return null;
  return {ArrowLeft:{pan:[-40,0]},ArrowRight:{pan:[40,0]},ArrowUp:{pan:[0,-40]},ArrowDown:{pan:[0,40]},'+':{zoom:1.25},'=':{zoom:1.25},'-':{zoom:.8},Home:{fit:true},'0':{actual:true}}[event.key]||null;
}
