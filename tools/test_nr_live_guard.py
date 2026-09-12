"""Synthetic retained-format guard fixtures; never start an NR engine.

The 624-leaf octree is a complete synthetic partition with the pilot's level
counts. Its fields test scalar reduction and file handling, not Einstein
evolution or the physical adequacy of the pilot mesh.
"""
import hashlib
import json
import math
import os
from pathlib import Path
import struct
import tempfile
import unittest
import uuid
from unittest import mock

import numpy as np

import nr_live_guard as live


HEADER = """<job>
basename = fixture
<mesh>
nx1 = 32
nx2 = 32
nx3 = 32
nghost = 2
x1min = -15
x1max = 15
x2min = -15
x2max = 15
x3min = -15
x3max = 15
<meshblock>
nx1 = 8
nx2 = 8
nx3 = 8
<mesh_refinement>
refinement = static
num_levels = 5
<refined_region1>
level = 4
x1min = 2.2
x1max = 3.8
x2min = -0.8
x2max = 0.8
x3min = -0.8
x3max = 0.8
<refined_region2>
level = 4
x1min = -3.8
x1max = -2.2
x2min = -0.8
x2max = 0.8
x3min = -0.8
x3max = 0.8
<time>
tlim = 1
nlim = 128
<z4c>
excise_chi = 0.0625
<par_end>
"""


def synthetic_leaves():
    """Refine whole cells; preserve exact coverage and never overlap leaves."""
    current = [(x, y, z, 2) for z in range(4) for y in range(4) for x in range(4)]
    leaves = []
    for refine_count in (16, 16, 24, 24):
        current.sort()
        leaves.extend(current[refine_count:])
        current = [(2*x+dx, 2*y+dy, 2*z+dz, level+1)
                   for x, y, z, level in current[:refine_count]
                   for dz in (0, 1) for dy in (0, 1) for dx in (0, 1)]
    leaves.extend(current)
    return tuple(sorted(leaves))


LEAVES = synthetic_leaves()
METRIC_NAMES = tuple('z4c_g'+name for name in ('xx', 'xy', 'xz', 'yy', 'yz', 'zz')) + ('z4c_chi', 'z4c_alpha')


def field_values(kind, *, h=2.0, m=3.0, nonfinite=None):
    """con_M is already squared; the first cell is deliberately excised."""
    if kind == 'z4c':
        arrays = [np.full((8, 8, 8), 1.0 if name[-2:] in ('xx', 'yy', 'zz') else 0.0,
                          dtype='<f4') for name in METRIC_NAMES[:6]]
        arrays += [np.ones((8, 8, 8), dtype='<f4'), np.ones((8, 8, 8), dtype='<f4')]
        arrays[6][0, 0, 0] = 0.03125
        names = METRIC_NAMES
    else:
        names = ('con_H', 'con_M')
        arrays = [np.full((8, 8, 8), h, dtype='<f4'), np.full((8, 8, 8), m*m, dtype='<f4')]
        arrays[0][0, 0, 0] = 1e6
        arrays[1][0, 0, 0] = 1e12
    if nonfinite is not None:
        # Even nonfinite data in a chi-excised cell must fail the native decoder.
        arrays[0][0, 0, 0] = nonfinite
    return names, b''.join(array.tobytes(order='C') for array in arrays)


def write_native(path, kind, *, time=0.0, cycle=0, h=2.0, m=3.0,
                 leaves=LEAVES, header=HEADER, nonfinite=None):
    params = header.encode('ascii')
    names, values = field_values(kind, h=h, m=m, nonfinite=nonfinite)
    prefix = (f'Athena binary output version=1.1\nsize of preheader=5\ntime={time:.17e}\ncycle={cycle}\n'
              f'size of location=8\nsize of variable=4\nnumber of variables={len(names)}\n'
              f'variables: {" ".join(names)}\nheader offset={len(params)}\n').encode('ascii')
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('wb') as stream:
        stream.write(prefix)
        stream.write(params)
        for x, y, z, level in leaves:
            width = 30.0 / (2**level)
            geometry = tuple(value for logical in (x, y, z)
                             for value in (-15.0+logical*width, -15.0+(logical+1)*width))
            stream.write(struct.pack('<6i4i6d', 2, 9, 2, 9, 2, 9, x, y, z, level, *geometry))
            stream.write(values)
    return path


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


class LiveGuardTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.run = self.root/'work'
        self.run.mkdir()
        self.output = self.root/'guard'
        self.now = 100.0
        self.clock = mock.patch.object(live.time, 'monotonic', side_effect=lambda: self.now)
        self.clock.start()
        self.addCleanup(self.clock.stop)
        self.addCleanup(self.temp.cleanup)
        self.guard = live.Guard(self.run, self.output)

    def pair(self, index=0, *, time=0.0, cycle=0, h=2.0, m=3.0, **kwargs):
        paths = []
        for kind in ('z4c', 'con'):
            paths.append(write_native(self.run/f'bin/fixture.{kind}.{index:05d}.bin', kind,
                                      time=time, cycle=cycle, h=h, m=m, **kwargs))
        return paths

    def poll(self, advance=0.0, *, stopped=False):
        self.now += advance
        state = self.guard.poll(stopped=stopped)
        self.assertIn(state['status'], ('waiting', 'monitoring', 'stopped', 'failed'))
        self.assertIn(state['exit_code'], (None, 0, 2, 3))
        self.assertEqual(json.loads((self.output/'state.json').read_text()), state)
        return state

    def stable(self, *, stopped=False):
        self.poll()
        return self.poll(1.01, stopped=stopped)

    def observations(self):
        return [json.loads(line) for line in (self.output/'observations.jsonl').read_text().splitlines()]

    def assert_failed(self, state):
        self.assertEqual(state['status'], 'failed')
        self.assertEqual(state['exit_code'], 2)
        self.assertTrue((self.output/'receipt.json').is_file())

    def assert_partial_stop(self, state):
        self.assertEqual(state['status'], 'stopped')
        self.assertEqual(state['exit_code'], 3)
        self.assertTrue((self.output/'receipt.json').is_file())

    def test_fixture_is_complete_partition_with_expected_level_counts(self):
        self.assertEqual(len(LEAVES), 624)
        self.assertEqual([sum(level == n for *_, level in LEAVES) for n in range(2, 7)],
                         [48, 112, 104, 168, 192])
        self.assertEqual(math.fsum((30.0/(2**level))**3 for *_, level in LEAVES), 27000.0)
        self.assertEqual(624*8**3, 319488)

    def test_output_must_be_new_and_initial_state_waits(self):
        state = self.poll()
        self.assertEqual(state['status'], 'waiting')
        self.assertIsNone(state['exit_code'])
        with self.assertRaises((ValueError, FileExistsError)):
            live.Guard(self.run, self.output)

    def test_initial_pair_requires_one_second_stability(self):
        self.pair()
        self.assertEqual(self.poll()['status'], 'waiting')
        self.assertEqual(self.poll(0.99)['status'], 'waiting')
        state = self.poll(0.02)
        self.assertEqual(state['status'], 'monitoring')
        self.assertIsNone(state['exit_code'])

    def test_scalar_masked_rms_and_retained_native_pins(self):
        paths = self.pair()
        state = self.stable()
        row = state['latest_observation']
        self.assertEqual(row['time'], 0.0)
        self.assertEqual(row['cycle'], 0)
        self.assertEqual(row['H_rms'], 2.0)
        self.assertEqual(row['M_rms'], 3.0)
        self.assertEqual(row['proper_volume'], 27000.0*511/512)
        for key, path in zip(('metric', 'constraints'), paths):
            self.assertEqual(row[key]['sha256'], sha256(path))
            self.assertEqual(row[key]['bytes'], path.stat().st_size)
        self.assertEqual(len(self.observations()), 1)
        self.poll(1.01)
        self.assertEqual(len(self.observations()), 1, 'Unchanged snapshots must not create duplicate observations')

    def test_exact_hundredfold_boundary_is_allowed_and_next_H_breaches(self):
        self.pair()
        self.assertEqual(self.stable()['status'], 'monitoring')
        self.pair(1, time=0.25, cycle=16, h=200.0, m=300.0)
        state = self.stable()
        self.assertEqual(state['status'], 'monitoring')
        self.assertEqual(state['latest_observation']['H_rms'], 200.0)
        self.assertEqual(state['latest_observation']['M_rms'], 300.0)
        self.pair(2, time=0.5, cycle=32, h=201.0, m=3.0)
        self.assert_failed(self.stable())

    def test_momentum_growth_uses_rms_not_already_squared_channel(self):
        self.pair()
        self.assertEqual(self.stable()['status'], 'monitoring')
        self.pair(1, time=0.25, cycle=16, h=2.0, m=301.0)
        self.assert_failed(self.stable())

    def test_zero_baseline_uses_nonzero_constraint_floor(self):
        self.pair(h=0.0, m=0.0)
        self.assertEqual(self.stable()['status'], 'monitoring')
        self.pair(1, time=0.25, cycle=16, h=1e-9, m=1e-9)
        self.assertEqual(self.stable()['status'], 'monitoring')
        self.pair(2, time=0.5, cycle=32, h=2e-8, m=0.0)
        self.assert_failed(self.stable())

    def test_stopped_writer_accepts_complete_pair_without_stability_delay(self):
        self.pair()
        state = self.poll(stopped=True)
        self.assertEqual(state['status'], 'stopped')
        self.assertEqual(state['exit_code'], 0)
        final = json.loads((self.output/'receipt.json').read_text())
        self.assertEqual(final['exit_code'], 0)
        self.assertEqual(state['latest_observation']['H_rms'], 2.0)

    def test_partial_pair_stays_pending_until_writer_stops(self):
        _, constraints = self.pair()
        with constraints.open('r+b') as stream:
            stream.truncate(constraints.stat().st_size-1)
        self.assertEqual(self.stable()['status'], 'waiting')
        self.assert_partial_stop(self.poll(1.01, stopped=True))

    def test_missing_constraint_partner_is_pending_then_partial(self):
        write_native(self.run/'bin/fixture.z4c.00000.bin', 'z4c')
        self.assertEqual(self.stable()['status'], 'waiting')
        self.assert_partial_stop(self.poll(1.01, stopped=True))

    def test_partial_header_stays_pending_until_writer_stops(self):
        path = self.run/'bin/fixture.z4c.00000.bin'
        path.parent.mkdir()
        path.write_bytes(b'Athena binary output version=1.1\nsize of preheader=')
        self.assertEqual(self.stable()['status'], 'waiting')
        self.assert_partial_stop(self.poll(1.01, stopped=True))

    def test_completed_partial_file_can_be_accepted_after_new_stability(self):
        _, constraints = self.pair()
        full = constraints.read_bytes()
        constraints.write_bytes(full[:-13])
        self.assertEqual(self.stable()['status'], 'waiting')
        constraints.write_bytes(full)
        self.assertEqual(self.poll()['status'], 'waiting')
        self.assertEqual(self.poll(1.01)['status'], 'monitoring')

    def test_nonfinite_complete_native_fails_even_in_excised_cell(self):
        for kind in ('z4c', 'con'):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as td:
                root = Path(td)
                work = root/'work'
                work.mkdir()
                for channel in ('z4c', 'con'):
                    write_native(work/f'bin/fixture.{channel}.00000.bin', channel,
                                 nonfinite=float('nan') if channel == kind else None)
                guard = live.Guard(work, root/'guard')
                guard.poll()
                self.now += 1.01
                self.assertEqual(guard.poll()['exit_code'], 2)

    def test_aligned_623_block_prefix_is_pending_and_never_accepted(self):
        self.pair(leaves=LEAVES[:-1])
        state = self.stable()
        self.assertEqual(state['status'], 'waiting')
        self.assertEqual(state['observed_pair_count'], 0)
        self.assertIsNone(state['latest_observation'])
        self.assertFalse(state['accuracy_validated'])
        self.assertEqual(self.observations(), [])
        state = self.poll(stopped=True)
        self.assert_partial_stop(state)
        self.assertEqual(state['observed_pair_count'], 0)
        self.assertIsNone(state['latest_observation'])
        self.assertFalse(state['accuracy_validated'])
        self.assertEqual(self.observations(), [])

    def test_missing_work_beneath_private_uuid_waits_then_observes(self):
        parent = self.root/str(uuid.uuid4())
        parent.mkdir()
        work = parent/'work'
        guard = live.Guard(work, self.root/'startup-guard')
        self.assertFalse(work.exists(), 'The read-only guard must not create the solver work directory')
        state = guard.poll()
        self.assertEqual(state['status'], 'waiting')
        self.assertIsNone(state['exit_code'])
        self.assertEqual(state['observed_pair_count'], 0)
        self.assertEqual(state['pending_files'][0]['reason'], 'source_directory_not_created')
        self.assertFalse(work.exists())
        work.mkdir()
        for kind in ('z4c', 'con'):
            write_native(work/f'bin/fixture.{kind}.00000.bin', kind)
        self.assertEqual(guard.poll()['status'], 'waiting')
        self.now += 1.01
        state = guard.poll()
        self.assertEqual(state['status'], 'monitoring')
        self.assertEqual(state['observed_pair_count'], 1)
        self.assertEqual(state['latest_observation']['H_rms'], 2.0)

    def mutate_original_during_real_decode(self, new_value):
        _, source = self.pair()
        before_sha = sha256(source)
        original_decoder = live.decoder.read_binary
        mutations = []

        def decode_then_write(frozen):
            self.assertNotEqual(Path(frozen), source)
            self.assertTrue(Path(frozen).is_relative_to(self.output/'.scratch'))
            decoded = original_decoder(frozen)
            if not mutations:
                with source.open('r+b') as stream:
                    stream.seek(-4, os.SEEK_END)
                    stream.write(struct.pack('<f', new_value))
                mutations.append(sha256(source))
            return decoded

        with mock.patch.object(live.decoder, 'read_binary', side_effect=decode_then_write):
            state = self.stable()
        self.assertEqual(len(mutations), 1)
        self.assertNotEqual(mutations[0], before_sha)
        self.assertEqual(state['status'], 'waiting')
        self.assertEqual(state['observed_pair_count'], 0)
        self.assertIsNone(state['latest_observation'])
        self.assertIsNone(self.guard.baseline)
        self.assertEqual(self.observations(), [])
        self.assertTrue(all(row['reason'] == 'source_write_in_progress' for row in state['pending_files']))
        return source

    def test_mid_decode_source_write_discards_then_accepts_new_stable_bytes(self):
        source = self.mutate_original_during_real_decode(10.0)
        self.assertEqual(self.poll()['status'], 'waiting')
        state = self.poll(1.01)
        self.assertEqual(state['status'], 'monitoring')
        self.assertEqual(state['observed_pair_count'], 1)
        self.assertEqual(len(self.observations()), 1)
        self.assertEqual(state['latest_observation']['constraints']['sha256'], sha256(source))
        self.assertEqual(state['latest_observation']['H_rms'], 2.0)
        self.assertGreater(state['latest_observation']['M_rms'], 3.0)

    def test_mid_decode_source_write_discards_then_rejects_stable_nonfinite_bytes(self):
        self.mutate_original_during_real_decode(float('nan'))
        self.assertEqual(self.poll()['status'], 'waiting')
        state = self.poll(1.01)
        self.assert_failed(state)
        self.assertEqual(state['reason'], 'invalid_native_frame')
        self.assertEqual(state['observed_pair_count'], 0)
        self.assertIsNone(state['latest_observation'])
        self.assertIsNone(self.guard.baseline)

    def test_initial_pair_requires_exact_zero_time_and_cycle(self):
        self.pair(time=0.25, cycle=16)
        self.assert_failed(self.stable())

    def test_metric_and_constraint_time_cycle_must_match_exactly(self):
        self.pair()
        write_native(self.run/'bin/fixture.con.00000.bin', 'con', time=0.0, cycle=1)
        self.assert_failed(self.stable())

    def test_later_cycle_must_increase_even_when_time_increases(self):
        self.pair()
        self.assertEqual(self.stable()['status'], 'monitoring')
        self.pair(1, time=0.25, cycle=0)
        self.assert_failed(self.stable())

    def test_wrong_recipe_header_is_rejected(self):
        self.pair(header=HEADER.replace('refinement = static', 'refinement = adaptive'))
        self.assert_failed(self.stable())

    def test_accepted_native_mutation_is_rejected(self):
        _, constraints = self.pair()
        self.assertEqual(self.stable()['status'], 'monitoring')
        with constraints.open('r+b') as stream:
            stream.seek(-4, os.SEEK_END)
            stream.write(struct.pack('<f', 10.0))
        self.assert_failed(self.poll(1.01))

    def test_accepted_native_same_size_same_mtime_mutation_is_rejected(self):
        _, constraints = self.pair()
        self.assertEqual(self.stable()['status'], 'monitoring')
        before = constraints.stat()
        with constraints.open('r+b') as stream:
            stream.seek(-4, os.SEEK_END)
            stream.write(struct.pack('<f', 10.0))
        os.utime(constraints, ns=(before.st_atime_ns, before.st_mtime_ns))
        self.assertEqual(constraints.stat().st_size, before.st_size)
        self.assertEqual(constraints.stat().st_mtime_ns, before.st_mtime_ns)
        self.assert_failed(self.poll(1.01))

    def test_oversize_native_file_is_rejected_before_decode(self):
        path = self.run/'bin/fixture.z4c.00000.bin'
        path.parent.mkdir()
        with path.open('wb') as stream:
            stream.seek(128*1024**2)
            stream.write(b'x')
        self.assert_failed(self.stable())

    def test_hardlinked_native_is_rejected(self):
        paths = self.pair()
        try:
            os.link(paths[0], self.root/'outside-native-copy.bin')
        except (OSError, NotImplementedError) as error:
            self.skipTest(f'Host cannot create hardlink fixture: {error}')
        self.assert_failed(self.stable())

    def test_symlinked_native_is_rejected(self):
        paths = self.pair()
        original = paths[0]
        target = self.root/'outside-native.bin'
        original.replace(target)
        try:
            original.symlink_to(target)
        except (OSError, NotImplementedError) as error:
            self.skipTest(f'Host cannot create symlink fixture: {error}')
        self.assert_failed(self.stable())


if __name__ == '__main__':
    unittest.main()
