"""Pure telemetry fixtures and benign query-process bounds; no CUDA workload."""
import copy
import importlib.util
import os
from pathlib import Path
import sys
import time
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location("nr_gpu", Path(__file__).with_name("nr_gpu.py"))
gpu = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gpu)
UUID = "GPU-7e1f5065-70e6-e409-5fe4-2ffdd18b2d6e"
ROW = (UUID + ", 16376, 328, 3753, 12296\n").encode()


def policy():
    return {"mode": "device_wide_soft_guard", "device_uuid": UUID,
            "maximum_device_used_growth_bytes": 4*1024**3,
            "minimum_free_bytes": 2*1024**3, "poll_interval_seconds": 1}


def snapshot():
    with mock.patch.object(gpu, "_capture", return_value=(0, ROW, b"", None)):
        return gpu.query(UUID)


class TelemetryTests(unittest.TestCase):
    def test_exact_trusted_invocation_and_real_free(self):
        with mock.patch.object(gpu, "_capture", return_value=(0, ROW, b"", None)) as capture:
            sample = gpu.query(UUID)
        argv, env = capture.call_args.args
        self.assertEqual(argv[0], "/usr/lib/wsl/lib/nvidia-smi")
        self.assertEqual(argv[1], "--id=" + UUID)
        self.assertEqual(env, {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
        self.assertEqual(sample["status"], "known")
        self.assertEqual(sample["free_bytes"], 12296 * 1024**2)
        self.assertNotEqual(sample["free_bytes"], sample["total_bytes"]-sample["used_bytes"])
        self.assertEqual(sample["reserved_bytes"], 328 * 1024**2)
        self.assertTrue(gpu.evaluate(policy(), sample, sample)["allowed"])

    def test_bad_rows_and_failures_are_unknown(self):
        rows = [b"", ROW+ROW, ROW.replace(b"3753", b"N/A"), ROW.replace(b"328", b"N/A"),
                ROW.replace(UUID.encode(), b"GPU-other"), ROW.replace(b"12296", b"-1"),
                ROW.replace(b"12296", b"16377"), ROW.replace(b"12296", b"12.5"), b"\xff"]
        outcomes = [(0, row, b"", None) for row in rows]
        outcomes += [(6, b"No devices were found", b"", None), (0, ROW, b"warning", None),
                     (-9, b"", b"", "telemetry_timeout"), (-9, ROW, b"", "telemetry_output_limit")]
        for outcome in outcomes:
            with self.subTest(outcome=outcome), mock.patch.object(gpu, "_capture", return_value=outcome):
                sample = gpu.query(UUID)
                self.assertEqual(sample["status"], "unknown")
                self.assertIsNone(sample["free_bytes"])
                self.assertFalse(gpu.evaluate(policy(), sample, sample)["allowed"])
        with mock.patch.object(gpu, "_capture", side_effect=FileNotFoundError("driver absent")):
            self.assertEqual(gpu.query(UUID)["status"], "unknown")

    def test_reserve_growth_boundary_and_other_application_releases(self):
        base = snapshot()
        now = base["observed_monotonic"]
        current = copy.deepcopy(base)
        current["free_bytes"] = policy()["minimum_free_bytes"]
        current["used_bytes"] += policy()["maximum_device_used_growth_bytes"]
        self.assertTrue(gpu.evaluate(policy(), base, current, now_monotonic=now)["allowed"])
        current["used_bytes"] += 1
        self.assertEqual(gpu.evaluate(policy(), base, current, now_monotonic=now)["reason"], "device_used_growth_exceeded")
        current["used_bytes"] = base["used_bytes"] - 1
        current["free_bytes"] -= 1
        self.assertEqual(gpu.evaluate(policy(), base, current, now_monotonic=now)["reason"], "device_free_reserve_breached")
        current["free_bytes"] += 1
        result = gpu.evaluate(policy(), base, current, now_monotonic=now)
        self.assertTrue(result["allowed"])
        self.assertEqual(result["device_used_growth_bytes"], -1)
        self.assertFalse(result["hard_vram_quota"])
        self.assertFalse(result["process_attribution"])

    def test_unknown_stale_future_and_missing_reserve_stop(self):
        base = snapshot()
        now = base["observed_monotonic"] + 10
        current = dict(base, observed_monotonic=now)
        self.assertTrue(gpu.evaluate(policy(), base, current, now_monotonic=now)["allowed"])
        mutations = [{"status": "unknown"}, {"observed_monotonic": now-2.001},
                     {"observed_monotonic": now+.001}, {"observed_monotonic": float("nan")},
                     {"reserved_bytes": None}, {"reserved_bytes": True}, {"device_uuid": "GPU-other"},
                     {"total_bytes": base["total_bytes"]+1}, {"query_duration_seconds": 1.001}]
        for mutation in mutations:
            with self.subTest(mutation=mutation):
                self.assertFalse(gpu.evaluate(policy(), base, dict(current, **mutation), now_monotonic=now)["allowed"])
        self.assertFalse(gpu.evaluate(policy(), base, base, now_monotonic=now)["allowed"])

    def test_policy_cannot_configure_commands_or_reduce_reserve(self):
        for change in [{"command": "/tmp/utility"}, {"minimum_free_bytes": 2*1024**3-1},
                       {"mode": "hard_quota"}, {"device_uuid": "0"}, {"poll_interval_seconds": 2},
                       {"poll_interval_seconds": True}, {"maximum_device_used_growth_bytes": 0}]:
            with self.subTest(change=change), self.assertRaises(ValueError):
                gpu.validate_policy(dict(policy(), **change))

    def test_freshness_includes_time_spent_acquiring_sample(self):
        base = snapshot()
        now = base["observed_monotonic"] + 10
        current = dict(base, observed_monotonic=now-1.25, query_duration_seconds=.9)
        self.assertEqual(gpu.evaluate(policy(), base, current, now_monotonic=now)["reason"], "telemetry_stale")
        current["observed_monotonic"] = now-1.1
        self.assertTrue(gpu.evaluate(policy(), base, current, now_monotonic=now)["allowed"])

    @unittest.skipUnless(sys.platform.startswith("linux"), "Actual process-pipe bounds require Linux")
    def test_actual_benign_process_success_timeout_and_output_bound(self):
        env = {"PATH": "/usr/bin:/bin", "LANG": "C"}
        code, stdout, stderr, reason = gpu._capture([sys.executable, "-I", "-B", "-c", "print('fixture')"], env)
        self.assertEqual((code, stdout, stderr, reason), (0, b"fixture\n", b"", None))
        for code_text, expected in [("import time;time.sleep(20)", "telemetry_timeout"),
                                    ("import os;os.write(1,b'x'*10000)", "telemetry_output_limit")]:
            start = time.monotonic()
            code, stdout, stderr, reason = gpu._capture([sys.executable, "-I", "-B", "-c", code_text], env)
            self.assertEqual(reason, expected)
            self.assertLessEqual(len(stdout)+len(stderr), gpu.MAX_OUTPUT_BYTES)
            self.assertLess(time.monotonic()-start, 3)
        # Descendant keeps pipes after leader exits: the group is still cleaned.
        code_text = "import os,time;pid=os.fork();\nif pid==0:time.sleep(20)\nelse:os._exit(0)"
        start = time.monotonic()
        outcome = gpu._capture([sys.executable, "-I", "-B", "-c", code_text], env)
        self.assertEqual(outcome[3], "telemetry_timeout")
        self.assertLess(time.monotonic()-start, 3)

    @unittest.skipUnless(sys.platform.startswith("linux"), "Actual process-group cleanup requires Linux")
    def test_successful_leader_cannot_leave_a_detached_pipe_descendant(self):
        code_text = ("import os,time\nreader,writer=os.pipe()\npid=os.fork()\n"
                     "if pid==0:\n os.close(reader);os.close(1);os.close(2);os.write(writer,b'c');os.close(writer);time.sleep(20)\n"
                     "else:\n os.close(writer);os.read(reader,1);os.close(reader);print(pid,flush=True);os._exit(0)\n")
        code, stdout, stderr, reason = gpu._capture([sys.executable, "-I", "-B", "-c", code_text], {"PATH":"/usr/bin:/bin"})
        self.assertEqual((code, stderr, reason), (0, b"", None))
        child = int(stdout.strip())
        end = time.monotonic()+.5
        while True:
            try:
                raw = Path(f"/proc/{child}/stat").read_text()
                state = raw[raw.rfind(")")+2:].split()[0]
                if state in ("Z", "X"):
                    break
            except FileNotFoundError:
                break
            self.assertLess(time.monotonic(), end, "Query descendant survived successful leader cleanup")
            time.sleep(.01)


if __name__ == "__main__":
    unittest.main()
