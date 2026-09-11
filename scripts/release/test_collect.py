"""Synthetic evidence only: no native installer, signer, scanner or provider executes."""
import copy, contextlib, io, json, os, pathlib, tempfile, unittest, zipfile
from unittest.mock import patch
import collect

COMMIT='1234567890abcdef1234567890abcdef12345678'
VERSION='0.8.0'

class CandidateCollectionTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory(prefix='phaseforge-collect-test-')
        self.root=pathlib.Path(self.temporary.name)
        self.target=collect.release_target('Darwin','arm64')
        self.folder=self.root/'.local/marketplace/macos-arm64'
        self.env=patch.dict(os.environ,{'GITHUB_SHA':COMMIT,'GITHUB_RUN_ID':'100','GITHUB_RUN_ATTEMPT':'1'})
        self.root_patch=patch.object(collect,'ROOT',self.root)
        self.env.start();self.root_patch.start()
        self.addCleanup(self.env.stop);self.addCleanup(self.root_patch.stop);self.addCleanup(self.temporary.cleanup)
        self.write(self.root/'desktop/package.json',{'version':VERSION})
        self.write(self.root/'scripts/release/tool-pins.json',{'fixture':True})
        installer=self.root/'desktop/dist'/f'PhaseForge_{VERSION}_macos_arm64.dmg'
        installer.parent.mkdir(parents=True);installer.write_bytes(b'Synthetic immutable artifact; never mounted or executed.')
        command={'status':0,'signal':None,'stdout':'','stderr':''}
        signature={'signature_observation':'ad-hoc','display':command,'verify':command,'gatekeeper_assessment':{**command,'status':3},'quarantine_attribute':{**command,'status':1}}
        self.acceptance={'passed':True,'source_commit':COMMIT,'version':VERSION,'platform':'macos','architecture':'arm64',
                         'host':{'system':'Darwin','machine':'arm64','macos_version':'fixture-observed-version','macos_build':'fixture-observed-build'},
                         'artifact':{'name':installer.name,'bytes':installer.stat().st_size,'sha256':collect.digest(installer)},
                         'macos_installation':{'copy_exact':True,'detached':True,'signature':signature}}
        self.write(self.folder/'NATIVE_ACCEPTANCE.json',self.acceptance)
        self.write(self.folder/'checks/SCAN_REVIEW.json',{'source_commit':COMMIT,'tools_completed':True,'unreviewed_high_critical':[],'secret_findings':[]})
        source={'source':{'commit':COMMIT,'dirty':False},'workflow':{'run_id':'100','run_attempt':'1'},'materials':[{'path':'source.rs','bytes':1,'sha256':'a'*64}]}
        for phase in ['before','after']:self.write(self.root/f'.local/marketplace/source/{phase}.json',source)
        self.write(self.folder/'installed-payload.json',{'source_commit':COMMIT,'version':VERSION,'resources_relative':'Contents/Resources','bytes':100,'backend':{'sha256':'b'*64,'machine':{'format':'macho','architecture':'arm64'}}})
        self.write(self.folder/'compiled-dependencies.json',{'source_commit':COMMIT,'binary_sha256':'b'*64})
        self.write(self.folder/'frontend-modules.json',{'source_commit':COMMIT,'summary':{'bundles':1},'gaps':['Fixture mapping scope remains explicit']})
        self.write(self.folder/'backend-signing.json',{'source_commit':COMMIT,'staged_signed_sha256':'b'*64,'compiler_dependency_section_unchanged':True,'verification':{'status':0}})
        observed={'build':{'sourceCommit':COMMIT,'sourceDirty':False,'version':VERSION,'platform':'darwin','architecture':'arm64'},
                  'runtime':{'appVersion':VERSION,'platform':'darwin','architecture':'arm64','versions':{'electron':'43.7.0'}},
                  'health':{'version':VERSION,'sqlite':{'version':'3.53.2','source_id':'fixture'}}}
        for index in range(1,4):
            self.write(self.folder/f'launch-{index}/runtime.json',observed)
            self.write(self.folder/f'launch-{index}/result.json',{'passed':True,'source_commit':COMMIT,'version':VERSION})
        for rel in ['checks/source.cdx.json','checks/payload.cdx.json','checks/asar.cdx.json','runtime-observations.cdx.json']:
            self.write(self.folder/rel,{'components':[{'type':'library','name':'fixture','version':'1'}]})

    def write(self,file,value):
        file.parent.mkdir(parents=True,exist_ok=True);file.write_text(json.dumps(value),encoding='utf-8')

    def validate(self):return collect.validated_inputs(self.folder,self.target,VERSION,COMMIT)

    def laboratory_fixture(self):
        # These are deliberately tiny synthetic receipts, never a native pass.
        evidence=self.folder/'laboratory/fixture.json';self.write(evidence,{'synthetic_fixture':True})
        lab={'schema':'phaseforge.native-laboratory.v1','passed':True,'source_commit':COMMIT,'version':VERSION,'backend_sha256':'b'*64,'provider_tokens':0,
             'checks':{key:True for key in ('openmm','diffusion','lpac','cancel','quit_recovery','backup_restore')},
             'evidence_files':[{'path':'laboratory/fixture.json','bytes':evidence.stat().st_size,'sha256':collect.digest(evidence)}]}
        runtimes={}
        for kind in ('science-v4','python-numpy-v3'):
            frozen=self.root/'tools/runtime-seeds'/f'{kind}.manifest.json';self.write(frozen,{'synthetic_fixture':kind})
            runtimes[kind]={'seed_pin_verification':{'valid':True},'copy_pin_verification':{'valid':True},'copy_comparison':{'valid':True},'frozen_source_manifest':{'matches_installed_seed_manifest':True,'sha256':collect.digest(frozen)}}
        managed={'source_commit':COMMIT,'delivery':'bundled_immutable_seeds','integrity_valid':True,'runtimes':runtimes}
        sqlite={'schema':'phaseforge.managed-sqlite-execution.v1','source_commit':COMMIT,'passed':True,'runtimes':{kind:{'passed':True,'sqlite_version':'3.53.4','module_version':'3.53.4','compile_options':['ENABLE_FTS5'],'fts5_rows':[[1,'retained result']],'seed_manifest_sha256':runtime['frozen_source_manifest']['sha256']} for kind,runtime in runtimes.items()}}
        for key,name,value in [('laboratory_evidence','LABORATORY_ACCEPTANCE.json',lab),('managed_runtime_evidence','managed-runtime-materials.json',managed),('managed_sqlite_evidence','managed-sqlite-runtime.json',sqlite)]:
            self.write(self.folder/name,value);self.acceptance[key]={'path':name,'sha256':collect.digest(self.folder/name)}
        self.write(self.folder/'NATIVE_ACCEPTANCE.json',self.acceptance)
        return lab,managed

    def test_laboratory_collection_requires_both_fixed_sqlite_queries_bound_to_exact_receipts(self):
        self.laboratory_fixture()
        sqlite_file=self.folder/'managed-sqlite-runtime.json'
        original=collect.load(sqlite_file)
        self.assertIsNotNone(collect.validate_laboratory_inputs(self.folder,VERSION,COMMIT,'b'*64,self.acceptance))
        mutations=[lambda data:data['runtimes'].pop('python-numpy-v3'),
                   lambda data:data['runtimes']['science-v4'].__setitem__('sqlite_version','3.50.4'),
                   lambda data:data['runtimes']['science-v4'].__setitem__('fts5_rows',[]),
                   lambda data:data['runtimes']['science-v4'].__setitem__('seed_manifest_sha256','0'*64)]
        for mutate in mutations:
            changed=copy.deepcopy(original);mutate(changed);self.write(sqlite_file,changed)
            acceptance=copy.deepcopy(self.acceptance);acceptance['managed_sqlite_evidence']['sha256']=collect.digest(sqlite_file)
            with self.assertRaisesRegex(ValueError,'SQLite'):
                collect.validate_laboratory_inputs(self.folder,VERSION,COMMIT,'b'*64,acceptance)
        self.write(sqlite_file,original)
        with sqlite_file.open('a') as stream:stream.write(' ')
        with self.assertRaisesRegex(ValueError,'SQLite execution evidence binding'):
            collect.validate_laboratory_inputs(self.folder,VERSION,COMMIT,'b'*64,self.acceptance)

    def test_native_target_mapping_rejects_cross_labeling_and_unsupported_hosts(self):
        self.assertEqual(collect.release_target('Windows','AMD64')['folder'],'windows-x64')
        self.assertEqual(collect.release_target('Linux','x86_64')['folder'],'linux-x64')
        self.assertEqual(self.target['installer_format'],'macos-dmg')
        for system,architecture in [('Darwin','x86_64'),('Windows','ARM64'),('Linux','aarch64'),('FreeBSD','x86_64')]:
            with self.subTest(system=system,architecture=architecture),self.assertRaises(ValueError):collect.release_target(system,architecture)

    def test_collects_macos_arm64_without_os_trust_or_minimum_claims(self):
        with patch.object(collect,'release_target',return_value=self.target),contextlib.redirect_stdout(io.StringIO()):collect.main()
        final=self.folder/'candidate';prefix=f'PhaseForge_{VERSION}_macos_arm64'
        manifest=collect.load(final/f'{prefix}_BUILD_MANIFEST.json')
        self.assertEqual((manifest['platform'],manifest['architecture'],manifest['installer_format']),('macos','arm64','macos-dmg'))
        self.assertEqual(manifest['channel'],'stable')
        self.assertEqual(manifest['version'],VERSION);self.assertFalse(manifest['intended_prerelease'])
        self.assertEqual(manifest['source_commit'],COMMIT);self.assertEqual(manifest['workflow']['commit'],COMMIT)
        self.assertEqual(manifest['observed_native_host'],self.acceptance['host'])
        signing=manifest['macos_code_signing'];self.assertTrue(signing['ad_hoc_code_integrity_verified'])
        self.assertEqual(signing['gatekeeper_assessment_status'],3);self.assertFalse(signing['developer_id_signing_performed']);self.assertFalse(signing['notarization_performed'])
        self.assertNotIn('minimum_macos',manifest);self.assertFalse(manifest['security']['marketplace_admitted'])
        for line in (final/f'{prefix}_SHA256SUMS.txt').read_text().splitlines():
            digest,name=line.split('  ',1);self.assertEqual(collect.digest(final/name),digest)
        with zipfile.ZipFile(final/f'{prefix}_checks.zip') as archive:
            self.assertIn('evidence/backend-signing.json',archive.namelist());self.assertIn('source/before.json',archive.namelist())
        with patch.object(collect,'release_target',return_value=self.target),self.assertRaises(FileExistsError):collect.main()

    def test_rejects_7471_evidence_or_another_run_instead_of_relabeling(self):
        changes=[('NATIVE_ACCEPTANCE.json',lambda r:r.update(source_commit='7471ff72ac2cba9a89e96e88235cda810ebf1c1d')),
                 ('compiled-dependencies.json',lambda r:r.update(source_commit='0'*40)),
                 ('launch-3/runtime.json',lambda r:r['build'].update(sourceCommit='0'*40)),
                 ('backend-signing.json',lambda r:r.update(staged_signed_sha256='c'*64))]
        for rel,change in changes:
            file=self.folder/rel;original=collect.load(file);value=copy.deepcopy(original);change(value);self.write(file,value)
            with self.subTest(path=rel),self.assertRaises(ValueError):self.validate()
            self.write(file,original)
        before=self.root/'.local/marketplace/source/before.json';value=collect.load(before);value['workflow']['run_id']='old-run';self.write(before,value)
        with self.assertRaisesRegex(ValueError,'another workflow'):self.validate()
        with self.assertRaisesRegex(ValueError,'full immutable'):collect.validated_inputs(self.folder,self.target,VERSION,'7471ff7')

    def test_windows_requires_outer_wrapper_scan_and_collects_its_plugin_findings(self):
        self.target=collect.release_target('Windows','AMD64')
        new_folder=self.root/'.local/marketplace/windows-x64'
        self.assertEqual(new_folder.parent,self.folder.parent)
        self.folder.rename(new_folder);self.folder=new_folder
        old=self.root/'desktop/dist'/self.acceptance['artifact']['name'];new=old.with_name(f'PhaseForge_{VERSION}_x64-setup.exe');old.rename(new)
        self.acceptance.update(platform='windows',architecture='x64',host={'system':'Windows','machine':'AMD64'})
        self.acceptance['artifact']['name']=new.name;self.write(self.folder/'NATIVE_ACCEPTANCE.json',self.acceptance)
        installed=collect.load(self.folder/'installed-payload.json');installed['resources_relative']='resources';installed['backend']['machine'].update(format='pe',architecture='x64');self.write(self.folder/'installed-payload.json',installed)
        for index in range(1,4):
            p=self.folder/f'launch-{index}/runtime.json';launch=collect.load(p);launch['build'].update(platform='win32',architecture='x64');launch['runtime'].update(platform='win32',architecture='x64');self.write(p,launch)
        with self.assertRaises(FileNotFoundError):self.validate()
        wrapper={'source_commit':COMMIT,'completed':True,'installer':self.acceptance['artifact'],'files':[{'path':'members/$PLUGINSDIR/nsis7z.dll','bytes':4,'sha256':'c'*64}]}
        self.write(self.folder/'installer-wrapper.json',wrapper)
        row={'scope':'installer-native','id':'CVE-fixture','severity':'High','artifact':'7-Zip','version':'19.00','locations':[{'path':'members/$PLUGINSDIR/nsis7z.dll'}]}
        review=collect.load(self.folder/'checks/SCAN_REVIEW.json');review['unreviewed_high_critical']=[row];review['installer_wrapper']={'completed':True,'installer_sha256':self.acceptance['artifact']['sha256'],'members_including_container':2};self.write(self.folder/'checks/SCAN_REVIEW.json',review)
        for scope in ['installer-wrapper','installer-native']:
            self.write(self.folder/f'checks/grype-{scope}.stdout',{'matches':[row]})
            self.write(self.folder/f'checks/grype-{scope}.command.json',{'exit_code':0})
        component={'type':'library','name':'7-Zip','version':'19.00','properties':[{'name':'phaseforge:installer-member','value':'members/$PLUGINSDIR/nsis7z.dll'}]}
        self.write(self.folder/'checks/installer-wrapper.cdx.json',{'components':[]})
        self.write(self.folder/'installer-native.cdx.json',{'components':[component]})
        self.laboratory_fixture()
        with patch.object(collect,'release_target',return_value=self.target),contextlib.redirect_stdout(io.StringIO()):collect.main()
        final=self.folder/'candidate';prefix=f'PhaseForge_{VERSION}_windows_x64';manifest=collect.load(final/f'{prefix}_BUILD_MANIFEST.json')
        self.assertEqual(manifest['security']['unreviewed_high_critical_count'],1)
        bom=collect.load(final/f'{prefix}_payload.cdx.json');actual=next(x for x in bom['components'] if x['name']=='7-Zip')
        self.assertEqual(actual['version'],'19.00');self.assertIn({'name':'phaseforge:inventory-scope','value':'installer-native'},actual['properties'])
        with zipfile.ZipFile(final/f'{prefix}_checks.zip') as archive:
            self.assertIn('evidence/installer-wrapper.json',archive.namelist());self.assertIn('evidence/installer-native.cdx.json',archive.namelist())
            self.assertEqual(json.loads(archive.read('evidence/checks/grype-installer-native.stdout'))['matches'],[row])
            self.assertIn('evidence/laboratory/fixture.json',archive.namelist())

    def test_old_profile_or_changed_laboratory_artifact_cannot_be_relabeled(self):
        validate=lambda:collect.validate_laboratory_inputs(self.folder,VERSION,COMMIT,'b'*64,self.acceptance)
        with self.assertRaises(FileNotFoundError):validate()
        self.laboratory_fixture();validate()
        (self.folder/'laboratory/fixture.json').write_bytes(b'changed')
        with self.assertRaisesRegex(ValueError,'evidence changed'):validate()
        self.laboratory_fixture()
        file=self.root/'tools/runtime-seeds/science-v4.manifest.json';file.write_bytes(b'other source seed')
        with self.assertRaisesRegex(ValueError,'frozen source'):validate()

    def test_omitted_native_phase_and_prerelease_versions_block_final_assembly(self):
        lab,_=self.laboratory_fixture();lab['checks']['quit_recovery']=False
        file=self.folder/'LABORATORY_ACCEPTANCE.json';self.write(file,lab)
        self.acceptance['laboratory_evidence']['sha256']=collect.digest(file)
        with self.assertRaisesRegex(ValueError,'quit_recovery'):
            collect.validate_laboratory_inputs(self.folder,VERSION,COMMIT,'b'*64,self.acceptance)
        with self.assertRaisesRegex(ValueError,'prerelease suffix'):
            collect.validated_inputs(self.folder,self.target,'0.8.0-alpha.1',COMMIT)

    def test_rejects_changed_materials_wrong_observed_architecture_and_traversal(self):
        for rel,change in [('installed-payload.json',lambda r:r.update(resources_relative='resources')),
                           ('launch-1/runtime.json',lambda r:r['runtime'].update(architecture='x64')),
                           ('NATIVE_ACCEPTANCE.json',lambda r:r['artifact'].update(name='../foreign.dmg')),
                           ('NATIVE_ACCEPTANCE.json',lambda r:r['host'].update(machine='x86_64'))]:
            file=self.folder/rel;original=collect.load(file);value=copy.deepcopy(original);change(value);self.write(file,value)
            with self.subTest(path=rel),self.assertRaises(ValueError):self.validate()
            self.write(file,original)
        after=self.root/'.local/marketplace/source/after.json';value=collect.load(after);value['materials'][0]['sha256']='c'*64;self.write(after,value)
        with self.assertRaisesRegex(ValueError,'materials changed'):self.validate()

    def test_signing_failures_remain_observations_and_never_become_trust_claims(self):
        value=copy.deepcopy(self.acceptance);value['macos_installation']['signature']['verify']['status']=1
        description,signing=collect.signing_observation(value,self.target)
        self.assertFalse(signing['ad_hoc_code_integrity_verified']);self.assertIn('status 1',description)
        value['macos_installation']['signature']['signature_observation']='unsigned'
        _,signing=collect.signing_observation(value,self.target);self.assertFalse(signing['ad_hoc_code_integrity_verified'])

    def test_collection_rejects_broken_or_unconfirmed_adhoc_seal_but_retains_os_trust_failures(self):
        # Non-notarized Gatekeeper/quarantine observations are already nonzero in
        # the valid fixture. They do not waive a failed application integrity check.
        self.validate()
        for change in [lambda s:s['verify'].update(status=1),lambda s:s['display'].update(status=1),
                       lambda s:s.update(signature_observation='undetermined'),lambda s:s.update(signature_observation='unsigned')]:
            value=copy.deepcopy(self.acceptance);change(value['macos_installation']['signature'])
            self.write(self.folder/'NATIVE_ACCEPTANCE.json',value)
            with self.assertRaisesRegex(ValueError,'verified ad-hoc application seal'):self.validate()
        self.write(self.folder/'NATIVE_ACCEPTANCE.json',self.acceptance)

if __name__=='__main__':unittest.main()
