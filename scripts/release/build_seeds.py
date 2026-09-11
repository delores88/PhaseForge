"""Build inspectable Windows runtime seeds from immutable upstream archives.

This does not install or execute Python, pip, wheels, or scientific code. The
embedded standard-library module set is replaced one-for-one with matching
CPython source. Native and wheel files are retained without exclusions.
"""
from __future__ import annotations

import argparse
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import stat
import tarfile
import urllib.request
import zipfile

SOURCES = [
    {"name": "python-3.13.15-embed-amd64.zip", "url": "https://www.python.org/ftp/python/3.13.15/python-3.13.15-embed-amd64.zip", "sha256": "d1f04d990aee1253d8569e8e5104e30fa9f5fa830899f14843448872d936a2cf", "upstream_metadata": "https://www.python.org/downloads/release/python-31315/"},
    {"name": "Python-3.13.15.tar.xz", "url": "https://www.python.org/ftp/python/3.13.15/Python-3.13.15.tar.xz", "sha256": "1e66a7945a48390ee4c2a4268a0e4185884059a13c4aab6d148aa208deea4a76", "upstream_metadata": "https://www.python.org/downloads/release/python-31315/"},
    {"name": "numpy-2.4.6-cp313-cp313-win_amd64.whl", "url": "https://files.pythonhosted.org/packages/b5/cd/9cc4dc876fb065d5c220aae4d5e14826b2715331bb7618ce1fb07a679d99/numpy-2.4.6-cp313-cp313-win_amd64.whl", "sha256": "c4fc99836233ea196540b17ab0983aff60ed07941751930f5f4d05bc3b3b7359", "upstream_metadata": "https://pypi.org/pypi/numpy/2.4.6/json"},
    {"name": "openmm-8.5.2-cp313-cp313-win_amd64.whl", "url": "https://files.pythonhosted.org/packages/69/9f/447f86684722ea5a5ec5d2785cd7e1151ccd15a8c6aab7e6b1b39833ff53/openmm-8.5.2-cp313-cp313-win_amd64.whl", "sha256": "289c870e28c946b03992215f1f01f0b7d870f84a6f4c0a57e103bd8d62712b46", "upstream_metadata": "https://pypi.org/pypi/OpenMM/8.5.2/json"},
    {"name": "pillow-12.3.0-cp313-cp313-win_amd64.whl", "url": "https://files.pythonhosted.org/packages/a6/9b/7a58e61d62be561da3a356fe2384d4059a6345fc130e23ef1c36a5b81d24/pillow-12.3.0-cp313-cp313-win_amd64.whl", "sha256": "1cca606cd25738df4ed873d5ad46bbdb3d83b5cbca291f6b4ff13a4df6b0bbe8", "upstream_metadata": "https://pypi.org/pypi/Pillow/12.3.0/json"},
]
MAX_ARCHIVE = 128 * 1024 * 1024
MAX_EXPANDED = 1024 * 1024 * 1024
NATIVE = {".exe", ".dll", ".pyd", ".so", ".dylib"}
PTH = b".\nLib\n# Isolated vendored runtime; no registry, user site, or executable .pth imports.\n"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def safe_name(name):
    if not isinstance(name, str) or "\\" in name or ":" in name or "\x00" in name:
        raise ValueError("Unsafe archive path")
    value = PurePosixPath(name)
    if value.is_absolute() or any(p in ("", ".", "..") for p in name.rstrip("/").split("/")):
        raise ValueError("Unsafe archive path")
    if any(p.rstrip(" .") != p or p.split(".")[0].upper() in {"CON", "PRN", "AUX", "NUL", *(f"COM{i}" for i in range(1, 10)), *(f"LPT{i}" for i in range(1, 10))} for p in value.parts):
        raise ValueError("Windows-special archive path")
    return value.as_posix()


def checked_path(path, must_exist=False):
    path = Path(path).absolute()
    for ancestor in [path, *path.parents]:
        if ancestor.exists():
            info = ancestor.lstat()
            if stat.S_ISLNK(info.st_mode) or getattr(info, "st_file_attributes", 0) & 0x400:
                raise ValueError("Linked seed/cache paths are refused")
            if ancestor == path and ancestor.is_file() and info.st_nlink != 1:
                raise ValueError("Hard-linked seed/cache files are refused")
    if must_exist and not path.exists():
        raise ValueError("Required seed input is absent")
    return path


