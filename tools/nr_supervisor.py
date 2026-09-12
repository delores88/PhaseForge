#!/usr/bin/env python3
"""Own one trusted Linux NR attempt; v2 adds a dedicated systemd memory scope.

Only a pinned executable and parameter file are admitted. Pause stops an attempt;
it does not promise checkpoint continuation. The caller must retain stdin until
the terminal receipt, and must stage requests in a private Linux filesystem.
"""
from __future__ import annotations
import argparse
import ctypes
import datetime as dt
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import select
import signal
import stat
import subprocess
import sys
import time
import types
import uuid

MAX_JSON = 1024 * 1024
POLL_SECONDS = 0.1
TERM_SECONDS = 1.0
KILL_SECONDS = 2.0
MAX_OUTPUT_FILES = 4096
RESOURCE_HEADROOM = 512 * 1024**2
REQUEST_KEYS = {"schema", "job_id", "engine_id", "engine_manifest_sha256", "input", "output_dir", "cpu_threads", "memory_limit_bytes", "output_limit_bytes", "deadline_at"}
POLICY_KEYS = {"backend", "memory_boundary", "address_space_limit_bytes", "tasks_max", "gpu_vram_policy"}
STOP_SIGNAL = None
GPU_MODULE = None


def gpu_module(expected_sha=None):
    global GPU_MODULE
    if GPU_MODULE is None:
        require(expected_sha is not None, "GPU helper needs its original manifest pin before loading")
        path = Path(__file__).with_name("nr_gpu.py")
        fd, raw, digest, _ = read_pinned(path, pin(expected_sha))
        os.close(fd)
        module = types.ModuleType("phaseforge_trusted_nr_gpu")
        module.__file__ = str(path)
        exec(compile(raw, str(path), "exec"), module.__dict__)
        module.source_sha256 = digest
        GPU_MODULE = module
    if expected_sha is not None:
        require(GPU_MODULE.source_sha256 == pin(expected_sha), "GPU helper source differs from the admitted manifest")
    return GPU_MODULE


def require(condition, message):
    if not condition:
        raise ValueError(message)


def utc():
    return dt.datetime.now(dt.timezone.utc).isoformat()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def pin(value):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{64}", value), "Expected a lowercase SHA-256 pin")
    return value


def private_path(path, *, directory=False):
    """Check every existing component without accepting aliases or links."""
    path = Path(path)
    require(path.is_absolute() and ".." not in path.parts and str(path) == os.path.normpath(str(path)), "An absolute non-aliased Linux path is required")
    current = Path("/")
    for part in path.parts[1:]:
        current /= part
        item = current.lstat()
        require(not stat.S_ISLNK(item.st_mode), "Linked path components are not admitted")
        require(item.st_uid in (0, os.getuid()), "Path ownership is outside the trusted user")
        require(not item.st_mode & 0o022, "Group/world-writable runtime paths are not admitted")
    item = path.lstat()
    require(stat.S_ISDIR(item.st_mode) if directory else stat.S_ISREG(item.st_mode), "Unexpected path type")
    if not directory:
        require(item.st_nlink == 1, "Hard-linked files are not admitted")
    return path


def read_pinned(path, expected=None, limit=MAX_JSON, *, sealed=False):
    path = private_path(path)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
    snapshot = None
    try:
        if sealed:
            snapshot = os.memfd_create("phaseforge-pinned-nr", os.MFD_CLOEXEC | os.MFD_ALLOW_SEALING)
        before = os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size <= limit, "File exceeds its type/link/size admission")
        chunks, digest, total = [], hashlib.sha256(), 0
        while True:
            raw = os.read(fd, 65536)
            if not raw:
                break
            total += len(raw)
            require(total <= limit, "File grew beyond admission")
            digest.update(raw)
            if snapshot is not None:
                pending = memoryview(raw)
                while pending:
                    pending = pending[os.write(snapshot, pending):]
            if limit == MAX_JSON:
                chunks.append(raw)
        after = os.fstat(fd)
        require((before.st_ino, before.st_size, before.st_mtime_ns) == (after.st_ino, after.st_size, after.st_mtime_ns) and total == before.st_size, "File changed during admission")
        actual = digest.hexdigest()
        require(expected is None or actual == pin(expected), "File SHA-256 mismatch")
        if snapshot is not None:
            os.fchmod(snapshot, 0o500)
            fcntl.fcntl(snapshot, fcntl.F_ADD_SEALS, fcntl.F_SEAL_WRITE | fcntl.F_SEAL_GROW | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_SEAL)
            os.close(fd); fd = snapshot; snapshot = None
        os.lseek(fd, 0, os.SEEK_SET)
        return fd, b"".join(chunks), actual, total
    except BaseException:
        os.close(fd)
        if snapshot is not None:
            os.close(snapshot)
        raise


def parse_json(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "Duplicate JSON field")
            result[key] = value
        return result
    return json.loads(raw.decode("utf-8"), object_pairs_hook=unique, parse_constant=lambda value: (_ for _ in ()).throw(ValueError("Nonfinite JSON value")))


def deadline(value):
    if value is None:
        return None
    require(isinstance(value, str), "deadline_at must be explicit UTC or null")
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(parsed.tzinfo is not None and parsed.utcoffset() == dt.timedelta(0), "deadline_at must have an explicit UTC offset")
    return parsed.timestamp()


def integer(value, lower, upper, name):
    require(type(value) is int and lower <= value <= upper, f"Invalid {name}")
    return value


