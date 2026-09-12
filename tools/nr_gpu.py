"""Read-only NVIDIA device-wide telemetry; sampled soft limits, never a VRAM quota.

The fixed WSL driver utility does not attribute allocations to the NR process.
Other applications may change the observation between polls or trigger a stop.
"""
from __future__ import annotations

import csv
import datetime as dt
import hashlib
import io
import math
import os
import re
import selectors
import signal
import subprocess
import time

NVIDIA_SMI = "/usr/lib/wsl/lib/nvidia-smi"
MIB = 1024**2
MINIMUM_FREE_BYTES = 2 * 1024**3
QUERY_TIMEOUT_SECONDS = 1.0
MAX_OUTPUT_BYTES = 4096
MAX_SAMPLE_AGE_SECONDS = 2.0
POLICY_KEYS = {"mode", "device_uuid", "maximum_device_used_growth_bytes", "minimum_free_bytes", "poll_interval_seconds"}
BYTE_FIELDS = ("total_bytes", "reserved_bytes", "used_bytes", "free_bytes")
UUID_PATTERN = re.compile(r"GPU-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")


def _require(condition, reason):
    if not condition:
        raise ValueError(reason)


def _uuid(value):
    _require(isinstance(value, str) and UUID_PATTERN.fullmatch(value), "An exact canonical GPU UUID is required")
    return value


def validate_policy(policy):
    _require(isinstance(policy, dict) and set(policy) == POLICY_KEYS, "Unexpected device-wide VRAM policy fields")
    _require(policy["mode"] == "device_wide_soft_guard", "Only a sampled device-wide soft guard is supported")
    _uuid(policy["device_uuid"])
    for key, minimum in (("minimum_free_bytes", MINIMUM_FREE_BYTES), ("maximum_device_used_growth_bytes", 1)):
        _require(type(policy[key]) is int and minimum <= policy[key] <= 1024**4, "Invalid " + key)
    _require(type(policy["poll_interval_seconds"]) in (int, float) and policy["poll_interval_seconds"] == 1,
             "Device memory must be sampled every one second")
    return dict(policy)


def _capture(argv, env):
    """Bound pipes while reading, and own cleanup of this read-only query tree."""
    start = time.monotonic()
    child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                             stderr=subprocess.PIPE, env=env, start_new_session=True)
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    reason = None
    try:
        with selectors.DefaultSelector() as selector:
            for name in buffers:
                stream = getattr(child, name)
                os.set_blocking(stream.fileno(), False)
                selector.register(stream, selectors.EVENT_READ, name)
            while selector.get_map():
                left = QUERY_TIMEOUT_SECONDS - (time.monotonic() - start)
                if left <= 0:
                    reason = "telemetry_timeout"
                    break
                for key, _ in selector.select(min(left, 0.05)):
                    raw = os.read(key.fileobj.fileno(), 1024)
                    if not raw:
                        selector.unregister(key.fileobj)
                        continue
                    size = sum(map(len, buffers.values()))
                    buffers[key.data].extend(raw[:max(0, MAX_OUTPUT_BYTES - size)])
                    if size + len(raw) > MAX_OUTPUT_BYTES:
                        reason = "telemetry_output_limit"
                        break
                if reason:
                    break
        if reason is None:
            try:
                child.wait(timeout=max(0.001, QUERY_TIMEOUT_SECONDS - (time.monotonic() - start)))
            except subprocess.TimeoutExpired:
                reason = "telemetry_timeout"
    finally:
        # Kill the owned process group even if its leader exited while a descendant
        # retained a pipe. nvidia-smi normally has no descendants.
        # Also close a successful query's group: a descendant can close both
        # pipes before its leader exits and otherwise escape the EOF path.
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        try:
            child.wait(timeout=1)
        finally:
            child.stdout.close()
            child.stderr.close()
    return child.returncode, bytes(buffers["stdout"]), bytes(buffers["stderr"]), reason


def _parse(raw, device_uuid):
    rows = list(csv.reader(io.StringIO(raw.decode("ascii", "strict"))))
    _require(len(rows) == 1 and len(rows[0]) == 5, "Expected exactly one device memory row")
    fields = [field.strip() for field in rows[0]]
    _require(fields[0] == device_uuid, "GPU identity changed")
    numbers = []
    for value in fields[1:]:
        _require(re.fullmatch(r"[0-9]{1,10}", value), "GPU memory field is unavailable or malformed")
        numbers.append(int(value) * MIB)
    result = dict(zip(BYTE_FIELDS, numbers))
    _require(result["total_bytes"] > 0 and all(0 <= value <= result["total_bytes"] for value in numbers[1:]),
             "GPU memory fields exceed device capacity")
    # SMI reports independently rounded MiB, so no total == used+free+reserved
    # assertion. In particular, free is authoritative; never derive it.
    return result


