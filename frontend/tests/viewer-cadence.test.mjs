import test from 'node:test';
import assert from 'node:assert/strict';
import {advanceRenderClock} from '../src/lib/viewer-cadence.mjs';

function replay(times,fps=60){let clock=0;const drawn=[];for(const now of times){const next=advanceRenderClock(now,clock,1000/fps);if(next!==null){clock=next;drawn.push(now);}}return drawn;}

test('active drawing stays near 60 per second across 60/120/144 Hz timestamp streams',()=>{
  for(const refresh of [60,120,144]){
    const times=Array.from({length:refresh*10},(_,i)=>Math.round((i+1)*1000/refresh*10)/10);
    const drawn=replay(times);assert.ok(drawn.length>=598&&drawn.length<=600,`${refresh} Hz produced ${drawn.length} draws`);
    assert.ok(drawn.every((time,i)=>i===0||time>drawn[i-1]));
  }
});

test('idle cadence retains its 15 Hz cap with fractional display timestamps',()=>{
  const times=Array.from({length:1440},(_,i)=>(i+1)*1000/144);
  assert.ok(Math.abs(replay(times,15).length-150)<=1);
});

test('delayed refreshes skip missed paints without bursts or cumulative drift',()=>{
  const times=Array.from({length:1200},(_,i)=>(i+1)*1000/120).filter(t=>!(t>2000&&t<2500)&&!(t>7000&&t<7200));
  const drawn=replay(times);assert.ok(drawn.length>=555&&drawn.length<=560,`Delayed trace produced ${drawn.length} draws`);
  for(const time of [2500,7200])assert.equal(drawn.filter(t=>t===time).length,1);
  const resumed=replay([100,100,100,100,116.8,133.5]);assert.deepEqual(resumed,[100,116.8,133.5]);
});
