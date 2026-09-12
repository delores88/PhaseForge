"""Bounded static native evidence. Never load or execute inspected libraries."""
import email.parser
import hashlib
from pathlib import Path
import re
import struct
import tarfile
import zipfile


def pe_imports(raw):
    """Read PE architecture and ordinary import names; no dependency resolution."""
    def unpack(fmt, at):
        if at < 0 or at + struct.calcsize(fmt) > len(raw):
            raise ValueError("Truncated PE structure")
        return struct.unpack_from(fmt, raw, at)
    try:
        if raw[:2] != b"MZ":
            raise ValueError("Not a PE image")
        pe, = unpack("<I", 0x3c)
        if raw[pe:pe+4] != b"PE\0\0":
            raise ValueError("Missing PE signature")
        machine, sections = unpack("<HH", pe + 4)
        optional_size, = unpack("<H", pe + 20)
        optional = pe + 24
        magic, = unpack("<H", optional)
        if magic not in (0x10b, 0x20b) or not 0 < sections <= 96:
            raise ValueError("Unsupported PE layout")
        directories = optional + (96 if magic == 0x10b else 112)
        count, = unpack("<I", directories - 4)
        if count < 2 or directories + 16 > optional + optional_size:
            raise ValueError("Missing import directory")
        import_rva, import_size = unpack("<II", directories + 8)
        ranges = []
        for index in range(sections):
            at = optional + optional_size + index * 40
            virtual_size, va, size, offset = unpack("<IIII", at + 8)
            if offset + size > len(raw):
                raise ValueError("PE section outside file")
            ranges.append((va, size, offset))
        def mapped(rva, size):
            matches = [offset + rva - va for va, length, offset in ranges
                       if va <= rva and rva + size <= va + length]
            if len(matches) != 1:
                raise ValueError("Ambiguous or unmapped import RVA")
            return matches[0]
        names = []
        if import_rva:
            if not 20 <= import_size <= 1024 * 1024:
                raise ValueError("Import table outside bound")
            for index in range(min(import_size // 20, 4096)):
                descriptor = unpack("<IIIII", mapped(import_rva + index * 20, 20))
                if not any(descriptor):
                    break
                at = mapped(descriptor[3], 1)
                end = raw.find(b"\0", at, min(at + 512, len(raw)))
                if end < 0:
                    raise ValueError("Unterminated import name")
                mapped(descriptor[3], end - at + 1)
                name = raw[at:end].decode("ascii")
                if not name or any(ord(c) < 32 or c in "/\\:" for c in name):
                    raise ValueError("Invalid import name")
                names.append(name)
            else:
                raise ValueError("Unterminated import table")
        return {"status": "observed_static_pe", "machine": hex(machine),
                "architecture": {0x8664: "x64", 0x14c: "x86", 0xaa64: "arm64"}.get(machine, "unknown"),
                "imports": sorted(set(names), key=str.casefold),
                "scope": "Ordinary static PE imports only; delay imports, dynamic loading, static-link identities and loaded dependency closure are not established"}
    except (ValueError, UnicodeError, struct.error) as error:
        return {"status": "missing", "reason": str(error), "imports": []}


def evidence(root, native_rows, sources, archive_root, *, record, opened, version, aliases=None, replacements=None):
    """Bind every native path to its original verified archive member, if supplied."""
    origins = {row["path"]: [] for row in native_rows}
    aliases = aliases or {}
    replacements = replacements or {}
    if not isinstance(aliases, dict):
        raise ValueError("Invalid frozen native aliases")
    if not isinstance(replacements, dict) or set(aliases) & set(replacements):
        raise ValueError("Invalid or overlapping frozen native replacements")
    pinned_sources = {source["name"]: source for source in sources}
    if len(pinned_sources) != len(sources):
        raise ValueError("Duplicate native source archive identity")
    openssl_pair = {"libcrypto-3.dll", "libssl-3.dll"}
    if set(replacements) & openssl_pair and not openssl_pair <= set(replacements):
        raise ValueError("OpenSSL replacement must include the complete DLL pair")
    replaced_members = {}
    auxiliary_members = {}
    for destination, replacement in replacements.items():
        is_openssl = destination in openssl_pair
        component, expected_version = ("OpenSSL", "3.0.22") if is_openssl else ("SQLite", "3.53.4")
        if (destination not in origins or not isinstance(replacement, dict)
                or (not is_openssl and destination != "sqlite3.dll")
                or replacement.get("component") != component
                or replacement.get("version") != expected_version or not replacement.get("reason")):
            raise ValueError("Invalid frozen native replacement")
        for role in ("original", "replacement"):
            pin = replacement.get(role)
            if not isinstance(pin, dict):
                raise ValueError("Native replacement lacks original/replacement member pin")
            source = pinned_sources.get(pin.get("archive_name"))
            member = ("cpython-bin-deps-openssl-bin-3.0.22/amd64/" + destination
                      if is_openssl and role == "replacement" else destination)
            if (source is None or source["sha256"] != pin.get("archive_sha256")
                    or pin.get("archive_member") != member
                    or not re.fullmatch(r"[a-f0-9]{64}", str(pin.get("sha256", "")))
                    or type(pin.get("bytes")) is not int or not 0 < pin["bytes"] <= 512 * 1024 * 1024):
                raise ValueError("Native replacement archive/member pin differs from frozen sources")
            if is_openssl and (pin.get("archive_name") != ("python-3.13.15-embed-amd64.zip" if role == "original" else "openssl-bin-3.0.22.zip")
                               or pin.get("version") != ("3.0.21" if role == "original" else "3.0.22")):
                raise ValueError("OpenSSL replacement source or version differs from reviewed pair")
        pin = replacement["replacement"]
        row = next(row for row in native_rows if row["path"] == destination)
        if (pin["sha256"] != row["sha256"] or pin["bytes"] != row["bytes"]
                or pin.get("version") != replacement["version"]
                or replacement["original"]["sha256"] == pin["sha256"]):
            raise ValueError("Native replacement does not match frozen file pin")
        auxiliary = replacement.get("auxiliary_members", [])
        if not isinstance(auxiliary, list) or len(auxiliary) != (0 if destination == "libssl-3.dll" else 1):
            raise ValueError("Native replacement must retain its exact auxiliary members")
        for item in auxiliary:
            expected_path, expected_member = (("licenses/OpenSSL-3.0.22-LICENSE.txt", "cpython-bin-deps-openssl-bin-3.0.22/amd64/LICENSE.txt")
                                               if is_openssl else ("source-evidence/sqlite-3.53.4/sqlite3.def", "sqlite3.def"))
            if (not isinstance(item, dict) or item.get("path") != expected_path
                    or item.get("archive_member") != expected_member
                    or not re.fullmatch(r"[a-f0-9]{64}", str(item.get("sha256", "")))
                    or type(item.get("bytes")) is not int or not 0 < item["bytes"] <= 1024 * 1024):
                raise ValueError("Invalid native auxiliary member pin")
            if record(Path(root) / item["path"]) != {"bytes": item["bytes"], "sha256": item["sha256"]}:
                raise ValueError("Native auxiliary file differs from frozen pin")
            if item["path"] in auxiliary_members:
                raise ValueError("Duplicate native auxiliary destination")
            auxiliary_members[item["path"]] = {**item, "archive": pin["archive_name"], "archive_sha256": pin["archive_sha256"], "archive_bytes_verified": False}
    for destination, alias in aliases.items():
        if (destination not in origins or not isinstance(alias, dict)
                or not isinstance(alias.get("archive_member"), str)
                or alias.get("sha256") != next(row["sha256"] for row in native_rows if row["path"] == destination)):
            raise ValueError("Native alias does not match frozen file pin")
    witnesses, archives = [], []
    wanted = (set(origins) | {alias["archive_member"] for alias in aliases.values()}
              | {entry[role]["archive_member"] for entry in replacements.values() for role in ("original", "replacement")}
              | {entry["archive_member"] for entry in auxiliary_members.values()})
    for source in sources:
        receipt = {**source, "archive_bytes_inspected": False}
        archives.append(receipt)
        if archive_root is None:
            continue
        archive_path = Path(archive_root) / source["name"]
        observed = record(archive_path)
        if observed != {"sha256": source["sha256"], "bytes": source["bytes"]}:
            raise ValueError("Native origin archive differs from frozen pin: " + source["name"])
        receipt["archive_bytes_inspected"] = True
        package = None
        def witness(name, raw):
            if len(raw) > 2 * 1024 * 1024:
                raise ValueError("Upstream metadata witness exceeds bound")
            item = {"archive": source["name"], "archive_sha256": source["sha256"],
                    "member": name, "bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest(),
                    "text": raw.decode("utf-8"),
                    "scope": "Exact upstream metadata/source claim; not an observed loaded-library or static-link closure"}
            witnesses.append(item)
            return item
        with opened(archive_path) as (stream, _):
            if source["name"].endswith((".zip", ".whl")):
                with zipfile.ZipFile(stream) as archive:
                    members = archive.infolist()
                    if len(members) > 100000 or sum(x.file_size for x in members) > 512 * 1024 * 1024:
                        raise ValueError("Native source archive exceeds bound")
                    if len({x.filename for x in members}) != len(members):
                        raise ValueError("Duplicate source archive member")
                    for member in members:
                        name = member.filename
                        metadata = (name.endswith((".dist-info/METADATA", ".dist-info/DELVEWHEEL"))
                                    or name in {"numpy/__config__.py", "numpy/version.py", "openmm/version.py"}
                                    or (".dist-info/sboms/" in name and name.endswith(".json")))
                        if metadata:
                            if member.file_size > 2 * 1024 * 1024:
                                raise ValueError("Upstream metadata witness exceeds bound")
                            item = witness(name, archive.read(member))
                            if name.endswith(".dist-info/METADATA"):
                                message = email.parser.Parser().parsestr(item["text"], headersonly=True)
                                if len(message.get_all("Name", [])) == 1 and len(message.get_all("Version", [])) == 1:
                                    package = {"name": message["Name"], "version": message["Version"],
                                               "member": name, "sha256": item["sha256"]}
                        if name in wanted:
                            if member.file_size > 512 * 1024 * 1024:
                                raise ValueError("Native archive member exceeds bound")
                            digest = hashlib.sha256(archive.read(member)).hexdigest()
                            origin = {"archive": source["name"], "archive_sha256": source["sha256"],
                                                  "url": source["url"], "member": name, "bytes": member.file_size,
                                                  "sha256": digest, "transformation": "none; exact original member bytes"}
                            if name in origins and name not in replacements:
                                origins[name].append(origin)
                            for item in auxiliary_members.values():
                                if item["archive"] == source["name"] and item["archive_member"] == name:
                                    if digest != item["sha256"] or member.file_size != item["bytes"]:
                                        raise ValueError("Native auxiliary archive member differs from frozen pin")
                                    item["archive_bytes_verified"] = True
                            for destination, replacement in replacements.items():
                                for role in ("original", "replacement"):
                                    pin = replacement[role]
                                    if pin["archive_name"] == source["name"] and pin["archive_member"] == name:
                                        if digest != pin["sha256"] or member.file_size != pin["bytes"]:
                                            raise ValueError("Native replacement member bytes differ from frozen pin: " + role)
                                        if role == "original":
                                            replaced_members[destination] = origin
                                        else:
                                            origins[destination].append({**origin, "destination": destination,
                                                "transformation": "explicit upstream native replacement; original member not shipped",
                                                "component": replacement["component"], "component_version": replacement["version"],
                                                "reason": replacement["reason"], "replaces": replacement["original"]})
                            for destination, alias in aliases.items():
                                if alias["archive_member"] == name:
                                    if digest != alias["sha256"]:
                                        raise ValueError("Native alias source bytes differ from frozen pin")
                                    origins[destination].append({**origin, "destination": destination,
                                        "transformation": "additional byte-identical app-local alias",
                                        "reason": alias.get("reason")})
            elif source["name"].endswith(".tar.xz"):
                # CPython's pinned build dependency declarations are witnesses,
                # not proof that every declared source became a loaded library.
                with tarfile.open(fileobj=stream, mode="r|xz") as archive:
                    for index, member in enumerate(archive):
                        if index >= 100000:
                            raise ValueError("Source tar member count exceeds bound")
                        if member.name.endswith(("/PCbuild/get_externals.bat", "/PCbuild/python.props")):
                            if not member.isfile() or member.size > 2 * 1024 * 1024:
                                raise ValueError("Invalid source metadata witness")
                            witness(member.name, archive.extractfile(member).read())
        receipt["distribution_metadata"] = package
        for values in origins.values():
            for value in values:
                if value["archive"] == source["name"]:
                    value["distribution_metadata"] = package
    rows = []
    if archive_root is not None and set(replaced_members) != set(replacements):
        raise ValueError("Native replacement original archive member is missing")
    if archive_root is not None and not all(item["archive_bytes_verified"] for item in auxiliary_members.values()):
        raise ValueError("Native auxiliary source archive member is missing")
    for row in native_rows:
        path = Path(root) / row["path"]
        with opened(path) as (stream, _):
            raw = stream.read()
            if len(raw) != row["bytes"] or hashlib.sha256(raw).hexdigest() != row["sha256"]:
                raise ValueError("Native file changed since inventory")
            observed_version = version(path)
        matches = [origin for origin in origins[row["path"]]
                   if origin["sha256"] == row["sha256"] and origin["bytes"] == row["bytes"]]
        if archive_root is not None and len(matches) != 1:
            raise ValueError("Native member does not map uniquely to frozen original archive: " + row["path"])
        rows.append({**row, "pe_version": observed_version, "pe": pe_imports(raw),
                     "origin": matches[0] if len(matches) == 1 else None,
                     "origin_status": "verified_exact_archive_member" if len(matches) == 1 else "missing_archive_bytes",
                     "component_identity_scope": "PE resource claims and exact upstream distribution ownership are separate; a wheel version is not the version of every transitive DLL"})
    return {"files": rows, "archives": archives, "upstream_metadata_witnesses": witnesses,
            "replaced_original_members": replaced_members,
            "replacement_auxiliary_members": list(auxiliary_members.values()),
            "all_native_archive_members_verified": bool(rows) and all(row["origin"] for row in rows),
            "missing_pe_versions": [row["path"] for row in rows if row["pe_version"]["status"] == "missing"],
            "missing_pe_imports": [row["path"] for row in rows if row["pe"]["status"] == "missing"],
            "loaded_dependency_closure_established": False, "complete_component_attribution": False}