def query(device_uuid):
    """Return known data or an explicit unknown record, never default memory to 0."""
    _uuid(device_uuid)
    start = time.monotonic()
    sample = {"schema": "phaseforge.nr-gpu-sample.v1", "status": "unknown", "device_uuid": device_uuid,
              "scope": "device_wide_soft_guard", "reason": None, "returncode": None,
              **dict.fromkeys(BYTE_FIELDS)}
    try:
        code, stdout, stderr, reason = _capture(
            [NVIDIA_SMI, "--id=" + device_uuid,
             "--query-gpu=uuid,memory.total,memory.reserved,memory.used,memory.free",
             "--format=csv,noheader,nounits"],
            {"PATH": "/usr/bin:/bin", "LANG": "C", "LC_ALL": "C"})
        sample.update(returncode=code, stdout_sha256=hashlib.sha256(stdout).hexdigest(),
                      stderr_sha256=hashlib.sha256(stderr).hexdigest(),
                      stdout_bytes=len(stdout), stderr_bytes=len(stderr))
        _require(reason is None, reason)
        _require(code == 0, "telemetry_nonzero_exit")
        _require(not stderr.strip(), "telemetry_stderr")
        sample.update(_parse(stdout, device_uuid))
        sample["status"] = "known"
    except (OSError, ValueError, UnicodeError, csv.Error, subprocess.SubprocessError) as error:
        sample["reason"] = str(error)[:256] or type(error).__name__
    sample["observed_monotonic"] = time.monotonic()
    sample["observed_at"] = dt.datetime.now(dt.timezone.utc).isoformat()
    sample["query_duration_seconds"] = sample["observed_monotonic"] - start
    if sample["query_duration_seconds"] > QUERY_TIMEOUT_SECONDS:
        sample.update(status="unknown", reason="telemetry_timeout", **dict.fromkeys(BYTE_FIELDS))
    return sample


def _validate_sample(sample, device_uuid, now, *, fresh):
    _require(isinstance(sample, dict) and sample.get("status") == "known", "telemetry_unknown")
    _require(sample.get("device_uuid") == device_uuid, "telemetry_identity_mismatch")
    stamp = sample.get("observed_monotonic")
    duration = sample.get("query_duration_seconds")
    _require(type(stamp) in (int, float) and math.isfinite(stamp) and stamp <= now, "telemetry_invalid_timestamp")
    _require(type(duration) in (int, float) and math.isfinite(duration) and 0 <= duration <= QUERY_TIMEOUT_SECONDS,
             "telemetry_invalid_duration")
    # Driver acquisition time is unknown. Conservatively age from query start,
    # rather than granting an extra query-duration interval after completion.
    _require(not fresh or now - stamp + duration <= MAX_SAMPLE_AGE_SECONDS, "telemetry_stale")
    _require(all(type(sample.get(key)) is int for key in BYTE_FIELDS), "telemetry_memory_unavailable")
    _require(sample["total_bytes"] > 0 and all(0 <= sample[key] <= sample["total_bytes"] for key in BYTE_FIELDS[1:]),
             "telemetry_invalid_memory")


def evaluate(policy, baseline, sample, *, now_monotonic=None):
    """Use (baseline, baseline) for admission; old baseline remains valid in-run.

    The current sample must remain fresh. Growth includes every application on
    the device and can be negative; it is not process GPU allocation telemetry.
    """
    policy = validate_policy(policy)
    now = time.monotonic() if now_monotonic is None else now_monotonic
    result = {"allowed": False, "reason": None, "scope": "device_wide_soft_guard",
              "device_used_growth_bytes": None, "hard_vram_quota": False, "process_attribution": False}
    try:
        _require(type(now) in (int, float) and math.isfinite(now), "telemetry_invalid_clock")
        _validate_sample(baseline, policy["device_uuid"], now, fresh=False)
        _validate_sample(sample, policy["device_uuid"], now, fresh=True)
        _require(baseline["total_bytes"] == sample["total_bytes"], "telemetry_capacity_changed")
        _require(sample["observed_monotonic"] >= baseline["observed_monotonic"], "telemetry_precedes_baseline")
        _require(baseline["free_bytes"] >= policy["minimum_free_bytes"], "baseline_free_reserve_breached")
        growth = sample["used_bytes"] - baseline["used_bytes"]
        result["device_used_growth_bytes"] = growth
        _require(sample["free_bytes"] >= policy["minimum_free_bytes"], "device_free_reserve_breached")
        _require(growth <= policy["maximum_device_used_growth_bytes"], "device_used_growth_exceeded")
        result["allowed"] = True
    except ValueError as error:
        result["reason"] = str(error)
    return result
