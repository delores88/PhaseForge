"""Read-only managed-runtime materials; never import or execute inspected code.

This is a bounded file inventory, not a complete SBOM, vulnerability scan, or
proof of an execution boundary. Only the explicit output JSON is written.
"""
from __future__ import annotations

import argparse
import contextlib
import ctypes
import datetime
import email.parser
import hashlib
import json
import math
import os
from pathlib import Path, PurePosixPath
import re
import stat
import native_identity


MAX_FILES = 100_000
MAX_BYTES = 4 * 1024**3
MAX_FILE = 512 * 1024**2
MAX_DOCUMENT = 8 * 1024**2
NATIVE_SUFFIXES = {".exe", ".dll", ".pyd", ".so", ".dylib"}
SHA256 = re.compile(r"[0-9a-f]{64}\Z")
FROZEN_MANIFEST_ROOT = Path(__file__).absolute().parents[2] / "tools/runtime-seeds"
SEED_MANIFEST = "phaseforge-runtime-seed.json"
SEED_LAYOUT = {"science-v5": "environments/science-v5",
               "python-numpy-v4": "environments/python-numpy-v4/runtime"}


class InventoryError(ValueError):
    pass


def absolute(path):
    value = Path(os.path.abspath(path))
    if os.name == "nt" and str(value).startswith("\\\\"):
        raise InventoryError("Network and device paths are outside this inventory scope")
    return value


def guard(path, *, missing=False):
    """Reject links in every existing component before reading or creating."""
    path = absolute(path)
    for part in [*reversed(path.parents), path]:
        try:
            info = part.lstat()
        except FileNotFoundError:
            if missing:
                continue
            raise InventoryError(f"Missing input: {part}") from None
        if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
            raise InventoryError(f"Symlink or reparse point refused: {part}")
        if stat.S_ISREG(info.st_mode) and info.st_nlink != 1:
            raise InventoryError(f"Hardlinked input refused: {part}")
        if not (stat.S_ISDIR(info.st_mode) or stat.S_ISREG(info.st_mode)):
            raise InventoryError(f"Non-regular input refused: {part}")
    return path


def relative_name(value):
    if not isinstance(value, str) or not value or "\\" in value or ":" in value:
        raise InventoryError("Invalid manifest relative path")
    parts = value.split("/")
    if any(part in {"", ".", ".."} or part.rstrip(" .") != part for part in parts):
        raise InventoryError("Invalid manifest relative path")
    if PurePosixPath(value).is_absolute():
        raise InventoryError("Absolute manifest path refused")
    return value