def execution_resources(root):
    memory = {}
    for line in Path("/proc/meminfo").read_text().splitlines():
        key, value = line.split(":", 1)
        if key in ("MemTotal", "MemAvailable"):
            parts = value.split()
            require(len(parts) == 2 and parts[1] == "kB", "Unsupported Linux memory observation")
            memory[key] = int(parts[0]) * 1024
    require(set(memory) == {"MemTotal", "MemAvailable"}, "Linux memory capacity is unavailable")
    disk = os.statvfs(root)
    free = disk.f_bavail * disk.f_frsize
    return {"cpu_count": os.cpu_count(), "available_affinity_cpus": sorted(os.sched_getaffinity(0)),
            "mem_total_bytes": memory["MemTotal"], "mem_available_bytes": memory["MemAvailable"], "job_root_free_bytes": free,
            "required_headroom_bytes": RESOURCE_HEADROOM, "memory_admission_ceiling_bytes": max(0, memory["MemAvailable"] - RESOURCE_HEADROOM),
            "output_admission_ceiling_bytes": max(0, free - RESOURCE_HEADROOM), "observed_at": utc()}


def execution_policy(request, manifest):
    if request["schema"] == "phaseforge.nr-request.v1":
        require(set(request) == REQUEST_KEYS, "Unexpected NR request fields")
        require(manifest["build"].get("backend") != "CUDA", "CUDA requires an explicitly admitted physical-memory and VRAM policy")
        return None
    require(request["schema"] == "phaseforge.nr-request.v2" and set(request) == REQUEST_KEYS | {"execution_policy"}, "Unexpected NR request schema or fields")
    policy = request["execution_policy"]
    require(isinstance(policy, dict) and set(policy) == POLICY_KEYS, "Unexpected trusted execution policy")
    require(policy["memory_boundary"] == "systemd_cgroup_v2" and policy["address_space_limit_bytes"] is None, "v2 requires verified cgroup RAM enforcement before removing RLIMIT_AS")
    integer(policy["tasks_max"], 8, 128, "kernel task cap")
    if policy["backend"] == "cpu":
        require(policy["gpu_vram_policy"] == {"mode": "not_admitted"} and manifest["build"].get("backend") == "Serial", "CPU execution cannot claim GPU admission")
    else:
        require(policy["backend"] == "cuda" and manifest["build"].get("backend") == "CUDA", "Execution policy and pinned build backend differ")
        require(resource.getrlimit(resource.RLIMIT_AS) == (resource.RLIM_INFINITY, resource.RLIM_INFINITY),
                "CUDA requires an unrestricted inherited address space plus the verified physical RAM boundary")
        runtime = manifest["build"].get("cuda_runtime", {})
        gpu_module(runtime.get("guard_source_sha256")).validate_policy(policy["gpu_vram_policy"])
    require(request["memory_limit_bytes"] % os.sysconf("SC_PAGE_SIZE") == 0, "Kernel memory cap must be page aligned")
    return policy


def cuda_runtime(manifest):
    """Admit exact private source bytes; the engine later uses only a job snapshot."""
    runtime = manifest["build"].get("cuda_runtime")
    require(isinstance(runtime, dict) and set(runtime) == {"schema", "library_directory", "driver_directory", "libraries", "build_receipt", "guard_source_sha256"}
            and runtime["schema"] == "phaseforge.nr-cuda-runtime.v1", "Missing exact CUDA runtime contract")
    gpu_module(runtime["guard_source_sha256"])
    directory = private_path(runtime["library_directory"], directory=True)
    require(directory.name == "lib" and directory.parent.name == "toolkit" and "stubs" not in directory.parts,
            "Only the admitted private toolkit/lib source is accepted")
    require(runtime["driver_directory"] == "/usr/lib/wsl/lib" and Path("/usr/lib/wsl/lib/libcuda.so.1").is_file(), "The existing WSL CUDA driver is unavailable")
    libraries = runtime["libraries"]
    require(isinstance(libraries, list) and len(libraries) == 1, "Only the required pinned cudart runtime is admitted")
    row = libraries[0]
    require(isinstance(row, dict) and set(row) == {"name", "sha256", "bytes", "aliases"}, "Unexpected CUDA library descriptor")
    require(row["name"] == "libcudart.so.12.9.79" and row["aliases"] == {
        "libcudart.so.12": "libcudart.so.12.9.79", "libcudart.so": "libcudart.so.12"}, "CUDA library/alias identity differs")
    integer(row["bytes"], 1, 32*1024**2, "private CUDA library size")
    admitted_names = {row["name"], *row["aliases"]}
    require({path.name for path in directory.iterdir() if re.search(r"\.so(?:\.|$)", path.name)} == admitted_names,
            "Unregistered shared library in the private CUDA source directory")
    for alias, target in row["aliases"].items():
        path = directory/alias
        info = path.lstat()
        require(stat.S_ISLNK(info.st_mode) and info.st_uid in (0, os.getuid()) and os.readlink(path) == target,
                "CUDA runtime alias is not the exact admitted same-directory link")
    descriptor = runtime["build_receipt"]
    require(isinstance(descriptor, dict) and set(descriptor) == {"path", "sha256"}, "Missing pinned completed CUDA build receipt")
    fd, raw, receipt_sha, _ = read_pinned(descriptor["path"], descriptor["sha256"])
    os.close(fd)
    built = parse_json(raw)
    require(built.get("schema") == "phaseforge.nr-cuda-build.v1" and built.get("built") is True,
            "CUDA runtime requires a completed build receipt")
    require(built.get("engine") == manifest["executable"] and built.get("source") == manifest["source"],
            "Completed CUDA build receipt refers to another executable or source")
    expected_build = {key: manifest["build"].get(key) for key in ("backend", "precision", "cuda_arch", "cuda_version")}
    require(expected_build == {"backend": "CUDA", "precision": "double", "cuda_arch": "ADA89", "cuda_version": "12.9.86"}
            and built.get("build") == expected_build, "Completed CUDA build configuration differs")
    require(built.get("runtime_libraries") == [{key: row[key] for key in ("name", "sha256", "bytes")}], "Completed CUDA build runtime library pins differ")
    fd, _, digest, count = read_pinned(directory/row["name"], row["sha256"], 32*1024**2, sealed=True)
    try:
        require(count == row["bytes"], "CUDA library byte count differs")
        return {"library_fd": fd, "source_directory": str(directory), "driver_directory": runtime["driver_directory"],
                "library": row, "build_receipt": {"path": descriptor["path"], "sha256": receipt_sha}}
    except BaseException:
        os.close(fd)
        raise


