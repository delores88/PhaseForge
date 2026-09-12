"""Build inspectable Windows runtime seeds from immutable upstream archives.

This does not install or execute Python, pip, wheels, or scientific code. The
embedded standard-library module set is replaced one-for-one with matching
CPython source. Native and wheel files are retained except explicitly pinned
upstream replacements recorded in the immutable manifest.
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
    {"name": "sqlite-dll-win-x64-3530400.zip", "url": "https://www.sqlite.org/2026/sqlite-dll-win-x64-3530400.zip", "sha256": "8b959b7eff4a81f6a62fc3468f9273e5cfe78d4a927e62215aed231b654fb104", "sha3_256": "deddee963c810d1eeac3ce5e15c7c41da21a1c54d7a39cf54fbf577d2f50de3a", "version": "3.53.4", "upstream_metadata": "https://www.sqlite.org/download.html", "license_url": "https://www.sqlite.org/copyright.html"},
    {"name": "openssl-bin-3.0.22.zip", "url": "https://codeload.github.com/python/cpython-bin-deps/zip/refs/tags/openssl-bin-3.0.22", "sha256": "f41c05d4e91ca5d687dcea3a5cc54b8692c86f5e7e8eb51d7b0bf537c7bcaddb", "bytes": 24671379, "version": "3.0.22", "upstream_commit": "8898e94682675b969ef09c0be4cdf46bf764fa9e", "upstream_metadata": "https://github.com/python/cpython/commit/87c9dfbf06de955e79c606d3163d9a40160d04cf", "license_url": "https://github.com/python/cpython-bin-deps/blob/8898e94682675b969ef09c0be4cdf46bf764fa9e/amd64/LICENSE.txt"},
]
MAX_ARCHIVE = 128 * 1024 * 1024
MAX_EXPANDED = 1024 * 1024 * 1024
NATIVE = {".exe", ".dll", ".pyd", ".so", ".dylib"}
PTH = b".\nLib\n# Isolated vendored runtime; no registry, user site, or executable .pth imports.\n"


def digest(data):
    return hashlib.sha256(data).hexdigest()


def archive_matches(payload, source):
    return (len(payload) <= MAX_ARCHIVE and digest(payload) == source["sha256"]
            and ("bytes" not in source or len(payload) == source["bytes"])
            and ("sha3_256" not in source or hashlib.sha3_256(payload).hexdigest() == source["sha3_256"]))


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
        if not archive_matches(payload, source):
            raise ValueError("Downloaded archive does not match its frozen hash")
        with target.open("xb") as stream:
            stream.write(payload)
    payload = target.read_bytes()
    if not archive_matches(payload, source):
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


MSVC_MEMBER = "numpy.libs/msvcp140-a4c2229bdc2a2a630acdc095b4d86008.dll"
MSVC_SHA256 = "a4c2229bdc2a2a630acdc095b4d86008e5c3e3bc7773174354f3da4f5beb9cde"
SQLITE_ORIGINAL_SHA256 = "c6812eaf0f8605df273b9bf7e359b83fd65038ecc66455c3f7a933768cb92f3f"
SQLITE_ORIGINAL_BYTES = 1584864
SQLITE_DLL_SHA256 = "ab57d0437795ecc757cb693f32ea224173fa9856594d95cfa6b5033e645cd1ec"
SQLITE_DLL_BYTES = 3285504
SQLITE_DEF_SHA256 = "a93b7867cc1fdb4e1dd0d4eef727cf910fe9f2bb6cd612d933744add1fa7f11b"
SQLITE_DEF_BYTES = 8722
OPENSSL_ARCHIVE_PREFIX = "cpython-bin-deps-openssl-bin-3.0.22/amd64/"
OPENSSL_DLLS = {
    "libcrypto-3.dll": {"original_sha256": "09499e186cf0e434ffa17ad153aa9a66d6048c54a2c03b823e2a06c49d9affe4", "original_bytes": 5238520, "sha256": "fce8b678990ce3266fd9a86fbda19f232afa7ba76de81d73145dc35cdb19513a", "bytes": 5243128},
    "libssl-3.dll": {"original_sha256": "28c395290279fa4f4f54a57b85cd66d853895f925fefb8f6fdebc0d8b1cdd07c", "original_bytes": 794872, "sha256": "55913d1e5287ee969c0dab64b101d880a7fd0d95a974c112d925579be097afbd", "bytes": 794872},
}
OPENSSL_LICENSE_SHA256 = "ed72ce2b51ee58f117e5a021e2e04af158857f40269fbc03491f0b2a99dbcc96"
OPENSSL_LICENSE_BYTES = 10352


def replace_sqlite(output, replacement):
    if ("sqlite3.dll" not in output or digest(output["sqlite3.dll"]) != SQLITE_ORIGINAL_SHA256
            or len(output["sqlite3.dll"]) != SQLITE_ORIGINAL_BYTES):
        raise ValueError("Original embedded SQLite DLL does not match its pinned identity")
    if set(replacement) != {"sqlite3.dll", "sqlite3.def"}:
        raise ValueError("SQLite archive requires exactly the reviewed DLL and definition members")
    for name, expected, size in (("sqlite3.dll", SQLITE_DLL_SHA256, SQLITE_DLL_BYTES), ("sqlite3.def", SQLITE_DEF_SHA256, SQLITE_DEF_BYTES)):
        if digest(replacement[name]) != expected or len(replacement[name]) != size:
            raise ValueError("Replacement SQLite member does not match its pinned identity: " + name)
    definition = "source-evidence/sqlite-3.53.4/sqlite3.def"
    if definition.casefold() in {name.casefold() for name in output}:
        raise ValueError("SQLite definition provenance destination is occupied")
    output["sqlite3.dll"] = replacement["sqlite3.dll"]
    output[definition] = replacement["sqlite3.def"]
    return {"sqlite3.dll": {
        "component": "SQLite", "version": "3.53.4",
        "original": {"archive_name": SOURCES[0]["name"], "archive_sha256": SOURCES[0]["sha256"], "archive_member": "sqlite3.dll", "sha256": SQLITE_ORIGINAL_SHA256, "bytes": SQLITE_ORIGINAL_BYTES, "version": "3.50.4"},
        "replacement": {"archive_name": SOURCES[5]["name"], "archive_sha256": SOURCES[5]["sha256"], "archive_member": "sqlite3.dll", "sha256": SQLITE_DLL_SHA256, "bytes": SQLITE_DLL_BYTES, "version": "3.53.4"},
        "auxiliary_members": [{"path": definition, "archive_member": "sqlite3.def", "sha256": SQLITE_DEF_SHA256, "bytes": SQLITE_DEF_BYTES}],
        "reason": "Replace the older embedded SQLite with the exact official fixed Windows x64 DLL",
        "upstream_advisory": "https://www.sqlite.org/cves.html"}}


def replace_openssl(output, archive):
    """Replace only the reviewed AMD64 pair, retaining original and new provenance."""
    prefix = OPENSSL_ARCHIVE_PREFIX
    native_members = {name for name in archive if name.startswith(prefix) and Path(name).suffix.lower() in NATIVE}
    if native_members != {prefix + name for name in OPENSSL_DLLS}:
        raise ValueError("OpenSSL archive requires exactly the reviewed AMD64 DLL pair")
    for name, pin in OPENSSL_DLLS.items():
        if ({key for key in output if key.casefold() == name.casefold()} != {name}
                or digest(output[name]) != pin["original_sha256"] or len(output[name]) != pin["original_bytes"]):
            raise ValueError("Original embedded OpenSSL DLL differs from pinned identity: " + name)
        if digest(archive[prefix + name]) != pin["sha256"] or len(archive[prefix + name]) != pin["bytes"]:
            raise ValueError("Replacement OpenSSL DLL differs from pinned identity: " + name)
    license_member = prefix + "LICENSE.txt"
    if (license_member not in archive or digest(archive[license_member]) != OPENSSL_LICENSE_SHA256
            or len(archive[license_member]) != OPENSSL_LICENSE_BYTES):
        raise ValueError("OpenSSL license differs from pinned identity")
    license_path = "licenses/OpenSSL-3.0.22-LICENSE.txt"
    if license_path.casefold() in {name.casefold() for name in output}:
        raise ValueError("OpenSSL license destination is occupied")
    result = {}
    for name, pin in OPENSSL_DLLS.items():
        output[name] = archive[prefix + name]
        result[name] = {
            "component": "OpenSSL", "version": "3.0.22",
            "original": {"archive_name": SOURCES[0]["name"], "archive_sha256": SOURCES[0]["sha256"], "archive_member": name, "sha256": pin["original_sha256"], "bytes": pin["original_bytes"], "version": "3.0.21"},
            "replacement": {"archive_name": SOURCES[6]["name"], "archive_sha256": SOURCES[6]["sha256"], "archive_member": prefix + name, "sha256": pin["sha256"], "bytes": pin["bytes"], "version": "3.0.22"},
            "auxiliary_members": [{"path": license_path, "archive_member": license_member, "sha256": OPENSSL_LICENSE_SHA256, "bytes": OPENSSL_LICENSE_BYTES}] if name == "libcrypto-3.dll" else [],
            "reason": "Replace the embedded OpenSSL pair with the exact official CPython 3.13 Windows AMD64 fixed dependency pair; interpreter and extension bytes remain unchanged",
            "upstream_advisory": "https://openssl-library.org/news/vulnerabilities-3.0/",
        }
    output[license_path] = archive[license_member]
    return result


def assemble(embedded, source, wheels, add_msvc_alias=False, sqlite_replacement=None, openssl_replacement=None):
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
    transformations = {"stdlib": "Exact embedded module set replaced with corresponding pinned CPython Lib source; other standard library resources and interpreter/native members retained except explicitly recorded native replacements", "stdlib_replacements": replacements, "stdlib_resources": resources, "configuration": {"path": "python313._pth", "before_sha256": digest(embedded["python313._pth"]), "after_sha256": digest(PTH), "reason": "Resolve local source Lib and vendored wheel files only; disable site imports"}, "upstream_stdlib_archive_sha256": digest(embedded["python313.zip"])}
    if add_msvc_alias:
        if MSVC_MEMBER not in output or digest(output[MSVC_MEMBER]) != MSVC_SHA256:
            raise ValueError("Missing or changed pinned MSVCP140 source member")
        if "msvcp140.dll" in occupied:
            raise ValueError("MSVCP140 destination already occupied")
        output["MSVCP140.dll"] = output[MSVC_MEMBER]
        transformations["native_aliases"] = {"MSVCP140.dll": {
            "archive_member": MSVC_MEMBER, "sha256": MSVC_SHA256,
            "reason": "Unmodified app-local alias for OpenMM's plain MSVCP140.dll imports; preserve NumPy's original renamed member for its own imports"}}
    if sqlite_replacement is not None:
        transformations["native_replacements"] = replace_sqlite(output, sqlite_replacement)
    if openssl_replacement is not None:
        transformations.setdefault("native_replacements", {}).update(replace_openssl(output, openssl_replacement))
    return output, transformations


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
    for kind, indexes in (("python-numpy-v4", (0, 1, 2, 5, 6)), ("science-v5", (0, 1, 2, 3, 4, 5, 6))):
        files, transformations = assemble(embedded, source, [zip_files(archives[i]) for i in indexes if i in (2, 3, 4)], add_msvc_alias=kind == "science-v5", sqlite_replacement=zip_files(archives[5]), openssl_replacement=zip_files(archives[6]))
        root = output / kind
        root.mkdir()
        for name, value in sorted(files.items()):
            destination = root.joinpath(*PurePosixPath(name).parts)
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(value)
        sources = [{**SOURCES[i], "bytes": len(archives[i])} for i in indexes]
        if kind == "python-numpy-v4":
            isolation = {"schema_version": 1, "python": "3.13.15", "numpy": "2.4.6", "sqlite": "3.53.4", "openssl": "3.0.22", "sources": sources, "files": {name: digest(value) for name, value in sorted(files.items())}, "stdlib": "Matching pinned source modules replace embedded bytecode", "security": "Pinned dependencies only; generated code requires the separately verified Windows LPAC boundary"}
            files["phaseforge-isolation-runtime.json"] = (json.dumps(isolation, indent=2) + "\n").encode()
            (root / "phaseforge-isolation-runtime.json").write_bytes(files["phaseforge-isolation-runtime.json"])
        hashes = {name: digest(value) for name, value in sorted(files.items())}
        sizes = {name: len(value) for name, value in sorted(files.items())}
        manifest = {"schema_version": 1, "schema": "phaseforge.runtime-seed.v1", "kind": kind, "python": "3.13.15", "numpy": "2.4.6", "sqlite": "3.53.4", "openssl": "3.0.22", "sources": sources, "files": hashes, "file_bytes": sizes, "native_files": {k: v for k, v in hashes.items() if Path(k).suffix.lower() in NATIVE}, "transformations": transformations, "licenses": [k for k in hashes if "license" in k.lower() or "copying" in k.lower()], "delivery": "Bundled inspectable files; copy bytes into a managed runtime after verifying this manifest", "security": "Pinned dependencies only; generated code still requires the separately verified Windows LPAC boundary"}
        if kind == "science-v5":
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