@contextlib.contextmanager
def opened(path):
    """Hash an unlinked regular file; on Windows deny replacement during read."""
    path = guard(path)
    before = path.lstat()
    if not stat.S_ISREG(before.st_mode) or before.st_size > MAX_FILE:
        raise InventoryError(f"Input exceeds regular-file bounds: {path}")
    if os.name == "nt":
        import msvcrt
        from ctypes import wintypes
        kernel = ctypes.WinDLL("kernel32", use_last_error=True)
        kernel.CreateFileW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD,
                                      ctypes.c_void_p, wintypes.DWORD, wintypes.DWORD, wintypes.HANDLE]
        kernel.CreateFileW.restype = wintypes.HANDLE
        kernel.CloseHandle.argtypes = [wintypes.HANDLE]
        kernel.GetFinalPathNameByHandleW.argtypes = [wintypes.HANDLE, wintypes.LPWSTR,
                                                   wintypes.DWORD, wintypes.DWORD]
        kernel.GetFinalPathNameByHandleW.restype = wintypes.DWORD
        handle = kernel.CreateFileW(str(path), 0x80000000, 1, None, 3, 0x00200000, None)
        if handle == wintypes.HANDLE(-1).value:
            raise ctypes.WinError(ctypes.get_last_error())
        try:
            buffer = ctypes.create_unicode_buffer(32768)
            count = kernel.GetFinalPathNameByHandleW(handle, buffer, len(buffer), 0)
            if not 0 < count < len(buffer):
                raise InventoryError("Cannot verify opened file location")
            final_path = buffer.value.removeprefix("\\\\?\\")
            if os.path.normcase(final_path) != os.path.normcase(str(path)):
                raise InventoryError(f"Opened input changed location: {path}")
            descriptor = msvcrt.open_osfhandle(handle, os.O_RDONLY | os.O_BINARY)
            handle = None
        finally:
            if handle is not None:
                kernel.CloseHandle(handle)
    else:
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    with os.fdopen(descriptor, "rb") as stream:
        current = os.fstat(stream.fileno())
        if (not stat.S_ISREG(current.st_mode) or current.st_nlink != 1
                or (before.st_dev, before.st_ino) != (current.st_dev, current.st_ino)
                or getattr(current, "st_file_attributes", 0) & 0x400):
            raise InventoryError(f"Input identity changed or linked: {path}")
        yield stream, current
        after = os.fstat(stream.fileno())
        if (current.st_size, current.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            raise InventoryError(f"Input changed during read: {path}")
        guard(path)


def file_record(path):
    with opened(path) as (stream, info):
        digest = hashlib.sha256()
        count = 0
        while block := stream.read(1024 * 1024):
            count += len(block)
            if count > MAX_FILE:
                raise InventoryError("File grew beyond inventory bound")
            digest.update(block)
        if count != info.st_size:
            raise InventoryError("File size changed during inventory")
    return {"bytes": count, "sha256": digest.hexdigest()}


def document(path):
    with opened(path) as (stream, info):
        if info.st_size > MAX_DOCUMENT:
            raise InventoryError(f"Metadata document exceeds bound: {path}")
        return stream.read(MAX_DOCUMENT + 1).decode("utf-8-sig")


def read_json(path):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise InventoryError("Duplicate JSON key")
            result[key] = value
        return result
    def nonfinite(value):
        raise InventoryError(f"Non-finite JSON number: {value}")
    def finite_float(value):
        parsed = float(value)
        if not math.isfinite(parsed):
            return nonfinite(value)
        return parsed
    return json.loads(document(path), object_pairs_hook=pairs, parse_constant=nonfinite, parse_float=finite_float)


def inventory(root):
    root = guard(root, missing=True)
    if not root.exists():
        return {"root": str(root), "present": False, "files": [], "native_files": [],
                "packages": [], "total_bytes": 0}
    if not root.is_dir():
        raise InventoryError(f"Runtime root must be a directory: {root}")
    files, seen, total = [], set(), 0
    pending = [root]
    directories = 0
    while pending:
        directory = guard(pending.pop())
        directories += 1
        if directories > MAX_FILES:
            raise InventoryError("Directory count exceeds inventory bound")
        with os.scandir(directory) as entries:
            for entry in entries:
                path = guard(entry.path)
                if path.is_dir():
                    pending.append(path)
                    continue
                relative = path.relative_to(root).as_posix()
                relative_name(relative)
                if relative.casefold() in seen:
                    raise InventoryError("Case-colliding inventory paths")
                seen.add(relative.casefold())
                row = {"path": relative, **file_record(path)}
                total += row["bytes"]
                files.append(row)
                if total > MAX_BYTES or len(files) > MAX_FILES:
                    raise InventoryError("Runtime exceeds inventory bounds")
    files.sort(key=lambda row: row["path"])
    packages = []
    for row in files:
        path = PurePosixPath(row["path"])
        if path.name == "METADATA" and path.parent.name.endswith(".dist-info"):
            metadata = email.parser.Parser().parsestr(document(root / row["path"]), headersonly=True)
            names, versions = metadata.get_all("Name", []), metadata.get_all("Version", [])
            valid = len(names) == 1 and len(versions) == 1 and bool(names[0].strip()) and bool(versions[0].strip())
            packages.append({"name": names[0].strip() if len(names) == 1 else None,
                             "version": versions[0].strip() if len(versions) == 1 else None,
                             "metadata": row, "status": "observed_metadata" if valid else "missing_or_ambiguous_metadata",
                             "scope": "Installed distribution METADATA; package not imported, version not inferred from directory name"})
    return {"root": str(root), "present": True, "files": files,
            "native_files": [row for row in files if Path(row["path"]).suffix.lower() in NATIVE_SUFFIXES],
            "packages": packages, "total_bytes": total}


def pe_version(path):
    """Read version resources through Windows version APIs, never LoadLibrary(path)."""
    if os.name != "nt":
        return {"status": "missing", "reason": "PE version-resource reader requires Windows"}
    from ctypes import wintypes
    version = ctypes.WinDLL("version", use_last_error=True)
    version.GetFileVersionInfoSizeW.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(wintypes.DWORD)]
    version.GetFileVersionInfoSizeW.restype = wintypes.DWORD
    version.GetFileVersionInfoW.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, wintypes.DWORD, ctypes.c_void_p]
    version.GetFileVersionInfoW.restype = wintypes.BOOL
    version.VerQueryValueW.argtypes = [ctypes.c_void_p, wintypes.LPCWSTR, ctypes.POINTER(ctypes.c_void_p), ctypes.POINTER(wintypes.UINT)]
    version.VerQueryValueW.restype = wintypes.BOOL
    unused = wintypes.DWORD()
    size = version.GetFileVersionInfoSizeW(str(guard(path)), ctypes.byref(unused))
    if not 0 < size <= MAX_DOCUMENT:
        return {"status": "missing", "reason": "No bounded readable PE version resource"}
    buffer = ctypes.create_string_buffer(size)
    if not version.GetFileVersionInfoW(str(path), 0, size, buffer):
        return {"status": "missing", "reason": "PE version resource could not be read"}
    pointer, length = ctypes.c_void_p(), wintypes.UINT()
    if not version.VerQueryValueW(buffer, "\\", ctypes.byref(pointer), ctypes.byref(length)) or length.value < 52:
        return {"status": "missing", "reason": "PE fixed version information missing"}
    fields = ctypes.cast(pointer, ctypes.POINTER(wintypes.DWORD * 13)).contents
    if fields[0] != 0xFEEF04BD:
        return {"status": "missing", "reason": "Unrecognized PE version signature"}
    def number(high, low):
        return ".".join(str(value) for value in (high >> 16, high & 65535, low >> 16, low & 65535))
    return {"status": "observed_pe_resource", "file_version": number(fields[2], fields[3]),
            "product_version": number(fields[4], fields[5]),
            "scope": "PE resource claim, not an executed Python version or authenticated provenance"}


