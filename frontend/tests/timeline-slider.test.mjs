import test from 'node:test';
import assert from 'node:assert/strict';
import {TIMELINE_SLIDER_MAX,timelineSliderTime,timelineSliderValue} from '../src/lib/timeline-slider.mjs';
import {TrajectoryStore} from '../src/lib/laboratory-trajectory.mjs';

test('End selects the exact retained endpoint after accumulated solver time roundoff',async()=>{
  const end=100.00000000011343;
  const frames=[{time:99.98000000011333,entities:[]},{time:end,entities:[]}];
  const store=new TrajectoryStore({index:{start_time:0,end_time:end,chunks:[{path:'final.json',start_time:frames[0].time,end_time:end,frame_count:2}]},loadJson:async()=>({frames})});
  // Chromium may serialize its decimal range step just below the final time.
  assert.equal((await store.sample(100.0000000001134)).left.time,frames[0].time);
  const chosen=timelineSliderTime(TIMELINE_SLIDER_MAX,0,end);
  assert.equal(chosen,end);assert.equal((await store.sample(chosen)).left.time,end);
  assert.equal(timelineSliderValue(end,0,end),TIMELINE_SLIDER_MAX);
});

test('timeline mapping preserves endpoints at small scales and nonzero origins',()=>{
  for(const [start,end] of [[1e-20,1.0001e-20],[-12.75,43.12678912345678],[0,0]]){
    assert.equal(timelineSliderTime(0,start,end),start);assert.equal(timelineSliderTime(TIMELINE_SLIDER_MAX,start,end),end);
    assert.equal(timelineSliderTime(TIMELINE_SLIDER_MAX/2,start,end),start+(end-start)/2);
  }
});
