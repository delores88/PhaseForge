"""Published-render provenance contracts without importing/executing Blender."""
import ast
import copy
import hashlib
import json
from pathlib import Path
from types import SimpleNamespace
import tempfile
import unittest

tree=ast.parse(Path(__file__).with_name("trajectory_render.py").read_text(encoding="utf-8"))
helpers={"committed_digest","publication_metadata","publication_title"}
namespace={"hashlib":hashlib,"json":json,"Path":Path}
exec(compile(ast.Module(body=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in helpers],type_ignores=[]),"trajectory_render.py","exec"),namespace)


def write(path,value):
    raw=json.dumps(value,separators=(",",":"),ensure_ascii=False).encode()
    path.write_bytes(raw);return hashlib.sha256(raw).hexdigest()


def fixture(root,particle):
    model={"id":"linear fixture", "description":"Retained numerical test states", "scope":"Synthetic data; no validated engine", "limitations":["No scientific validity claimed"]}
    representation="particle_trajectory" if particle else "scalar_field"
    original={"schema":"phaseforge.simulation.v1","representation":representation,"model":model}
    source_hash=write(root/"source-simulation.json",original)
    manifest_hash=write(root/"source-execution-manifest.json",{"fixture":True})
    source=SimpleNamespace(root=root,index={"representation":representation,"boundary":"isolated","position_unit":"AU","time_unit":"days"},index_sha256="b"*64,topology={"boundary":"isolated"},topology_sha256="c"*64)
    metadata={"kind":"published_simulation","representation":representation,"scientific_validation":"not_established_by_publication","model":model,
              "index_sha256":"b"*64,"topology_sha256":"c"*64 if particle else None,"source_sha256":source_hash,"source_manifest_sha256":manifest_hash,"source_code_sha256":"a"*64}
    return source,metadata


class PublishedRenderContracts(unittest.TestCase):
    def test_same_exact_model_and_source_metadata_accepted_as_plain_text(self):
        with tempfile.TemporaryDirectory() as temporary:
            for particle in (False,True):
                root=Path(temporary);source,metadata=fixture(root,particle)
                self.assertEqual(namespace["publication_metadata"]({"source_metadata":metadata},source),metadata)
                self.assertIn("model validity not established",namespace["publication_title"](metadata))
                self.assertNotIn("periodic",namespace["publication_title"](metadata))
            self.assertIsNone(namespace["publication_metadata"]({},source))

    def test_changed_hash_model_validity_claim_or_periodicity_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);source,metadata=fixture(root,True)
            for key,value in (("index_sha256","e"*64),("topology_sha256","e"*64),("source_sha256","e"*64),("source_code_sha256",""),("scientific_validation","validated")):
                changed=copy.deepcopy(metadata);changed[key]=value
                with self.subTest(key=key),self.assertRaises(ValueError):namespace["publication_metadata"]({"source_metadata":changed},source)
            changed=copy.deepcopy(metadata);changed["model"]["scope"]="Replacement model claim"
            with self.assertRaisesRegex(ValueError,"model declaration"):namespace["publication_metadata"]({"source_metadata":changed},source)
            source.index["wrapping"]="periodic [0,L)"
            with self.assertRaisesRegex(ValueError,"periodic"):namespace["publication_metadata"]({"source_metadata":metadata},source)
            source.index.pop("wrapping");source.topology["box"]=[1,1,1]
            with self.assertRaisesRegex(ValueError,"periodic"):namespace["publication_metadata"]({"source_metadata":metadata},source)

    def test_source_file_mutation_cannot_be_rendered_with_old_receipt(self):
        with tempfile.TemporaryDirectory() as temporary:
            source,metadata=fixture(Path(temporary),False)
            (source.root/"source-simulation.json").write_text('{"expression":"sin(t)"}',encoding="utf-8")
            with self.assertRaisesRegex(ValueError,"source bytes changed"):namespace["publication_metadata"]({"source_metadata":metadata},source)


if __name__=="__main__":unittest.main()
