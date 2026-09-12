#!/usr/bin/env python3
"""Publish hash-bound gauge diagnostics from retained AthenaK data, without rerunning it.

Strict CLI: --run WORK --execution-receipt RECEIPT --execution-receipt-sha256 SHA
--output NEW_DIRECTORY. Historical foundation receipts need --legacy-foundation.
Exit 0 means analytic checks pass, 3 means retained numerical checks fail, and 2
means processing/provenance was rejected. A completed solver is a separate fact.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import stat

import numpy as np
import athenak_decode as decoder
import check_athenak_gauge as checker

MAX_NATIVE_BYTES = 64 * 1024**2
MAX_MEMBER_BYTES = 16 * 1024**2
UNITS = {"length":"L", "time":"L/c", "length_scale":1.0, "speed_of_light":1.0,
         "system":"dimensionless geometrized code coordinates; L=1 and c=1", "si_mapping":None}
CHANNELS = {"alpha":{"unit":"1","label":"Lapse","derivation":"retained z4c_alpha"},
            "gxx":{"unit":"1","label":"Physical spatial metric g_xx","derivation":"z4c_gxx / z4c_chi"},
            "Kxx":{"unit":"1/L","label":"Extrinsic curvature K_xx","derivation":"z4c_Axx / z4c_chi + (z4c_Khat + 2*z4c_Theta)*gxx/3"},
            "hamiltonian":{"unit":"1/L^2","label":"Engine Hamiltonian constraint residual","derivation":"retained con_H; engine diagnostic, not an independent residual calculation"}}
SCOPE = "Exact XY cell-centre diagnostic slice of 3D Einstein-equation evolution: flat spacetime in a time-dependent harmonic coordinate gauge. Not a black-hole collision or physical gravitational radiation."


def require(condition, message):
    if not condition: raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def relative(value):
    require(isinstance(value,str) and value and '\\' not in value and ':' not in value, "Expected a strict relative artifact path")
    path=PurePosixPath(value)
    require(not path.is_absolute() and str(path)==value and all(part not in ('.','..') for part in path.parts), "Artifact path escapes its retained root")
    return Path(*path.parts)


def read_plain(path, expected=None, maximum=MAX_MEMBER_BYTES):
    path=Path(path)
    for entry in (path,*path.parents):
        if entry.exists() or entry.is_symlink():
            info=entry.lstat()
            require(not stat.S_ISLNK(info.st_mode) and not getattr(info,'st_file_attributes',0)&0x400, "Linked artifact path")
    info=path.stat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink==1 and info.st_size<=maximum, "Artifact type/link/size is outside the bounded reader")
    with path.open('rb') as stream:raw=stream.read(maximum+1)
    require(len(raw)<=maximum, "Artifact grew beyond its bound")
    if expected is not None:
        require(isinstance(expected,str) and re.fullmatch('[0-9a-f]{64}',expected) and digest(raw)==expected, "Artifact SHA-256 mismatch")
    return raw


def parse(raw):
    def pairs(items):
        result={}
        for key,value in items:
            require(key not in result,"Duplicate JSON field");result[key]=value
        return result
    return json.loads(raw.decode('utf-8'),object_pairs_hook=pairs,parse_constant=lambda value:(_ for _ in ()).throw(ValueError('Nonfinite JSON value')))


def json_file(path,value):
    raw=(json.dumps(value,allow_nan=False,separators=(',',':'))+'\n').encode()
    with path.open('xb') as stream:stream.write(raw)
    return raw


def file_pin(path,root):
    raw=read_plain(path)
    return {'path':path.relative_to(root).as_posix(),'bytes':len(raw),'sha256':digest(raw)}


def normalize_report_paths(report,root):
    report['run']='native'
    for frame in report['frames']:frame['source']['path']=Path(frame['source']['path']).relative_to(root).as_posix()
    for frame in report['constraints']:frame['path']=Path(frame['path']).relative_to(root).as_posix()


def produce(run,execution_receipt,execution_receipt_sha256,output,*,legacy_foundation=False):
    run,output=Path(run),Path(output)
    require(not output.exists() and not output.is_symlink(),"The result directory must be a new immutable attempt")
    require(not output.resolve().is_relative_to(run.resolve()),"Result output cannot be inside its native source tree")
    raw_receipt=read_plain(execution_receipt,execution_receipt_sha256)
    receipt=parse(raw_receipt)
    if legacy_foundation:
        require('schema' not in receipt and type(receipt.get('nx')) is int and 'stopped_reason' in receipt and 'at' in receipt,"Expected an explicit historical foundation receipt")
        execution={'receipt_type':'historical_foundation','job_id':None,'success':receipt.get('exit_code')==0 and receipt.get('stopped_reason') is None,
                   'exit_code':receipt.get('exit_code'),'process_group_drained':None,'note':'Historical execution evidence; no modern supervisor or job identity is invented.'}
    else:
        require(receipt.get('schema')=='phaseforge.nr-process-receipt.v1',"A pinned NR process receipt is required")
        require(receipt.get('engine_id') in ('athenak_gauge_wave','athenak_z4c_gauge_wave'),"This postprocessor only admits the gauge-wave engine identity")
        require(receipt.get('source',{}).get('athenak_commit')==decoder.UPSTREAM_COMMIT,"Unsupported AthenaK source identity")
        execution={'receipt_type':receipt['schema'],'job_id':receipt['job_id'],'success':receipt.get('exit_code')==0 and receipt.get('termination_reason')=='completed' and receipt.get('process_group_drained') is True,
                   'exit_code':receipt.get('exit_code'),'termination_reason':receipt.get('termination_reason'),'process_group_drained':receipt.get('process_group_drained')}
    rows=receipt.get('files');require(isinstance(rows,list) and len(rows)<=4096,"Missing bounded native receipt inventory")
    names=set();native=[];total=0
    for row in rows:
        name=row.get('path');path=relative(name)
        require(name not in names,"Duplicate execution artifact pin");names.add(name)
        if path.suffix!='.bin':continue
        raw=read_plain(run/path,row.get('sha256'))
        require(type(row.get('bytes')) is int and len(raw)==row['bytes'],"Native artifact length mismatch")
        total+=len(raw);require(total<=MAX_NATIVE_BYTES and len(native)<64,"Native gauge snapshot exceeds its bounded size/count")
        native.append((path,raw))
    require(native,"No pinned native metric/constraint states")
    actual={path.relative_to(run).as_posix() for path in run.rglob('*.bin')}
    require(actual=={path.as_posix() for path,_ in native},"Unregistered or missing native frame")
    output.mkdir(parents=True,exist_ok=False);snapshot=output/'native';snapshot.mkdir()
    source_pins=[]
    for path,raw in native:
        destination=snapshot/path;destination.parent.mkdir(parents=True,exist_ok=True)
        with destination.open('xb') as stream:stream.write(raw)
        destination.chmod(0o444)
        source_pins.append({'path':destination.relative_to(output).as_posix(),'bytes':len(raw),'sha256':digest(raw)})
    (output/'sources').mkdir();(output/'sources/execution-receipt.json').write_bytes(raw_receipt)
    # Existing decoder and unchanged checker operate on the same retained snapshot.
    report=checker.check_run(snapshot)
    normalize_report_paths(report,output)
    analytic={'schema':'phaseforge.nr-gauge-analytic.v1','passed':bool(report['passed']),'status':'passed' if report['passed'] else 'failed',
              'limits':{'linf':checker.MAX_ERRORS,'native_hamiltonian_linf':checker.MAX_NATIVE_HAMILTONIAN},'check':report,
              'scope':'Frozen harmonic gauge-wave reference only; no merger or radiation validation.'}
    json_file(output/'analytic.json',analytic)
    metric=[decoder.read_binary(output/frame['source']['path']) for frame in report['frames']]
    constraints=[decoder.read_binary(output/frame['path']) for frame in report['constraints']]
    (output/'blocks').mkdir();(output/'slices').mkdir()
    frames=[];series=[]
    for number,(frame,constraint,checked) in enumerate(zip(metric,constraints,report['frames'])):
        checker.validate_parameters(frame);checker.validate_parameters(constraint)
        require(frame['time']==constraint['time'] and frame['cycle']==constraint['cycle'],"Slice constraint/metric frames do not match")
        block=frame['blocks'][0];con=constraint['blocks'][0]
        for first,second in zip(block['coordinates'],con['coordinates']):require(np.array_equal(first,second),"Constraint grid differs from metric grid")
        x,y,z=block['coordinates'];plane=int(np.argmin(np.abs(z-.5)))
        fields=block['fields'];chi=fields['z4c_chi'].astype('<f8')
        gxx=fields['z4c_gxx'].astype('<f8')/chi
        trace=fields['z4c_Khat'].astype('<f8')+2*fields['z4c_Theta'].astype('<f8')
        full={'alpha':fields['z4c_alpha'].astype('<f8'),'gxx':gxx,
              'Kxx':fields['z4c_Axx'].astype('<f8')/chi+trace*gxx/3,'hamiltonian':con['fields']['con_H'].astype('<f8')}
        require(all(np.isfinite(values).all() for values in full.values()),"Nonfinite reconstructed channel")
        block_file=output/f'blocks/metric-{number:05d}.npz'
        np.savez(block_file,x1=x,x2=y,x3=z,**fields)
        con_file=output/f'blocks/constraints-{number:05d}.npz'
        np.savez(con_file,x1=con['coordinates'][0],x2=con['coordinates'][1],x3=con['coordinates'][2],**con['fields'])
        sliced={key:values[plane,:,:] for key,values in full.items()}
        data_file=output/f'slices/frame-{number:05d}.npz';np.savez(data_file,x=x,y=y,z=np.array([z[plane]],dtype='<f8'),**sliced)
        json_path=output/f'slices/frame-{number:05d}.json'
        json_file(json_path,{'schema':'phaseforge.nr-diagnostic-slice-data.v1','x':x.tolist(),'y':y.tolist(),'z':float(z[plane]),
                             'shape':[len(y),len(x)],'axis_order':['y','x'],'channels':{key:values.tolist() for key,values in sliced.items()}})
        source=checked['source'];source_constraint=report['constraints'][number]
        row={'index':number,'time':frame['time'],'scientific_time':frame['time'],'cycle':frame['cycle'],'time_unit':UNITS['time'],
             'plane':{'axes':['x','y'],'normal_axis':'z','index':plane,'coordinate':float(z[plane]),'coordinate_unit':'L','selection':'retained cell centre nearest z=0.5; lower centre on a tie'},
             'shape':[len(y),len(x)],'source_frame':source,'source_constraint_frame':{'path':source_constraint['path'],'sha256':source_constraint['sha256']},
             'source_block':{'logical':list(block['logical']),'index':list(block['index']),'geometry':list(block['geometry']),
                             'metric':file_pin(block_file,output),'constraints':file_pin(con_file,output),'native_variable_bytes':frame['variable_bytes'],
                             'encoding':'Decoded original 3D block in NPZ at native field precision; original coordinates retained. These hashes identify the decoded block artifacts; source_frame pins the original binary bytes.'},
             'arrays':file_pin(data_file,output),'data':file_pin(json_path,output),'analytic_passed':checked['passed']}
        frames.append(row)
        series.append({'time':frame['time'],'cycle':frame['cycle'],'analytic_passed':checked['passed'],'errors':checked['errors'],
                       'engine_hamiltonian':{key:source_constraint[key] for key in ('hamiltonian_l1','hamiltonian_linf','provenance')},
                       'channels_3d':{key:{'min':float(values.min()),'max':float(values.max()),'mean':math.fsum(float(value) for value in values.flat)/values.size,'unit':CHANNELS[key]['unit']} for key,values in full.items()}})
    index={'schema':'phaseforge.nr-diagnostic-slices.v1','representation':'nr_diagnostic_slice','scope':SCOPE,'units':UNITS,'channels':CHANNELS,
           'axis_order':['y','x'],'grid_location':'cell_center','interpolation':'none','primary_channel':'alpha','frame_count':len(frames),
           'initial_time':frames[0]['time'],'final_time':frames[-1]['time'],'time_unit':UNITS['time'],'frames':frames,
           'black_hole_collision':False,'physical_radiation':False}
    json_file(output/'slices/index.json',index)
    json_file(output/'measurements.json',{'schema':'phaseforge.nr-gauge-measurements.v1','units':UNITS,'scope':SCOPE,'series':series})
    # Never certify arrays if the decoder/checker's retained source changed.
    for item in source_pins:read_plain(output/item['path'],item['sha256'])
    artifacts=[file_pin(path,output) for path in sorted(output.rglob('*')) if path.is_file()]
    result={'schema':'phaseforge.nr-gauge-result.v1','engine':'athenak_gauge_wave','scientific_scope':SCOPE,'units':UNITS,'execution':execution,
            'analytic':{'status':analytic['status'],'passed':analytic['passed'],'report':file_pin(output/'analytic.json',output)},
            'fulfillment':{'gauge_reference_validated':execution['success'] and analytic['passed'],'black_hole_collision_validated':False,'physical_radiation':False},
            'timeframes':frames,'frame_count':len(frames),'initial_time':frames[0]['time'],'final_time':frames[-1]['time'],
            'slice_index':file_pin(output/'slices/index.json',output),'measurements':file_pin(output/'measurements.json',output),
            'sources':{'execution_receipt':file_pin(output/'sources/execution-receipt.json',output),'native_frames':source_pins,
                       'postprocessor_sha256':decoder.sha256(Path(__file__)),'decoder_sha256':decoder.sha256(Path(decoder.__file__)),
                       'checker_sha256':decoder.sha256(Path(checker.__file__)),'upstream_format_commit':decoder.UPSTREAM_COMMIT},'artifacts':artifacts}
    json_file(output/'result.json',result)
    return result


def read_result(directory,expected_sha256):
    """Re-read retained data and every artifact pin; no expressions or code execute."""
    root=Path(directory);result=parse(read_plain(root/'result.json',expected_sha256))
    require(result.get('schema')=='phaseforge.nr-gauge-result.v1',"Unsupported result schema")
    names=set()
    for item in result['artifacts']:
        path=relative(item['path']);require(item['path'] not in names,"Duplicate result artifact");names.add(item['path'])
        raw=read_plain(root/path,item['sha256']);require(len(raw)==item['bytes'],"Result artifact length mismatch")
    return result


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--run',type=Path,required=True);parser.add_argument('--execution-receipt',type=Path,required=True)
    parser.add_argument('--execution-receipt-sha256',required=True);parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--legacy-foundation',action='store_true');args=parser.parse_args()
    try:
        result=produce(args.run,args.execution_receipt,args.execution_receipt_sha256,args.output,legacy_foundation=args.legacy_foundation)
        print(json.dumps({'result':str(args.output/'result.json'),'result_sha256':decoder.sha256(args.output/'result.json'),
                          'execution_success':result['execution']['success'],'analytic_passed':result['analytic']['passed'],'frame_count':result['frame_count']}))
        return 0 if result['analytic']['passed'] and result['execution']['success'] else 3
    except (OSError,ValueError,KeyError,TypeError) as error:
        print(json.dumps({'error':str(error),'result_published':False}));return 2


if __name__=='__main__':
    raise SystemExit(main())