def external_base(science_root, bootstrap):
    cfg = science_root / "pyvenv.cfg"
    if not guard(cfg, missing=True).exists():
        return {"status": "missing_pyvenv_cfg", "bundled": False, "files": []}, None
    raw = document(cfg)
    values = {}
    for line in raw.splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            key = key.strip().lower()
            if key in values:
                raise InventoryError("Duplicate pyvenv.cfg key")
            values[key] = value.strip()
    result = {"status": "observed_configuration", "bundled": False, "configuration": values,
              "files": [], "scope": "External host prerequisite. Root native files, Python ZIP archives and DLLs directory only; standard library, loaded-module closure and host installation provenance remain incomplete."}
    home = values.get("home")
    if not home or not Path(home).is_absolute():
        result["status"] = "missing_or_relative_base_home"
        return result, {"text": raw, **file_record(cfg)}
    base = guard(home, missing=True)
    result["root"] = str(base)
    result["bootstrap_matches_configured_home"] = bootstrap.parent == base
    configured_executable = values.get("executable")
    if configured_executable:
        result["bootstrap_matches_configured_executable"] = os.path.normcase(str(bootstrap)) == os.path.normcase(configured_executable)
    if not base.exists():
        result["status"] = "base_home_missing"
        return result, {"text": raw, **file_record(cfg)}
    if not base.is_dir():
        raise InventoryError("pyvenv base home must be a directory")
    total = 0
    with os.scandir(base) as entries:
        for entry in entries:
            candidate = Path(entry.path)
            if candidate.suffix.lower() in NATIVE_SUFFIXES or (candidate.name.lower().startswith("python") and candidate.suffix.lower() == ".zip"):
                candidate = guard(candidate)
                if not candidate.is_file():
                    raise InventoryError("External runtime material must be a regular file")
                row = {"path": candidate.name, **file_record(candidate)}
                result["files"].append(row)
                total += row["bytes"]
                if len(result["files"]) > MAX_FILES or total > MAX_BYTES:
                    raise InventoryError("External runtime material exceeds inventory bounds")
    dlls = inventory(base / "DLLs")
    result["files"].extend({**row, "path": "DLLs/" + row["path"]} for row in dlls["files"])
    result["files"].sort(key=lambda row: row["path"])
    if len(result["files"]) > MAX_FILES or sum(row["bytes"] for row in result["files"]) > MAX_BYTES:
        raise InventoryError("External runtime material exceeds inventory bounds")
    return result, {"text": raw, **file_record(cfg)}


