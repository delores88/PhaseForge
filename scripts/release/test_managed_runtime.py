"""Tiny synthetic files only; no inspected Python, solver or package executes."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

import managed_runtime as inventory


class ManagedRuntimeTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="phaseforge-managed-materials-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.workspace = self.root / "app-data"
        self.science = self.workspace / "environments/science-v1"
        self.runtime = self.workspace / "environments/python-numpy-v1/runtime"
        self.bootstrap = self.root / "external-python/python.exe"
        self.put(self.bootstrap, b"fixture, not an executable")
        self.put(self.bootstrap.parent / "python313.dll", b"external base DLL fixture")
        self.put(self.bootstrap.parent / "DLLs/_ssl.pyd", b"external extension fixture")
        self.put(self.science / "Scripts/python.exe", b"venv launcher fixture")
        self.put(self.science / "pyvenv.cfg", f"home = {self.bootstrap.parent}\nexecutable = {self.bootstrap}\nversion = 3.13.fixture\n".encode())
        self.put(self.science / "requirements.txt", b"openmm==fixture-pinned-version\n")
        self.json(self.science / "phaseforge-environment.json", {"requirements": "openmm==fixture-pinned-version\n"})
        self.put(self.science / "Lib/site-packages/openmm-wrong-directory-version.dist-info/METADATA", b"Metadata-Version: 2.1\nName: OpenMM\nVersion: 9.fixture.actual-metadata\n\n")
        self.put(self.runtime / "python.exe", b"pinned embed fixture")
        self.put(self.runtime / "numpy/_core.pyd", b"pinned native fixture")
        self.put(self.runtime / "numpy-untrusted-dir.dist-info/METADATA", b"Name: numpy\nVersion: 8.fixture.metadata\n\n")
        archive = self.runtime.parent / ".isolation-downloads/python-fixture.zip"
        self.put(archive, b"synthetic archive; never extracted")
        self.manifest = {"schema_version": 1, "python": "fixture-claim", "numpy": "fixture-claim",
                         "sources": [{"name": archive.name, "url": "https://example.invalid/fixture.zip", "sha256": self.sha(archive)}],
                         "files": {path.relative_to(self.runtime).as_posix(): self.sha(path) for path in self.runtime.rglob("*") if path.is_file()}}
        self.json(self.runtime / "phaseforge-isolation-runtime.json", self.manifest)

    @staticmethod
    def put(path, data):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)

    def json(self, path, value):
        self.put(path, json.dumps(value).encode())

    @staticmethod
    def sha(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def report(self):
        return inventory.build_report(self.workspace, self.bootstrap, "fixture-commit")

    def test_observed_metadata_and_exact_files_do_not_imply_download_provenance(self):
        report = self.report()
        self.assertTrue(report["integrity_valid"])
        self.assertFalse(report["runtime_execution"])
        self.assertFalse(report["complete_sbom"])
        self.assertEqual(report["source_commit"], "fixture-commit")
        science, numpy = report["runtimes"]["science"], report["runtimes"]["python_numpy"]
        self.assertEqual(science["packages"][0]["version"], "9.fixture.actual-metadata")
        self.assertEqual(numpy["packages"][0]["version"], "8.fixture.metadata")
        self.assertFalse(science["download_provenance"]["hash_pinned_downloads_established"])
        self.assertTrue(any("version pins are not download hash pins" in gap for gap in report["missing_provenance"]))
        self.assertTrue(numpy["cached_archives"][0]["matches_manifest_pin"])
        self.assertEqual(numpy["manifest"]["content"]["sources"], self.manifest["sources"])
        self.assertEqual({row["path"] for row in science["external_base_runtime"]["files"]}, {"python.exe", "python313.dll", "DLLs/_ssl.pyd"})
        self.assertFalse(science["external_base_runtime"]["bundled"])
        self.assertEqual(report["bootstrap_python"]["pe_version"]["status"], "missing")

    def test_changed_pinned_file_produces_explicit_failure_receipt(self):
        self.put(self.runtime / "numpy/_core.pyd", b"changed fixture bytes")
        output = self.root / "evidence.json"
        status = inventory.main(["--workspace", str(self.workspace), "--bootstrap-python", str(self.bootstrap), "--output", str(output)])
        report = json.loads(output.read_text())
        self.assertEqual(status, 1)
        self.assertEqual(report["runtimes"]["python_numpy"]["pin_verification"]["changed_files"][0]["path"], "numpy/_core.pyd")
        self.assertFalse(report["integrity_valid"])

    def test_missing_archive_preserves_declared_pin_without_inventing_observed_bytes(self):
        (self.runtime.parent / ".isolation-downloads/python-fixture.zip").unlink()
        report = self.report()
        archive = report["runtimes"]["python_numpy"]["cached_archives"][0]
        self.assertFalse(archive["present"])
        self.assertIsNone(archive["matches_manifest_pin"])
        self.assertNotIn("sha256", archive)
        self.assertTrue(any("archive bytes not retained" in gap for gap in report["missing_provenance"]))

    def test_no_subprocess_or_package_import_is_used_and_output_is_exclusive(self):
        output = self.root / "evidence.json"
        with patch("subprocess.Popen", side_effect=AssertionError("No subprocess permitted")), patch("os.system", side_effect=AssertionError("No shell permitted")):
            self.assertEqual(inventory.main(["--workspace", str(self.workspace), "--bootstrap-python", str(self.bootstrap), "--output", str(output)]), 0)
            with self.assertRaises(FileExistsError):
                inventory.main(["--workspace", str(self.workspace), "--bootstrap-python", str(self.bootstrap), "--output", str(output)])

    def test_manifest_escape_and_nonfinite_json_are_rejected(self):
        self.manifest["files"]["../escape.dll"] = "a" * 64
        self.json(self.runtime / "phaseforge-isolation-runtime.json", self.manifest)
        with self.assertRaises(inventory.InventoryError):
            self.report()
        self.put(self.runtime / "phaseforge-isolation-runtime.json", b'{"schema_version":1,"files":NaN}')
        with self.assertRaises(inventory.InventoryError):
            self.report()
        self.put(self.runtime / "phaseforge-isolation-runtime.json", b'{"schema_version":1,"files":1e999}')
        with self.assertRaises(inventory.InventoryError):
            self.report()

    def test_hardlinks_are_refused(self):
        os.link(self.runtime / "python.exe", self.runtime / "linked.exe")
        with self.assertRaisesRegex(inventory.InventoryError, "Hardlink"):
            self.report()

    def test_symlink_and_reparse_attributes_are_refused(self):
        original = Path.lstat
        target = self.runtime / "python.exe"
        class Reparse:
            st_mode = 0o100644
            st_nlink = 1
            st_file_attributes = 0x400
        def fake_lstat(path, *args, **kwargs):
            return Reparse() if path == target else original(path, *args, **kwargs)
        with patch.object(Path, "lstat", fake_lstat):
            with self.assertRaisesRegex(inventory.InventoryError, "reparse"):
                self.report()

    def test_output_cannot_modify_inspected_data(self):
        output = self.workspace / "evidence.json"
        with self.assertRaisesRegex(inventory.InventoryError, "outside inspected"):
            inventory.main(["--workspace", str(self.workspace), "--bootstrap-python", str(self.bootstrap), "--output", str(output)])
        self.assertFalse(output.exists())

    def test_missing_metadata_version_is_not_inferred_from_directory(self):
        self.put(self.science / "Lib/site-packages/openmm-wrong-directory-version.dist-info/METADATA", b"Name: OpenMM\n\n")
        report = self.report()
        self.assertIsNone(report["runtimes"]["science"]["packages"][0]["version"])
        self.assertTrue(any("missing/ambiguous" in gap for gap in report["missing_provenance"]))

    def bundled_fixtures(self):
        seeds = self.root / "installed/resources/runtime/runtime-seeds"
        frozen = self.root / "frozen-source-manifests"
        for name, destination_relative in inventory.SEED_LAYOUT.items():
            source = seeds / name
            self.put(source / "python.exe", b"portable fixture; never executable")
            self.put(source / "numpy/_core.pyd", b"native fixture")
            self.put(source / "numpy-fixture.dist-info/METADATA", b"Name: numpy\nVersion: fixture-exact-version\n")
            archive = {"name": "fixture.zip", "url": "https://example.invalid/fixture.zip", "sha256": "a" * 64,
                       "bytes": 42, "upstream_metadata": "https://example.invalid/metadata"}
            if name == "python-numpy-v2":
                inner = {"schema_version": 1, "python": "fixture-python", "numpy": "fixture-numpy", "sources": [archive],
                         "files": {path.relative_to(source).as_posix(): self.sha(path) for path in source.rglob("*") if path.is_file()}}
                self.json(source / "phaseforge-isolation-runtime.json", inner)
            files = {path.relative_to(source).as_posix(): self.sha(path) for path in source.rglob("*") if path.is_file()}
            sizes = {path.relative_to(source).as_posix(): path.stat().st_size for path in source.rglob("*") if path.is_file()}
            manifest = {"schema_version": 1, "schema": "phaseforge.runtime-seed.v1", "kind": name,
                        "python": "fixture-python", "numpy": "fixture-numpy", "files": files, "file_bytes": sizes,
                        "native_files": {key: digest for key, digest in files.items() if Path(key).suffix in inventory.NATIVE_SUFFIXES},
                        "sources": [archive], "transformations": {"fixture": True}, "licenses": []}
            self.json(source / inventory.SEED_MANIFEST, manifest)
            self.put(frozen / f"{name}.manifest.json", (source / inventory.SEED_MANIFEST).read_bytes())
            shutil.copytree(source, self.workspace / destination_relative)
        return seeds, frozen

    def test_bundled_fixed_mapping_matches_every_file_and_needs_no_host_python(self):
        seeds, frozen = self.bundled_fixtures()
        output = self.root / "bundled-evidence.json"
        with patch.object(inventory, "FROZEN_MANIFEST_ROOT", frozen), patch("subprocess.Popen", side_effect=AssertionError("Never execute managed runtimes")):
            status = inventory.main(["--workspace", str(self.workspace), "--seed-root", str(seeds), "--source-commit", "fixture", "--output", str(output)])
        report = json.loads(output.read_text())
        self.assertEqual(status, 0)
        self.assertTrue(report["integrity_valid"])
        self.assertIsNone(report["bootstrap_python"])
        self.assertEqual(report["delivery"], "bundled_immutable_seeds")
        for name, runtime in report["runtimes"].items():
            self.assertEqual(runtime["mapping"]["workspace_relative"], inventory.SEED_LAYOUT[name])
            self.assertFalse(runtime["external_base_runtime_required"])
            self.assertTrue(runtime["copy_comparison"]["outer_manifest_identical"])
            self.assertTrue(runtime["frozen_source_manifest"]["matches_installed_seed_manifest"])
            self.assertFalse(runtime["archive_provenance"]["archive_bytes_inspected"])
        self.assertTrue(report["runtimes"]["python-numpy-v2"]["copy_pin_verification"]["isolation_manifest_verification"]["valid"])

    def test_bundled_copy_mutation_missing_file_and_added_cache_are_detected(self):
        seeds, frozen = self.bundled_fixtures()
        destination = self.workspace / inventory.SEED_LAYOUT["science-v2"]
        self.put(destination / "python.exe", b"mutated")
        (destination / "numpy/_core.pyd").unlink()
        self.put(destination / "__pycache__/unexpected.pyc", b"unexpected cache")
        with patch.object(inventory, "FROZEN_MANIFEST_ROOT", frozen):
            report = inventory.build_report(self.workspace, seed_root=seeds)
        self.assertFalse(report["integrity_valid"])
        comparison = report["runtimes"]["science-v2"]["copy_comparison"]
        self.assertEqual(comparison["changed_files"], ["python.exe"])
        self.assertEqual(comparison["missing_files"], ["numpy/_core.pyd"])
        self.assertEqual(comparison["unexpected_files"], ["__pycache__/unexpected.pyc"])

    def test_mutating_both_outer_manifests_cannot_replace_frozen_source_anchor(self):
        seeds, frozen = self.bundled_fixtures()
        for root in [seeds / "science-v2", self.workspace / inventory.SEED_LAYOUT["science-v2"]]:
            manifest = json.loads((root / inventory.SEED_MANIFEST).read_text())
            manifest["transformations"] = {"fixture": "changed claim"}
            self.json(root / inventory.SEED_MANIFEST, manifest)
        with patch.object(inventory, "FROZEN_MANIFEST_ROOT", frozen):
            report = inventory.build_report(self.workspace, seed_root=seeds)
        science = report["runtimes"]["science-v2"]
        self.assertTrue(science["seed_pin_verification"]["valid"])
        self.assertTrue(science["copy_comparison"]["outer_manifest_identical"])
        self.assertFalse(science["frozen_source_manifest"]["matches_installed_seed_manifest"])
        self.assertFalse(report["integrity_valid"])

    def test_wrong_seed_kind_is_rejected(self):
        seeds, frozen = self.bundled_fixtures()
        outer = seeds / "science-v2" / inventory.SEED_MANIFEST
        manifest = json.loads(outer.read_text())
        manifest["kind"] = "python-numpy-v2"
        self.json(outer, manifest)
        with patch.object(inventory, "FROZEN_MANIFEST_ROOT", frozen), self.assertRaisesRegex(inventory.InventoryError, "Unsupported runtime seed"):
            inventory.build_report(self.workspace, seed_root=seeds)

    def test_bundled_byte_count_claim_is_independently_checked(self):
        seeds, frozen = self.bundled_fixtures()
        outer = seeds / "science-v2" / inventory.SEED_MANIFEST
        manifest = json.loads(outer.read_text())
        manifest["file_bytes"]["python.exe"] += 1
        self.json(outer, manifest)
        with patch.object(inventory, "FROZEN_MANIFEST_ROOT", frozen):
            report = inventory.build_report(self.workspace, seed_root=seeds)
        self.assertFalse(report["integrity_valid"])
        changed = report["runtimes"]["science-v2"]["seed_pin_verification"]["changed_files"]
        self.assertEqual(changed[0]["path"], "python.exe")
        self.assertEqual(changed[0]["expected_bytes"], changed[0]["observed_bytes"] + 1)


if __name__ == "__main__":
    unittest.main()
