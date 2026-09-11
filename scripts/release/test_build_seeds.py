"""Small synthetic archive regressions; no downloads or runtime execution."""
import io
import hashlib
import json
from contextlib import ExitStack
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile
import build_seeds as seeds


def bundle(files):
    value = io.BytesIO()
    with zipfile.ZipFile(value, "w") as archive:
        for name, payload in files.items():
            archive.writestr(name, payload)
    return value.getvalue()


class SeedRecipeTests(unittest.TestCase):
    sqlite_original = b"original SQLite fixture"
    sqlite_dll = b"fixed SQLite fixture"
    sqlite_def = b"definition fixture"

    def sqlite_pins(self):
        stack = ExitStack()
        for prefix, value in (("SQLITE_ORIGINAL", self.sqlite_original), ("SQLITE_DLL", self.sqlite_dll), ("SQLITE_DEF", self.sqlite_def)):
            stack.enter_context(patch.object(seeds, prefix + "_SHA256", seeds.digest(value)))
            stack.enter_context(patch.object(seeds, prefix + "_BYTES", len(value)))
        return stack

    def sqlite_archive(self):
        return {"sqlite3.dll": self.sqlite_dll, "sqlite3.def": self.sqlite_def}

    def inputs(self):
        return ({"python.exe": b"native interpreter", "python313.dll": b"native DLL", "python313.zip": bundle({"encodings/__init__.pyc": b"bytecode", "json.pyc": b"json bytecode"}), "python313._pth": b"python313.zip\n."}, {"Python-3.13.15/Lib/encodings/__init__.py": b"# source encodings", "Python-3.13.15/Lib/json.py": b"# source json", "Python-3.13.15/LICENSE": b"PSF license"})

    def test_exact_stdlib_source_mapping_retains_all_native_wheel_and_license_files(self):
        embedded, source = self.inputs()
        wheel = {"pkg/native.pyd": b"extension", "pkg-1.dist-info/METADATA": b"Name: pkg", "pkg-1.dist-info/licenses/LICENSE": b"license"}
        files, receipt = seeds.assemble(embedded, source, [wheel])
        for key in ("python.exe", "python313.dll"):
            self.assertEqual(files[key], embedded[key])
        for key, payload in wheel.items():
            self.assertEqual(files[key], payload)
        self.assertEqual(files["Lib/json.py"], source["Python-3.13.15/Lib/json.py"])
        self.assertNotIn("python313.zip", files)
        self.assertFalse(any(name.endswith(".pyc") for name in files))
        self.assertEqual(len(receipt["stdlib_replacements"]), 2)
        self.assertEqual(receipt["upstream_stdlib_archive_sha256"], seeds.digest(embedded["python313.zip"]))
        self.assertEqual(files["python313._pth"], seeds.PTH)
        self.assertNotIn(b"import site", seeds.PTH)

    def test_missing_source_never_silently_drops_a_module(self):
        embedded, source = self.inputs()
        del source["Python-3.13.15/Lib/json.py"]
        with self.assertRaisesRegex(ValueError, "No exact matching"):
            seeds.assemble(embedded, source, [])

    def test_science_alias_preserves_original_bytes_and_rejects_wrong_member(self):
        embedded, source = self.inputs()
        member = {seeds.MSVC_MEMBER: b"fixture MSVC"}
        with self.assertRaisesRegex(ValueError, "pinned MSVCP"):
            seeds.assemble(embedded, source, [member], add_msvc_alias=True)
        with patch.object(seeds, "MSVC_SHA256", seeds.digest(b"fixture MSVC")):
            files, receipt = seeds.assemble(embedded, source, [member], add_msvc_alias=True)
            self.assertEqual(files['MSVCP140.dll'], files[seeds.MSVC_MEMBER])
            self.assertEqual(receipt['native_aliases']['MSVCP140.dll']['archive_member'], seeds.MSVC_MEMBER)
            with self.assertRaisesRegex(ValueError, "pinned MSVCP"):
                seeds.assemble(embedded, source, [], add_msvc_alias=True)

    def test_wheel_cannot_replace_native_or_smuggle_bytecode(self):
        for wheel in ({"PYTHON.EXE": b"replacement"}, {"pkg/x.pyc": b"opaque"}, {"pkg.data/scripts/run.exe": b"needs mapping"}):
            with self.subTest(wheel=wheel), self.assertRaises(ValueError):
                seeds.assemble(*self.inputs(), [wheel])

    def test_archive_paths_and_case_collisions_fail_before_extracting(self):
        for name in ("../escape", "/absolute", "C:/drive", "a\\b", "NUL.txt", "a/./b", "space. "):
            with self.subTest(name=name), self.assertRaises(ValueError):
                seeds.safe_name(name)
        with self.assertRaisesRegex(ValueError, "Case-equivalent"):
            seeds.zip_files(bundle({"A": b"1", "a": b"2"}))

    def test_cache_pin_mismatch_does_not_download_or_overwrite(self):
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary)
            source = {"name": "fixture.zip", "url": "https://invalid.example/no-network", "sha256": seeds.digest(b"original")}
            (cache / source["name"]).write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError, "Cached archive"):
                seeds.cached(source, cache, offline=True)
            self.assertEqual((cache / source["name"]).read_bytes(), b"changed")

    def test_official_sha3_is_required_even_when_sha256_matches(self):
        payload = b"reviewed fixture archive"
        source = {"name": "sqlite.zip", "url": "https://invalid.example/no-network", "sha256": seeds.digest(payload), "sha3_256": "0" * 64}
        with tempfile.TemporaryDirectory() as temporary:
            cache = Path(temporary)
            (cache / source["name"]).write_bytes(payload)
            with self.assertRaisesRegex(ValueError, "Cached archive"):
                seeds.cached(source, cache, offline=True)
            source["sha3_256"] = hashlib.sha3_256(payload).hexdigest()
            self.assertEqual(seeds.cached(source, cache, offline=True), payload)

    def test_sqlite_replacement_preserves_other_members_and_records_both_origins(self):
        embedded, source = self.inputs()
        embedded["sqlite3.dll"] = self.sqlite_original
        wheel = {seeds.MSVC_MEMBER: b"fixture MSVC", "pkg/native.pyd": b"native"}
        with self.sqlite_pins(), patch.object(seeds, "MSVC_SHA256", seeds.digest(b"fixture MSVC")):
            files, receipt = seeds.assemble(embedded, source, [wheel], add_msvc_alias=True, sqlite_replacement=self.sqlite_archive())
        self.assertEqual(files["sqlite3.dll"], self.sqlite_dll)
        self.assertEqual(files["MSVCP140.dll"], files[seeds.MSVC_MEMBER])
        self.assertEqual(files["python313.dll"], embedded["python313.dll"])
        self.assertEqual(files["pkg/native.pyd"], wheel["pkg/native.pyd"])
        entry = receipt["native_replacements"]["sqlite3.dll"]
        self.assertEqual(entry["original"]["sha256"], seeds.digest(self.sqlite_original))
        self.assertEqual(entry["original"]["archive_sha256"], seeds.SOURCES[0]["sha256"])
        self.assertEqual(entry["replacement"]["sha256"], seeds.digest(self.sqlite_dll))
        self.assertEqual(entry["replacement"]["archive_sha256"], seeds.SOURCES[5]["sha256"])
        self.assertEqual(files[entry["auxiliary_members"][0]["path"]], self.sqlite_def)
        self.assertEqual(entry["version"], "3.53.4")

    def test_sqlite_replacement_rejects_missing_changed_ambiguous_and_unreviewed_members(self):
        cases = [({}, self.sqlite_archive()), ({"sqlite3.dll": b"wrong"}, self.sqlite_archive()),
                 ({"SQLITE3.DLL": self.sqlite_original}, self.sqlite_archive()),
                 ({"sqlite3.dll": self.sqlite_original}, {"sqlite3.dll": self.sqlite_dll}),
                 ({"sqlite3.dll": self.sqlite_original}, {**self.sqlite_archive(), "extra.dll": b"extra"}),
                 ({"sqlite3.dll": self.sqlite_original}, {**self.sqlite_archive(), "SQLITE3.DLL": self.sqlite_dll}),
                 ({"sqlite3.dll": self.sqlite_original}, {**self.sqlite_archive(), "sqlite3.dll": b"changed"}),
                 ({"sqlite3.dll": self.sqlite_original}, {**self.sqlite_archive(), "sqlite3.def": b"changed"}),
                 ({"sqlite3.dll": self.sqlite_original, "SOURCE-EVIDENCE/SQLITE-3.53.4/SQLITE3.DEF": b"occupied"}, self.sqlite_archive())]
        for output, replacement in cases:
            with self.subTest(output=list(output), members=list(replacement)), self.sqlite_pins(), self.assertRaises(ValueError):
                seeds.replace_sqlite(output, replacement)

    def test_sqlite_member_lengths_are_independently_pinned(self):
        for name in ("SQLITE_ORIGINAL_BYTES", "SQLITE_DLL_BYTES", "SQLITE_DEF_BYTES"):
            with self.subTest(pin=name), self.sqlite_pins(), patch.object(seeds, name, 999), self.assertRaises(ValueError):
                seeds.replace_sqlite({"sqlite3.dll": self.sqlite_original}, self.sqlite_archive())

    def test_freeze_is_explicit_one_time_and_default_rebuild_never_rewrites_changed_pins(self):
        embedded, source = self.inputs()
        embedded["sqlite3.dll"] = self.sqlite_original
        archives = [bundle(embedded), b"fixture source", bundle({"numpy/native.pyd": b"numpy", seeds.MSVC_MEMBER: b"fixture MSVC"}), bundle({"openmm/native.dll": b"openmm"}), bundle({"PIL/native.pyd": b"pillow"}), bundle(self.sqlite_archive())]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            def build(name, freeze=False):
                with self.sqlite_pins(), patch.object(seeds, "cached", side_effect=archives), patch.object(seeds, "source_files", return_value=source), patch.object(seeds, "MSVC_SHA256", seeds.digest(b"fixture MSVC")):
                    return seeds.build(root/name, root/"cache", offline=True, freeze=freeze, manifest_root=root/"frozen")
            report = build("initial", True)
            self.assertEqual(set(report["seeds"]), {"science-v4", "python-numpy-v3"})
            for kind, count in (("science-v4", 6), ("python-numpy-v3", 4)):
                self.assertEqual(len(report["seeds"][kind]["sources"]), count)
                manifest = json.loads((root/"frozen"/(kind + ".manifest.json")).read_bytes())
                self.assertEqual(manifest["sqlite"], "3.53.4")
                self.assertEqual(manifest["files"]["sqlite3.dll"], seeds.digest(self.sqlite_dll))
            isolation = json.loads((root/"initial/python-numpy-v3/phaseforge-isolation-runtime.json").read_bytes())
            self.assertEqual(isolation["sqlite"], "3.53.4")
            frozen = root/"frozen/science-v4.manifest.json"
            initial = frozen.read_bytes()
            build("replayed")
            self.assertEqual(frozen.read_bytes(), initial)
            frozen.write_bytes(initial.replace(b'"python": "3.13.15"', b'"python": "9.99.99"'))
            changed = frozen.read_bytes()
            with self.assertRaisesRegex(ValueError, "differs from its committed"):
                build("changed")
            self.assertEqual(frozen.read_bytes(), changed)
            with self.assertRaises(FileExistsError):
                build("cannot_refreeze", True)


if __name__ == "__main__":
    unittest.main()