def cached(source, cache, offline=False):
    target = checked_path(cache / source["name"])
    if not target.exists():
        if offline:
            raise ValueError("Pinned archive missing from offline cache: " + source["name"])
        with urllib.request.urlopen(source["url"], timeout=60) as response:
            if response.url != source["url"]:
                raise ValueError("Unexpected redirect for pinned archive")
            payload = response.read(MAX_ARCHIVE + 1)
        if len(payload) > MAX_ARCHIVE or digest(payload) != source["sha256"]:
            raise ValueError("Downloaded archive does not match its frozen hash")
        with target.open("xb") as stream:
            stream.write(payload)
    payload = target.read_bytes()
    if len(payload) > MAX_ARCHIVE or digest(payload) != source["sha256"]:
        raise ValueError("Cached archive does not match its frozen hash")
    return payload


def zip_files(payload):
    result, seen, size = {}, set(), 0
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        for member in archive.infolist():
            name = safe_name(member.filename)
            if (member.external_attr >> 16) & 0o170000 == stat.S_IFLNK or member.flag_bits & 1:
                raise ValueError("Linked/encrypted archive member refused")
            if name.casefold() in seen:
                raise ValueError("Case-equivalent duplicate archive member")
            seen.add(name.casefold())
            size += member.file_size
            if size > MAX_EXPANDED or len(seen) > 100000:
                raise ValueError("Archive expansion exceeds its bound")
            if not member.is_dir():
                result[name] = archive.read(member)
    return result


def source_files(payload):
    result, seen, size = {}, set(), 0
    with tarfile.open(fileobj=io.BytesIO(payload), mode="r:xz") as archive:
        for member in archive:
            name = safe_name(member.name)
            if name.casefold() in seen:
                raise ValueError("Duplicate source archive member")
            seen.add(name.casefold())
            if not member.isfile() and not member.isdir():
                raise ValueError("Linked/special source archive member refused")
            size += member.size
            if size > MAX_EXPANDED or len(seen) > 100000:
                raise ValueError("Source archive expansion exceeds its bound")
            if member.isfile() and (name.startswith("Python-3.13.15/Lib/") or name == "Python-3.13.15/LICENSE"):
                result[name] = archive.extractfile(member).read()
    return result


def assemble(embedded, source, wheels):
    """Pure archive transformation; every departure is represented in receipts."""
    if "python313.zip" not in embedded or "python313._pth" not in embedded:
        raise ValueError("Official embedded standard library/configuration absent")
    output = {k: v for k, v in embedded.items() if k not in {"python313.zip", "python313._pth"}}
    occupied = {name.casefold() for name in output}
    replacements = []
    resources = []
    for name, old in zip_files(embedded["python313.zip"]).items():
        if not name.endswith(".pyc"):
            relative = "Lib/" + name
            if relative.casefold() in occupied:
                raise ValueError("Standard library resource collides")
            output[relative] = old
            occupied.add(relative.casefold())
            resources.append({"upstream_member": "python313.zip!/" + name, "path": relative, "sha256": digest(old)})
            continue
        relative = "Lib/" + name[:-1]
        original = "Python-3.13.15/" + relative
        if original not in source:
            raise ValueError("No exact matching source module for embedded bytecode: " + name)
        if relative.casefold() in occupied:
            raise ValueError("Standard library replacement collides")
        output[relative] = source[original]
        occupied.add(relative.casefold())
        replacements.append({"upstream_member": "python313.zip!/" + name, "upstream_sha256": digest(old), "source_member": original, "path": relative, "sha256": digest(output[relative])})
    output["python313._pth"] = PTH
    occupied.add("python313._pth")
    output["licenses/CPython-source-LICENSE.txt"] = source["Python-3.13.15/LICENSE"]
    occupied.add("licenses/cpython-source-license.txt")
    for wheel in wheels:
        for name, value in wheel.items():
            if name.casefold() in occupied:
                raise ValueError("Wheel attempts to replace an installed file: " + name)
            if name.endswith((".pyc", ".pyo")):
                raise ValueError("Unexpected wheel bytecode requires explicit review: " + name)
            if ".data/" in name:
                raise ValueError("Wheel install scheme requires explicit mapping: " + name)
            occupied.add(name.casefold())
            output[name] = value
    return output, {"stdlib": "Exact embedded module set replaced with corresponding pinned CPython Lib source; other standard library resources and all interpreter/native members retained", "stdlib_replacements": replacements, "stdlib_resources": resources, "configuration": {"path": "python313._pth", "before_sha256": digest(embedded["python313._pth"]), "after_sha256": digest(PTH), "reason": "Resolve local source Lib and vendored wheel files only; disable site imports"}, "upstream_stdlib_archive_sha256": digest(embedded["python313.zip"])}