def isolation_evidence(runtime, missing):
    root = Path(runtime["root"])
    manifest_path = root / "phaseforge-isolation-runtime.json"
    if not guard(manifest_path, missing=True).exists():
        missing.append("python_numpy: retained runtime source/file manifest missing")
        return {"manifest": None, "pin_verification": {"valid": False, "reason": "manifest_missing"}, "cached_archives": []}
    manifest = read_json(manifest_path)
    if not isinstance(manifest, dict) or manifest.get("schema_version") != 1:
        raise InventoryError("Unsupported isolation manifest schema")
    pins, sources = manifest.get("files"), manifest.get("sources")
    if not isinstance(pins, dict) or not 0 < len(pins) <= MAX_FILES or not isinstance(sources, list) or len(sources) > 32:
        raise InventoryError("Invalid isolation manifest pins or sources")
    seen = set()
    for name, digest in pins.items():
        relative_name(name)
        if name.casefold() in seen or not isinstance(digest, str) or not SHA256.fullmatch(digest):
            raise InventoryError("Invalid or case-colliding isolation file pin")
        seen.add(name.casefold())
    observed = {row["path"]: row for row in runtime["files"] if row["path"] != manifest_path.name}
    missing_files = sorted(set(pins) - set(observed))
    unexpected = sorted(set(observed) - set(pins))
    changed = [{"path": name, "expected_sha256": pins[name], "observed_sha256": observed[name]["sha256"]}
               for name in sorted(set(pins) & set(observed)) if pins[name] != observed[name]["sha256"]]
    archives, source_names = [], set()
    for source in sources:
        if not isinstance(source, dict):
            raise InventoryError("Invalid isolation source")
        name = relative_name(source.get("name"))
        if "/" in name or name.casefold() in source_names or not isinstance(source.get("sha256"), str) or not SHA256.fullmatch(source["sha256"]):
            raise InventoryError("Invalid isolation archive pin")
        if not isinstance(source.get("url"), str) or len(source["url"]) > 4096:
            raise InventoryError("Invalid retained source URL")
        source_names.add(name.casefold())
        archive = guard(root.parent / ".isolation-downloads" / name, missing=True)
        row = {"source": source, "path": str(archive), "present": archive.exists()}
        if row["present"]:
            row.update(file_record(archive))
            row["matches_manifest_pin"] = row["sha256"] == source["sha256"]
        else:
            row["matches_manifest_pin"] = None
            missing.append(f"python_numpy: pinned archive bytes not retained: {name}")
        archives.append(row)
    if not sources:
        missing.append("python_numpy: no retained archive source pins")
    valid = not missing_files and not unexpected and not changed and all(row.get("matches_manifest_pin") is not False for row in archives)
    return {"manifest": {"path": manifest_path.name, **file_record(manifest_path), "content": manifest},
            "pin_verification": {"valid": valid, "pinned_files": len(pins), "missing_files": missing_files,
                                 "unexpected_files": unexpected, "changed_files": changed,
                                 "scope": "Comparison with the retained manifest; does not independently authenticate manifest origins or assert current shipped contract pins"},
            "cached_archives": archives}


