"""Actual checker subprocess isolation/no-bytecode regression; no science run."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("field_checker", Path(__file__).with_name("check_field_worker.py"))
checker = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(checker)


class FieldCheckerLaunchTests(unittest.TestCase):
    def test_actual_child_preserves_immutable_source_imports(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            imported = root / "retained_fixture_module.py"
            imported.write_text("value = 12345\n", encoding="utf-8")
            probe = root / "probe.py"
            probe.write_text(
                "import importlib.util, json, pathlib, sys\n"
                "path = pathlib.Path(__file__).with_name('retained_fixture_module.py')\n"
                "spec = importlib.util.spec_from_file_location('retained_fixture_module', path)\n"
                "module = importlib.util.module_from_spec(spec)\n"
                "spec.loader.exec_module(module)\n"
                "print(json.dumps({'isolated':sys.flags.isolated, 'no_bytecode':sys.dont_write_bytecode, 'value':module.value}))\n",
                encoding="utf-8",
            )
            runner = checker.Runner(probe, root)
            (root / "input.json").write_text("{}", encoding="utf-8")
            command = runner.command(root / "input.json", root / "output")
            self.assertEqual(command[1:3], ["-I", "-B"])
            receipt = runner.execute(root, root / "input.json", root / "output", timeout=10)
            self.assertEqual(receipt["exit_code"], 0)
            actual = json.loads((root / "process.stdout.txt").read_text(encoding="utf-8"))
            self.assertEqual(actual, {"isolated": 1, "no_bytecode": True, "value": 12345})
            self.assertFalse((root / "__pycache__").exists())
            self.assertEqual(imported.read_text(encoding="utf-8"), "value = 12345\n")


if __name__ == "__main__":
    unittest.main()
