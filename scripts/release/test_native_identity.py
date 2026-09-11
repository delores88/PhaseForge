"""Static synthetic PE/archive fixtures; no inspected program executes."""
import contextlib
import hashlib
from pathlib import Path
import struct
import tempfile
import unittest
import zipfile
import native_identity as native


class NativeIdentityTests(unittest.TestCase):
    @staticmethod
    def pe():
        data = bytearray(1024)
        data[:2] = b'MZ'
        struct.pack_into('<I', data, 0x3c, 0x80)
        data[0x80:0x84] = b'PE\0\0'
        struct.pack_into('<HH', data, 0x84, 0x8664, 1)
        struct.pack_into('<H', data, 0x94, 240)
        struct.pack_into('<H', data, 0x98, 0x20b)
        struct.pack_into('<I', data, 0x98 + 108, 16)
        struct.pack_into('<II', data, 0x98 + 120, 0x1000, 40)
        struct.pack_into('<IIII', data, 0x98 + 240 + 8, 512, 0x1000, 512, 512)
        struct.pack_into('<IIIII', data, 512, 0, 0, 0, 0x1050, 0)
        data[592:605] = b'KERNEL32.dll\0'
        return bytes(data)

    def test_actual_import_and_architecture_parse_with_truncation_refusal(self):
        result = native.pe_imports(self.pe())
        self.assertEqual(result['architecture'], 'x64')
        self.assertEqual(result['imports'], ['KERNEL32.dll'])
        self.assertEqual(native.pe_imports(self.pe()[:600])['status'], 'missing')
        corrupt = bytearray(self.pe())
        struct.pack_into('<I', corrupt, 524, 0xffffffff)
        self.assertEqual(native.pe_imports(corrupt)['status'], 'missing')

    def test_exact_native_archive_mapping_and_missing_version_are_distinct(self):
        with tempfile.TemporaryDirectory(prefix='pf-native-') as temporary:
            root = Path(temporary).resolve()
            raw = self.pe()
            (root / 'test.pyd').write_bytes(raw)
            archive = root / 'fixture.whl'
            with zipfile.ZipFile(archive, 'w') as output:
                output.writestr('test.pyd', raw)
                output.writestr('fixture.dist-info/METADATA', 'Name: fixture\nVersion: 3.2.1\n')
            def record(path):
                data = Path(path).read_bytes()
                return {'sha256': hashlib.sha256(data).hexdigest(), 'bytes': len(data)}
            @contextlib.contextmanager
            def opened(path):
                with Path(path).open('rb') as stream:
                    yield stream, None
            source = {'name': archive.name, 'url': 'https://example.invalid/fixture.whl', **record(archive)}
            row = {'path': 'test.pyd', **record(root / 'test.pyd')}
            def inspect(sources, archive_root):
                return native.evidence(root, [row], sources, archive_root, record=record, opened=opened,
                                       version=lambda _: {'status': 'missing', 'reason': 'fixture'})
            receipt = inspect([source], root)
            self.assertTrue(receipt['all_native_archive_members_verified'])
            self.assertEqual(receipt['missing_pe_versions'], ['test.pyd'])
            self.assertEqual(receipt['files'][0]['origin']['distribution_metadata']['version'], '3.2.1')
            self.assertFalse(receipt['complete_component_attribution'])
            self.assertIsNone(inspect([source], None)['files'][0]['origin'])
            alias_path = root / 'MSVCP140.dll'
            alias_path.write_bytes(raw)
            alias_row = {'path': alias_path.name, **record(alias_path)}
            alias_pin = {'MSVCP140.dll': {'archive_member': 'test.pyd', 'sha256': row['sha256'], 'reason': 'fixture alias'}}
            mapped = native.evidence(root, [row, alias_row], [source], root, record=record, opened=opened,
                version=lambda _: {'status': 'missing'}, aliases=alias_pin)
            self.assertEqual(mapped['files'][1]['origin']['member'], 'test.pyd')
            self.assertEqual(mapped['files'][1]['origin']['destination'], 'MSVCP140.dll')
            self.assertTrue(mapped['all_native_archive_members_verified'])
            with self.assertRaisesRegex(ValueError, 'frozen file pin'):
                native.evidence(root, [alias_row], [source], root, record=record, opened=opened,
                    version=lambda _: {'status': 'missing'}, aliases={'MSVCP140.dll': {'archive_member': 'test.pyd', 'sha256': '0'*64}})
            with self.assertRaisesRegex(ValueError, 'frozen pin'):
                inspect([{**source, 'sha256': '0' * 64}], root)
            (root / 'test.pyd').write_bytes(raw + b'changed')
            with self.assertRaisesRegex(ValueError, 'changed since inventory'):
                inspect([source], root)


if __name__ == '__main__':
    unittest.main()
