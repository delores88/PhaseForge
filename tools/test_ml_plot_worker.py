"""Independent plotting checks using explicitly synthetic, complete data fixtures."""
import copy
import hashlib
import json
import math
from pathlib import Path
import statistics
import tempfile
import unittest

from PIL import Image
import ml_plot_worker as worker

def encoded(value):return json.dumps(value,sort_keys=True,separators=(',',':')).encode()
def hashed(value):return hashlib.sha256(encoded(value)).hexdigest()

def fixture():
    roles=['calibration','train','calibration','test','train','calibration','calibration','train','train','test','train','calibration','validation','validation','test','calibration','regime_test','regime_test','regime_test','regime_test','validation','calibration','train','train','train','train','train','train','calibration','test','calibration','train']
    conditions=[]
    for i in range(32):
        t=[220,250,280,310,325,370,400,430][i//4];rho=[.25,.45,.65,.85][i%4];key=f'phaseforge-ml-argon-v1|T={t:g}|rho={rho:.2f}'
        conditions.append({'condition_index':i,'temperature_kelvin':t,'density_g_cm3':rho,'role':roles[i],'split_key':key,'split_sha256':hashlib.sha256(key.encode()).hexdigest()})
    common={'schema_version':1,'study_id':'synthetic-plot-contract','proposal_sha256':'1'*64,'protocol_sha256':'2'*64}
    split={**common,'conditions':conditions,'ood':{'condition_index':32,'temperature_kelvin':180,'density_g_cm3':.55,'role':'ood'},'protocol':{'engine':'openmm_argon','target':{'replicates':3,'name':'mean_pressure_bar'},'feature_schema':[{'bounds':[220,430],'name':'temperature_kelvin','unit':'K'},{'bounds':[.25,.85],'name':'density_g_cm3','unit':'g/cm^3'}]}}
    common['protocol_sha256']=hashed(split['protocol']);split['protocol_sha256']=common['protocol_sha256']
    def seeds(condition,mean):return [{**condition,'seed':110000+condition['condition_index']*1000+i,'replicate_index':i,'pressure_bar':mean+[-6,0,6][i],'solver_job_id':f'synthetic-{condition["condition_index"]}-{i}','fixture_only':True} for i in range(3)]
    cases=[]
    for c in conditions:
        if c['role'] not in ('test','regime_test'):continue
        pressure=180+c['condition_index']*10;prediction=pressure+(8 if c['condition_index']%2 else -8);se=6/math.sqrt(3)
        cases.append({**c,'pressure_bar':pressure,'prediction_bar':prediction,'prediction_Z':prediction/500,'p0_bar':500,'seed_sd_bar':6,'seed_se_bar':se,'seed_t95_bar':[pressure-4.302653*se,pressure+4.302653*se],'interval_bar':[prediction-20,prediction+20],'covered':True,'support_admitted':True,'support_distance':.2,'seeds':seeds(c,pressure)})
    evaluation={**common,'split_sha256':hashed(split),'model_sha256':'3'*64,'calibration_sha256':'4'*64,'held_out_evaluation_complete':True,'held_out_count':8,'coverage_count':8,'support_count':8,'q_Z':.04,'cases':cases,'gates':{'accuracy':True,'baseline_improvement':False,'coverage_and_support':True,'warm_speed':True,'total_workload_cost':False},'rejections':['baseline_improvement','total_workload_cost'],'useful_acceleration':False,'cost_evaluation_complete':True}
    ood={**common,'split_sha256':hashed(split),'role':'ood','seeds':seeds(split['ood'],95)}
    return {'evaluation':evaluation,'split':split,'ood':ood}

def write_fixture(base, data):
    spec={'schema_version':1}
    for key,value in data.items():
        blob=encoded(value);(base/(key+'.json')).write_bytes(blob)
        spec[key]={'path':key+'.json','sha256':hashlib.sha256(blob).hexdigest()}
    (base/'input.json').write_bytes(encoded(spec));return spec

class PlotContract(unittest.TestCase):
    def test_actual_pixels_and_independent_axis_projection(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);data=fixture();spec=write_fixture(base,data);result=worker.render(spec,base,base/'output')
            report=json.loads((base/'output/plot.json').read_text());image=Image.open(base/'output/plot.png').convert('RGB')
            self.assertEqual(image.size,(1600,1000));self.assertTrue(report['synthetic_fixture']);self.assertFalse(report['useful_acceleration'])
            self.assertEqual(len(report['coverage']['points']),32);self.assertIsNone(report['coverage']['ood']['prediction'])
            self.assertEqual(report['coverage']['ood']['mean_bar'],95)
            # Independent affine equations, without calling worker's mapper.
            for point in report['held_out']['points']:
                xmin,xmax,ymin,ymax=report['held_out']['axis_domain'];left,top,right,bottom=report['held_out']['plot_rect']
                def xy(x,y):return [left+(x-xmin)/(xmax-xmin)*(right-left),bottom-(y-ymin)/(ymax-ymin)*(bottom-top)]
                pairs=[(point['center_px'],xy(point['pressure_bar'],point['prediction_bar']))]
                pairs.extend((actual,xy(value,point['prediction_bar'])) for actual,value in zip(point['horizontal_endpoints_px'],point['seed_t95_bar']))
                pairs.extend((actual,xy(point['pressure_bar'],value)) for actual,value in zip(point['vertical_endpoints_px'],point['interval_bar']))
                rgb=tuple(int(point['color'][i:i+2],16) for i in (1,3,5))
                for actual,expected in pairs:
                    self.assertLess(max(abs(a-b) for a,b in zip(actual,expected)),1e-9)
                    x,y=map(round,actual)
                    self.assertTrue(any(image.getpixel((x+dx,y+dy))==rgb for dx in range(-2,3) for dy in range(-2,3)),(point['condition_index'],actual))
            for artifact in result['artifacts']:
                self.assertEqual(hashlib.sha256((base/'output'/artifact['path']).read_bytes()).hexdigest(),artifact['sha256'])

    def test_changed_source_hash_and_nonfinite_json_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);spec=write_fixture(base,fixture());(base/'ood.json').write_bytes(b'{}')
            with self.assertRaisesRegex(ValueError,'SHA-256'):worker.render(spec,base,base/'output')
            raw=b'{"schema_version":1,"value":NaN}';(base/'ood.json').write_bytes(raw);spec['ood']['sha256']=hashlib.sha256(raw).hexdigest()
            with self.assertRaisesRegex(ValueError,'Non-finite'):worker.render(spec,base,base/'output')

    def test_identity_dimension_and_uncertainty_tampering_are_rejected(self):
        changes=[lambda d:d['ood'].update(study_id='other-study'),lambda d:d['ood']['seeds'].pop(),lambda d:d['evaluation']['cases'].pop(),lambda d:d['evaluation']['cases'][0].update(seed_se_bar=999),lambda d:d['evaluation']['cases'][0].update(interval_bar=[0,1]),lambda d:d['evaluation']['cases'][0].update(density_g_cm3=.9),lambda d:d['evaluation'].update(useful_acceleration=True),lambda d:d['ood']['seeds'][1].update(seed=d['ood']['seeds'][0]['seed'])]
        for change in changes:
            with self.subTest(change=change),tempfile.TemporaryDirectory() as directory:
                data=fixture();change(data);base=Path(directory);spec=write_fixture(base,data)
                with self.assertRaises(ValueError):worker.render(spec,base,base/'output')

if __name__=='__main__':unittest.main()
