// Native range inputs round decimal steps. Keep their value integral and return
// the authoritative endpoints verbatim, so End cannot select the penultimate state.
export const TIMELINE_SLIDER_MAX=10000;
export function timelineSliderTime(value,start,end){
  const fraction=Math.max(0,Math.min(1,Number(value)/TIMELINE_SLIDER_MAX));
  if(!Number.isFinite(fraction)||fraction<=0||end<=start)return start;
  if(fraction>=1)return end;
  return start+(end-start)*fraction;
}
export function timelineSliderValue(time,start,end){
  if(!Number.isFinite(time)||end<=start)return 0;
  return Math.round(Math.max(0,Math.min(1,(time-start)/(end-start)))*TIMELINE_SLIDER_MAX);
}
