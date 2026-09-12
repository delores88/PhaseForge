import test from 'node:test';
import assert from 'node:assert/strict';
import {fitImage,navigateImage,imageKeyCommand} from '../src/lib/image-navigation.mjs';

test('fit preserves every original pixel inside narrow and wide inspection windows',()=>{
  for(const viewport of [{width:320,height:300},{width:1200,height:680}])for(const image of [{width:2048,height:1440},{width:50,height:4000}]){
    const view=fitImage(viewport,image);
    assert.ok(view.x>=0&&view.y>=0);
    assert.ok(view.x+image.width*view.scale<=viewport.width);
    assert.ok(view.y+image.height*view.scale<=viewport.height);
    const original={...image};navigateImage(view,{actual:true},viewport,image);assert.deepEqual(image,original);
  }
});
test('zoom keeps the source pixel under the pointer and visible pan/keyboard use the same direction',()=>{
  const viewport={width:520,height:400},image={width:2048,height:1440},view=fitImage(viewport,image),anchor={x:140,y:220};
  const next=navigateImage(view,{zoom:1.25,anchor},viewport,image);
  assert.ok(Math.abs((anchor.x-view.x)/view.scale-(anchor.x-next.x)/next.scale)<1e-9);
  assert.ok(Math.abs((anchor.y-view.y)/view.scale-(anchor.y-next.y)/next.scale)<1e-9);
  const panned=navigateImage(next,imageKeyCommand({key:'ArrowRight'}),viewport,image);assert.equal(panned.x,next.x-40);
  assert.deepEqual(navigateImage(panned,{fit:true},viewport,image),view);
  assert.equal(navigateImage(view,{actual:true},viewport,image).scale,1);
  assert.equal(imageKeyCommand({key:'ArrowLeft',altKey:true}),null);
});
