import {test} from 'node:test';
import assert from 'node:assert/strict';
import {illustrationCoordinates,illustrationNodeId} from '../src/lib/illustration-coordinates.mjs';
test('normalized Blender/glTF camera positions roundtrip in authored scientific coordinates',()=>{
  const mapping=illustrationCoordinates({source_center:[10,-3,7],display_scale:2});
  assert.deepEqual(mapping.toAsset([11,-1,10]),[2,6,-4]);assert.deepEqual(mapping.toSource([2,6,-4]),[11,-1,10]);assert.deepEqual(mapping.directionToAsset([0,0,1]),[0,1,-0]);
  const camera={position:[123.45,-5.7,80],target:[-20,55,3],up:[0,0,1]};
  for(const key of ['position','target'])assert.deepEqual(mapping.toSource(mapping.toAsset(camera[key])),camera[key]);
  assert.deepEqual(mapping.directionToSource(mapping.directionToAsset(camera.up)),camera.up);
});
test('model highlights use authored stable IDs carried by ancestors, without inventing IDs for generic assets',()=>{
  assert.equal(illustrationNodeId({parent:{userData:{phaseforge_node_id:'nucleus'}}}),'nucleus');assert.equal(illustrationNodeId({name:'some mesh'}),null);
  assert.deepEqual(illustrationCoordinates(null).toSource([1,2,3]),[1,2,3]);
});
