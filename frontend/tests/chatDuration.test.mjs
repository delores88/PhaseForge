import test from 'node:test';
import assert from 'node:assert/strict';
import {durationSeconds,savedDuration,customMinutesToSeconds} from '../src/lib/chatDuration.mjs';
import {conversationTimeLimit} from '../src/lib/workbenchRequests.mjs';
test('custom and Off policy survive project storage without becoming the default',()=>{
  assert.equal(customMinutesToSeconds('47'),2820);assert.equal(savedDuration('2820'),'2820');assert.equal(conversationTimeLimit('p',{getItem:()=> '2820'}),2820);
  assert.equal(durationSeconds('off'),null);assert.equal(conversationTimeLimit('p',{getItem:()=> 'off'}),null);
  assert.equal(durationSeconds('604800'),604800);
});
test('invalid duration drafts cannot silently submit Off or a NaN timer',()=>{
  for(const value of ['',0,-1,1.5,10081,'oops'])assert.throws(()=>customMinutesToSeconds(value));
  for(const value of ['custom','NaN',-1,9,604801])assert.throws(()=>durationSeconds(value));
  assert.equal(savedDuration('custom'),'900');
});