def cuda_snapshot(admission):
    runtime = admission["cuda_runtime"]
    directory = admission["job"]/"cuda-runtime"
    directory.mkdir(mode=0o700, exist_ok=False)
    (admission["work"] / ".tmp" / "cuda-cache").mkdir(mode=0o700, parents=True, exist_ok=False)
    row = runtime["library"]
    target = directory/row["name"]
    output = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o500)
    try:
        os.lseek(runtime["library_fd"], 0, os.SEEK_SET)
        total, digest = 0, hashlib.sha256()
        while True:
            raw = os.read(runtime["library_fd"], 65536)
            if not raw: break
            total += len(raw); digest.update(raw)
            require(total <= row["bytes"], "Pinned CUDA snapshot exceeded admission")
            pending = memoryview(raw)
            while pending: pending = pending[os.write(output, pending):]
        require(total == row["bytes"] and digest.hexdigest() == row["sha256"], "Pinned CUDA snapshot changed")
        os.fsync(output)
    finally:
        os.close(output)
    for alias, destination in row["aliases"].items(): os.symlink(destination, directory/alias)
    return {"directory": str(directory), "source_directory": runtime["source_directory"],
            "driver_directory": runtime["driver_directory"], "library": row,
            "build_receipt": runtime["build_receipt"], "ld_library_path": str(directory)+":"+runtime["driver_directory"]}


def observe_gpu(policy, baseline=None):
    module = gpu_module()
    sample = module.query(policy["device_uuid"])
    decision = module.evaluate(policy, sample if baseline is None else baseline, sample)
    if baseline is None and decision["allowed"]:
        # This is a budget proposal, not reserved VRAM. A different application
        # can consume the device between this query and the next sample.
        if policy["maximum_device_used_growth_bytes"] > sample["free_bytes"]-policy["minimum_free_bytes"]:
            decision.update(allowed=False, reason="growth_budget_exceeds_initial_free_reserve")
    return sample, decision


def process_cgroup(pid):
    rows = Path(f"/proc/{pid}/cgroup").read_text().splitlines()
    require(len(rows) == 1 and rows[0].startswith("0::/"), "A single unified cgroup hierarchy is required")
    return rows[0][3:]


