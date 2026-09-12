#!/usr/bin/env python3
"""Read-only live guard for the admitted 624-block candidate02 NR recipe.

Exit 0: caller stopped, all discovered numerical pairs observed. Exit 3: caller
stopped with missing/partial pairs. Exit 2: numerical/provenance/monitor failure.
No outcome certifies accuracy, physical boost, horizons or merger. No solver is
launched or stopped here; the caller owns the stop marker and NR cancellation.
"""
from __future__ import annotations

import argparse
import contextlib
import datetime as dt
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time
import uuid


def sibling(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name+'.py'))
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


decoder = sibling('athenak_decode')
results = sibling('nr_black_hole_result')
MAX_FILE_BYTES = 128 * 1024**2
MAX_FILES = 4096
EXPECTED_BLOCKS = 624
POLL_SECONDS = 1.0
EXCISE_CHI = .0625
GROWTH_FACTOR = 100.0
INITIAL_FLOOR = 1e-10
NATIVE_NAME = re.compile(r'([A-Za-z0-9_-]+)\.(z4c|con)\.([0-9]{5})\.bin')


def require(condition, message):
    if not condition:
        raise ValueError(message)


class Pending(Exception):
    pass


class GuardFailure(Exception):
    def __init__(self, reason, detail):
        super().__init__(detail)
        self.reason = reason


def utc():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def identity(info):
    # CPython 3.13 Windows path.stat and fstat can expose different historical
    # creation/change-time meanings. Exact opened identity and content pins are
    # checked separately; never treat that mismatched ctime as a file mutation.
    return (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns,
            info.st_ctime_ns if os.name != 'nt' else None)


def plain(path, *, directory=False):
    path = Path(path)
    require(path.is_absolute() and '..' not in path.parts, 'An absolute non-aliased path is required')
    for parent in reversed((path, *path.parents)):
        info = parent.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400,
                'Linked source path is not admitted')
    info = path.lstat()
    require(stat.S_ISDIR(info.st_mode) if directory else stat.S_ISREG(info.st_mode) and info.st_nlink == 1,
            'Unexpected source type or hard link')
    return info


def digest_fd(fd):
    os.lseek(fd, 0, os.SEEK_SET)
    digest, count = hashlib.sha256(), 0
    while True:
        raw = os.read(fd, 1024**2)
        if not raw:
            break
        count += len(raw)
        require(count <= MAX_FILE_BYTES, 'Native file exceeds read bound')
        digest.update(raw)
    return digest.hexdigest(), count


@contextlib.contextmanager
def snapshot(source, scratch):
    before = plain(source)
    require(before.st_size <= MAX_FILE_BYTES, 'Native file exceeds read bound')
    fd = os.open(source, os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0))
    copy_fd, name = tempfile.mkstemp(prefix='native-', suffix='.bin', dir=scratch)
    frozen = Path(name)
    try:
        opened = os.fstat(fd)
        require(identity(opened) == identity(before) and stat.S_ISREG(opened.st_mode) and opened.st_nlink == 1,
                'Source changed type or identity during open')
        digest, count = hashlib.sha256(), 0
        with os.fdopen(copy_fd, 'wb') as copied:
            copy_fd = None
            while True:
                raw = os.read(fd, 1024**2)
                if not raw:
                    break
                count += len(raw)
                require(count <= MAX_FILE_BYTES, 'Native file grew beyond read bound')
                digest.update(raw)
                copied.write(raw)
        sha = digest.hexdigest()
        def unchanged():
            try:
                current = plain(source)
                return (identity(current) == identity(before) == identity(os.fstat(fd))
                        and digest_fd(fd) == (sha, count))
            except (OSError, ValueError):
                return False
        if count != before.st_size or not unchanged():
            raise Pending('source_write_in_progress')
        yield frozen, {'bytes': count, 'sha256': sha}, unchanged
    finally:
        os.close(fd)
        if copy_fd is not None:
            os.close(copy_fd)
        frozen.unlink(missing_ok=True)


