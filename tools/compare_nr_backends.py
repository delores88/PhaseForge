#!/usr/bin/env python3
"""Compare pinned CPU/CUDA native output; never certify physical accuracy.

Requires independently retained nr_black_hole_result directories and exact result
SHA256s. No engine runs, interpolation, reference fitting, or tolerance overrides.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import math
from pathlib import Path
import sys
import tempfile

# Fixed sibling tools are part of the source-pinned comparator, also under -I.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import numpy as np
import athenak_decode as decoder
import nr_black_hole_result as retained


# Accepted by root before any actual GPU data on 2026-09-12. Do not retune after
# seeing an execution. A change would be a newly preregistered comparator version.
CRITERIA = {
    'schema': 'phaseforge.nr-backend-agreement-criteria.v1',
    'preregistered_date': '2026-09-12',
    'native_output_precision': 'IEEE754 binary32', 'evolution_precision': 'double',
    'maximum_absolute_difference': {'float32_epsilon_multiplier': 64, 'scale_floor': 1.0},
    'rms_difference': {'relative_multiplier': 2e-4, 'rms_scale_floor': 1e-6},
    'derived_constraint_rms': {'absolute_floor': 1e-10, 'relative_multiplier': 1e-3},
    'minimum_exact_common_times': 2, 'requires_nonzero_evolution': True,
    'positive_physical_metric_required': True,
    'rationale': 'Native float32 serialization is coarser than double evolution. The absolute bound allows 64 float32 epsilons at the field peak (minimum unit code scale); the RMS bound limits bulk disagreement even when an extreme cell sets that peak. A small RMS floor avoids unstable relative division for nearly zero fields. ULP and sign changes remain separate diagnostics. These conservative engineering tolerances have no physical-accuracy interpretation.',
    'scope': 'Computational agreement at exact retained common states only. Not convergence, horizon validation, boost calibration, physical accuracy, or proof of per-process GPU kernel execution.',
}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), allow_nan=False)+'\n').encode()


def criteria_sha256():
    return hashlib.sha256(canonical(CRITERIA)).hexdigest()


def load_retained(directory, result_sha256, expected_backend):
    root = Path(directory)
    preflight = retained.parse(retained.read(root/'result.json',result_sha256))
    artifacts = preflight.get('artifacts')
    require(isinstance(artifacts,list) and len(artifacts)<=4096
            and all(type(row.get('bytes')) is int and 0<=row['bytes']<=retained.MAX_TOTAL for row in artifacts), 'Unbounded retained artifact inventory')
    require(sum(row['bytes'] for row in artifacts)<=2*retained.MAX_TOTAL
            and sum(row['bytes'] for row in artifacts if row.get('path','').startswith('native/'))<=retained.MAX_TOTAL,
            'Retained native sources exceed 1 GiB or derived artifacts exceed 2 GiB')
    result = retained.read_result(root, result_sha256)
    require(result.get('units') == retained.UNITS, 'Retained NR code units differ from the supported diagnostic contract')
    inventory = {row['path']: row for row in result['artifacts']}
    require(len(inventory) == len(result['artifacts']), 'Duplicate retained artifact')
    def source(name):
        row = inventory.get('sources/'+name)
        require(row is not None, 'Missing pinned source '+name)
        return retained.read(root/row['path'], row['sha256'])
    receipt = retained.parse(source('execution-receipt.json'))
    manifest = retained.parse(source('engine.json'))
    request = retained.parse(source('request.json'))
    input_raw = source('input.athinput')
    require(retained.sha(source('execution-receipt.json')) == result['sources']['receipt_sha256'], 'Source receipt/result pin differs')
    require(retained.sha(source('engine.json')) == receipt['engine_manifest_sha256'] == result['sources']['manifest_sha256'], 'Source manifest pin differs')
    require(retained.sha(source('request.json')) == receipt['request_sha256'] == result['sources']['request_sha256'], 'Source request pin differs')
    require(retained.sha(input_raw) == result['sources']['input_sha256'], 'Source input/result pin differs')
    retained.identity(receipt, manifest, request, input_raw)
    require(manifest['engine_id'] == ('athenak_two_punctures_serial' if expected_backend == 'cpu' else 'athenak_two_punctures_cuda'), 'Unexpected comparison backend')
    require(result['job_id'] == receipt['job_id'] and result['engine_identity'] == manifest, 'Result execution identity differs')
    frame_pin = result['frame_inventory']; require(inventory.get(frame_pin['path']) == frame_pin, 'Frame index outside retained inventory')
    index = retained.parse(retained.read(root/frame_pin['path'], frame_pin['sha256']))
    require(index.get('schema') == 'phaseforge.nr-amr-frame-inventory.v1' and index.get('interpolation') == 'none', 'Unsupported native inventory')
    require(len(index['frames']) <= retained.MAX_FRAMES*2, 'Native frame inventory exceeds bound')
    frames = {}
    receipt_files = {row['path']: row for row in receipt['files']}
    for row in index['frames']:
        time = row.get('time'); cycle = row.get('cycle'); kind = row.get('kind')
        require(type(time) in (int,float) and math.isfinite(time) and time >= 0 and row.get('scientific_time') == time
                and type(cycle) is int and cycle >= 0 and kind in ('metric','constraints'), 'Malformed native frame identity')
        key = (time, cycle, kind); require(key not in frames, 'Duplicate native frame identity')
        native = row['native']; require(inventory.get(native['path']) == native and native['path'].startswith('native/'), 'Native pin outside artifact inventory')
        original = receipt_files.get(native['path'][7:])
        require(original is not None and original['sha256'] == native['sha256'] and original['bytes'] == native['bytes'], 'Native frame differs from execution receipt')
        frames[key] = native
    return {'root': root, 'result': result, 'receipt': receipt, 'manifest': manifest, 'input': input_raw,
            'frames': frames, 'result_sha256': result_sha256, 'request': request}


def decode_snapshot(run, key, scratch, tag):
    descriptor = run['frames'][key]
    destination = Path(scratch)/f'{tag}.bin'
    retained.snapshot_file(run['root']/descriptor['path'], destination, descriptor)
    try:
        frame = decoder.read_binary(destination)
        require(frame['sha256'] == descriptor['sha256'] and frame['bytes'] == descriptor['bytes'], 'Decoded native snapshot pin differs')
        require((frame['time'], frame['cycle']) == key[:2], 'Native header/index time differs')
        require(frame['variable_bytes'] == 4, 'Comparator requires original float32 native output')
        expected_input = decoder.parameter_blocks(run['input'].decode('utf-8'))
        # AthenaK appends defaults and runtime/output counters to native headers.
        # Every originally declared setting must still match its exact input.
        require(all(frame['parameters'].get(section,{}).get(name)==value for section,settings in expected_input.items()
                    for name,value in settings.items()), 'Native parameter header changed a declared input setting')
        return frame
    finally:
        # snapshot_file seals our private temporary copy; Windows requires its
        # read-only bit removed before deleting it. Original artifacts stay sealed.
        destination.chmod(0o600)
        destination.unlink()


def ulp_statistics(a, b):
    """Monotone integer float ordering; +0/-0 are numerically equal."""
    aa = np.asarray(a, dtype='<f4'); bb = np.asarray(b, dtype='<f4')
    def ordered(values):
        bits = values.view('<u4').astype('<u8')
        return np.where(bits & 0x80000000, 0x80000000-(bits & 0x7fffffff), 0x80000000+bits).astype('<i8')
    difference = np.abs(ordered(aa)-ordered(bb))
    return {'maximum': int(difference.max()), 'different_values': int(np.count_nonzero(difference)),
            'greater_than_1': int(np.count_nonzero(difference > 1)), 'greater_than_4': int(np.count_nonzero(difference > 4)),
            'greater_than_64': int(np.count_nonzero(difference > 64)),
            'nonzero_sign_changes': int(np.count_nonzero((np.signbit(aa) != np.signbit(bb)) & (aa != 0) & (bb != 0))),
            'signed_zero_changes': int(np.count_nonzero((aa == 0) & (bb == 0) & (np.signbit(aa) != np.signbit(bb))))}


def compare_fields(cpu, cuda):
    for key in ('time','cycle','variables','root_shape','block_shape','nghost','location_bytes','variable_bytes'):
        require(cpu[key] == cuda[key], 'Native geometry/variable/header mismatch: '+key)
    require(cpu['variable_bytes'] == 4, 'Comparator requires native binary32 output')
    others = {tuple(b['logical']): b for b in cuda['blocks']}
    require(len(cpu['blocks']) == len(others) <= retained.MAX_BLOCKS, 'Native block inventory differs')
    fields = {name: {'values': 0, 'max_absolute_difference': 0., 'cpu_max_absolute': 0., 'cuda_max_absolute': 0.,
                     'cpu_squares': [], 'cuda_squares': [], 'difference_squares': [],
                     'ulp': {key:0 for key in ('maximum','different_values','greater_than_1','greater_than_4','greater_than_64','nonzero_sign_changes','signed_zero_changes')}} for name in cpu['variables']}
    for block in cpu['blocks']:
        other = others.get(tuple(block['logical']))
        require(other is not None and block['geometry'] == other['geometry'] and block['index'] == other['index']
                and all(np.array_equal(a,b) for a,b in zip(block['coordinates'],other['coordinates'])), 'Native block coordinates/levels differ')
        for name, stats in fields.items():
            a = block['fields'][name]; b = other['fields'][name]
            require(a.shape == b.shape and np.isfinite(a).all() and np.isfinite(b).all(), 'Nonfinite or inconsistent native field '+name)
            aa, bb = a.astype('<f8'), b.astype('<f8'); difference = aa-bb
            stats['values'] += a.size
            stats['max_absolute_difference'] = max(stats['max_absolute_difference'],float(np.abs(difference).max()))
            stats['cpu_max_absolute'] = max(stats['cpu_max_absolute'],float(np.abs(aa).max()))
            stats['cuda_max_absolute'] = max(stats['cuda_max_absolute'],float(np.abs(bb).max()))
            for label, values in [('cpu',aa),('cuda',bb),('difference',difference)]:
                stats[label+'_squares'].append(math.fsum(float(v)*float(v) for v in values.flat))
            ulp = ulp_statistics(a,b)
            for key,value in ulp.items():
                stats['ulp'][key] = max(stats['ulp'][key],value) if key == 'maximum' else stats['ulp'][key]+value
    for stats in fields.values():
        sums = {label:math.fsum(stats.pop(label+'_squares')) for label in ('cpu','cuda','difference')}
        for label in sums:
            stats[label+'_rms'] = math.sqrt(sums[label]/stats['values'])
        stats['relative_l2_to_cpu'] = math.sqrt(sums['difference'])/math.sqrt(sums['cpu']) if sums['cpu'] > 0 else 0. if sums['difference'] == 0 else None
        stats['relative_l2_denominator_zero'] = sums['cpu'] == 0
        absolute = CRITERIA['maximum_absolute_difference']; rms = CRITERIA['rms_difference']
        stats['absolute_tolerance'] = absolute['float32_epsilon_multiplier']*float(np.finfo(np.float32).eps)*max(absolute['scale_floor'],stats['cpu_max_absolute'],stats['cuda_max_absolute'])
        stats['rms_tolerance'] = rms['relative_multiplier']*max(stats['cpu_rms'],stats['cuda_rms'],rms['rms_scale_floor'])
        stats['passed'] = stats['max_absolute_difference'] <= stats['absolute_tolerance'] and stats['difference_rms'] <= stats['rms_tolerance']
        stats['finite_states'] = True
    return fields


def compare(cpu_directory, cpu_sha256, cuda_directory, cuda_sha256, *, scratch_parent=None):
    cpu = load_retained(cpu_directory,cpu_sha256,'cpu'); cuda = load_retained(cuda_directory,cuda_sha256,'cuda')
    require(cpu['input'] == cuda['input'], 'Computational comparison requires byte-identical scientific input')
    require(cpu['receipt']['job_id'] != cuda['receipt']['job_id'] and cpu['manifest']['executable']['sha256'] != cuda['manifest']['executable']['sha256'], 'CPU/CUDA must be distinct retained executable attempts')
    # identity() separately enforces the three exact upstream source commits for
    # both builds; auxiliary build receipts are allowed to differ by backend.
    ck, gk = set(cpu['frames']), set(cuda['frames']); common = sorted(ck & gk)
    field_rows, reductions, errors = [], [], []
    with tempfile.TemporaryDirectory(prefix='nr-backend-compare-',dir=scratch_parent) as scratch:
        for key in common:
            try:
                a=decode_snapshot(cpu,key,scratch,'cpu'); b=decode_snapshot(cuda,key,scratch,'cuda')
                fields=compare_fields(a,b); del a,b
                field_rows.append({'time':key[0],'cycle':key[1],'kind':key[2],'cpu_native':cpu['frames'][key],
                                   'cuda_native':cuda['frames'][key],'fields':fields,'passed':all(f['passed'] for f in fields.values())})
            except (ValueError,KeyError,OSError) as error:
                errors.append({'time':key[0],'cycle':key[1],'kind':key[2],'error':str(error)})
        paired_times = sorted({key[:2] for key in common if key[2]=='metric' and (*key[:2],'constraints') in common})
        excise = retained.number(decoder.parameter_blocks(cpu['input'].decode('utf-8')).get('z4c',{}).get('excise_chi','0.0625'))
        for time,cycle in paired_times:
            try:
                row={'time':time,'cycle':cycle}
                for label,run in [('cpu',cpu),('cuda',cuda)]:
                    metric=decode_snapshot(run,(time,cycle,'metric'),scratch,'metric'); constraints=decode_snapshot(run,(time,cycle,'constraints'),scratch,'constraints')
                    row[label]=retained.reduce_constraints(metric,constraints,excise); del metric,constraints
                differences={}
                for quantity in ('H_rms','M_rms'):
                    a,b=row['cpu'][quantity],row['cuda'][quantity]; limit=CRITERIA['derived_constraint_rms']; tolerance=max(limit['absolute_floor'],limit['relative_multiplier']*max(abs(a),abs(b)))
                    differences[quantity]={'absolute_difference':abs(a-b),'tolerance':tolerance,'passed':abs(a-b)<=tolerance}
                row.update(differences=differences,passed=all(d['passed'] for d in differences.values()),positive_physical_metrics=True)
                reductions.append(row)
            except (ValueError,KeyError,OSError) as error:
                errors.append({'time':time,'cycle':cycle,'kind':'derived_constraints','error':str(error)})
    sufficient=len(paired_times)>=CRITERIA['minimum_exact_common_times'] and len({t for t,_ in paired_times})>=2 and any(t>0 for t,_ in paired_times)
    agreement=sufficient and not errors and bool(field_rows) and all(r['passed'] for r in field_rows) and len(reductions)==len(paired_times) and all(r['passed'] for r in reductions)
    unmatched=lambda keys:[{'time':t,'cycle':c,'kind':k} for t,c,k in sorted(keys)]
    return {'schema':'phaseforge.nr-backend-comparison.v1','criteria':CRITERIA,'criteria_sha256':criteria_sha256(),
            'units':retained.UNITS,
            'source_pins':{'comparator_sha256':decoder.sha256(Path(__file__)),'decoder_sha256':decoder.sha256(Path(decoder.__file__)),'reduction_reader_sha256':decoder.sha256(Path(retained.__file__))},
            'input_sha256':retained.sha(cpu['input']),
            'cpu':{'result_sha256':cpu_sha256,'job_id':cpu['receipt']['job_id'],'engine':cpu['manifest'],'execution':cpu['result']['execution']},
            'cuda':{'result_sha256':cuda_sha256,'job_id':cuda['receipt']['job_id'],'engine':cuda['manifest'],'execution':cuda['result']['execution']},
            'common_states':[{'time':t,'cycle':c} for t,c in paired_times], 'sufficient_nonzero_common_evolution':sufficient,
            'unmatched_cpu':unmatched(ck-gk),'unmatched_cuda':unmatched(gk-ck),'fields':field_rows,'derived_constraints':reductions,'errors':errors,
            'common_state_agreement':bool(agreement),'full_recorded_trajectory_agreement':bool(agreement and ck==gk),
            'coverage_note':'Extra retained states are coverage differences, not numerical disagreement. Full recorded-trajectory agreement is false whenever either attempt has unmatched states.',
            'physical_accuracy_validated':False,'convergence_validated':False,'physical_boost_validated':False,'kernel_execution_observed':None}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('cpu-result','cpu-result-sha256','cuda-result','cuda-result-sha256','output'):
        p.add_argument('--'+name,required=True)
    args=p.parse_args(); output=Path(args.output)
    require(not output.exists(),'Comparison report must have a new immutable path')
    output.parent.mkdir(parents=True,exist_ok=True)
    try:
        result=compare(args.cpu_result,args.cpu_result_sha256,args.cuda_result,args.cuda_result_sha256,scratch_parent=output.parent)
    except (ValueError,KeyError,OSError) as error:
        result={'schema':'phaseforge.nr-backend-comparison.v1','status':'rejected_evidence','error':str(error),
                'criteria':CRITERIA,'criteria_sha256':criteria_sha256(),'common_state_agreement':False,'full_recorded_trajectory_agreement':False}
    with output.open('xb') as handle:handle.write(canonical(result))
    print(json.dumps({'report':str(output),'sha256':decoder.sha256(output),'common_state_agreement':result['common_state_agreement'],
                      'full_recorded_trajectory_agreement':result['full_recorded_trajectory_agreement']}))
    return 0 if result['common_state_agreement'] else 3


if __name__=='__main__':
    raise SystemExit(main())
