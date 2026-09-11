import io, json, pathlib, tarfile, unittest, zipfile
from install_tools import binary_member

class ToolArchiveTests(unittest.TestCase):
    def test_extracts_only_selected_executable_bytes_without_following_archive_path(self):
        stream=io.BytesIO()
        with zipfile.ZipFile(stream,'w') as archive:
            archive.writestr('nested/tool.exe',b'MZactual');archive.writestr('../../unrelated',b'never written')
        self.assertEqual(binary_member('tool.zip','tool.exe',stream.getvalue()),b'MZactual')

    def test_refuses_ambiguous_duplicate_executables(self):
        stream=io.BytesIO()
        with zipfile.ZipFile(stream,'w') as archive:
            archive.writestr('a/tool.exe',b'a');archive.writestr('b/tool.exe',b'b')
        with self.assertRaises(ValueError):binary_member('tool.zip','tool.exe',stream.getvalue())

    def test_refuses_tar_symlink_as_executable(self):
        stream=io.BytesIO()
        with tarfile.open(fileobj=stream,mode='w') as archive:
            link=tarfile.TarInfo('tool');link.type=tarfile.SYMTYPE;link.linkname='/etc/passwd';archive.addfile(link)
        with self.assertRaises(ValueError):binary_member('tool.tar.gz','tool',stream.getvalue())

    def test_all_platform_pins_are_exact_sha256_official_assets(self):
        pins=json.loads((pathlib.Path(__file__).parent/'tool-pins.json').read_text(encoding='utf-8-sig'))
        self.assertEqual(set(pins),{'syft','grype','gh','cargo-auditable','gitleaks'})
        for name,record in pins.items():
            self.assertEqual(record['tag'],'v'+record['version'])
            for target in ('windows','linux','macos'):
                self.assertRegex(record[target]['sha256'],r'^[0-9a-f]{64}$')
                self.assertNotIn('/',record[target]['archive'])

if __name__=='__main__':unittest.main()