class CgroupScope:
    """Only a newly created UUID scope; never reconfigure an existing unit."""
    COUNTERS = ("memory.current", "memory.peak", "memory.events", "memory.events.local", "memory.swap.current", "pids.current", "pids.events", "cgroup.events")

    @staticmethod
    def bus(*args, allow_error=False):
        completed = subprocess.run(["/usr/bin/busctl", "--system", "--json=short", *map(str, args)],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            env={"PATH": "/usr/bin:/bin", "LANG": "C.UTF-8"}, timeout=3, check=False)
        require(len(completed.stdout) <= 65536 and len(completed.stderr) <= 65536, "Systemd reply exceeds its bound")
        if completed.returncode:
            if allow_error: return None
            raise ValueError("Systemd scope operation failed: " + completed.stderr.decode("utf-8", "replace")[:1000])
        if not completed.stdout.strip(): return None
        data = parse_json(completed.stdout)["data"]
        return data[0] if isinstance(data, list) and len(data) == 1 else data

    @staticmethod
    def scope_name(job_id):
        require(str(uuid.UUID(job_id)) == job_id, "Canonical private scope UUID required")
        return "phaseforge-nr-" + job_id + ".scope"

    @classmethod
    def create(cls, job_id, child, memory, tasks):
        scope = cls()
        scope.unit = cls.scope_name(job_id)
        scope.group_path = "/system.slice/" + scope.unit
        scope.path = Path("/sys/fs/cgroup") / scope.group_path.lstrip("/")
        scope.fds = {}
        scope.kill_fd = None
        scope.inode = None
        scope.last = {}
        scope.parent_cgroup = process_cgroup(os.getpid())
        require(child > 1 and child != os.getpid() and process_cgroup(child) == scope.parent_cgroup, "Only the gated owned child may enter the new scope")
        child_stat = Path(f"/proc/{child}/stat").read_text()
        child_fields = child_stat[child_stat.rfind(")") + 2:].split()
        require(int(child_fields[1]) == os.getpid() and int(child_fields[2]) == child and int(child_fields[3]) == child, "Scope PID is not the supervisor's gated session-leading child")
        require(not scope.path.exists(), "A previous cgroup attempt must not be reused")
        manager = ("call", "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager")
        require(cls.bus(*manager, "GetUnit", "s", scope.unit, allow_error=True) is None, "An existing systemd unit must not be reused")
        cls.bus(*manager, "StartTransientUnit", "ssa(sv)a(sa(sv))", scope.unit, "fail", 8,
            "Description", "s", "PhaseForge private NR attempt " + job_id,
            "Slice", "s", "system.slice", "PIDs", "au", 1, child,
            "MemoryMax", "t", memory, "MemorySwapMax", "t", 0,
            "TasksMax", "t", tasks, "OOMPolicy", "s", "kill", "TimeoutStopUSec", "t", 1000000, 0)
        try:
            scope.object_path = cls.bus(*manager, "GetUnit", "s", scope.unit)
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and process_cgroup(child) != scope.group_path:
                time.sleep(.01)
            require(scope.property("ControlGroup") == scope.group_path and process_cgroup(child) == scope.group_path, "Owned child was not assigned to the exact private scope")
            require(process_cgroup(os.getpid()) == scope.parent_cgroup and process_cgroup(1) != scope.group_path, "Scope admission moved a protected control process")
            require(not scope.path.is_symlink() and scope.path.is_dir(), "Unexpected scope directory")
            scope.inode = scope.path.stat().st_ino
            expected = {"memory.max": memory, "memory.swap.max": 0, "pids.max": tasks, "memory.oom.group": 1}
            scope.effective = {name: int((scope.path / name).read_text().strip()) for name in expected}
            require(scope.effective == expected, "Kernel scope limits differ from the exact admission")
            require([int(value) for value in (scope.path / "cgroup.procs").read_text().split()] == [child], "Unexpected processes in the new private scope")
            scope.kill_fd = os.open(scope.path / "cgroup.kill", os.O_WRONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
            for name in cls.COUNTERS:
                if (scope.path / name).exists():
                    scope.fds[name] = os.open(scope.path / name, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW)
            scope.sample()
            return scope
        except BaseException:
            try: scope.stop()
            finally: scope.close()
            raise

    def property(self, name):
        try:
            return self.bus("get-property", "org.freedesktop.systemd1", self.object_path, "org.freedesktop.systemd1.Scope", name, allow_error=True)
        except (OSError, subprocess.TimeoutExpired, ValueError):
            return None

    def sample(self):
        values, unavailable = {}, []
        for name, fd in self.fds.items():
            try:
                os.lseek(fd, 0, os.SEEK_SET)
                raw = os.read(fd, 8192).decode().strip()
                values[name] = {key: int(value) for key, value in (line.split() for line in raw.splitlines())} if "events" in name else int(raw)
            except (OSError, ValueError): unavailable.append(name)
        self.last.update(values)
        return {"observed_at": utc(), "values": values, "unavailable": unavailable, "last_observed_values": dict(self.last)}

    def drained(self):
        try:
            if not self.path.exists(): return True
            if self.inode is None: return False
            require(self.path.stat().st_ino == self.inode, "Private scope identity changed")
            values = dict(line.split() for line in (self.path / "cgroup.events").read_text().splitlines())
            return values.get("populated") == "0"
        except FileNotFoundError:
            return not self.path.exists()

    def stop(self):
        if self.kill_fd is not None:
            try: os.write(self.kill_fd, b"1")
            except OSError:
                require(not self.path.exists(), "Cannot stop the private cgroup")
        else:
            # Only used for failed setup of this just-created unique unit.
            self.bus("call", "org.freedesktop.systemd1", "/org/freedesktop/systemd1", "org.freedesktop.systemd1.Manager", "StopUnit", "ss", self.unit, "fail", allow_error=True)
        until = time.monotonic() + KILL_SECONDS
        while time.monotonic() < until and not self.drained(): time.sleep(.02)
        return self.drained()

    def close(self):
        for fd in [*self.fds.values(), self.kill_fd]:
            if fd is not None:
                try: os.close(fd)
                except OSError: pass
        self.fds = {}
        self.kill_fd = None


def admit(manifest_path, request_path, job_root):
    require(sys.platform.startswith("linux") and hasattr(os, "fork") and Path("/proc/self/stat").exists(), "The NR supervisor requires Linux /proc, fork and resource limits")
    root = private_path(job_root, directory=True)
    require(root.stat().st_uid == os.getuid() and root.stat().st_mode & 0o077 == 0, "Job root must be owned by this user and private (0700)")
    fds = []
    try:
        mf, raw_manifest, manifest_hash, _ = read_pinned(manifest_path); fds.append(mf)
        rf, raw_request, request_hash, _ = read_pinned(request_path); fds.append(rf)
        manifest, request = parse_json(raw_manifest), parse_json(raw_request)
        require(set(manifest) == {"schema", "engine_id", "executable", "source", "build"} and manifest["schema"] == "phaseforge.nr-engine.v1", "Unexpected trusted engine manifest schema")
        require(isinstance(manifest["engine_id"], str) and re.fullmatch(r"[a-z][a-z0-9_]{0,79}", manifest["engine_id"]), "Invalid engine identity")
        require(request["engine_id"] == manifest["engine_id"] and pin(request["engine_manifest_sha256"]) == manifest_hash, "Request selects a different engine manifest")
        require(isinstance(manifest["source"], dict) and manifest["source"] and isinstance(manifest["build"], dict) and manifest["build"], "Source and build provenance are required")
        policy = execution_policy(request, manifest)
        require(set(manifest["executable"]) == {"path", "sha256"}, "Unexpected executable fields")
        job_id = str(uuid.UUID(request["job_id"]))
        require(job_id == request["job_id"], "Expected a canonical job UUID")
        job = private_path(root / job_id, directory=True)
        require(job.stat().st_uid == os.getuid() and job.stat().st_mode & 0o077 == 0, "Job directory must be private (0700)")
        require(Path(request_path) == job / "request.json", "Request must be that job's request.json")
        require(set(request["input"]) == {"path", "sha256"} and request["input"]["path"] == "input.athinput", "Only the fixed parameter filename is admitted")
        work = job / "work"
        require(request["output_dir"] == str(work), "Output must be exactly the job's work directory")
        if work.exists() or work.is_symlink():
            private_path(work, directory=True)
            require(not any(work.iterdir()), "A new attempt requires an empty output directory")
        observed_resources = execution_resources(root)
        allowed = observed_resources["available_affinity_cpus"]
        count = integer(request["cpu_threads"], 1, len(allowed), "CPU thread count")
        integer(request["memory_limit_bytes"], 64 * 1024**2, 512 * 1024**3, "memory limit")
        integer(request["output_limit_bytes"], 65536, 1024**4, "output limit")
        require(request["memory_limit_bytes"] <= observed_resources["memory_admission_ceiling_bytes"], "Memory cap exceeds fresh Linux MemAvailable minus 512 MiB headroom")
        require(request["output_limit_bytes"] <= observed_resources["output_admission_ceiling_bytes"], "Output cap exceeds fresh job-root free space minus 512 MiB headroom")
        stop_at = deadline(request["deadline_at"])
        require(stop_at is None or stop_at > time.time(), "The original deadline has elapsed")
        ef, _, engine_hash, engine_bytes = read_pinned(manifest["executable"]["path"], manifest["executable"]["sha256"], 1024**3, sealed=True); fds.append(ef)
        require(os.fstat(ef).st_mode & 0o111, "Engine is not executable")
        inf, _, input_hash, input_bytes = read_pinned(job / "input.athinput", request["input"]["sha256"], sealed=True); fds.append(inf)
        runtime = cuda_runtime(manifest) if policy and policy["backend"] == "cuda" else None
        if runtime: fds.append(runtime["library_fd"])
        os.close(mf); fds.remove(mf); os.close(rf); fds.remove(rf)
        return {"manifest": manifest, "request": request, "manifest_sha256": manifest_hash, "request_sha256": request_hash,
                "engine_sha256": engine_hash, "engine_bytes": engine_bytes, "input_sha256": input_hash, "input_bytes": input_bytes,
                "job": job, "work": work, "deadline": stop_at, "affinity": allowed[:count], "execution_resources": observed_resources,
                "execution_policy": policy, "cuda_runtime": runtime, "engine_fd": ef, "input_fd": inf}
    except BaseException:
        for fd in fds:
            os.close(fd)
        raise


def atomic_json(path, value):
    temporary = path.with_name(path.name + ".tmp")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as stream:
            json.dump(value, stream, allow_nan=False, separators=(",", ":")); stream.write("\n"); stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, path)
    finally:
        if temporary.exists():
            temporary.unlink()


