"""Benign negative fixture, executed INSIDE the Windows experiment boundary.

Only supplied synthetic canaries are probed; never pass real credential files.
The host verifies the report and retained numerical output independently.
"""
from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

PYTHON_SHA256 = "d1f04d990aee1253d8569e8e5104e30fa9f5fa830899f14843448872d936a2cf"
NUMPY_SHA256 = "c4fc99836233ea196540b17ab0983aff60ed07941751930f5f4d05bc3b3b7359"


def provision(destination: Path):
    """Trusted host operation: exact pinned archives, no generated setup code."""
    import urllib.request
    import uuid
    import zipfile
    destination = destination.absolute()
    if os.name == "nt":
        # Use the native long-path form without changing machine policy. NumPy's
        # packaged test-data filenames can exceed MAX_PATH in a nested app root.
        name = str(destination)
        if name.startswith("\\\\") and not name.startswith("\\\\?\\"):
            raise ValueError("A local managed-runtime directory is required")
        if not name.startswith("\\\\?\\"):
            destination = Path("\\\\?\\" + name)
    if destination.exists():
        raise ValueError("Choose a fresh versioned runtime directory; existing files will not be overwritten")
    destination.parent.mkdir(parents=True, exist_ok=True)
    staging = destination.with_name(destination.name + "-install-" + uuid.uuid4().hex)
    staging.mkdir()
    cache = destination.parent / ".isolation-downloads"
    cache.mkdir(exist_ok=True)
    with urllib.request.urlopen("https://pypi.org/pypi/numpy/2.4.6/json", timeout=30) as response:
        metadata = json.loads(response.read(4 * 1024 * 1024))
    wheel = next(item for item in metadata["urls"] if item["filename"] == "numpy-2.4.6-cp313-cp313-win_amd64.whl")
    assert wheel["digests"]["sha256"] == NUMPY_SHA256 and wheel["url"].startswith("https://files.pythonhosted.org/")
    sources = [
        {"name": "python-3.13.15-embed-amd64.zip", "url": "https://www.python.org/ftp/python/3.13.15/python-3.13.15-embed-amd64.zip", "sha256": PYTHON_SHA256},
        {"name": wheel["filename"], "url": wheel["url"], "sha256": NUMPY_SHA256},
    ]
    for source in sources:
        archive = cache / source["name"]
        if not archive.exists():
            with urllib.request.urlopen(source["url"], timeout=60) as response:
                payload = response.read(64 * 1024 * 1024 + 1)
            assert len(payload) <= 64 * 1024 * 1024
            assert hashlib.sha256(payload).hexdigest() == source["sha256"], "Pinned download hash mismatch"
            archive.write_bytes(payload)
        assert hashlib.sha256(archive.read_bytes()).hexdigest() == source["sha256"], "Pinned cache hash mismatch"
        with zipfile.ZipFile(archive) as bundle:
            assert sum(item.file_size for item in bundle.infolist()) <= 512 * 1024 * 1024
            for item in bundle.infolist():
                relative = Path(item.filename)
                assert not relative.is_absolute() and ".." not in relative.parts and ":" not in item.filename
                assert (item.external_attr >> 16) & 0o170000 != 0o120000, "Archive symlink refused"
                target = staging / relative
                assert target.resolve().is_relative_to(staging.resolve())
                if item.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    assert not target.exists(), "Archive attempted to replace an installed file"
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(bundle.read(item))
    files = {path.relative_to(staging).as_posix(): hashlib.sha256(path.read_bytes()).hexdigest() for path in sorted(staging.rglob("*")) if path.is_file()}
    manifest = {"schema_version": 1, "python": "3.13.15", "numpy": "2.4.6", "sources": sources, "files": files,
                "security": "Pinned dependencies only; generated code requires the separately verified Windows LPAC boundary"}
    (staging / "phaseforge-isolation-runtime.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    staging.rename(destination)
    print(json.dumps({"runtime": str(destination), "python": manifest["python"], "numpy": manifest["numpy"], "files": len(files), "sources": sources}))


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
        return {"denied": isinstance(error, PermissionError), "winerror": error.winerror, "errno": error.errno}
    return {"denied": False, "error": "Unexpected access"}


def connect(address):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as connection:
        connection.settimeout(2)
        connection.connect(address)


def main():
    parser = argparse.ArgumentParser()
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--input", type=Path)
    mode.add_argument("--provision", type=Path)
    parser.add_argument("--hold", action="store_true")
    arguments = parser.parse_args()
    if arguments.provision:
        provision(arguments.provision)
        return
    config = json.loads(arguments.input.read_text())
    root = Path.cwd()
    if arguments.hold:
        child = subprocess.Popen([sys.executable, "-I", "-c", "import time; time.sleep(120)"])
        (root / "tree.json").write_text(json.dumps({"parent": os.getpid(), "child": child.pid}))
        time.sleep(120)
        return
    import numpy as np
    matrix = np.array([[4., 1., 0.], [1., 3., 1.], [0., 1., 2.]])
    rhs = np.array([1., 2., 3.])
    solution = np.linalg.solve(matrix, rhs)
    retained = root / "calculation.npz"
    np.savez_compressed(retained, matrix=matrix, rhs=rhs, solution=solution)
    with np.load(retained, allow_pickle=False) as saved:
        residual = float(np.max(np.abs(saved["matrix"] @ saved["solution"] - saved["rhs"])))
    report = {
        "python": sys.version,
        "numpy": np.__version__,
        "token": token_flags(),
        "linear_residual": residual,
        "solution": solution.tolist(),
        "retained_sha256": hashlib.sha256(retained.read_bytes()).hexdigest(),
        "outside_read": denied(lambda: Path(config["read_canary"]).read_bytes()),
        "outside_hardlink": denied(lambda: os.link(config["read_canary"], root / "forbidden-hardlink.txt")),
        "outside_write": denied(lambda: Path(config["write_canary"]).write_text("must not be created")),
        "runtime_write": denied(lambda: Path(config["runtime_canary"]).write_text("must remain read only")),
        "loopback_network": denied(lambda: connect(("127.0.0.1", config["listening_port"]))),
        "external_network": denied(lambda: connect(("192.0.2.1", 443))),
        "provider_environment_present": [key for key in ("OPENAI_API_KEY", "ANTHROPIC_API_KEY", "PHASEFORGE_LAUNCH_TOKEN", "PYTHONPATH") if key in os.environ],
    }
    (root / "isolation-report.json").write_text(json.dumps(report, indent=2, allow_nan=False))
    assert report["token"] == {"appcontainer": 1, "less_privileged": 1}
    assert residual < 1e-12 and not report["provider_environment_present"]
    for name in ("outside_read", "outside_hardlink", "outside_write", "runtime_write", "loopback_network", "external_network"):
        assert report[name]["denied"], (name, report[name])


if __name__ == "__main__":
    main()
