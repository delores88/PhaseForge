"""Exercise the worker's data validation without importing the Blender runtime."""
import ast
import bisect
from collections import OrderedDict
import copy
import hashlib
import json
import math
from pathlib import Path
import sys
import tempfile
import unittest
from types import SimpleNamespace

source = ast.parse(Path(__file__).with_name('trajectory_render.py').read_text(encoding='utf-8'))
helpers = {'finite', 'integer', 'retained_time', 'recorded_index', 'export_frame_count',
           'committed_digest', 'pinned_source_index', 'verify_source_bytes', 'Trajectory',
           'particle_radius', 'particle_domain', 'normalized_position'}
namespace = {'math': math, 'sys': sys, 'bisect': bisect, 'OrderedDict': OrderedDict,
             'json': json, 'hashlib': hashlib, 'Path': Path,
             'vec': lambda value: [namespace['finite'](x, 'coordinate', -1e12, 1e12) for x in value]}
exec(compile(ast.Module(body=[node for node in source.body if isinstance(node, (ast.FunctionDef, ast.ClassDef)) and node.name in helpers], type_ignores=[]), 'trajectory_render.py', 'exec'), namespace)


def write_json(path, value):
    path.write_text(json.dumps(value, indent=2), encoding='utf-8')


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def fixture(root):
    (root / 'trajectory').mkdir()
    write_json(root / 'topology.json', {'entities': [{'id': 'body-0'}], 'box_nm': [4., 4., 4.]})
    chunks = []
    for number in range(2):
        path = root / f'trajectory/chunk-{number}.json'
        frames = [{'time': (number * 2 + i) / 10, 'entities': [
            {'id': 'body-0', 'position': [float(number * 2 + i), 0., 0.]}]} for i in range(2)]
        write_json(path, {'frames': frames})
        chunks.append({'path': path.relative_to(root).as_posix(), 'sha256': digest(path),
                       'start_time': frames[0]['time'], 'end_time': frames[-1]['time'],
                       'start_frame': number * 2})
    index = {'chunks': chunks, 'start_time': 0., 'end_time': .3,
             'frame_count': 4, 'time_unit': 'ps', 'position_unit': 'nm'}
    write_json(root / 'trajectory/index.json', index)
    return index


def source_pin(root, index, number=0):
    snapshot = root / 'snapshot.json'
    snapshot.write_bytes((root / 'trajectory/index.json').read_bytes())
    entry = index['chunks'][number]
    return {'index_snapshot_path': str(snapshot), 'index_sha256': digest(snapshot),
            'entry_number': number, 'topology_sha256': digest(root / 'topology.json'),
            'files': {entry['path']: entry['sha256']}}


class ExportContract(unittest.TestCase):
    def test_isolated_units_and_full_retained_bounds_have_no_periodic_cell(self):
        source=SimpleNamespace(topology={'boundary':'isolated','units':{'position':'L0','time':'T0'},'entities':[{'id':'heavy','mass':3,'display_radius':.04}]},index={'boundary':'isolated','position_unit':'L0','time_unit':'T0'},chunks=[{'bounds':{'min':[-2,1,-4],'max':[3,6,0]}},{'bounds':{'min':[-8,-3,-2],'max':[1,12,5]}}],pin=None)
        source.chunk=lambda number: self.fail('Committed bounds must avoid rereading every numerical chunk')
        domain=namespace['particle_domain'](source)
        self.assertEqual(domain['min'],[-8,-3,-4]);self.assertEqual(domain['max'],[3,12,5]);self.assertFalse(domain['periodic']);self.assertEqual(domain['units'],{'position':'L0','time':'T0'})
        self.assertEqual(namespace['particle_radius'](source.topology['entities'][0]),.04)
        source.topology['box_nm']=[1,1,1]
        with self.assertRaisesRegex(ValueError,'isolated'):namespace['particle_domain'](source)

    def test_periodic_bounds_and_float64_translation_are_preserved(self):
        source=SimpleNamespace(topology={'box_nm':[2,3,4],'position_unit':'nm'},index={'wrapping':'periodic [0,L)','time_unit':'ps'})
        domain=namespace['particle_domain'](source);self.assertEqual(domain['min'],[0,0,0]);self.assertEqual(domain['max'],[2,3,4]);self.assertTrue(domain['periodic']);self.assertEqual(namespace['particle_radius']({'radius_nm':.17}),.17)
        point=namespace['normalized_position']([1000000.05,0,0],[1000000.025,0,0],80)
        self.assertAlmostEqual(point[0],2,places=8)

    def test_explicit_count_and_half_up_fallback(self):
        count = namespace['export_frame_count']
        self.assertEqual(count({'frame_count': 5}, .15, 30, 'video'), 5)
        self.assertEqual(count({}, .15, 30, 'video'), 5)
        for invalid in [4, 5.1, True, 216001]:
            with self.assertRaises(ValueError):
                count({'frame_count': invalid}, .15, 30, 'video')
        self.assertEqual(count({}, .1, 1, 'png'), 1)
        with self.assertRaises(ValueError):
            count({}, .1, 1, 'video')

    def test_only_roundoff_is_clamped(self):
        retained = namespace['retained_time']
        endpoint = 9.999999999999897
        self.assertEqual(retained(10, 0, endpoint), endpoint)
        interior = math.nextafter(endpoint, 0)
        self.assertEqual(retained(interior, 0, endpoint), interior)
        self.assertEqual(retained(1e-20 + 1e-35, 0, 1e-20), 1e-20)
        for value, end in [(10.00000001, endpoint), (1.001e-20, 1e-20), (float('nan'), endpoint)]:
            with self.assertRaises(ValueError):
                retained(value, 0, end)

    def test_round_trip_selects_exact_record_without_skipping_intervals(self):
        index = namespace['recorded_index']
        times = [0., .1, .2, 9.999999999999897]
        for number, value in enumerate(times):
            self.assertEqual(index(times, math.nextafter(value, -math.inf)), number)
        self.assertEqual(index(times, .199999), 1)
        self.assertEqual(index(times, 9.99999999), 2)
        # Even adjacent representable times remain separate saved states.
        self.assertEqual(index([1., math.nextafter(1., math.inf)], 1.), 0)

    def test_endpoint_clamping_preserves_adjacent_distinct_records(self):
        retained, index = namespace['retained_time'], namespace['recorded_index']
        times = [1., math.nextafter(1., math.inf)]
        self.assertEqual(index(times, retained(times[0], times[0], times[1])), 0)
        self.assertEqual(retained(times[0], times[0], times[1]), times[0])
        ordinary = [0., .1, .2]
        self.assertEqual(index(ordinary, retained(math.nextafter(.2, 0), 0, .2)), 2)