def prctl(option, value):
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(option, value, 0, 0, 0) != 0:
        raise OSError(ctypes.get_errno(), "Required Linux process ownership operation failed")


def group_rows(group):
    rows = []
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            raw = Path("/proc", name, "stat").read_text()
            fields = raw[raw.rfind(")") + 2:].split()
            if int(fields[2]) == group and fields[0] not in ("Z", "X"):
                rows.append({"pid": int(name), "state": fields[0], "rss_bytes": int(fields[21]) * os.sysconf("SC_PAGE_SIZE")})
        except (FileNotFoundError, ProcessLookupError):
            continue
    return rows


def terminate_group(group):
    for signum, grace in ((signal.SIGTERM, TERM_SECONDS), (signal.SIGKILL, KILL_SECONDS)):
        try:
            os.killpg(group, signum)
        except ProcessLookupError:
            return True
        end = time.monotonic() + grace
        while time.monotonic() < end:
            if not group_rows(group):
                return True
            time.sleep(0.02)
    return not group_rows(group)


def close_except(keep):
    for name in os.listdir("/proc/self/fd"):
        fd = int(name)
        if fd not in keep:
            try:
                os.close(fd)
            except OSError:
                pass


def guardian_loop(read_fd, group, scope=None):
    # Independent session survives loss of the host transport/supervisor group.
    os.setsid()
    close_except({read_fd} | ({scope.kill_fd} if scope else set()))
    if scope: scope.fds = {}
    try:
        while True:
            command = os.read(read_fd, 1)
            if command == b"D":
                os._exit(0)
            if not command:
                if scope: scope.stop()
                terminate_group(group)
                os._exit(0)
    except BaseException:
        if scope:
            try: scope.stop()
            except BaseException: pass
        terminate_group(group)
        os._exit(1)


def engine_environment(admission):
    threads = str(admission["request"]["cpu_threads"])
    env = {"PATH": "/usr/bin:/bin", "HOME": str(admission["job"]), "TMPDIR": str(admission["work"] / ".tmp"),
           "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "OMP_NUM_THREADS": threads, "OPENBLAS_NUM_THREADS": threads,
           "MKL_NUM_THREADS": threads, "NUMEXPR_NUM_THREADS": threads, "KOKKOS_NUM_THREADS": threads,
           "CUDA_VISIBLE_DEVICES": "", "HIP_VISIBLE_DEVICES": "", "PYTHONDONTWRITEBYTECODE": "1"}
    if admission.get("cuda_runtime"):
        env.update(CUDA_VISIBLE_DEVICES=admission["execution_policy"]["gpu_vram_policy"]["device_uuid"],
                   LD_LIBRARY_PATH=admission["cuda_snapshot"]["ld_library_path"],
                   CUDA_CACHE_PATH=str(admission["work"] / ".tmp" / "cuda-cache"))
    return env


def engine_child(admission, gate_fd, ready_fd, stdout_fd, stderr_fd, parent):
    try:
        os.setsid()
        prctl(1, signal.SIGKILL)  # PR_SET_PDEATHSIG closes the pre-guardian race.
        if os.getppid() != parent:
            os._exit(125)
        for signum in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(signum, signal.SIG_DFL)
        os.sched_setaffinity(0, admission["affinity"])
        request = admission["request"]
        if admission["execution_policy"] is None:
            resource.setrlimit(resource.RLIMIT_AS, (request["memory_limit_bytes"], request["memory_limit_bytes"]))
        resource.setrlimit(resource.RLIMIT_FSIZE, (request["output_limit_bytes"], request["output_limit_bytes"]))
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        os.umask(0o077)
        os.chdir(admission["work"])
        null = os.open("/dev/null", os.O_RDONLY)
        os.dup2(null, 0); os.dup2(stdout_fd, 1); os.dup2(stderr_fd, 2)
        keep = {0, 1, 2, gate_fd, ready_fd, admission["engine_fd"], admission["input_fd"]}
        close_except(keep)
        os.write(ready_fd, b"R")
        if os.read(gate_fd, 1) != b"G":
            os._exit(125)
        os.close(gate_fd); os.close(ready_fd)
        os.set_inheritable(admission["input_fd"], True)
        os.set_inheritable(admission["engine_fd"], True)
        os.execve(admission["engine_fd"], [admission["manifest"]["executable"]["path"], "-i", f'/proc/self/fd/{admission["input_fd"]}'], engine_environment(admission))
    except BaseException as error:
        try:
            os.write(2, ("NR engine admission/exec failed: " + str(error) + "\n").encode())
        finally:
            os._exit(126)


