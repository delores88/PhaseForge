"""Static synthetic PE/archive fixtures; no inspected program executes."""
import contextlib
import copy
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

    def test_explicit_sqlite_replacement_replays_both_origins_and_refuses_forged_mapping(self):
        with tempfile.TemporaryDirectory(prefix='pf-native-sqlite-') as temporary:
            root = Path(temporary).resolve()
            original, replacement, definition = self.pe(), self.pe()+b'new upstream DLL', b'EXPORTS\nsqlite3_open\n'
            def record(path):
                data=Path(path).read_bytes()
                return {'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)}
            @contextlib.contextmanager
            def opened(path):
                with Path(path).open('rb') as stream:yield stream,None
            sources=[]
            for name,raw in [('python.zip',original),('sqlite.zip',replacement)]:
                with zipfile.ZipFile(root/name,'w') as archive:
                    archive.writestr('sqlite3.dll',raw)
                    if name=='sqlite.zip':archive.writestr('sqlite3.def',definition)
                sources.append({'name':name,'url':'https://example.invalid/'+name,**record(root/name)})
            (root/'sqlite3.dll').write_bytes(replacement)
            aux=root/'source-evidence/sqlite-3.53.4/sqlite3.def';aux.parent.mkdir(parents=True);aux.write_bytes(definition)
            row={'path':'sqlite3.dll',**record(root/'sqlite3.dll')}
            pin={'component':'SQLite','version':'3.53.4','reason':'Synthetic upstream replacement fixture',
                 'original':{'archive_name':'python.zip','archive_sha256':sources[0]['sha256'],'archive_member':'sqlite3.dll','sha256':hashlib.sha256(original).hexdigest(),'bytes':len(original),'version':'3.50.4'},
                 'replacement':{'archive_name':'sqlite.zip','archive_sha256':sources[1]['sha256'],'archive_member':'sqlite3.dll','sha256':row['sha256'],'bytes':row['bytes'],'version':'3.53.4'},
                 'auxiliary_members':[{'path':'source-evidence/sqlite-3.53.4/sqlite3.def','archive_member':'sqlite3.def',**record(aux)}]}
            def inspect(value=pin,archive_root=root):
                return native.evidence(root,[row],sources,archive_root,record=record,opened=opened,
                                       version=lambda _: {'status':'missing'},replacements={'sqlite3.dll':value})
            result=inspect();observed=result['files'][0]
            self.assertEqual(observed['origin']['archive'],'sqlite.zip')
            self.assertEqual(observed['origin']['component_version'],'3.53.4')
            self.assertEqual(result['replaced_original_members']['sqlite3.dll']['archive'],'python.zip')
            self.assertTrue(result['replacement_auxiliary_members'][0]['archive_bytes_verified'])
            self.assertTrue(result['all_native_archive_members_verified'])
            self.assertIsNone(inspect(archive_root=None)['files'][0]['origin'])
            for mutate in [lambda p:p['original'].__setitem__('sha256','a'*64),lambda p:p['replacement'].__setitem__('archive_name','python.zip'),lambda p:p['replacement'].__setitem__('sha256','b'*64),lambda p:p.__setitem__('original',{}),lambda p:p['auxiliary_members'][0].__setitem__('sha256','c'*64),lambda p:p['auxiliary_members'][0].__setitem__('path','../sqlite3.def')]:
                bad=copy.deepcopy(pin);mutate(bad)
                with self.assertRaises(ValueError):inspect(bad)
            aux.write_bytes(definition+b'changed')
            with self.assertRaisesRegex(ValueError,'auxiliary file'):inspect()


if __name__ == '__main__':
    unittest.main()
