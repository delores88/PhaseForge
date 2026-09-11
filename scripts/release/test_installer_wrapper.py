"""Synthetic NSIS listing/member fixtures; no installer, scanner or application runs."""
import hashlib, pathlib, tempfile, unittest
import installer_wrapper as wrapper

LISTING='Type = Nsis\n\n----------\nPath = $PLUGINSDIR\\nsis7z.dll\nSize = \n\nPath = $PLUGINSDIR\\app-64.7z\nSize = 3\n'

class InstallerWrapperTests(unittest.TestCase):
    def test_outer_plugin_missing_from_installed_payload_is_inventoried_and_versioned(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary);archive=root/'setup.exe';archive.write_bytes(b'inert wrapper fixture')
            members={'$PLUGINSDIR\\nsis7z.dll':b'inert plugin fixture','$PLUGINSDIR\\app-64.7z':b'app'};calls=[]
            def reader(executable,args,output,error,limit):
                calls.append(args);data=LISTING.encode() if args[0]=='l' else members[args[-1]]
                self.assertLessEqual(len(data),limit);output.write_bytes(data);error.write_bytes(b'')
            records=wrapper.extract_members(archive,root/'wrapper',root/'reader.exe',root/'checks',runner=reader)
            self.assertEqual(len(records),3)
            plugin=next(r for r in records if r['path'].endswith('nsis7z.dll'))
            self.assertEqual(plugin['sha256'],hashlib.sha256(members[plugin['member']]).hexdigest())
            metadata={'status':'observed_version_resource','product_version_parts':[19,0,0,0],
                      'strings':[{'ProductName':'7-Zip','FileDescription':'7-Zip Plugin for NSIS','ProductVersion':'19.00'}]}
            components=wrapper.version_components([plugin],[metadata])
            self.assertEqual(components[0]['version'],'19.00')
            self.assertIn(':7-zip:7-zip:19.00:',components[0]['cpe'])
            self.assertIn({'name':'syft:location:0:path','value':plugin['path']},components[0]['properties'])
            self.assertTrue(all(args[:5]==['x','-so','-y','-spd','--'] for args in calls[1:]))
            self.assertFalse(any(str(archive)==args[0] for args in calls))
            self.assertEqual((root/'wrapper/container/setup.exe').read_bytes(),archive.read_bytes())

    def test_rejects_traversal_case_aliases_links_and_non_nsis_containers(self):
        for name in ['../escape.dll','/absolute','C:\\drive','folder//file','folder/./file','CON.dll','trailing.','bad\x00name']:
            with self.subTest(name=name),self.assertRaises(ValueError):wrapper.member_path(name)
        with self.assertRaises(ValueError):wrapper.parse_listing(LISTING+'\nPath = $pluginsdir\\NSIS7Z.DLL\nSize = 4\n')
        with self.assertRaises(ValueError):wrapper.parse_listing(LISTING.replace('Size = \n','Symbolic Link = other\nSize = \n'))
        with self.assertRaises(ValueError):wrapper.parse_listing(LISTING.replace('Type = Nsis','Type = 7z'))

    def test_builtin_file_payload_needs_no_nested_archive_or_seven_zip_plugin(self):
        listing='Type = Nsis\n\n----------\nPath = resources\\runtime\\phaseforge-backend.exe\nSize = 3\n\nPath = $PLUGINSDIR\\System.dll\nSize = 4\n'
        with tempfile.TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary);archive=root/'setup.exe';archive.write_bytes(b'inert wrapper')
            members={'resources\\runtime\\phaseforge-backend.exe':b'app','$PLUGINSDIR\\System.dll':b'tool'}
            def reader(executable,args,output,error,limit):
                output.write_bytes(listing.encode() if args[0]=='l' else members[args[-1]]);error.write_bytes(b'')
            records=wrapper.extract_members(archive,root/'wrapper',root/'reader.exe',root/'checks',runner=reader)
            self.assertEqual({r.get('member') for r in records if r.get('member')},set(members))
            self.assertEqual(wrapper.version_components(records,[{'status':'no_version_resource'}]*len(records)),[])

    def test_declared_size_mismatch_and_existing_destination_fail_closed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=pathlib.Path(temporary);archive=root/'setup.exe';archive.write_bytes(b'fixture')
            def reader(executable,args,output,error,limit):
                output.write_bytes((LISTING if args[0]=='l' else 'wrong length').encode());error.write_bytes(b'')
            with self.assertRaisesRegex(ValueError,'differs from listing'):
                wrapper.extract_members(archive,root/'wrapper',root/'reader.exe',root/'checks',runner=reader)
            with self.assertRaisesRegex(ValueError,'must be new'):
                wrapper.extract_members(archive,root/'wrapper',root/'reader.exe',root/'checks',runner=reader)

    def test_component_requires_observed_product_identity_and_consistent_fixed_version(self):
        file={'path':'members/$PLUGINSDIR/nsis7z.dll','sha256':'a'*64}
        self.assertEqual(wrapper.version_components([file],[{'status':'no_version_resource'}]),[])
        unrelated={'status':'observed_version_resource','product_version_parts':[19,0,0,0],'strings':[{'ProductName':'Other product','ProductVersion':'19.00'}]}
        self.assertEqual(wrapper.version_components([file],[unrelated]),[])
        inconsistent={**unrelated,'strings':[{'ProductName':'7-Zip','ProductVersion':'25.01'}]}
        with self.assertRaisesRegex(ValueError,'disagree'):wrapper.version_components([file],[inconsistent])

if __name__=='__main__':unittest.main()