def verify_seed_manifest(runtime, expected_kind):
    root = Path(runtime["root"])
    manifest_path = guard(root / SEED_MANIFEST, missing=True)
    if not manifest_path.exists():
        return {"valid": False, "manifest": None, "reason": "seed_manifest_missing"}
    manifest = read_json(manifest_path)
    if (not isinstance(manifest, dict) or manifest.get("schema_version") != 1
            or manifest.get("schema") != "phaseforge.runtime-seed.v1" or manifest.get("kind") != expected_kind):
        raise InventoryError("Unsupported runtime seed manifest")
    pins, sizes, native = manifest.get("files"), manifest.get("file_bytes"), manifest.get("native_files")
    if (not isinstance(pins, dict) or not 0 < len(pins) <= MAX_FILES
            or not isinstance(sizes, dict) or set(sizes) != set(pins) or not isinstance(native, dict)):
        raise InventoryError("Invalid seed file, size or native inventory")
    seen = set()
    for name, digest in pins.items():
        relative_name(name)
        if (name == SEED_MANIFEST or name.casefold() in seen or not isinstance(digest, str)
                or not SHA256.fullmatch(digest) or type(sizes[name]) is not int
                or not 0 <= sizes[name] <= MAX_FILE):
            raise InventoryError("Invalid seed file hash/size pin")
        seen.add(name.casefold())
    expected_native = {name: digest for name, digest in pins.items() if Path(name).suffix.lower() in NATIVE_SUFFIXES}
    if native != expected_native:
        raise InventoryError("Seed native-file pins do not match its file inventory")
    if "sqlite3.dll" in native:
        replacement = manifest.get("transformations", {}).get("native_replacements", {}).get("sqlite3.dll")
        if (manifest.get("sqlite") != "3.53.4" or not isinstance(replacement, dict)
                or replacement.get("version") != manifest["sqlite"]):
            raise InventoryError("Active runtime lacks the exact SQLite replacement provenance")
    if "libcrypto-3.dll" in native or "libssl-3.dll" in native:
        replacements = manifest.get("transformations", {}).get("native_replacements", {})
        if manifest.get("openssl") != "3.0.22" or any(
                name not in native or not isinstance(replacements.get(name), dict)
                or replacements[name].get("component") != "OpenSSL" or replacements[name].get("version") != manifest["openssl"]
                for name in ("libcrypto-3.dll", "libssl-3.dll")):
            raise InventoryError("Active runtime lacks the exact OpenSSL pair replacement provenance")
    sources = manifest.get("sources")
    if not isinstance(sources, list) or not 0 < len(sources) <= 32:
        raise InventoryError("Seed archive source pins are missing or invalid")
    seen_sources = set()
    for source in sources:
        if not isinstance(source, dict):
            raise InventoryError("Invalid seed archive source")
        name = relative_name(source.get("name"))
        if ("/" in name or name.casefold() in seen_sources
                or not isinstance(source.get("sha256"), str) or not SHA256.fullmatch(source["sha256"])
                or type(source.get("bytes")) is not int or not 0 <= source["bytes"] <= MAX_FILE
                or not isinstance(source.get("url"), str) or len(source["url"]) > 4096):
            raise InventoryError("Invalid seed archive hash/size/source pin")
        seen_sources.add(name.casefold())
    observed = {row["path"]: row for row in runtime["files"] if row["path"] != SEED_MANIFEST}
    missing = sorted(set(pins) - set(observed))
    unexpected = sorted(set(observed) - set(pins))
    changed = [{"path": name, "expected_sha256": pins[name], "observed_sha256": observed[name]["sha256"],
                "expected_bytes": sizes[name], "observed_bytes": observed[name]["bytes"]}
               for name in sorted(set(pins) & set(observed))
               if pins[name] != observed[name]["sha256"] or sizes[name] != observed[name]["bytes"]]
    inner_check = None
    if expected_kind == "python-numpy-v4":
        inner_path = root / "phaseforge-isolation-runtime.json"
        if not guard(inner_path, missing=True).exists():
            inner_check = {"valid": False, "reason": "isolation_manifest_missing"}
        else:
            inner = read_json(inner_path)
            expected_inner = {name: row["sha256"] for name, row in observed.items() if name != inner_path.name}
            inner_check = {"valid": isinstance(inner, dict) and inner.get("schema_version") == 1
                           and inner.get("files") == expected_inner and inner.get("sources") == sources
                           and inner.get("python") == manifest.get("python") and inner.get("numpy") == manifest.get("numpy")
                           and inner.get("sqlite") == manifest.get("sqlite") and inner.get("openssl") == manifest.get("openssl"),
                           "scope": "Inner file hashes cover runtime members except both manifest files. The frozen outer manifest separately pins the inner manifest; its own bytes are compared with the frozen source-tree manifest."}
    return {"valid": not missing and not unexpected and not changed and (inner_check is None or inner_check["valid"]),
            "manifest": {"path": SEED_MANIFEST, **file_record(manifest_path), "content": manifest},
            "pinned_files": len(pins), "missing_files": missing, "unexpected_files": unexpected,
            "changed_files": changed, "isolation_manifest_verification": inner_check}


