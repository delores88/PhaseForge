"""Data-only gauge postprocessing tests; never execute AthenaK."""
import json
from pathlib import Path
import tempfile
import unittest
import uuid
import numpy as np
import nr_gauge_result as result
from test_athenak_decode import write_native,analytic_fields


class GaugeResultTests(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.run=self.root/'work';self.run.mkdir();self.receipt=self.root/'receipt.json'
        for index,time in enumerate((0,.05,.1)):
            write_native(self.run/f'metric-{index}.bin',time)
            write_native(self.run/f'con-{index}.bin',time,data=np.zeros((1,4,4,32)),names=['con_H'])
        self.pin_receipt()

    def tearDown(self):
        # Snapshot read-only flags are part of publication; remove only test copies.
        for path in self.root.rglob('*'):
            if path.is_file():path.chmod(0o600)
        self.temp.cleanup()

    def pin_receipt(self,**changes):
        receipt={'schema':'phaseforge.nr-process-receipt.v1','engine_id':'athenak_gauge_wave','job_id':str(uuid.uuid4()),'source':{'athenak_commit':result.decoder.UPSTREAM_COMMIT},
                 'exit_code':0,'termination_reason':'completed','process_group_drained':True,
                 'files':[{'path':path.name,'bytes':path.stat().st_size,'sha256':result.digest(path.read_bytes())} for path in sorted(self.run.glob('*.bin'))]}
        receipt.update(changes);self.receipt.write_text(json.dumps(receipt));self.receipt_hash=result.digest(self.receipt.read_bytes())

    def produce(self):return result.produce(self.run,self.receipt,self.receipt_hash,self.root/'result')

    def test_reopenable_data_preserves_original_coordinates_channels_and_physical_scope(self):
        saved=self.produce();self.assertTrue(saved['execution']['success']);self.assertTrue(saved['analytic']['passed'])
        self.assertFalse(saved['fulfillment']['black_hole_collision_validated']);self.assertFalse(saved['fulfillment']['physical_radiation'])
        self.assertEqual(saved['units']['time'],'L/c');self.assertIsNone(saved['units']['si_mapping'])
        self.assertEqual([row['time'] for row in saved['timeframes']],[0,.05,.1])
        frame=saved['timeframes'][1];self.assertEqual(frame['plane']['coordinate'],.375);self.assertEqual(frame['shape'],[4,32])
        root=self.root/'result';data=json.loads((root/frame['data']['path']).read_text())
        self.assertEqual(data['x'][:2],[.015625,.046875]);self.assertEqual(data['y'],[.125,.375,.625,.875])
        native=result.decoder.read_binary(root/frame['source_frame']['path'])
        fields=native['blocks'][0]['fields'];chi=float(fields['z4c_chi'][1,2,9])
        gxx=float(fields['z4c_gxx'][1,2,9])/chi
        expected=float(fields['z4c_Axx'][1,2,9])/chi+(float(fields['z4c_Khat'][1,2,9])+2*float(fields['z4c_Theta'][1,2,9]))*gxx/3
        self.assertEqual(data['channels']['Kxx'][2][9],expected)
        with np.load(root/frame['arrays']['path'],allow_pickle=False) as array:
            np.testing.assert_array_equal(array['alpha'],np.asarray(data['channels']['alpha']))
        with np.load(root/frame['source_block']['metric']['path'],allow_pickle=False) as block:
            np.testing.assert_array_equal(block['z4c_alpha'],fields['z4c_alpha'])
        result_hash=result.digest((root/'result.json').read_bytes())
        self.assertEqual(result.read_result(root,result_hash),saved)
        (root/frame['data']['path']).write_text('{}')
        with self.assertRaisesRegex(ValueError,'SHA-256'):result.read_result(root,result_hash)

    def test_numeric_failure_does_not_relabel_successful_execution_or_relax_limits(self):
        data=analytic_fields(.1);data[8,0,0,0]+=.01
        write_native(self.run/'metric-2.bin',.1,data=data);self.pin_receipt()
        saved=self.produce();self.assertTrue(saved['execution']['success']);self.assertFalse(saved['analytic']['passed'])
        self.assertEqual(saved['analytic']['status'],'failed');self.assertFalse(saved['fulfillment']['gauge_reference_validated'])
        analytic=json.loads((self.root/'result/analytic.json').read_text())
        self.assertEqual(analytic['limits']['linf']['Kxx'],.0005)
        self.assertGreater(analytic['check']['frames'][-1]['errors']['Kxx']['linf'],.0005)

    def test_failed_execution_cannot_be_certified_by_valid_reference_arrays(self):
        self.pin_receipt(exit_code=7,termination_reason='engine_exit')
        saved=self.produce();self.assertTrue(saved['analytic']['passed']);self.assertFalse(saved['execution']['success'])
        self.assertFalse(saved['fulfillment']['gauge_reference_validated'])

    def test_wrong_receipt_or_frame_hash_and_unregistered_frame_fail_before_publication(self):
        with self.assertRaisesRegex(ValueError,'SHA-256'):result.produce(self.run,self.receipt,'0'*64,self.root/'result')
        source=self.run/'metric-0.bin';original=source.read_bytes();source.write_bytes(original+b'x')
        with self.assertRaisesRegex(ValueError,'SHA-256'):self.produce()
        self.assertFalse((self.root/'result').exists());source.write_bytes(original)
        (self.run/'extra.bin').write_bytes(original)
        with self.assertRaisesRegex(ValueError,'Unregistered'):self.produce()

    def test_black_hole_or_unrelated_engine_cannot_be_mislabeled_as_flat_gauge_data(self):
        self.pin_receipt(engine_id='athenak_black_hole_pilot')
        with self.assertRaisesRegex(ValueError,'only admits the gauge-wave'):self.produce()
        self.assertFalse((self.root/'result').exists())

    def test_immutable_output_and_untrusted_paths_are_refused(self):
        self.produce()
        with self.assertRaisesRegex(ValueError,'new immutable'):self.produce()
        for path in ('../escape.bin','/absolute.bin','bad\\name.bin','a/../b.bin','a//b.bin'):
            with self.subTest(path=path),self.assertRaises(ValueError):result.relative(path)
        with self.assertRaisesRegex(ValueError,'inside'):result.produce(self.run,self.receipt,self.receipt_hash,self.run/'derived')

    def test_legacy_evidence_requires_explicit_mode_and_never_invents_job_identity(self):
        receipt=json.loads(self.receipt.read_text());receipt.pop('schema');receipt.pop('job_id');receipt.update(nx=32,at='retained historical receipt',stopped_reason=None)
        self.receipt.write_text(json.dumps(receipt));self.receipt_hash=result.digest(self.receipt.read_bytes())
        with self.assertRaisesRegex(ValueError,'process receipt'):self.produce()
        saved=result.produce(self.run,self.receipt,self.receipt_hash,self.root/'result',legacy_foundation=True)
        self.assertIsNone(saved['execution']['job_id']);self.assertIsNone(saved['execution']['process_group_drained'])
        self.assertEqual(saved['execution']['receipt_type'],'historical_foundation')


if __name__=='__main__':unittest.main()