def build(output, cache, offline=False, source_commit=None, freeze=False, manifest_root=None):
    output, cache = checked_path(output), checked_path(cache)
    if output.exists() or output == cache or output.is_relative_to(cache) or cache.is_relative_to(output):
        raise ValueError("Use a new seed destination and a separate cache")
    if source_commit is not None and not re.fullmatch(r"[a-f0-9]{40}", source_commit):
        raise ValueError("Expected full source commit")
    cache.mkdir(parents=True, exist_ok=True)
    archives = [cached(s, cache, offline) for s in SOURCES]
    embedded, source = zip_files(archives[0]), source_files(archives[1])
    output.mkdir(parents=True)
    report = {"schema": "phaseforge.runtime-seed-build.v1", "source_commit": source_commit, "host_python_is_build_tool_only": True, "managed_executables_run": False, "seeds": {}}
    manifest_root = checked_path(manifest_root or Path(__file__).resolve().parents[2] / "tools/runtime-seeds")
    if freeze:
        manifest_root.mkdir(parents=True, exist_ok=True)
    for kind, count in (("python-numpy-v2", 3), ("science-v2", 5)):
        files, transformations = assemble(embedded, source, [zip_files(a) for a in archives[2:count]])
        root = output / kind
        root.mkdir()
        for name, value in sorted(files.items()):
            destination = root.joinpath(*PurePosixPath(name).parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(value)
        sources = [{**s, "bytes": len(a)} for s, a in zip(SOURCES[:count], archives[:count])]
        if kind == "python-numpy-v2":
            isolation = {"schema_version": 1, "python": "3.13.15", "numpy": "2.4.6", "sources": sources, "files": {name: digest(value) for name, value in sorted(files.items())}, "stdlib": "Matching pinned source modules replace embedded bytecode", "security": "Pinned dependencies only; generated code requires the separately verified Windows LPAC boundary"}
            files["phaseforge-isolation-runtime.json"] = (json.dumps(isolation, indent=2) + "\n").encode()
            (root / "phaseforge-isolation-runtime.json").write_bytes(files["phaseforge-isolation-runtime.json"])
        hashes = {name: digest(value) for name, value in sorted(files.items())}
        sizes = {name: len(value) for name, value in sorted(files.items())}
        manifest = {"schema_version": 1, "schema": "phaseforge.runtime-seed.v1", "kind": kind, "python": "3.13.15", "numpy": "2.4.6", "sources": sources, "files": hashes, "file_bytes": sizes, "native_files": {k: v for k, v in hashes.items() if Path(k).suffix.lower() in NATIVE}, "transformations": transformations, "licenses": [k for k in hashes if "license" in k.lower() or "copying" in k.lower()], "delivery": "Bundled inspectable files; copy bytes into a managed runtime after verifying this manifest", "security": "Pinned dependencies only; generated code still requires the separately verified Windows LPAC boundary"}
        if kind == "science-v2":
            manifest.update(openmm="8.5.2", pillow="12.3.0")
        name = "phaseforge-runtime-seed.json"
        raw = (json.dumps(manifest, indent=2, ensure_ascii=True) + "\n").encode()
        frozen = checked_path(manifest_root / (kind + ".manifest.json"))
        if freeze:
            with frozen.open("xb") as stream:
                stream.write(raw)
        elif not frozen.is_file() or frozen.read_bytes() != raw:
            raise ValueError("Rebuilt seed differs from its committed manifest: " + kind)
        (root / name).write_bytes(raw)
        report["seeds"][kind] = {"manifest": f"{kind}/{name}", "manifest_sha256": digest(raw), "files": len(files), "bytes": sum(sizes.values()), "native_files": len(manifest["native_files"]), "sources": sources}
    (output / "seed-build.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--offline", action="store_true")
    parser.add_argument("--source-commit")
    parser.add_argument("--freeze", action="store_true", help="Write new source manifests once; refuse existing files. CI only verifies.")
    args = parser.parse_args()
    print(json.dumps(build(args.output, args.cache, args.offline, args.source_commit, args.freeze)))


if __name__ == "__main__":
    main()