def bundled_report(workspace, seed_root, bootstrap_python, source_commit, archive_root=None):
    workspace, seed_root = guard(workspace), guard(seed_root)
    if not workspace.is_dir() or not seed_root.is_dir():
        raise InventoryError("Workspace and seed root must be existing directories")
    missing = ["Package/Syft completeness, static-link identities and loaded native dependency closure are unknown; this file inventory is not a complete SBOM",
               "PE version resources may be missing. Upstream wheel ownership and metadata are not the version of every transitive DLL; native_identity retains the explicit gaps and review witnesses",
               "Full standard-library/configuration transformations require separate build material receipts; native_identity verifies original native archive members only when an archive root is supplied"]
    runtimes = {}
    valid = True
    for name, destination_relative in SEED_LAYOUT.items():
        source, destination = inventory(seed_root / name), inventory(workspace / destination_relative)
        source_check, destination_check = verify_seed_manifest(source, name), verify_seed_manifest(destination, name)
        identity = native_identity.evidence(Path(source["root"]), source["native_files"],
                                            source_check.get("manifest", {}).get("content", {}).get("sources", []) if source_check.get("manifest") else [],
                                            guard(archive_root) if archive_root is not None else None,
                                            record=file_record, opened=opened, version=pe_version,
                                            aliases=source_check.get("manifest", {}).get("content", {}).get("transformations", {}).get("native_aliases", {}) if source_check.get("manifest") else {},
                                            replacements=source_check.get("manifest", {}).get("content", {}).get("transformations", {}).get("native_replacements", {}) if source_check.get("manifest") else {})
        source_rows = {row["path"]: row for row in source["files"]}
        destination_rows = {row["path"]: row for row in destination["files"]}
        changed = [path for path in sorted(set(source_rows) & set(destination_rows)) if source_rows[path] != destination_rows[path]]
        absent = sorted(set(source_rows) - set(destination_rows))
        unexpected = sorted(set(destination_rows) - set(source_rows))
        frozen_path = guard(FROZEN_MANIFEST_ROOT / f"{name}.manifest.json", missing=True)
        frozen = {"path": str(frozen_path), "present": frozen_path.exists(), "matches_installed_seed_manifest": False}
        if frozen["present"]:
            frozen.update(file_record(frozen_path))
            source_manifest = source_check.get("manifest")
            frozen["matches_installed_seed_manifest"] = bool(source_manifest and frozen["sha256"] == source_manifest["sha256"] and frozen["bytes"] == source_manifest["bytes"])
        else:
            missing.append(f"{name}: frozen source-tree seed manifest missing")
        comparison_valid = (source["present"] and destination["present"] and source_check["valid"]
                            and destination_check["valid"] and not changed and not absent and not unexpected
                            and frozen["matches_installed_seed_manifest"])
        valid = valid and comparison_valid
        for scope, runtime in [("installed_seed", source), ("managed_copy", destination)]:
            if not runtime["present"]:
                missing.append(f"{name}: {scope} directory missing")
            for package in runtime["packages"]:
                if package["status"] != "observed_metadata":
                    missing.append(f"{name}/{scope}: package version/name missing or ambiguous in {package['metadata']['path']}")
        runtimes[name] = {"delivery": "bundled_immutable_seed_copied_to_app_data", "external_base_runtime_required": False,
                          "mapping": {"seed_relative": name, "workspace_relative": destination_relative,
                                      "file_mapping": "Identical relative paths under these explicitly mapped roots, including the exact outer manifest bytes"},
                          "installed_seed": source, "managed_copy": destination,
                          "native_identity": identity,
                          "seed_pin_verification": source_check, "copy_pin_verification": destination_check,
                          "frozen_source_manifest": frozen,
                          "copy_comparison": {"valid": bool(comparison_valid), "changed_files": changed,
                                              "missing_files": absent, "unexpected_files": unexpected,
                                              "outer_manifest_identical": bool(source_rows.get(SEED_MANIFEST) and source_rows.get(SEED_MANIFEST) == destination_rows.get(SEED_MANIFEST))},
                          "archive_provenance": {"status": "verified_archive_bytes_and_original_native_members" if archive_root is not None else "declared_hash_and_size_pins_in_seed_manifest",
                                                 "manifest_matches_frozen_source": frozen["matches_installed_seed_manifest"],
                                                 "sources": source_check.get("manifest", {}).get("content", {}).get("sources", []) if source_check.get("manifest") else [],
                                                 "archive_bytes_inspected": archive_root is not None,
                                                 "scope": "Runtime delivery is from installed bundled seeds, not post-install executable downloads. When explicitly supplied, pinned cache archives are read locally and native_identity binds original native members. No upstream endpoint was queried."}}
    bootstrap = None
    if bootstrap_python is not None:
        path = guard(bootstrap_python)
        bootstrap = {"path": str(path), **file_record(path), "pe_version": pe_version(path), "executed": False,
                     "role": "optional_host_reference_only", "runtime_prerequisite": False,
                     "scope": "Explicitly supplied host file; the portable bundled runtimes do not depend on this host Python"}
        if file_record(path)["sha256"] != bootstrap["sha256"]:
            raise InventoryError("Host Python reference changed during metadata read")
    return {"schema_version": 1, "source_commit": source_commit, "workspace": str(workspace), "seed_root": str(seed_root),
            "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "delivery": "bundled_immutable_seeds", "runtime_execution": False, "complete_sbom": False,
            "integrity_valid": bool(valid), "bootstrap_python": bootstrap, "runtimes": runtimes,
            "scope": "Read-only exact file/hash/size comparison of installed bundled seeds, fixed app-data copies and frozen source-tree seed manifests. No managed runtimes or packages executed; no installer, solver, network, scanner or build invoked. Matching bytes do not establish complete SBOM or loaded-module coverage.",
            "missing_provenance": missing}