def output_inventory(work, *, hashes=False):
    total, files = 0, []
    for directory, dirs, names in os.walk(work, followlinks=False):
        for name in dirs:
            require(not Path(directory, name).is_symlink(), "Linked output directory")
        for name in names:
            path = Path(directory, name)
            item = path.lstat()
            require(stat.S_ISREG(item.st_mode) and item.st_nlink == 1, "Linked or nonregular engine output")
            total += item.st_size
            require(len(files) < MAX_OUTPUT_FILES, "Output file-count limit exceeded")
            row = {"path": path.relative_to(work).as_posix(), "bytes": item.st_size}
            if hashes:
                fd, _, digest, count = read_pinned(path, limit=max(item.st_size, MAX_JSON + 1)); os.close(fd)
                require(count == item.st_size, "Output changed while retaining hashes")
                row["sha256"] = digest
            files.append(row)
    return total, sorted(files, key=lambda row: row["path"])


def emit(kind, **value):
    print(json.dumps({"event": kind, "at": utc(), **value}, allow_nan=False, separators=(",", ":")), flush=True)


def signal_stop(signum, _frame):
    global STOP_SIGNAL
    STOP_SIGNAL = signal.Signals(signum).name


def stopped_before_release(admission, control):
    if STOP_SIGNAL: return control, "supervisor_signal:" + STOP_SIGNAL
    if admission["deadline"] is not None and time.time() >= admission["deadline"]: return control, "deadline"
    reason = None
    if select.select([0], [], [], 0)[0]:
        raw = os.read(0, 4096)
        if not raw: return control, "parent_lost"
        control += raw
        require(len(control) <= 16384, "Control input exceeds its bound before engine release")
        while b"\n" in control:
            line, control = control.split(b"\n", 1)
            command = parse_json(line)
            require(set(command) == {"action"} and command["action"] in ("pause", "cancel"), "Unsupported supervisor control before launch")
            reason = command["action"]
    return control, reason