class TrajectorySourceContract(unittest.TestCase):
    def test_missing_malformed_or_mutated_chunk_digest_is_rejected(self):
        for bad in (None, '', 'f' * 63, 'z' * 64, '0' * 64):
            with self.subTest(digest=bad), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                index = fixture(root)
                index['chunks'][0]['sha256'] = bad
                write_json(root / 'trajectory/index.json', index)
                with self.assertRaisesRegex(ValueError, 'digest'):
                    namespace['Trajectory'](root).sample(0.)

    def test_exact_hold_does_not_open_the_next_chunk(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            index = fixture(root)
            (root / index['chunks'][1]['path']).unlink()
            source = namespace['Trajectory'](root)
            positions, receipt = source.sample(.15, False)
            self.assertEqual(positions['body-0'], [1., 0., 0.])
            self.assertEqual(receipt['source_times'], [.1, .1])
            self.assertEqual(set(source.hashes), {index['chunks'][0]['path']})
            with self.assertRaises(FileNotFoundError):
                source.sample(.15, True)

    def test_pin_accepts_append_and_uses_frozen_index(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            original = fixture(root)
            pin = source_pin(root, original)
            live = copy.deepcopy(original)
            live['chunks'].append({'path': 'trajectory/later.json', 'sha256': 'a' * 64,
                                   'start_time': .4, 'end_time': .5, 'start_frame': 4})
            live.update(end_time=.5, frame_count=6)
            write_json(root / 'trajectory/index.json', live)
            source = namespace['Trajectory'](root, pin)
            self.assertEqual(source.index, original)
            self.assertEqual(source.index_sha256, pin['index_sha256'])
            self.assertEqual(source.sample(.15)[1]['display_time'], .1)
            with self.assertRaisesRegex(ValueError, 'different pinned'):
                source.sample(.25)

    def test_pin_rejects_changed_metadata_entry_snapshot_and_topology(self):
        for mutation in ('metadata', 'entry', 'snapshot', 'topology', 'missing_topology'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                index = fixture(root)
                pin = source_pin(root, index)
                if mutation == 'metadata':
                    index['position_unit'] = 'm'
                elif mutation == 'entry':
                    index['chunks'][0]['end_time'] = .099
                elif mutation == 'snapshot':
                    (root / 'snapshot.json').write_bytes(b'{}')
                elif mutation == 'topology':
                    write_json(root / 'topology.json', {'entities': []})
                else:
                    del pin['topology_sha256']
                write_json(root / 'trajectory/index.json', index)
                with self.assertRaises(ValueError):
                    namespace['Trajectory'](root, pin)

    def test_pin_requires_exact_loaded_file_bytes(self):
        for mutation in ('pin_digest', 'source_bytes', 'missing_file_pin'):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                index = fixture(root)
                pin = source_pin(root, index)
                path = index['chunks'][0]['path']
                if mutation == 'pin_digest':
                    pin['files'][path] = '0' * 64
                elif mutation == 'missing_file_pin':
                    pin['files'] = {'trajectory/unrelated.json': '0' * 64}
                source = namespace['Trajectory'](root, pin)
                if mutation == 'source_bytes':
                    (root / path).write_text('{}', encoding='utf-8')
                with self.assertRaisesRegex(ValueError, 'digest'):
                    source.sample(.1)


if __name__ == '__main__':
    unittest.main()