def build_report(workspace, bootstrap_python=None, source_commit=None, seed_root=None, archive_root=None):
    if seed_root is not None:
        return bundled_report(workspace, seed_root, bootstrap_python, source_commit, archive_root)
    if bootstrap_python is None:
        raise InventoryError("Historical venv inventory requires --bootstrap-python; bundled inventory requires --seed-root")
    workspace, bootstrap = guard(workspace), guard(bootstrap_python)
    if not workspace.is_dir():
        raise InventoryError("Workspace must be an existing app-data directory")
    if not bootstrap.is_file():
        raise InventoryError("Bootstrap Python must be an explicit existing regular executable file")
    bootstrap_record = {"path": str(bootstrap), **file_record(bootstrap), "pe_version": pe_version(bootstrap),
                        "bundled": False, "executed": False}
    if file_record(bootstrap)["sha256"] != bootstrap_record["sha256"]:
        raise InventoryError("Bootstrap changed while reading version metadata")
    missing = ["Package/Syft completeness and native dependency closure are unknown; this file inventory is not a complete SBOM",
               "External bootstrap Python installation/download provenance is not established by file hashes or PE version resources"]
    if bootstrap_record["pe_version"]["status"] == "missing":
        missing.append("Bootstrap Python version is missing; no executable was invoked to obtain it")
    science = inventory(workspace / "environments/science-v1")
    numpy = inventory(workspace / "environments/python-numpy-v1/runtime")
    for name, runtime in [("science", science), ("python_numpy", numpy)]:
        if not runtime["present"]:
            missing.append(f"{name}: managed runtime directory missing")
        for package in runtime["packages"]:
            if package["status"] != "observed_metadata":
                missing.append(f"{name}: package version or name missing/ambiguous in {package['metadata']['path']}")
    marker = Path(science["root"]) / "phaseforge-environment.json"
    science["environment_manifest"] = ({"content": read_json(marker), **file_record(marker)}
                                       if guard(marker, missing=True).exists() else None)
    requirements = Path(science["root"]) / "requirements.txt"
    science["requirements"] = ({"text": document(requirements), **file_record(requirements)}
                                if guard(requirements, missing=True).exists() else None)
    science["external_base_runtime"], science["pyvenv_cfg"] = external_base(Path(science["root"]), bootstrap)
    science["download_provenance"] = {
        "status": "unresolved_installed_package_archive_provenance",
        "provisioner_pinning": "package_versions_only",
        "hash_pinned_downloads_established": False,
        "retained_archives": [row for row in science["files"] if row["path"].lower().endswith((".whl", ".zip"))],
        "scope": "The science provisioner retains version requirements and an environment marker, not a resolved pip installation report with downloaded wheel hashes. Any archives inventoried here are observed bytes only; their association with installed packages is unproven. Host/global pip caches were not searched."}
    missing.extend(["science: resolved installed-wheel source URLs and archive SHA-256 provenance are missing; version pins are not download hash pins",
                    "science: external base Python standard library, loaded modules and complete installation materials are outside this bounded inventory"])
    if science["environment_manifest"] is None:
        missing.append("science: environment manifest missing")
    if science["requirements"] is None:
        missing.append("science: retained requirements missing")
    if science["external_base_runtime"]["status"] != "observed_configuration":
        missing.append("science: readable absolute external base Python configuration missing")
    numpy.update(isolation_evidence(numpy, missing))
    integrity_valid = science["present"] and numpy["present"] and numpy["pin_verification"]["valid"]
    return {"schema_version": 1, "source_commit": source_commit, "workspace": str(workspace),
            "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "scope": "Read-only snapshot of managed runtime files and retained provenance, plus explicitly identified external Python prerequisite materials. No imports, execution, installation, downloads, network requests, native tests or vulnerability scans of inspected runtimes. Missing provenance is not waived by matching file hashes.",
            "runtime_execution": False, "complete_sbom": False, "integrity_valid": bool(integrity_valid),
            "bootstrap_python": bootstrap_record, "runtimes": {"science": science, "python_numpy": numpy},
            "missing_provenance": missing}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--bootstrap-python", type=Path)
    parser.add_argument("--seed-root", type=Path,
                        help="Installed resources/runtime/runtime-seeds directory; selects the fixed science-v5 / python-numpy-v4 source/destination mapping")
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--source-commit")
    parser.add_argument("--archive-root", type=Path, help="Existing pinned seed archive cache; verify original native members without executing them")
    args = parser.parse_args(argv)
    report = build_report(args.workspace, args.bootstrap_python, args.source_commit, args.seed_root, args.archive_root)
    output = guard(args.output, missing=True)
    protected = [absolute(args.workspace)]
    if args.bootstrap_python is not None:
        protected.append(absolute(args.bootstrap_python).parent)
    if args.seed_root is not None:
        protected.extend([absolute(args.seed_root), absolute(FROZEN_MANIFEST_ROOT)])
    if args.archive_root is not None:
        protected.append(absolute(args.archive_root))
    base = report["runtimes"].get("science", {}).get("external_base_runtime", {}).get("root")
    if base:
        protected.append(absolute(base))
    if any(output.is_relative_to(root) for root in protected):
        raise InventoryError("Output must be outside inspected application data and Python prerequisite directories")
    guard(output.parent)
    with output.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(report, stream, indent=2, allow_nan=False)
        stream.write("\n")
    return 0 if report["integrity_valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