def complete_layout(path):
    """Use the frozen decoder's header helpers before allocating any arrays.

    A decoder rejection of NaN is only numerical evidence after all 624 full
    block records have arrived. An aligned incomplete prefix is still pending.
    """
    try:
        with path.open('rb') as stream:
            require(decoder._line(stream) == 'Athena binary output version=1.1', 'Unsupported native version')
            require(int(decoder._property(stream, 'size of preheader')) == 5, 'Invalid native preheader')
            native_time = float(decoder._property(stream, 'time'))
            cycle = int(decoder._property(stream, 'cycle'))
            loc = int(decoder._property(stream, 'size of location'))
            width = int(decoder._property(stream, 'size of variable'))
            count = int(decoder._property(stream, 'number of variables'))
            names = decoder._line(stream).split()
            require(names and names[0] == 'variables:' and len(names) == count+1 and 1 <= count <= 128,
                    'Invalid variable table')
            size = int(decoder._property(stream, 'header offset'))
            require(0 < size <= decoder.MAX_HEADER_BYTES, 'Invalid native header bound')
            params = decoder.parameter_blocks(decoder._exact(stream, size).decode('utf-8'))
            body_offset = stream.tell()
    except Pending:
        raise
    except (ValueError, UnicodeError, KeyError, IndexError) as error:
        # A partial header can stop between any bytes; the trusted writer may
        # complete it on a later poll. Final caller-stop retains this as partial.
        raise Pending('incomplete_or_invalid_native_layout: '+str(error)) from error
    # A complete header establishes its recipe; an incompatible one cannot be
    # repaired by appending more block bytes and is rejected immediately.
    require(loc in (4, 8) and width in (4, 8), 'Invalid native numeric width')
    require(tuple(int(params['meshblock']['nx'+str(a)]) for a in (1,2,3)) == (8,8,8), 'Recipe requires block8')
    require(tuple(int(params['mesh']['nx'+str(a)]) for a in (1,2,3)) == (32,32,32), 'Recipe requires root32')
    require(int(params['mesh']['nghost']) == 2, 'Recipe requires two ghost cells')
    require(params['mesh_refinement']['refinement'] == 'static' and int(params['mesh_refinement']['num_levels']) == 5,
            'Recipe requires five static levels')
    require(float(params['z4c']['excise_chi']) == EXCISE_CHI, 'Excision threshold differs')
    expected = body_offset + EXPECTED_BLOCKS * (40 + 6*loc + 8**3 * count * width)
    actual = path.stat().st_size
    if actual < expected:
        raise Pending('partial_native_frame')
    require(actual == expected, 'Native frame does not contain exactly 624 full active blocks')
    require(math.isfinite(native_time) and native_time >= 0 and cycle >= 0, 'Invalid native time/cycle')
    return {'time': native_time, 'cycle': cycle}


def mesh_identity(frame):
    return hashlib.sha256(json.dumps({'root_shape': frame['root_shape'], 'block_shape': frame['block_shape'],
        'nghost': frame['nghost'], 'blocks': sorted((b['logical'], b['index'], b['geometry']) for b in frame['blocks'])},
        separators=(',', ':'), allow_nan=False).encode()).hexdigest()


