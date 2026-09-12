// Actual frontend numerical-data decoder acceptance, using retained worker output.
// This is not an interactive/native UI test or an installed-app claim.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { FieldStore, normalizeFieldIndex, recordedFieldNumber, fieldCell } from '../frontend/src/lib/laboratory-field.mjs';

const root = path.resolve(process.argv[2] || '');
const output = path.resolve(process.argv[3] || '');
assert.ok(process.argv[2] && process.argv[3], 'Usage: node tools/check_continuum_consumers.mjs <checker-output> <new-report.json>');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const report = { scope: 'Source-level actual FieldStore decoder against computed data; no browser or installed application', passed: false, sources: {}, cases: [] };
for (const relative of ['frontend/src/lib/laboratory-field.mjs', 'tools/continuum_worker.py']) {
  report.sources[relative] = hash(await fs.readFile(new URL('../' + relative, import.meta.url)));
}
for (const name of ['heat-refinement-32', 'flow-taylor-green', 'flow-inviscid-interactions', 'heat-retained-array']) {
  const directory = path.join(root, name);
  const rawIndex = await fs.readFile(path.join(directory, 'fields/index.json'));
  const raw = JSON.parse(rawIndex);
  const index = normalizeFieldIndex(raw);
  const manifest = JSON.parse(await fs.readFile(path.join(directory, 'manifest.json')));
  assert.equal(index.length_unit, 'um');
  assert.equal(index.time_unit, 's');
  assert.equal(manifest.units.length, 'm');
  assert.equal(index.lx, manifest.input.parameters.length_x_m * 1e6);
  assert.equal(index.ly, manifest.input.parameters.length_y_m * 1e6);
  const loadBytes = relative => fs.readFile(path.join(directory, relative));
  const store = new FieldStore({ index: raw, loadBytes, maxFrames: 2 });
  for (let i = 0; i < index.frames.length; ++i) {
    const frame = await store.frame(i);
    assert.equal(frame.time, index.frames[i].step * manifest.input.parameters.dt_s);
    assert.equal(frame.field_unit, index.field_unit);
    assert.equal(frame.source_sha256, index.frames[i].sha256);
    assert.equal(recordedFieldNumber(index, frame.time), i);
    const cell = fieldCell(index, .37, .62);
    assert.equal(cell.position[0], index.solver_coordinates.x_m[cell.x] * 1e6);
    assert.equal(cell.position[1], index.solver_coordinates.y_m[cell.y] * 1e6);
    assert.ok(Number.isFinite(frame.values[cell.y][cell.x]));
  }
  assert.ok(store.cache.size <= 2);
  const corrupt = new FieldStore({ index: raw, loadBytes: async relative => {
    const data = Buffer.from(await loadBytes(relative)); data[data.length - 1] ^= 1; return data;
  } });
  await assert.rejects(corrupt.frame(0), /checksum/);
  store.close(); corrupt.close();
  report.cases.push({ name, engine: manifest.engine, index_sha256: hash(rawIndex), field_unit: index.field_unit, frames: index.frames.length, physical_start_s: index.start, physical_end_s: index.end, corruption_rejected: true });
}
report.passed = true;
report.checker_sha256 = hash(await fs.readFile(fileURLToPath(import.meta.url)));
await fs.writeFile(output, JSON.stringify(report, null, 2), { flag: 'wx' });
console.log(JSON.stringify(report));
