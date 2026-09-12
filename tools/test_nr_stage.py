"""Data-only Linux/WSL staging fixtures. No engine or supervisor is launched."""
from __future__ import annotations

import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
import uuid


STAGER = Path(__file__).with_name("nr_stage.py").resolve()
if sys.platform.startswith("linux"):
    spec = importlib.util.spec_from_file_location("nr_stage", STAGER)
    nr = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(nr)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


@unittest.skipUnless(sys.platform.startswith("linux"), "Requires Linux private-path and no-follow semantics")
class StagingTests(unittest.TestCase):
    def setUp(self):
        # /tmp is intentionally unsuitable: its ancestor is world-writable.
        self.temp = tempfile.TemporaryDirectory(prefix=".phaseforge-nr-stage-test-", dir=Path.home())
        self.root = Path(self.temp.name)
        self.root.chmod(0o700)
        self.jobs = self.root / "jobs"
        self.jobs.mkdir(mode=0o700)
        nr.private_root(self.jobs)
        self.job_id = str(uuid.uuid4())
        self.packet = self.packet_for(self.job_id)

    def tearDown(self):
        self.temp.cleanup()

    def packet_for(self, job_id, text="<comment>\nsynthetic staging fixture — μ, not numerical evidence\n"):
        return {"request": {
            "schema": "phaseforge.nr-request.v1", "engine_id": "synthetic_staging_fixture",
            "job_id": job_id, "engine_manifest_sha256": "a" * 64,
            "input": {"path": "input.athinput", "sha256": digest(text.encode("utf-8"))},
            "output_dir": str(self.jobs / job_id / "work"), "cpu_threads": 2,
            "memory_limit_bytes": 256 * 1024**2, "output_limit_bytes": 64 * 1024**2,
            "deadline_at": None,
        }, "input_utf8": text}

    def rejected_without_attempt(self, packet, job_id=None):
        before = sorted(item.name for item in self.jobs.iterdir())
        with self.assertRaises((ValueError, TypeError, OSError)):
            nr.prepare(self.jobs, job_id or self.job_id, packet)
        self.assertEqual(sorted(item.name for item in self.jobs.iterdir()), before)

    def run_cli(self, raw, job_id=None):
        return subprocess.run(
            [sys.executable, "-I", "-B", str(STAGER), "--job-root", str(self.jobs),
             "--job-id", job_id or self.job_id],
            input=raw, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5, check=False,
        )

    def test_exact_utf8_and_canonical_request_are_pinned_and_private(self):
        receipt = nr.prepare(self.jobs, self.job_id, self.packet)
        directory = self.jobs / self.job_id
        raw = self.packet["input_utf8"].encode("utf-8")
        canonical = (json.dumps(self.packet["request"], sort_keys=True, separators=(",", ":"), allow_nan=False) + "\n").encode()
        self.assertEqual((directory / "input.athinput").read_bytes(), raw)
        self.assertEqual((directory / "request.json").read_bytes(), canonical)
        self.assertEqual(receipt["input_sha256"], digest(raw))
        self.assertEqual(receipt["request_sha256"], digest(canonical))
        self.assertEqual(receipt["directory"], str(directory))
        self.assertEqual(receipt["job_id"], self.job_id)
        self.assertEqual(receipt["schema"], "phaseforge.nr-staging.v1")
        self.assertIs(receipt["executed"], False)
        self.assertEqual(sorted(item.name for item in directory.iterdir()), ["input.athinput", "request.json"])
        self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
        for path in directory.iterdir():
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
            self.assertEqual(path.stat().st_nlink, 1)

    def test_v2_physical_memory_policy_is_retained_exactly_without_admitting_execution(self):
        self.packet["request"]["schema"] = "phaseforge.nr-request.v2"
        policy = {"backend": "cpu", "memory_boundary": "systemd_cgroup_v2",
                  "address_space_limit_bytes": None, "tasks_max": 128,
                  "gpu_vram_policy": {"mode": "not_admitted"}}
        self.packet["request"]["execution_policy"] = policy
        receipt = nr.prepare(self.jobs, self.job_id, self.packet)
        raw = (self.jobs / self.job_id / "request.json").read_bytes()
        self.assertEqual(json.loads(raw), self.packet["request"])
        self.assertEqual(receipt["request_sha256"], digest(raw))
        self.assertIs(receipt["executed"], False)
        self.assertFalse((self.jobs / self.job_id / "work").exists())

    def test_completed_staging_cannot_be_reused_or_overwritten(self):
        nr.prepare(self.jobs, self.job_id, self.packet)
        directory = self.jobs / self.job_id
        before = {path.name: path.read_bytes() for path in directory.iterdir()}
        for packet in (self.packet, self.packet_for(self.job_id, "different valid data\n")):
            with self.subTest(input=packet["input_utf8"]):
                with self.assertRaises(FileExistsError):
                    nr.prepare(self.jobs, self.job_id, packet)
                self.assertEqual({path.name: path.read_bytes() for path in directory.iterdir()}, before)

    def test_interrupted_partial_attempt_remains_immutable(self):
        original_open = nr.os.open

        def interrupt_second_file(path, *args, **kwargs):
            if Path(path).name == "request.json":
                raise OSError("synthetic interruption before request write")
            return original_open(path, *args, **kwargs)

        with mock.patch.object(nr.os, "open", side_effect=interrupt_second_file):
            with self.assertRaisesRegex(OSError, "synthetic interruption"):
                nr.prepare(self.jobs, self.job_id, self.packet)
        directory = self.jobs / self.job_id
        original = (directory / "input.athinput").read_bytes()
        self.assertEqual(sorted(path.name for path in directory.iterdir()), ["input.athinput"])
        with self.assertRaises(FileExistsError):
            nr.prepare(self.jobs, self.job_id, self.packet_for(self.job_id, "new request"))
        self.assertEqual((directory / "input.athinput").read_bytes(), original)

    def test_identity_hash_output_and_fixed_input_filename_must_agree(self):
        request_changes = [
            ("schema", "other-schema"), ("job_id", str(uuid.uuid4())),
            ("output_dir", str(self.root / "outside")),
            ("output_dir", str(self.jobs / self.job_id / ".." / "work")),
            ("input", {"path": "input.athinput", "sha256": "b" * 64}),
            ("input", {"path": "../input.athinput", "sha256": self.packet["request"]["input"]["sha256"]}),
            ("input", {**self.packet["request"]["input"], "other": True}),
        ]
        for key, value in request_changes:
            with self.subTest(key=key, value=value):
                packet = copy.deepcopy(self.packet)
                packet["request"][key] = value
                self.rejected_without_attempt(packet)

    def test_exact_packet_types_and_input_byte_limits(self):
        for packet in [[], None, {"request": {}}, {**self.packet, "command": "ignored"},
                       {"request": [], "input_utf8": "text"}, {"request": {}, "input_utf8": 3}]:
            with self.subTest(packet=packet):
                self.rejected_without_attempt(packet)
        for text in ("", "contains\0nul", "x" * (128 * 1024 + 1), "μ" * (64 * 1024 + 1)):
            with self.subTest(length=len(text)):
                self.rejected_without_attempt(self.packet_for(self.job_id, text))

    def test_large_or_nonfinite_request_is_rejected_before_creation(self):
        for value in ("x" * (64 * 1024), float("nan"), float("inf")):
            packet = copy.deepcopy(self.packet)
            packet["request"]["extra_fixture_field"] = value
            self.rejected_without_attempt(packet)

    def test_job_uuid_is_canonical_and_cannot_traverse(self):
        for job_id in ("../escape", "A" * 32, "{12345678-1234-1234-1234-123456789abc}",
                       "12345678-1234-1234-1234-123456789ABC", "12345678123412341234123456789abc"):
            with self.subTest(job_id=job_id):
                self.rejected_without_attempt(self.packet, job_id)

    def test_root_requires_private_mode_and_nonwritable_ancestors(self):
        self.jobs.chmod(0o755)
        self.rejected_without_attempt(self.packet)
        self.jobs.chmod(0o700)
        self.root.chmod(0o777)
        self.rejected_without_attempt(self.packet)
        self.root.chmod(0o700)

    def test_symlink_root_and_ancestor_are_rejected(self):
        direct = self.root / "linked-jobs"
        direct.symlink_to(self.jobs, target_is_directory=True)
        with self.assertRaises(ValueError):
            nr.prepare(direct, self.job_id, self.packet)
        actual = self.root / "actual"
        actual.mkdir(mode=0o700)
        nested_jobs = actual / "jobs"
        nested_jobs.mkdir(mode=0o700)
        ancestor = self.root / "linked-parent"
        ancestor.symlink_to(actual, target_is_directory=True)
        with self.assertRaises(ValueError):
            nr.prepare(ancestor / "jobs", self.job_id, self.packet)
        self.assertEqual(list(self.jobs.iterdir()), [])
        self.assertEqual(list(nested_jobs.iterdir()), [])

    def test_existing_job_symlink_never_writes_into_target(self):
        target = self.root / "other-data"
        target.mkdir(mode=0o700)
        marker = target / "preserve.txt"
        marker.write_bytes(b"prior fixture data")
        (self.jobs / self.job_id).symlink_to(target, target_is_directory=True)
        with self.assertRaises(FileExistsError):
            nr.prepare(self.jobs, self.job_id, self.packet)
        self.assertEqual([(path.name, path.read_bytes()) for path in target.iterdir()], [("preserve.txt", b"prior fixture data")])

    def test_timer_and_caps_are_preserved_not_interpreted_or_widened(self):
        # Actual admission is intentionally the supervisor's job. Staging must
        # preserve both Off and even an expired deadline without changing it.
        for deadline in (None, "2026-01-01T00:00:00Z"):
            job_id = str(uuid.uuid4())
            packet = self.packet_for(job_id)
            packet["request"]["deadline_at"] = deadline
            nr.prepare(self.jobs, job_id, packet)
            retained = json.loads((self.jobs / job_id / "request.json").read_bytes())
            self.assertEqual(retained, packet["request"])

    def test_input_remains_data_and_no_work_or_engine_is_created(self):
        marker = self.root / "must-not-exist"
        text = f"#!/bin/sh\ntouch '{marker}'\n$(touch '{marker}')\n"
        packet = self.packet_for(self.job_id, text)
        nr.prepare(self.jobs, self.job_id, packet)
        self.assertFalse(marker.exists())
        self.assertFalse((self.jobs / self.job_id / "work").exists())
        self.assertEqual((self.jobs / self.job_id / "input.athinput").read_text(), text)

    def test_cli_eof_returns_exact_receipt_without_starting_execution(self):
        process = self.run_cli(json.dumps(self.packet).encode())
        self.assertEqual(process.returncode, 0, process.stderr.decode())
        receipt = json.loads(process.stdout)
        self.assertIs(receipt["executed"], False)
        self.assertEqual(receipt["request_sha256"], digest((self.jobs / self.job_id / "request.json").read_bytes()))
        self.assertFalse((self.jobs / self.job_id / "work").exists())

    def test_cli_bad_json_and_oversized_packet_fail_without_attempt(self):
        nonfinite = copy.deepcopy(self.packet)
        nonfinite["request"]["cpu_threads"] = float("nan")
        for raw in (b"{", b"\xff", b"x" * (nr.MAX_PACKET + 1), json.dumps(nonfinite).encode()):
            with self.subTest(length=len(raw)):
                process = self.run_cli(raw)
                self.assertEqual(process.returncode, 2, process.stderr.decode())
                self.assertEqual(json.loads(process.stdout)["kind"], "rejected")
                self.assertEqual(list(self.jobs.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