class Guard:
    def __init__(self, run, output):
        self.run, self.output = Path(run), Path(output)
        require(self.run.is_absolute() and self.output.is_absolute(), 'Absolute run/output paths required')
        require(not self.output.is_relative_to(self.run) and not self.run.is_relative_to(self.output),
                'Monitor output must be outside the source tree')
        plain(self.output.parent, directory=True)
        if self.run.exists():
            plain(self.run, directory=True)
        else:
            plain(self.run.parent, directory=True)
            require(self.run.name == 'work' and str(uuid.UUID(self.run.parent.name)) == self.run.parent.name,
                    'Only the fixed work child of an existing private job may be awaited')
        self.output.mkdir(mode=0o700)
        self.scratch = self.output/'.scratch'
        self.scratch.mkdir(mode=0o700)
        (self.output/'observations.jsonl').touch(exist_ok=False)
        self.seen, self.accepted, self.observed, self.ever_seen = {}, {}, {}, set()
        self.baseline, self.layout, self.last_ordinal = None, None, -1
        self.chain = None
        self.state = {'schema': 'phaseforge.nr-live-guard.v1', 'status': 'waiting', 'exit_code': None,
            'reason': None, 'detail': None, 'observed_pair_count': 0, 'last_time': None, 'last_cycle': None,
            'latest_observation': None, 'pending_files': [], 'pending_file_count': 0, 'accuracy_validated': False,
            'guard_thresholds': {'expected_blocks': EXPECTED_BLOCKS, 'static_mesh': True, 'excise_chi': EXCISE_CHI,
                'growth_factor': GROWTH_FACTOR, 'initial_rms_floor': INITIAL_FLOOR, 'poll_interval_seconds': POLL_SECONDS,
                'native_file_limit_bytes': MAX_FILE_BYTES, 'all_source_file_count_limit': MAX_FILES},
            'source_sha256': {name: decoder.sha256(Path(__file__).with_name(name)) for name in
                ('nr_live_guard.py','athenak_decode.py','nr_black_hole_result.py')}}
        self.emit()

    def atomic(self, name, value):
        raw = (json.dumps(value, allow_nan=False, separators=(',', ':'))+'\n').encode()
        require(len(raw) <= 65536, 'Monitor state exceeds bound')
        target, temporary = self.output/name, self.output/(name+'.tmp')
        with temporary.open('xb') as stream:
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        for attempt in range(20):
            try:
                os.replace(temporary, target)
                return
            except PermissionError:
                if attempt == 19:
                    raise
                time.sleep(.05)

    def emit(self):
        self.state['updated_at'] = utc()
        self.atomic('state.json', self.state)

    def append(self, value):
        value = {**value, 'previous_entry_sha256': self.chain}
        raw = (json.dumps(value, allow_nan=False, separators=(',', ':'))+'\n').encode()
        require(len(raw) <= 16384, 'Observation exceeds bound')
        plain(self.output/'observations.jsonl')
        with (self.output/'observations.jsonl').open('ab') as stream:
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        self.chain = hashlib.sha256(raw).hexdigest()

    def finish(self, status, code, reason=None, detail=None):
        self.state.update(status=status, exit_code=code, reason=reason, detail=detail)
        self.emit()
        self.atomic('receipt.json', {**self.state, 'observations': {'path': 'observations.jsonl',
            'sha256': decoder.sha256(self.output/'observations.jsonl'),
            'bytes': (self.output/'observations.jsonl').stat().st_size, 'last_entry_sha256': self.chain}})
        return self.state

    def inventory(self):
        plain(self.run, directory=True)
        files, folders, stack = {}, 0, [(self.run, 0)]
        while stack:
            directory, depth = stack.pop()
            require(depth <= 8, 'Source tree depth exceeds bound')
            with os.scandir(directory) as rows:
                for row in rows:
                    path = Path(row.path)
                    info = path.lstat()
                    require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400,
                            'Linked source member')
                    if stat.S_ISDIR(info.st_mode):
                        folders += 1
                        require(folders <= MAX_FILES, 'Source directory count exceeds bound')
                        stack.append((path, depth+1))
                    else:
                        require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1, 'Non-file or hard-linked source member')
                        name = path.relative_to(self.run).as_posix()
                        self.ever_seen.add(name)
                        require(len(self.ever_seen) <= MAX_FILES, 'Observed source file count exceeds bound')
                        if name.endswith('.bin'):
                            require(depth == 1 and path.parent.name == 'bin' and NATIVE_NAME.fullmatch(path.name),
                                    'Unexpected native output path')
                            require(info.st_size <= MAX_FILE_BYTES, 'Native file exceeds read bound')
                            files[name] = info
        for name, stored in self.accepted.items():
            if name not in files or identity(files[name]) != stored['identity']:
                raise GuardFailure('source_mutated', 'An already observed numerical source changed: '+name)
            # Windows creation time is not a reliable content-change counter;
            # preserve the pin even if a writer restores the old mtime/length.
            source = self.run/name
            fd = os.open(source, os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0))
            try:
                if (identity(os.fstat(fd)) != stored['identity'] or os.fstat(fd).st_nlink != 1
                        or digest_fd(fd) != (stored['sha256'], stored['bytes'])
                        or identity(plain(source)) != stored['identity']):
                    raise GuardFailure('source_mutated', 'An already observed numerical source pin changed: '+name)
            finally:
                os.close(fd)
        return files

    def observe(self, ordinal, pair):
        names = {kind: pair[kind] for kind in ('z4c','con')}
        with contextlib.ExitStack() as stack:
            snapshots = {kind: stack.enter_context(snapshot(self.run/name, self.scratch)) for kind,name in names.items()}
            error, row, headers = None, None, {}
            try:
                for kind, (frozen, _, _) in snapshots.items():
                    headers[kind] = complete_layout(frozen)
                metric = decoder.read_binary(snapshots['z4c'][0])
                constraints = decoder.read_binary(snapshots['con'][0])
                require(len(metric['blocks']) == EXPECTED_BLOCKS == len(constraints['blocks']), 'Expected exactly 624 blocks')
                require(ordinal == self.last_ordinal+1, 'A native numerical pair was skipped')
                if self.baseline is None:
                    require(metric['time'] == 0 and metric['cycle'] == 0, 'Initial time-zero state is missing')
                else:
                    require(metric['time'] > self.state['last_time'] and metric['cycle'] > self.state['last_cycle'],
                            'Native time/cycle is not strictly increasing')
                layout = mesh_identity(metric)
                require(mesh_identity(constraints) == layout and (self.layout is None or self.layout == layout), 'Static mesh layout changed')
                row = results.reduce_constraints(metric, constraints, EXCISE_CHI)
                require(snapshots['z4c'][1]['sha256'] == metric['sha256'] and snapshots['con'][1]['sha256'] == constraints['sha256'],
                        'Decoder source pin mismatch')
            except (ValueError, KeyError, OverflowError, Pending) as caught:
                error = caught
            if not all(check() for _, _, check in snapshots.values()):
                raise Pending('source_write_in_progress')
            if error is not None:
                if isinstance(error, Pending):
                    raise error
                self.state['failure_evidence'] = {'ordinal': ordinal, 'headers': headers,
                    'metric': {'path': names['z4c'], **snapshots['z4c'][1]},
                    'constraints': {'path': names['con'], **snapshots['con'][1]}}
                raise GuardFailure('invalid_native_frame', str(error)) from error
            observation = {'ordinal': ordinal, 'time': row['time'], 'cycle': row['cycle'],
                'block_count': EXPECTED_BLOCKS, 'static_mesh_sha256': layout,
                'metric': {'path': names['z4c'], **snapshots['z4c'][1]},
                'constraints': {'path': names['con'], **snapshots['con'][1]},
                'H_rms': row['H_rms'], 'M_rms': row['M_rms'], 'proper_volume': row['proper_volume'],
                'included_cells': row['included_cells'], 'mask': row['mask'], 'units': row['units'],
                'finite': True, 'positive_physical_spatial_metric': True, 'accuracy_validated': False,
                'method': row['method'], 'observed_at': utc()}
            baseline = self.baseline or observation
            limits = {key: GROWTH_FACTOR*max(baseline[key], INITIAL_FLOOR) for key in ('H_rms','M_rms')}
            observation['rms_stop_thresholds'] = limits
            observation['growth_guard_passed'] = all(observation[key] <= limits[key] for key in limits)
            self.append(observation)
            for kind,name in names.items():
                self.accepted[name] = {'identity': identity(plain(self.run/name)), **snapshots[kind][1]}
            self.observed[ordinal] = observation
            self.baseline, self.layout, self.last_ordinal = baseline, layout, ordinal
            self.state.update(status='monitoring', observed_pair_count=len(self.observed), last_time=row['time'],
                last_cycle=row['cycle'], latest_observation=observation)
            if not observation['growth_guard_passed']:
                raise GuardFailure('numerical_guard_failed', 'H or M RMS exceeded 100*max(initial RMS,1e-10)')

    def poll(self, stopped=False):
        if self.state['exit_code'] is not None:
            return self.state
        pending = []
        try:
            if not self.run.exists() and not self.run.is_symlink():
                plain(self.run.parent, directory=True)
                pending = [{'path': 'work', 'reason': 'source_directory_not_created'}]
            else:
                files = self.inventory()
                pairs, basename = {}, None
                now = time.monotonic()
                for name, info in sorted(files.items()):
                    match = NATIVE_NAME.fullmatch(Path(name).name)
                    require(basename is None or basename == match[1], 'Multiple numerical basenames')
                    basename = match[1]
                    ordinal = int(match[3])
                    pairs.setdefault(ordinal, {})[match[2]] = name
                    previous = self.seen.get(name)
                    if previous is None or previous[0] != identity(info):
                        self.seen[name] = (identity(info), now)
                blocked = False
                for ordinal, pair in sorted(pairs.items()):
                    if ordinal in self.observed:
                        continue
                    if blocked or ordinal != self.last_ordinal+1 or set(pair) != {'z4c','con'}:
                        pending += [{'path': name, 'reason': 'missing_or_earlier_unfinished_pair'} for name in pair.values()]
                        blocked = True
                        continue
                    if not stopped and any(now-self.seen[name][1] < POLL_SECONDS for name in pair.values()):
                        pending += [{'path': name, 'reason': 'source_stability_pending'} for name in pair.values()]
                        blocked = True
                        continue
                    try:
                        self.observe(ordinal, pair)
                    except Pending as error:
                        pending += [{'path': name, 'reason': str(error)[:256]} for name in pair.values()]
                        blocked = True
                if stopped:
                    for name, saved in self.accepted.items():
                        with snapshot(self.run/name, self.scratch) as (_, current, unchanged):
                            if current['sha256'] != saved['sha256'] or not unchanged():
                                raise GuardFailure('source_mutated', 'Observed source pin changed at caller stop: '+name)
            self.state.update(pending_files=pending[:64], pending_file_count=len(pending))
            if stopped:
                partial = bool(pending) or self.baseline is None
                return self.finish('stopped', 3 if partial else 0, 'incomplete_retained_outputs' if partial else 'caller_stopped')
            self.emit()
        except GuardFailure as error:
            self.append({'event': 'failure', 'reason': error.reason, 'detail': str(error)[:512],
                'failure_evidence': self.state.get('failure_evidence'), 'observed_at': utc()})
            return self.finish('failed', 2, error.reason, str(error)[:512])
        except (OSError, ValueError, Pending) as error:
            self.append({'event': 'failure', 'reason': 'monitor_or_provenance_failure', 'detail': str(error)[:512], 'observed_at': utc()})
            return self.finish('failed', 2, 'monitor_or_provenance_failure', str(error)[:512])
        return self.state


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--stop-file', type=Path, required=True)
    args = parser.parse_args()
    require(args.stop_file.is_absolute() and not args.stop_file.is_relative_to(args.run), 'Caller stop marker must be outside source')
    plain(args.stop_file.parent, directory=True)
    guard = Guard(args.run, args.output)
    while True:
        stopped = args.stop_file.exists()
        if stopped:
            require(plain(args.stop_file).st_size <= 65536, 'Caller stop marker exceeds bound')
        state = guard.poll(stopped=stopped)
        if state['exit_code'] is not None:
            print(json.dumps({key: state[key] for key in ('status','exit_code','reason','observed_pair_count','last_time')}, allow_nan=False))
            return state['exit_code']
        time.sleep(POLL_SECONDS)


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError) as error:
        print(json.dumps({'status': 'failed', 'exit_code': 2, 'reason': 'monitor_startup_failure', 'detail': str(error)[:512]}))
        raise SystemExit(2)
