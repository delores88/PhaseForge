export const CHAT_DURATION_PRESETS=[['300','5 minutes'],['900','15 minutes'],['3600','1 hour'],['14400','4 hours'],['28800','8 hours'],['off','Timer off']];
export function durationSeconds(value){
  if(value==='off')return null;
  const seconds=Number(value);
  if(!Number.isInteger(seconds)||seconds<10||seconds>604800)throw Error('Choose a work limit from 10 seconds to 7 days, or Timer off.');
  return seconds;
}
export function savedDuration(value){try{return value==='off'?'off':String(durationSeconds(value));}catch{return '900';}}
export function customMinutesToSeconds(value){
  const minutes=Number(value);
  if(!Number.isInteger(minutes)||minutes<1||minutes>10080)throw Error('Custom time must be a whole number of minutes from 1 to 10,080.');
  return minutes*60;
}
