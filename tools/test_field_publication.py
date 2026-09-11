"""Actual Windows publication contention, with retained evidence and no skips.

Uses real CreateFileW handles that omit FILE_SHARE_DELETE, the shipped worker's
publication functions, and a real diffusion subprocess. No os.replace mocking.
Run with the pinned science Python and --output <new evidence directory>.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
import unittest
import uuid

import numpy as np


OUTPUT = None
WORKER = Path(__file__).with_name('field_worker.py').resolve()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def encode(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False).encode()


def save(path, value):
    path.write_bytes(encode(value))


def windows_api():
    if os.name != 'nt':
        raise RuntimeError('These tests require actual Windows file handles; skipping is prohibited')
    api = ctypes.WinDLL('kernel32', use_last_error=True)
    api.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p,
                               wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
    api.CreateFileW.restype = wintypes.HANDLE
    api.CloseHandle.argtypes = [wintypes.HANDLE]
    api.CloseHandle.restype = wintypes.BOOL
    api.GetFileSizeEx.argtypes = [wintypes.HANDLE, ctypes.POINTER(ctypes.c_longlong)]
    api.GetFileSizeEx.restype = wintypes.BOOL
    api.SetFilePointerEx.argtypes = [wintypes.HANDLE, ctypes.c_longlong, ctypes.c_void_p, wintypes.DWORD]
    api.SetFilePointerEx.restype = wintypes.BOOL
    api.ReadFile.argtypes = [wintypes.HANDLE, ctypes.c_void_p, wintypes.DWORD,
                            ctypes.POINTER(wintypes.DWORD), ctypes.c_void_p]
    api.ReadFile.restype = wintypes.BOOL
    api.GetFileAttributesW.argtypes = [wintypes.LPCWSTR]
    api.GetFileAttributesW.restype = wintypes.DWORD
    api.SetFileAttributesW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD]
    api.SetFileAttributesW.restype = wintypes.BOOL
    return api


class NativeReader:
    def __init__(self, path, allow_delete=False):
        self.api = windows_api()
        share = 1 | 2 | (4 if allow_delete else 0)
        self.handle = self.api.CreateFileW(str(path), 0x80000000, share, None, 3, 0x80, None)
        if self.handle == ctypes.c_void_p(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())

    def read(self):
        size = ctypes.c_longlong()
        if not self.api.GetFileSizeEx(self.handle, ctypes.byref(size)):
            raise ctypes.WinError(ctypes.get_last_error())
        if not 0 <= size.value <= 16 * 1024 * 1024:
            raise ValueError('Unexpected publication test file size')
        if not self.api.SetFilePointerEx(self.handle, 0, None, 0):
            raise ctypes.WinError(ctypes.get_last_error())
        buffer = ctypes.create_string_buffer(max(1, size.value))
        count = wintypes.DWORD()
        if not self.api.ReadFile(self.handle, buffer, size.value, ctypes.byref(count), None):
            raise ctypes.WinError(ctypes.get_last_error())
        if count.value != size.value:
            raise AssertionError('A native reader observed a partial file')
        return buffer.raw[:count.value]

    def close(self):
        if self.handle is not None:
            handle, self.handle = self.handle, None
            if not self.api.CloseHandle(handle):
                raise ctypes.WinError(ctypes.get_last_error())

    def __enter__(self):
        return self

    def __exit__(self, *unused):
        self.close()


def numeric_bytes(kind, offset):
    stream = io.BytesIO()
    array = np.arange(1024, dtype=np.float64).reshape(32, 32) + offset
    if kind == 'npy':
        np.save(stream, array, allow_pickle=False)
    else:
        np.savez(stream, field=array)
    return stream.getvalue()


class PublicationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        windows_api()  # Unsupported hosts fail; there are no platform skips.
        cls.root = OUTPUT or (WORKER.parents[1] / '.local/validation' / ('field-publication-' + uuid.uuid4().hex))
        cls.root.mkdir(parents=True, exist_ok=False)
        spec = importlib.util.spec_from_file_location('field_publication_worker', WORKER)
        cls.worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.worker)
        (cls.root / 'worker-source.py').write_bytes(WORKER.read_bytes())
        (cls.root / 'test-source.py').write_bytes(Path(__file__).read_bytes())

    def setUp(self):
        self.folder = self.root / self._testMethodName
        self.folder.mkdir()
        self.receipt = {'test': self._testMethodName, 'started_unix_s': time.time(),
                        'worker_sha256': sha(WORKER.read_bytes()), 'real_windows_handles': True,
                        'replace_mocked': False}

    def tearDown(self):
        self.receipt['finished_unix_s'] = time.time()
        save(self.folder / 'receipt.json', self.receipt)

    def confirm_native_denial(self, path):
        probe = path.with_name(path.name + '.replace-probe')
        probe.write_bytes(b'Never published: deterministic OS denial probe')
        with self.assertRaises(OSError) as raised:
            os.replace(probe, path)
        self.assertIn(raised.exception.winerror, (5, 32, 33))
        self.receipt['native_replace_probe_winerror'] = raised.exception.winerror

    def transient(self, kind):
        path = self.folder / ('progress.json' if kind == 'json' else 'state.' + kind)
        old_value, new_value = {'generation': 0, 'payload': 'old' * 20000}, {'generation': 1, 'payload': 'new' * 20000}
        old = encode(old_value) if kind == 'json' else numeric_bytes(kind, 0)
        new = encode(new_value) if kind == 'json' else numeric_bytes(kind, 1)
        path.write_bytes(old)
        temporary = path.with_name(path.name + '.tmp')
        errors, observations, read_errors, reader_failures = [], [], [], []
        self.receipt.update(reader_sharing_errors=read_errors, reader_failures=reader_failures)
        stopped = threading.Event()
        def watch():
            while not stopped.is_set():
                try:
                    with NativeReader(path, allow_delete=True) as handle:
                        observations.append(handle.read())
                except OSError as error:
                    read_errors.append(getattr(error, 'winerror', None))
                except BaseException as error:
                    reader_failures.append(repr(error))
                    break
                time.sleep(.002)
        def publish():
            try:
                if kind == 'json':
                    self.worker.write_json(path, new_value)
                else:
                    with temporary.open('wb') as stream:
                        stream.write(new); stream.flush(); os.fsync(stream.fileno())
                    self.worker.replace_published(temporary, path)
            except BaseException as error:
                errors.append(error)
        observer, writer = threading.Thread(target=watch, daemon=True), threading.Thread(target=publish, daemon=True)
        started = time.monotonic()
        try:
            with NativeReader(path) as blocker:
                self.confirm_native_denial(path)
                observer.start(); writer.start()
                deadline = time.monotonic() + 1
                while not temporary.exists() and writer.is_alive() and time.monotonic() < deadline:
                    time.sleep(.002)
                time.sleep(.15)
                self.assertTrue(writer.is_alive(), f'Publication did not retry under the real handle: {errors}')
                self.assertEqual(blocker.read(), old)
        finally:
            if writer.ident is not None:
                writer.join(4)
            time.sleep(.02)
            stopped.set()
            if observer.ident is not None:
                observer.join(2)
        self.assertFalse(writer.is_alive(), 'Publication exceeded its bounded retry')
        self.assertFalse(observer.is_alive())
        self.assertEqual(errors, [])
        self.assertEqual(reader_failures, [])
        self.assertEqual(path.read_bytes(), new)
        self.assertFalse(temporary.exists())
        self.assertTrue(observations)
        self.assertEqual(set(observations), {old, new}, 'Reader saw partial/mixed bytes or missed a generation')
        if kind == 'json':
            self.assertTrue(all(json.loads(raw) in (old_value, new_value) for raw in observations))
        self.receipt.update(elapsed_s=time.monotonic()-started, released_after_s=.15,
                            observed_generations=2, whole_file_reads=len(observations),
                            reader_sharing_errors=read_errors, old_sha256=sha(old), new_sha256=sha(new),
                            final_sha256=sha(path.read_bytes()), passed=True)

    def test_transient_json_reader_blocks_then_releases(self):
        self.transient('json')

    def test_transient_npy_reader_blocks_then_releases(self):
        self.transient('npy')

    def test_transient_checkpoint_npz_reader_blocks_then_releases(self):
        self.transient('npz')

    def persistent(self, readonly=False, kind='json'):
        path = self.folder / ('progress.json' if kind == 'json' else 'checkpoint.npz')
        old = encode({'generation': 0}) if kind == 'json' else numeric_bytes('npz', 0)
        new = encode({'generation': 1}) if kind == 'json' else numeric_bytes('npz', 1)
        temporary = path.with_name(path.name + '.tmp')
        path.write_bytes(old); temporary.write_bytes(new)
        api, attributes, blocker = windows_api(), None, None
        try:
            if readonly:
                attributes = api.GetFileAttributesW(str(path))
                self.assertNotEqual(attributes, 0xFFFFFFFF)
                self.assertTrue(api.SetFileAttributesW(str(path), attributes | 1))
                self.assertTrue(api.GetFileAttributesW(str(path)) & 1)
            else:
                blocker = NativeReader(path)
                self.confirm_native_denial(path)
            started = time.monotonic()
            with self.assertRaises(OSError) as raised:
                self.worker.replace_published(temporary, path)
            elapsed = time.monotonic() - started
            self.assertIn(raised.exception.winerror, (5, 32, 33))
            self.assertLess(elapsed, self.worker.PUBLICATION_RETRY_SECONDS + 1.5)
            self.assertEqual(path.read_bytes(), old)
            self.assertEqual(temporary.read_bytes(), new)
            self.receipt.update(elapsed_s=elapsed, winerror=raised.exception.winerror,
                                denial='real readonly target attribute' if readonly else 'persistent native sharing handle',
                                old_sha256=sha(old), preserved_target_sha256=sha(path.read_bytes()),
                                complete_temporary_sha256=sha(temporary.read_bytes()), passed=True)
        finally:
            if blocker is not None:
                blocker.close()
            if attributes is not None and not api.SetFileAttributesW(str(path), attributes):
                raise ctypes.WinError(ctypes.get_last_error())

    def test_persistent_sharing_denial_preserves_json(self):
        self.persistent()

    def test_real_readonly_denial_preserves_json(self):
        self.persistent(readonly=True)

    def test_real_readonly_denial_preserves_checkpoint(self):
        self.persistent(readonly=True, kind='npz')

    def test_real_diffusion_subprocess_with_native_progress_readers(self):
        out = self.folder / 'output'
        inp = self.folder / 'input.json'
        parameters = {'nx': 32, 'ny': 32, 'length_x_um': 10., 'length_y_um': 10.,
                      'diffusivity_um2_s': .2, 'dt_s': .02, 'steps': 128, 'record_interval': 1,
                      'initial': {'kind': 'fourier', 'baseline': 1., 'amplitude': .2, 'mode_x': 1, 'mode_y': 2},
                      'boundary': 'periodic'}
        save(inp, {'engine': 'diffusion_2d', 'parameters': parameters})
        command = [sys.executable, str(WORKER), '--input', str(inp), '--output', str(out)]
        self.receipt.update(command=command, input_sha256=sha(inp.read_bytes()))
        progress_path, samples, read_errors, reader_failures = out / 'progress.json', [], [], []
        self.receipt.update(reader_sharing_errors=read_errors, reader_failures=reader_failures)
        stopped = threading.Event()
        def watch():
            while not stopped.is_set():
                try:
                    with NativeReader(progress_path) as reader:
                        time.sleep(.003)
                        samples.append(json.loads(reader.read()))
                except OSError as error:
                    read_errors.append(getattr(error, 'winerror', None))
                except BaseException as error:
                    reader_failures.append(repr(error))
                    break
                time.sleep(.008)
        observer = threading.Thread(target=watch, daemon=True)
        started, process = time.monotonic(), None
        with (self.folder / 'stdout.txt').open('wb') as stdout, (self.folder / 'stderr.txt').open('wb') as stderr:
            try:
                process = subprocess.Popen(command, stdout=stdout, stderr=stderr)
                deadline = time.monotonic() + 30
                while not progress_path.exists() and process.poll() is None and time.monotonic() < deadline:
                    time.sleep(.001)
                self.assertTrue(progress_path.exists(), 'Worker published no initial progress')
                with NativeReader(progress_path) as blocker:
                    previous = blocker.read()
                    old = json.loads(previous)
                    self.assertLess(old['step'], parameters['steps'])
                    self.confirm_native_denial(progress_path)
                    pending = out / 'progress.json.tmp'
                    deadline = time.monotonic() + 1
                    while not pending.exists() and process.poll() is None and time.monotonic() < deadline:
                        time.sleep(.001)
                    self.assertTrue(pending.exists(), 'Worker did not attempt publication under the native lock')
                    time.sleep(.15)
                    self.assertIsNone(process.poll(), 'Worker failed while a transient reader held progress')
                    self.assertEqual(blocker.read(), previous)
                    self.receipt.update(forced_block_step=old['step'], pending_complete_json=json.loads(pending.read_bytes()))
                observer.start()
                exit_code = process.wait(timeout=60)
                self.receipt.update(exit_code=exit_code, elapsed_s=time.monotonic()-started)
                self.assertEqual(exit_code, 0, (self.folder / 'stderr.txt').read_text()[-2000:])
            finally:
                if process is not None and process.poll() is None:
                    process.kill(); process.wait()
                stopped.set()
                if observer.ident is not None:
                    observer.join(3)
        self.assertTrue(samples, 'No concurrent native reader samples')
        self.assertFalse(observer.is_alive())
        self.assertEqual(reader_failures, [], 'Native reader observed incomplete/invalid progress JSON')
        self.assertTrue(all(isinstance(row, dict) and 0 <= row['step'] <= parameters['steps'] for row in samples))
        result = json.loads((out / 'result.json').read_bytes())
        self.assertEqual(result['status'], 'completed')
        index = json.loads((out / 'fields/index.json').read_bytes())
        self.assertEqual([row['step'] for row in index['frames']], list(range(129)))
        for frame in index['frames']:
            self.assertEqual(sha((out / frame['path']).read_bytes()), frame['sha256'])
        checkpoint = json.loads((out / 'checkpoint.json').read_bytes())
        self.assertEqual(checkpoint['checkpoint_sha256'], sha((out / 'checkpoint.npz').read_bytes()))
        self.receipt.update(whole_json_progress_reads=len(samples), observed_steps=sorted({row['step'] for row in samples}),
                            reader_sharing_errors=read_errors, retained_frames=129,
                            result_sha256=sha((out / 'result.json').read_bytes()), passed=True)


def main():
    global OUTPUT
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    OUTPUT = args.output.resolve()
    if OUTPUT.exists():
        parser.error('Use a new evidence directory; prior attempts are preserved')
    started = time.monotonic()
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(PublicationTests))
    report = {'passed': result.wasSuccessful(), 'tests_run': result.testsRun, 'skipped': result.skipped,
              'failures': [(str(test), detail) for test, detail in result.failures],
              'errors': [(str(test), detail) for test, detail in result.errors],
              'worker_sha256': sha(WORKER.read_bytes()), 'test_sha256': sha(Path(__file__).read_bytes()),
              'elapsed_s': time.monotonic()-started, 'os_name': os.name, 'python': sys.version,
              'native_windows_api': 'CreateFileW; FILE_SHARE_READ|FILE_SHARE_WRITE; FILE_SHARE_DELETE deliberately absent',
              'replace_mocked': False}
    OUTPUT.mkdir(parents=True, exist_ok=True)
    save(OUTPUT / 'report.json', report)
    print(str(OUTPUT / 'report.json'))
    return 0 if report['passed'] and not report['skipped'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