def supervise(admission):
    global STOP_SIGNAL
    STOP_SIGNAL = None
    job, work, request = admission["job"], admission["work"], admission["request"]
    # An interrupted attempt is never silently rerun in the same directory.
    lock = os.open(job / "supervisor.lock", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(lock, "w") as stream:
        stream.write(json.dumps({"pid": os.getpid(), "request_sha256": admission["request_sha256"]})); stream.flush(); os.fsync(stream.fileno())
    work.mkdir(mode=0o700, exist_ok=True); (work / ".tmp").mkdir(mode=0o700)
    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(signum, signal_stop)
    prctl(36, 1)  # PR_SET_CHILD_SUBREAPER: reap engine descendants on normal stop.
    start = time.monotonic()
    policy = admission["execution_policy"]
    receipt = {"schema": "phaseforge.nr-process-receipt.v2" if policy else "phaseforge.nr-process-receipt.v1", "job_id": request["job_id"], "started_at": utc(),
               "engine_id": request["engine_id"], "engine_manifest_sha256": admission["manifest_sha256"], "request_sha256": admission["request_sha256"],
               "executable": {"path": admission["manifest"]["executable"]["path"], "sha256": admission["engine_sha256"], "bytes": admission["engine_bytes"]},
               "input": {"path": "input.athinput", "sha256": admission["input_sha256"], "bytes": admission["input_bytes"]},
               "source": admission["manifest"]["source"], "build": admission["manifest"]["build"], "deadline_at": request["deadline_at"],
               "execution_resources": admission["execution_resources"],
               "affinity_cpus": admission["affinity"], "cpu_threads": request["cpu_threads"], "memory_limit_bytes": request["memory_limit_bytes"],
               "output_limit_bytes": request["output_limit_bytes"], "poll_seconds": POLL_SECONDS,
               "limits": {"hard": ["CPU affinity mask", "per-process RLIMIT_AS", "per-file RLIMIT_FSIZE", "core dumps disabled"],
                          "sampled_soft": ["aggregate process-group RSS including shared pages", "aggregate work-directory bytes", "absolute UTC deadline"],
                          "limitations": "Not a filesystem/network sandbox or cgroup. Thread environment values are hints; affinity limits runnable CPUs. Trusted children must not escape their process group. Sampling may overshoot; GPU memory is not admitted. Pause/cancel do not guarantee a checkpoint."},
               "peak_sampled_group_rss_bytes": 0, "peak_sampled_output_bytes": 0, "observed_pids": [], "checkpoint_resume_supported": False}
    if policy:
        receipt["execution_policy"] = policy
        receipt["limits"]["hard"] = ["CPU affinity mask", "dedicated cgroup memory.max", "memory.swap.max=0", "pids.max counts all tasks/threads", "per-file RLIMIT_FSIZE", "core dumps disabled"]
        receipt["limits"]["sampled_soft"] = ["aggregate work-directory bytes", "absolute UTC deadline"]
        receipt["limits"]["limitations"] = "The dedicated cgroup charges workload RAM, not CUDA virtual mappings or GPU VRAM. No hard GPU-memory cap or GPU execution admission is claimed. Supervisor and guardian memory is outside this workload boundary. This is not a filesystem/network sandbox. Trusted processes must not change their cgroup or privileges. Sampling may overshoot. Pause stops this immutable attempt."
        receipt["gpu_vram"] = {"execution_admitted": False, "hard_limit_enforced": False, "attributable_process_usage": None,
                               "kernel_execution_observed": None, "policy": policy["gpu_vram_policy"]}
        if policy["backend"] == "cuda":
            receipt["limits"]["sampled_soft"].append("device-wide GPU free-memory reserve and used-memory growth, every 1 second")
            receipt["limits"]["limitations"] = "Workload RAM is enforced by a dedicated cgroup. GPU memory is a device-wide sampled soft guard, not reserved capacity, a hard quota, or per-process attribution. Other applications can trigger this attempt's stop. GPU kernel execution is not inferred from admission. Supervisor/guardian stay outside the RAM boundary. Not a filesystem/network sandbox; trusted processes must not change cgroup or privileges. Sampling may overshoot. Pause stops this immutable attempt."
            receipt["gpu_vram"].update(guard_source_sha256=gpu_module().source_sha256, sample_count=0)
    atomic_json(job / "launch.json", receipt)
    emit("admitted", job_id=request["job_id"], **{key: receipt[key] for key in ("deadline_at", "affinity_cpus", "execution_resources", "limits")})
    out = os.open(work / "solver.stdout.log", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    err = os.open(work / "solver.stderr.log", os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    gate_r, gate_w = os.pipe(); ready_r, ready_w = os.pipe()
    group = guard = None
    scope = None
    guard_w = None
    reason, engine_status, drained = None, None, False
    control = b""
    try:
        baseline = None
        gpu_policy = policy["gpu_vram_policy"] if policy and policy["backend"] == "cuda" else None
        next_gpu_sample = 0.0
        if gpu_policy:
            admission["cuda_snapshot"] = cuda_snapshot(admission)
            receipt["cuda_runtime_snapshot"] = admission["cuda_snapshot"]
            baseline, decision = observe_gpu(gpu_policy)
            receipt["gpu_vram"].update(baseline=baseline, admission_guard=decision, sample_count=1)
            if not decision["allowed"]: reason = "gpu_admission:" + decision["reason"]
            require(decision["allowed"], "Fresh GPU admission failed")
        parent = os.getpid(); group = os.fork()
        if group == 0:
            engine_child(admission, gate_r, ready_w, out, err, parent)
        os.close(gate_r); os.close(ready_w); os.close(out); os.close(err)
        require(select.select([ready_r], [], [], 3)[0] and os.read(ready_r, 1) == b"R", "Engine launch gate was not established")
        os.close(ready_r)
        if policy:
            scope = CgroupScope.create(request["job_id"], group, request["memory_limit_bytes"], policy["tasks_max"])
            receipt["memory_boundary"] = {"kind": "systemd_cgroup_v2", "unit": scope.unit, "path": scope.group_path,
                "inode": scope.inode, "effective_kernel_limits": scope.effective, "supervisor_cgroup": scope.parent_cgroup,
                "child_cgroup_before_exec": process_cgroup(group), "before": scope.sample(), "address_space_limit_bytes": None}
        guard_r, guard_w = os.pipe()
        guard = os.fork()
        if guard == 0:
            guardian_loop(guard_r, group, scope)
        os.close(guard_r)
        receipt.update({"engine_pid": group, "process_group_id": group, "guardian_pid": guard, "supervisor_pid": parent})
        if scope:
            receipt["memory_boundary"]["guardian_cgroup"] = process_cgroup(guard)
            require(process_cgroup(guard) != scope.group_path, "Guardian must remain outside the workload memory scope")
        if gpu_policy:
            sample, decision = observe_gpu(gpu_policy, baseline)
            receipt["gpu_vram"].update(before_launch=sample, before_launch_guard=decision, sample_count=2)
            if not decision["allowed"]: reason = "gpu_admission:" + decision["reason"]
            require(decision["allowed"], "GPU admission changed while the engine gate was closed")
            control, reason = stopped_before_release(admission, control)
            require(reason is None, "The original task stopped before engine release")
            receipt["gpu_vram"]["execution_admitted"] = True
            next_gpu_sample = time.monotonic()+gpu_policy["poll_interval_seconds"]
        atomic_json(job / "launch.json", receipt)
        if gpu_policy:
            # A slow durable write is still part of admission. Recheck the same
            # sample and original control/deadline immediately before gate G;
            # never refresh the timer or silently replace stale observations.
            decision = gpu_module().evaluate(gpu_policy, baseline, receipt["gpu_vram"]["before_launch"])
            receipt["gpu_vram"]["before_launch_guard"] = decision
            control, reason = stopped_before_release(admission, control)
            if reason is None and not decision["allowed"]: reason = "gpu_admission:" + decision["reason"]
            if reason is not None:
                receipt["gpu_vram"]["execution_admitted"] = False
                atomic_json(job / "launch.json", receipt)
            require(reason is None, "Engine release refused after the durable launch record")
        os.write(gate_w, b"G"); os.close(gate_w)
        emit("running", job_id=request["job_id"], engine_pid=group, process_group_id=group, guardian_pid=guard)
        os.set_blocking(0, False)
        last_emit = 0.0
        while reason is None:
            if STOP_SIGNAL:
                reason = "supervisor_signal:" + STOP_SIGNAL; break
            if admission["deadline"] is not None and time.time() >= admission["deadline"]:
                reason = "deadline"; break
            if select.select([0], [], [], 0)[0]:
                raw = os.read(0, 4096)
                if not raw:
                    reason = "parent_lost"; break
                control += raw
                require(len(control) <= 16384, "Control input exceeds its bound")
                while b"\n" in control:
                    line, control = control.split(b"\n", 1)
                    command = parse_json(line)
                    require(set(command) == {"action"} and command["action"] in ("pause", "cancel"), "Unsupported supervisor control")
                    reason = command["action"]
            rows = group_rows(group)
            rss = sum(row["rss_bytes"] for row in rows)
            total, _ = output_inventory(work)
            receipt["peak_sampled_group_rss_bytes"] = max(receipt["peak_sampled_group_rss_bytes"], rss)
            receipt["peak_sampled_output_bytes"] = max(receipt["peak_sampled_output_bytes"], total)
            receipt["observed_pids"] = sorted(set(receipt["observed_pids"]) | {row["pid"] for row in rows})
            if scope:
                sampled = scope.sample()
                receipt["memory_boundary"]["latest_sample"] = sampled
                if sampled["last_observed_values"].get("memory.events", {}).get("oom_kill", 0): reason = reason or "cgroup_oom"
            elif rss > request["memory_limit_bytes"]: reason = reason or "aggregate_rss_limit"
            if gpu_policy and time.monotonic() >= next_gpu_sample:
                previous = receipt["gpu_vram"].get("latest_sample",receipt["gpu_vram"]["before_launch"])
                decision = gpu_module().evaluate(gpu_policy, baseline, previous)
                if decision["allowed"]:
                    sample_start = time.monotonic()
                    sample, decision = observe_gpu(gpu_policy, baseline)
                    next_gpu_sample = sample_start+gpu_policy["poll_interval_seconds"]
                else:
                    sample = previous
                receipt["gpu_vram"].update(latest_sample=sample, latest_guard=decision,
                    sample_count=receipt["gpu_vram"]["sample_count"]+1)
                growth = decision["device_used_growth_bytes"]
                if growth is not None:
                    receipt["gpu_vram"]["peak_sampled_device_used_growth_bytes"] = max(receipt["gpu_vram"].get("peak_sampled_device_used_growth_bytes",0),growth)
                if not decision["allowed"]: reason = reason or "gpu_guard:" + decision["reason"]
            if total > request["output_limit_bytes"]: reason = reason or "aggregate_output_limit"
            if os.waitpid(guard, os.WNOHANG)[0]:
                guard = None; reason = reason or "guardian_lost"
            if engine_status is None:
                pid, status = os.waitpid(group, os.WNOHANG)
                if pid:
                    engine_status = status
                    if scope and scope.property("Result") == "oom-kill": reason = reason or "cgroup_oom"
                    reason = reason or ("completed" if os.waitstatus_to_exitcode(status) == 0 and not group_rows(group) else "engine_exit")
            if time.monotonic() - last_emit >= 1:
                emit("resources", job_id=request["job_id"], rss_bytes=rss, output_bytes=total, elapsed_seconds=time.monotonic() - start)
                last_emit = time.monotonic()
            if reason is None: time.sleep(POLL_SECONDS)
    except BrokenPipeError as error:
        reason = "parent_lost"
        receipt["error"] = "Parent status transport closed"
    except BaseException as error:
        reason = reason or "supervisor_error"
        receipt["error"] = f"{type(error).__name__}: {error}"
    finally:
        if scope:
            try:
                receipt["memory_boundary"]["before_cleanup"] = scope.sample()
                receipt["memory_boundary"]["systemd_result"] = scope.property("Result")
                if receipt["memory_boundary"]["systemd_result"] == "oom-kill": reason = "cgroup_oom"
                receipt["memory_boundary"]["cgroup_drained"] = scope.stop()
            except BaseException as error:
                receipt["memory_boundary"]["cgroup_drained"] = False
                receipt["memory_boundary"]["cleanup_error"] = str(error)
        if group:
            drained = terminate_group(group)
            if scope: drained = drained and receipt["memory_boundary"]["cgroup_drained"]
            if engine_status is None:
                end = time.monotonic() + KILL_SECONDS
                while time.monotonic() < end:
                    try:
                        pid, status = os.waitpid(group, os.WNOHANG)
                    except ChildProcessError:
                        break
                    if pid:
                        engine_status = status; break
                    time.sleep(0.02)
        else:
            drained = True  # Admission can fail before any workload is forked.
        if guard_w is not None:
            try:
                if drained: os.write(guard_w, b"D")
            except BrokenPipeError:
                pass
            os.close(guard_w)
        if guard:
            end = time.monotonic() + TERM_SECONDS + KILL_SECONDS + 1
            while time.monotonic() < end:
                if os.waitpid(guard, os.WNOHANG)[0]: break
                time.sleep(0.02)
        for key in ("engine_fd", "input_fd"):
            os.close(admission[key])
        if admission.get("cuda_runtime"):
            os.close(admission["cuda_runtime"]["library_fd"])
        if scope:
            receipt["memory_boundary"]["after_cleanup"] = scope.sample()
            scope.close()
    receipt.update({"finished_at": utc(), "elapsed_seconds": time.monotonic() - start, "termination_reason": reason,
                    "process_group_drained": drained, "exit_code": None if engine_status is None else os.waitstatus_to_exitcode(engine_status)})
    try:
        total, files = output_inventory(work, hashes=True)
        receipt.update({"retained_bytes": total, "files": files})
        if total > request["output_limit_bytes"] and reason == "completed": receipt["termination_reason"] = "aggregate_output_limit"
    except BaseException as error:
        receipt["inventory_error"] = str(error)
        if reason == "completed": receipt["termination_reason"] = "invalid_output"
    atomic_json(job / "receipt.json", receipt)
    emit("terminal", **receipt)
    return 0 if receipt["termination_reason"] == "completed" and drained else 3


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--engine-manifest", required=True)
    parser.add_argument("--request", required=True)
    parser.add_argument("--job-root", required=True)
    args = parser.parse_args()
    try:
        return supervise(admit(args.engine_manifest, args.request, args.job_root))
    except BaseException as error:
        try:
            emit("rejected", error=f"{type(error).__name__}: {error}")
        except BrokenPipeError:
            pass
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
