"""Synthetic native acceptance payload; execute only via the normal LPAC job API."""
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import time


def token_flags():
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    security = ctypes.WinDLL("advapi32", use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.CloseHandle.argtypes = [wintypes.HANDLE]
    security.OpenProcessToken.argtypes = [wintypes.HANDLE, wintypes.DWORD, ctypes.POINTER(wintypes.HANDLE)]
    security.GetTokenInformation.argtypes = [wintypes.HANDLE, ctypes.c_int, ctypes.c_void_p, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD)]
    handle = wintypes.HANDLE()
    if not security.OpenProcessToken(kernel.GetCurrentProcess(), 8, ctypes.byref(handle)):
        raise ctypes.WinError(ctypes.get_last_error())
    result = {}
    try:
        for name, category in [("appcontainer", 29), ("less_privileged", 46)]:
            value, length = wintypes.DWORD(), wintypes.DWORD()
            if security.GetTokenInformation(handle, category, ctypes.byref(value), 4, ctypes.byref(length)):
                result[name] = value.value
                continue
            if category != 46:
                raise ctypes.WinError(ctypes.get_last_error())
            class UnicodeString(ctypes.Structure):
                _fields_ = [("length", wintypes.WORD), ("maximum_length", wintypes.WORD), ("buffer", ctypes.c_void_p)]
            class Attribute(ctypes.Structure):
                _fields_ = [("name", UnicodeString), ("value_type", wintypes.WORD), ("reserved", wintypes.WORD), ("flags", wintypes.DWORD), ("count", wintypes.DWORD), ("values", ctypes.POINTER(ctypes.c_uint64))]
            class Attributes(ctypes.Structure):
                _fields_ = [("version", wintypes.WORD), ("reserved", wintypes.WORD), ("count", wintypes.DWORD), ("attributes", ctypes.POINTER(Attribute))]
            security.GetTokenInformation(handle, 39, None, 0, ctypes.byref(length))
            assert ctypes.sizeof(Attributes) <= length.value <= 65536
            buffer = ctypes.create_string_buffer(length.value)
            if not security.GetTokenInformation(handle, 39, buffer, len(buffer), ctypes.byref(length)):
                raise ctypes.WinError(ctypes.get_last_error())
            header = Attributes.from_buffer(buffer)
            assert header.version == 1 and header.count < 512
            result[name] = 0
            for index in range(header.count):
                attribute = header.attributes[index]
                if ctypes.wstring_at(attribute.name.buffer, attribute.name.length // 2) == "WIN://NOALLAPPPKG":
                    assert attribute.value_type in (2, 6) and attribute.count == 1
                    result[name] = int(attribute.values[0] != 0)
    finally:
        kernel.CloseHandle(handle)
    return result


def denied(operation):
    try:
        operation()
    except OSError as error:
        return {"denied": isinstance(error, PermissionError), "winerror": getattr(error, "winerror", None), "errno": error.errno}
    return {"denied": False, "error": "Unexpected access to synthetic canary"}


def connect(address):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as connection:
        connection.settimeout(2)
        connection.connect(address)


def main():
    config = json.loads(Path("input.json").read_text())
    assert config["fixture"] == "phaseforge.native-laboratory.v1"
    token = token_flags()
    assert token == {"appcontainer": 1, "less_privileged": 1}
    if config["mode"] == "sleep":
        assert config["seconds"] in (8, 60)
        Path("started.json").write_text(json.dumps({"pid": os.getpid(), "token": token, "seconds": config["seconds"]}))
        time.sleep(config["seconds"])
        Path("result.json").write_text(json.dumps({"fixture": config["fixture"], "mode": "sleep", "seconds": config["seconds"], "pid": os.getpid(), "token": token}))
        return
    assert config["mode"] == "boundary"
    import numpy as np
    matrix = np.array([[4., 1., 0.], [1., 3., 1.], [0., 1., 2.]])
    rhs = np.array([1., 2., 3.])
    solve_started = time.perf_counter()
    solution = np.linalg.solve(matrix, rhs)
    solve_elapsed = time.perf_counter() - solve_started
    report = {"fixture": config["fixture"], "pid": os.getpid(), "python": sys.version,
              "numpy": np.__version__, "token": token, "matrix": matrix.tolist(), "rhs": rhs.tolist(),
              "solution": solution.tolist(), "linear_residual": float(np.max(np.abs(matrix @ solution - rhs))),
              "linear_solve_elapsed_seconds": solve_elapsed, "host_observation_hold_seconds": 2,
              "outside_read": denied(lambda: Path(config["read_canary"]).read_bytes()),
              "outside_hardlink": denied(lambda: os.link(config["read_canary"], "forbidden-hardlink.txt")),
              "outside_write": denied(lambda: Path(config["write_canary"]).write_text("synthetic forbidden write")),
              "runtime_write": denied(lambda: Path(config["runtime_canary"]).write_text("synthetic forbidden write")),
              "loopback_network": denied(lambda: connect(("127.0.0.1", config["listening_port"]))),
              "external_network": denied(lambda: connect(("192.0.2.1", 443))),
              "provider_environment_present": [name for name in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY", "PHASEFORGE_LAUNCH_TOKEN", "PYTHONPATH") if name in os.environ]}
    assert report["linear_residual"] < 1e-12 and not report["provider_environment_present"]
    assert all(report[name]["denied"] for name in ("outside_read", "outside_hardlink", "outside_write", "runtime_write", "loopback_network", "external_network"))
    Path("numerical.json").write_text(json.dumps(report, allow_nan=False))
    # Keep the real process observable long enough for the host identity sample.
    time.sleep(2)
    Path("result.json").write_text(json.dumps({**report, "artifacts": ["numerical.json"], "numerical_sha256": hashlib.sha256(Path("numerical.json").read_bytes()).hexdigest()}, allow_nan=False))


if __name__ == "__main__":
    main()
