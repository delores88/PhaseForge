#!/usr/bin/env python3
"""Read pinned TwoPunctures/AthenaK artifacts. No evolution or boost certification.

CLI matches nr_gauge_result; engine.json, input.athinput and request.json default
to siblings of the execution receipt. Output must be a new directory. Exit 0
means process AND retained time target complete, 3 partial, 2 rejected provenance.
Neither exit 0 nor finite diagnostics certifies a horizon, merger or accuracy.
"""
from __future__ import annotations
import argparse
from datetime import datetime
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import stat
import uuid

import numpy as np
import athenak_decode as decoder

MAX_TOTAL = 1024**3
MAX_MEMBERS = 4096
MAX_TEXT = 16 * 1024**2
MAX_FRAMES = 128
MAX_BLOCKS = 65536
MAX_SLICE_CELLS = 262144
SLICE_CHANNELS = {
    'chi': {'label': 'Conformal factor chi', 'unit': '1', 'source': 'z4c_chi'},
    'lapse': {'label': 'Lapse alpha', 'unit': '1', 'source': 'z4c_alpha'},
    'hamiltonian': {'label': 'Hamiltonian constraint', 'unit': '1/L^2', 'source': 'con_H'},
}
KOKKOS = '6739bc623081648af9e752b616d9671527922cbf'
TWOPUNCTURES = 'ec563aeb672235b9443c330f9cde65f7246e8ea4'
ENGINES = {'athenak_two_punctures_serial', 'athenak_two_punctures_cuda'}
UNITS = {'system': 'geometrized input code units, G=c=1; no SI conversion',
         'length': 'L', 'time': 'L/c', 'mass': 'c^2 L/G', 'H_rms': '1/L^2',
         'M_rms': '1/L^2', 'proper_volume': 'L^3', 'expansion_rms': '1/L',
         'rPsi4': '1/L', 'si_mapping': None,
         'normalization': 'Input coordinates; target puncture-end ADM masses are not validated horizon rest masses or total ADM energy.'}
SCOPE = 'Retained 3D vacuum Einstein evolution with Bowen-York puncture initial data; diagnostics only. Physical boost, merger and numerical convergence are not certified.'
DIAGNOSTIC_LABEL_SEMANTICS = 'pre_increment_mesh_time_for_final_RK_stage_state'
DIAGNOSTIC_TIME_MEANING = (
    'Native diagnostic label from mesh time before the final RK state advances the time counter. '
    'It is not a verified evolved-state timestamp; label 0 is not initial-state evidence. '
    'No nominal timestep offset or match to retained field times is inferred.'
)


def require(ok, message):
    if not ok:
        raise ValueError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def hash_string(value):
    require(isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value), 'Missing exact SHA256 pin')
    return value


def relative(value):
    require(isinstance(value, str) and value and '\\' not in value and ':' not in value, 'Unsafe relative path')
    path = PurePosixPath(value)
    require(not path.is_absolute() and str(path) == value and all(p not in ('.', '..') for p in path.parts), 'Unsafe relative path')
    return Path(*path.parts)


def plain(path, maximum=MAX_TOTAL):
    path = Path(path)
    for parent in (path, *path.parents):
        info = parent.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400, 'Linked source path')
    info = path.stat()
    require(stat.S_ISREG(info.st_mode) and info.st_nlink == 1 and info.st_size <= maximum, 'Unbounded, linked or non-file source')
    return info


def pin(path, root=None):
    info = plain(path)
    return {'path': Path(path).relative_to(root).as_posix() if root else str(path),
            'bytes': info.st_size, 'sha256': decoder.sha256(Path(path))}


def read(path, expected=None, maximum=MAX_TEXT):
    plain(path, maximum)
    with Path(path).open('rb') as handle:
        raw = handle.read(maximum + 1)
    require(len(raw) <= maximum, 'Source grew past read bound')
    if expected is not None:
        require(sha(raw) == hash_string(expected), 'Source SHA256 mismatch')
    return raw


def parse(raw):
    def pairs(items):
        result = {}
        for key, value in items:
            require(key not in result, 'Duplicate JSON field')
            result[key] = value
        return result
    return json.loads(raw.decode('utf-8'), object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('Nonfinite JSON')))


def save(path, value):
    with Path(path).open('xb') as handle:
        handle.write((json.dumps(value, allow_nan=False, separators=(',', ':')) + '\n').encode())


def snapshot_file(source, target, expected):
    before = plain(source)
    require(type(expected.get('bytes')) is int and before.st_size == expected['bytes'], 'Source length mismatch')
    digest = hashlib.sha256()
    count = 0
    target.parent.mkdir(parents=True, exist_ok=True)
    with source.open('rb') as src, target.open('xb') as dst:
        for chunk in iter(lambda: src.read(1024**2), b''):
            count += len(chunk)
            require(count <= expected['bytes'], 'Source grew during snapshot')
            digest.update(chunk)
            dst.write(chunk)
    require(count == expected['bytes'] and digest.hexdigest() == hash_string(expected.get('sha256')), 'Source SHA256 mismatch')
    target.chmod(0o444)


def number(value):
    result = float(value)
    require(math.isfinite(result), 'Nonfinite numeric parameter')
    return result


def identity(receipt, manifest, request, input_raw):
    version = 2 if request.get('schema') == 'phaseforge.nr-request.v2' else 1
    require(receipt.get('schema') == f'phaseforge.nr-process-receipt.v{version}', 'Unsupported supervisor receipt')
    require(manifest.get('schema') == 'phaseforge.nr-engine.v1' and request.get('schema') == f'phaseforge.nr-request.v{version}', 'Unsupported engine/request schema')
    require(receipt.get('engine_id') in ENGINES and manifest.get('engine_id') == receipt['engine_id'] == request.get('engine_id'), 'Unsupported or mismatched TwoPunctures engine identity')
    require(str(uuid.UUID(receipt['job_id'])) == receipt['job_id'] == request['job_id'], 'Job identity mismatch')
    require(manifest.get('source') == receipt.get('source') and manifest.get('build') == receipt.get('build'), 'Source/build identity mismatch')
    source = receipt['source']
    require(source.get('athenak', source.get('athenak_commit')) == decoder.UPSTREAM_COMMIT
            and source.get('kokkos', source.get('kokkos_commit')) == KOKKOS
            and source.get('twopunctures', source.get('twopunctures_commit')) == TWOPUNCTURES, 'Unsupported source pin')
    require(isinstance(receipt['build'], dict) and bool(receipt['build']), 'Missing actual build metadata')
    build = receipt['build']
    if receipt['engine_id'].endswith('_cuda'):
        require(build.get('backend') == 'CUDA' and build.get('precision') == 'double'
                and build.get('problem') == 'z4c/two_punctures/z4c_two_puncture'
                and bool(build.get('cuda_arch')) and bool(build.get('cuda_version')), 'CUDA build metadata is incomplete or inconsistent')
    else:
        legacy = build.get('mode') == 'CPU serial double'
        current = build.get('backend') in ('Serial', 'CPU') and build.get('precision') == 'double' and build.get('problem') == 'z4c/two_punctures/z4c_two_puncture'
        require(legacy or current, 'CPU build metadata is incomplete or inconsistent')
    for key in ('path', 'sha256'):
        require(manifest.get('executable', {}).get(key) == receipt.get('executable', {}).get(key), 'Executable identity mismatch')
    hash_string(receipt['executable']['sha256'])
    require(type(receipt['executable'].get('bytes')) is int and receipt['executable']['bytes'] > 0, 'Missing executable byte receipt')
    require(sha(input_raw) == hash_string(receipt.get('input', {}).get('sha256')) == request.get('input', {}).get('sha256'), 'Input pin mismatch')
    require(len(input_raw) == receipt['input'].get('bytes') and receipt['input'].get('path') == request['input'].get('path'), 'Input binding mismatch')
    for key in ('cpu_threads', 'memory_limit_bytes', 'output_limit_bytes'):
        require(request.get(key) == receipt.get(key), 'Resource request/receipt mismatch')
    # Offset spelling can differ between Windows and Linux, but Off cannot change.
    rd, qd = receipt.get('deadline_at'), request.get('deadline_at')
    require((rd is None and qd is None) or (isinstance(rd, str) and isinstance(qd, str)
            and datetime.fromisoformat(rd.replace('Z', '+00:00')) == datetime.fromisoformat(qd.replace('Z', '+00:00'))), 'Deadline mismatch')
    resource_identity(receipt, manifest, request, version)


def resource_identity(receipt, manifest, request, version):
    cuda = receipt['engine_id'].endswith('_cuda')
    if version == 1:
        require(not cuda and 'execution_policy' not in request and 'execution_policy' not in receipt,
                'Legacy v1 receipts cannot admit CUDA execution')
        return
    policy = request.get('execution_policy')
    require(isinstance(policy, dict) and policy == receipt.get('execution_policy')
            and set(policy) == {'backend', 'memory_boundary', 'address_space_limit_bytes', 'tasks_max', 'gpu_vram_policy'}, 'Execution policy binding differs')
    require(policy['backend'] == ('cuda' if cuda else 'cpu') and policy['memory_boundary'] == 'systemd_cgroup_v2'
            and policy['address_space_limit_bytes'] is None, 'Invalid physical-memory execution policy')
    tasks = policy['tasks_max']; require(type(tasks) is int and 8 <= tasks <= 128, 'Kernel task cap differs')
    gpu = receipt.get('gpu_vram')
    require(isinstance(gpu, dict) and gpu.get('policy') == policy['gpu_vram_policy']
            and gpu.get('hard_limit_enforced') is False and gpu.get('attributable_process_usage') is None
            and gpu.get('kernel_execution_observed') is None, 'Unsupported GPU attribution or enforcement claim')
    if cuda:
        cuda_identity(receipt, manifest, request)
    else:
        require(manifest['build'].get('backend') == 'Serial' and policy['gpu_vram_policy'] == {'mode': 'not_admitted'} and gpu.get('execution_admitted') is False,
                'CPU receipt cannot claim GPU admission')
    boundary = receipt.get('memory_boundary')
    if boundary is None:
        require(cuda and gpu.get('execution_admitted') is False and receipt.get('engine_pid') is None
                and receipt.get('termination_reason') != 'completed', 'Executed v2 job has no workload memory boundary')
        return  # A rejected GPU pre-gate attempt has no workload to promote.
    unit = f"phaseforge-nr-{receipt['job_id']}.scope"; group = '/system.slice/'+unit
    require(isinstance(boundary, dict) and boundary.get('kind') == 'systemd_cgroup_v2' and boundary.get('unit') == unit
            and boundary.get('path') == group and boundary.get('child_cgroup_before_exec') == group
            and type(boundary.get('inode')) is int and boundary['inode'] > 0, 'Workload memory scope identity differs')
    for owner in ('supervisor_cgroup', 'guardian_cgroup'):
        location = boundary.get(owner)
        require(isinstance(location, str) and location.startswith('/') and location != group and not location.startswith(group+'/'),
                'Supervisor/guardian must remain outside the workload memory scope')
    require(boundary.get('effective_kernel_limits') == {'memory.max': request['memory_limit_bytes'], 'memory.swap.max': 0,
                                                       'pids.max': tasks, 'memory.oom.group': 1}
            and boundary.get('cgroup_drained') is True and 'address_space_limit_bytes' in boundary
            and boundary['address_space_limit_bytes'] is None, 'Kernel memory limits or workload drainage differ')


def cuda_identity(receipt, manifest, request):
    policy = request['execution_policy']['gpu_vram_policy']; gpu = receipt['gpu_vram']
    require(isinstance(policy, dict) and set(policy) == {'mode', 'device_uuid', 'maximum_device_used_growth_bytes', 'minimum_free_bytes', 'poll_interval_seconds'}, 'Invalid GPU policy shape')
    require(policy['mode'] == 'device_wide_soft_guard' and isinstance(policy['device_uuid'], str)
            and re.fullmatch(r'GPU-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}', policy['device_uuid']), 'GPU UUID/policy differs')
    require(type(policy['minimum_free_bytes']) is int and 2*1024**3 <= policy['minimum_free_bytes'] <= 1024**4
            and type(policy['maximum_device_used_growth_bytes']) is int and 0 < policy['maximum_device_used_growth_bytes'] <= 1024**4
            and type(policy['poll_interval_seconds']) in (int, float) and policy['poll_interval_seconds'] == 1, 'Invalid device-wide GPU limits')
    runtime = manifest['build'].get('cuda_runtime')
    require(isinstance(runtime, dict) and runtime.get('schema') == 'phaseforge.nr-cuda-runtime.v1'
            and gpu.get('guard_source_sha256') == hash_string(runtime.get('guard_source_sha256')), 'GPU guard source pin differs')
    admitted = gpu.get('execution_admitted'); require(type(admitted) is bool, 'GPU admission outcome is missing')
    if not admitted:
        require(receipt.get('termination_reason') != 'completed'
                and not any(row.get('path', '').endswith('.bin') for row in receipt.get('files', [])),
                'An unadmitted GPU attempt cannot contain numerical evolution states')
        return
    snapshot = receipt.get('cuda_runtime_snapshot'); libraries = runtime.get('libraries')
    require(isinstance(snapshot, dict) and isinstance(libraries, list) and len(libraries) == 1
            and snapshot.get('library') == libraries[0] and snapshot.get('source_directory') == runtime.get('library_directory')
            and snapshot.get('driver_directory') == runtime.get('driver_directory') == '/usr/lib/wsl/lib'
            and snapshot.get('build_receipt') == runtime.get('build_receipt'), 'CUDA runtime snapshot provenance differs')
    library = libraries[0]
    require(isinstance(library, dict) and isinstance(runtime.get('build_receipt'), dict), 'CUDA runtime/build descriptors are invalid')
    hash_string(library.get('sha256')); hash_string(runtime['build_receipt'].get('sha256'))
    require(library.get('name') == 'libcudart.so.12.9.79' and type(library.get('bytes')) is int and 0 < library['bytes'] <= 32*1024**2
            and library.get('aliases') == {'libcudart.so.12': 'libcudart.so.12.9.79', 'libcudart.so': 'libcudart.so.12'}, 'CUDA runtime library identity differs')
    require(isinstance(request.get('output_dir'), str), 'CUDA workload path is missing')
    work = PurePosixPath(request['output_dir'])
    require(work.is_absolute() and str(work) == request['output_dir'] and work.name == 'work' and work.parent.name == receipt['job_id']
            and all(part not in ('.', '..') for part in work.parts), 'CUDA workload path identity differs')
    directory = str(work.parent/'cuda-runtime')
    require(snapshot.get('directory') == directory and snapshot.get('ld_library_path') == directory+':/usr/lib/wsl/lib', 'CUDA loader snapshot is outside the admitted job')
    require(type(gpu.get('sample_count')) is int and gpu['sample_count'] >= 2, 'GPU launch has no two admission observations')
    baseline = gpu.get('baseline'); before = gpu.get('before_launch')
    for sample, decision in [(baseline, gpu.get('admission_guard')), (before, gpu.get('before_launch_guard'))]:
        require(isinstance(sample, dict) and sample.get('schema') == 'phaseforge.nr-gpu-sample.v1' and sample.get('status') == 'known'
                and sample.get('device_uuid') == policy['device_uuid'] and sample.get('scope') == 'device_wide_soft_guard'
                and sample.get('returncode') == 0, 'GPU pre-launch observation is unavailable or mismatched')
        total = sample.get('total_bytes')
        require(type(total) is int and total > 0 and all(type(sample.get(k)) is int and 0 <= sample[k] <= total for k in ('used_bytes','free_bytes','reserved_bytes')),
                'GPU pre-launch quantities are invalid')
        require(type(sample.get('query_duration_seconds')) in (int, float) and 0 <= sample['query_duration_seconds'] <= 1
                and type(sample.get('observed_monotonic')) in (int, float) and math.isfinite(sample['observed_monotonic']), 'GPU observation timing is invalid')
        growth = sample['used_bytes'] - baseline['used_bytes']
        require(sample['free_bytes'] >= policy['minimum_free_bytes'] and growth <= policy['maximum_device_used_growth_bytes']
                and sample['total_bytes'] == baseline['total_bytes'] and sample['observed_monotonic'] >= baseline['observed_monotonic'], 'GPU before-gate memory admission was not satisfied')
        require(isinstance(decision, dict) and decision.get('allowed') is True and decision.get('reason') is None
                and decision.get('scope') == 'device_wide_soft_guard' and decision.get('hard_vram_quota') is False
                and decision.get('process_attribution') is False and decision.get('device_used_growth_bytes') == growth, 'GPU launch guard did not admit the observed device')


def metadata(frame):
    blocks = []
    for block in frame['blocks']:
        blocks.append({'logical': list(block['logical']), 'native_logical_level': block['logical'][3],
                       'index': list(block['index']), 'geometry': list(block['geometry']),
                       'coordinates': [axis.tolist() for axis in block['coordinates']],
                       'shape_zyx': list(next(iter(block['fields'].values())).shape)})
    return {key: frame[key] for key in ('time', 'cycle', 'variables', 'root_shape', 'block_shape', 'nghost', 'location_bytes', 'variable_bytes')} | {
        'blocks': blocks, 'block_count': len(blocks), 'active_cells': sum(math.prod(b['shape_zyx']) for b in blocks),
        'all_native_values_finite': True, 'interpolation': 'none', 'grid_location': 'cell_center',
        'level_semantics': 'Native logical octree level; do not relabel as relative refinement depth.',
        'channel_ranges': {name: {'min': min(float(b['fields'][name].min()) for b in frame['blocks']),
                                  'max': max(float(b['fields'][name].max()) for b in frame['blocks'])} for name in frame['variables']}}


def reduce_constraints(metric, constraints, excise_chi):
    require(metric['time'] == constraints['time'] and metric['cycle'] == constraints['cycle'], 'Metric/constraint time mismatch')
    cb = {b['logical']: b for b in constraints['blocks']}
    require(len(cb) == len(metric['blocks']) <= MAX_BLOCKS, 'Metric/constraint block mismatch')
    hsum, msum, vsum, kept, valid_metric = [], [], [], 0, True
    for block in metric['blocks']:
        other = cb.get(block['logical'])
        require(other is not None and block['index'] == other['index'] and block['geometry'] == other['geometry'], 'Metric/constraint geometry mismatch')
        active_index = tuple(value for axis, size in enumerate(metric['block_shape'])
                             for value in ((metric['nghost'] if axis == 0 or size > 1 else 0),
                                           (metric['nghost'] if axis == 0 or size > 1 else 0)+size-1))
        require(block['index'] == active_index, 'Constraint reduction requires complete active cells without ghost zones')
        f = block['fields']; cf = other['fields']; chi = f['z4c_chi'].astype('<f8')
        require((chi > 0).all(), 'Nonpositive retained chi')
        xx, xy, xz, yy, yz, zz = [f['z4c_g' + a].astype('<f8') / chi for a in ('xx', 'xy', 'xz', 'yy', 'yz', 'zz')]
        det = xx*yy*zz + 2*xy*xz*yz - xx*yz*yz - yy*xz*xz - zz*xy*xy
        valid_metric &= bool((xx > 0).all() and (xx*yy - xy*xy > 0).all() and (det > 0).all())
        require(valid_metric and np.isfinite(det).all(), 'Nonpositive physical spatial metric')
        mask = chi >= excise_chi
        cellvol = math.prod((block['geometry'][2*a+1] - block['geometry'][2*a]) / metric['block_shape'][a] for a in range(3))
        vol = cellvol * np.sqrt(det[mask]); h = cf['con_H'][mask].astype('<f8'); m = cf['con_M'][mask].astype('<f8')
        require((m >= 0).all(), 'Negative retained squared momentum contraction')
        kept += int(mask.sum())
        vsum.append(math.fsum(float(x) for x in vol.flat))
        hsum.append(math.fsum(float(x) for x in (vol*h*h).flat))
        msum.append(math.fsum(float(x) for x in (vol*m).flat))
    volume, h2, m2 = math.fsum(vsum), math.fsum(hsum), math.fsum(msum)
    require(volume > 0 and all(math.isfinite(v) for v in (volume, h2, m2)), 'Invalid masked constraint reduction')
    return {'time': metric['time'], 'cycle': metric['cycle'], 'mask': {'field': 'z4c_chi', 'operator': '>=', 'value': excise_chi},
            'included_cells': kept, 'proper_volume': volume, 'H_integral': h2, 'M_integral': m2,
            'H_rms': math.sqrt(h2/volume), 'M_rms': math.sqrt(m2/volume), 'units': UNITS,
            'positive_physical_spatial_metric': valid_metric, 'accuracy_accepted': None,
            'method': 'Scalar math.fsum, physical metric determinant and retained con_H / already-squared con_M; engine residuals, not independent derivative checks.'}


def diagnostic_slice(metric, constraints, native_metric, native_constraints, excise_chi, requested_z=0.):
    """Sample actual leaf-block centres; never resample AMR onto a uniform plane."""
    require(metric['time'] == constraints['time'] and metric['cycle'] == constraints['cycle'], 'Slice time mismatch')
    cb = {tuple(b['logical']): b for b in constraints['blocks']}
    require(len(cb) == len(metric['blocks']) <= MAX_BLOCKS, 'Slice block pairing differs')
    domain_upper = max(b['geometry'][5] for b in metric['blocks'])
    blocks, cells = [], 0
    for block in sorted(metric['blocks'], key=lambda b: tuple(b['logical'])):
        geometry = block['geometry']; lo, hi = geometry[4:6]
        # A shared block face belongs only to its positive-z neighbour. At the
        # domain's upper edge include the final block instead of inventing a row.
        if not (lo <= requested_z < hi or requested_z == domain_upper == hi):
            continue
        other = cb.get(tuple(block['logical']))
        require(other is not None and block['index'] == other['index'] and geometry == other['geometry'], 'Slice geometry mismatch')
        x, y, z = block['coordinates']
        iz = min(range(len(z)), key=lambda n: (abs(float(z[n])-requested_z), float(z[n])))
        nx, ny = len(x), len(y); cells += nx*ny
        require(cells <= MAX_SLICE_CELLS, 'Diagnostic slice exceeds retained cell bound')
        channels = {}
        for name, spec in SLICE_CHANNELS.items():
            fields = other['fields'] if name == 'hamiltonian' else block['fields']
            values = fields[spec['source']][iz].astype('<f8')
            require(values.shape == (ny, nx) and np.isfinite(values).all(), 'Invalid retained slice channel')
            channels[name] = values.tolist()
        blocks.append({'logical': list(block['logical']), 'native_logical_level': block['logical'][3],
                       'source_index': list(block['index']), 'geometry': list(geometry), 'shape': [ny, nx],
                       'x': x.tolist(), 'y': y.tolist(), 'z': float(z[iz]), 'z_index': iz,
                       'x_edges': np.linspace(geometry[0], geometry[1], nx+1).tolist(),
                       'y_edges': np.linspace(geometry[2], geometry[3], ny+1).tolist(), 'channels': channels})
    require(blocks, 'No retained leaf blocks intersect the requested diagnostic plane')
    ranges = {name: {'min': min(min(row) for b in blocks for row in b['channels'][name]),
                     'max': max(max(row) for b in blocks for row in b['channels'][name])} for name in SLICE_CHANNELS}
    return {'schema': 'phaseforge.nr-amr-diagnostic-slice.v1', 'time': metric['time'], 'scientific_time': metric['time'],
            'cycle': metric['cycle'], 'time_unit': 'L/c', 'coordinate_unit': 'L', 'axis_order': ['y', 'x'],
            'interpolation': 'none', 'grid_location': 'cell_center', 'blocks': blocks, 'cell_count': cells,
            'plane': {'axes': ['x', 'y'], 'normal_axis': 'z', 'requested_coordinate': requested_z,
                      'selection': 'Leaf blocks intersecting the half-open target plane; nearest native z centre in each block, lower centre on a tie.',
                      'actual_z_varies_by_block': len({b['z'] for b in blocks}) > 1,
                      'actual_z_min': min(b['z'] for b in blocks), 'actual_z_max': max(b['z'] for b in blocks)},
            'source_metric': native_metric, 'source_constraints': native_constraints, 'channel_ranges': ranges,
            'reduction_mask': {'field': 'chi', 'operator': '>=', 'value': excise_chi},
            'scope': 'XY diagnostic sampled at native AMR leaf-block centres. Each block retains its actual z; this is not an interpolated common-z surface or a 3D spacetime embedding. All cells are displayed, including cells excluded from masked RMS reductions.'}


def initial_diagnostics(text, params):
    def last(pattern):
        matches = re.findall(pattern, text)
        return number(matches[-1]) if matches else None
    errors = re.findall(r'ADM mass error: M_p_err=(\S+), M_m_err=(\S+)', text)
    residual = last(r'\|F\|=(\S+)')
    plus = last(r'Puncture 1 ADM mass is (\S+)'); minus = last(r'Puncture 2 ADM mass is (\S+)')
    bare = re.findall(r'The two puncture masses are mp=(\S+) and mm=(\S+)', text)
    problem = params.get('problem', {})
    tolerance = number(problem.get('Newton_tol', '1e-10'))
    achieved = plus is not None and minus is not None and residual is not None and residual <= tolerance
    adm_tolerance = number(problem.get('adm_tol', '1e-10'))
    if problem.get('give_bare_mass', 'false').lower() == 'false':
        achieved &= bool(errors and abs(number(errors[-1][0])) <= adm_tolerance and abs(number(errors[-1][1])) <= adm_tolerance)
    return {'status': 'reported_solver_convergence' if achieved else 'missing_or_unmet_reported_convergence',
            'requested_initial_data': {key: value for key, value in problem.items()
                                       if key.startswith(('par_', 'target_M_', 'npoints_')) or key in ('give_bare_mass', 'grid_setup_method')},
            'reported_residual': residual, 'requested_residual_tolerance': tolerance,
            'reported_target_errors': list(map(number, errors[-1])) if errors else None,
            'bare_masses': list(map(number, bare[-1])) if bare else None, 'puncture_end_ADM_masses': [plus, minus],
            'total_ADM_energy': last(r'The total ADM mass is (\S+)'),
            'horizon_rest_masses': None, 'spatial_convergence_validated': False,
            'note': 'Parsed pinned solver diagnostics; end masses, bare masses and horizon masses are distinct. A nonlinear residual alone is not accuracy validation.'}


def table(text, width, allow_nan=False):
    result = []
    for line in text.splitlines():
        if not line.strip() or line.startswith('#'):
            continue
        row = [float(x) for x in line.split()]
        require(len(row) == width and all(math.isfinite(x) or allow_nan and math.isnan(x) for x in row), 'Malformed diagnostic table')
        result.append(row)
        require(len(result) <= 100000, 'Diagnostic row bound exceeded')
    return result


def diagnostic_time_metadata(native_label_time):
    # Pinned upstream z4c_tasks performs the final RK update and diagnostic
    # evaluation before driver.cpp increments mesh time. Preserve the actual
    # printed value; a physical-time mapping needs verified per-step provenance.
    return {'native_label_time': native_label_time,
            'label_semantics': DIAGNOSTIC_LABEL_SEMANTICS,
            'physical_frame_time': None, 'time_mapping': None,
            'time_meaning': DIAGNOSTIC_TIME_MEANING}


def horizon_records(verbose, summary, horizon):
    attempts = []
    for section in re.split(r'(?=^time=)', verbose, flags=re.M):
        match = re.match(r'time=([\d.e+-]+), cycle=(\d+)', section)
        if not match:
            continue
        center = re.search(r'center = \(([^,]+), ([^,]+), ([^)]+)\)', section)
        failures = re.findall(r'^Failed.*$', section, re.M)
        found = re.search(rf'^Found horizon {horizon}\s*$', section, re.M) is not None
        attempts.append({'time_rounded': number(match[1]), 'iteration_stage': int(match[2]),
                         'current_search_status': 'found_by_mass_change' if found else 'failed' if failures else 'interrupted',
                         'current_found_flag': found, 'failure': failures[-1] if failures else None,
                         'center_from_rounded_verbose': list(map(number, center.groups())) if center else None,
                         'horizon_validated': False, 'surface': None})
    rows = table(summary, 12, allow_nan=True)
    # SIGTERM can interrupt the latest verbose search before the summary write.
    require(len(rows) <= len(attempts) and len(attempts)-len(rows) <= 1, 'Unmatched horizon search/summary records')
    for index, attempt in enumerate(attempts):
        row = rows[index] if index < len(rows) else None
        if row:
            require(abs(row[1]-attempt['time_rounded']) <= 0.000051, 'Horizon time correspondence mismatch')
            attempt['time'] = row[1]
            fields = ['mass', 'Sx', 'Sy', 'Sz', 'spin_norm', 'area', 'expansion_squared_area_average', 'expansion_integral', 'meanradius', 'minradius']
            values = {k: v if math.isfinite(v) else None for k, v in zip(fields, row[2:])}
            square = values['expansion_squared_area_average']
            values['expansion_rms'] = math.sqrt(square) if square is not None and square >= 0 else None
            attempt['reported_summary'] = values
            attempt['stale_summary'] = not attempt['current_found_flag'] and values['mass'] is not None
            attempt['current_properties'] = values if attempt['current_found_flag'] else None
        else:
            attempt.update(time=attempt['time_rounded'], reported_summary=None, stale_summary=False, current_properties=None)
        attempt.update(diagnostic_time_metadata(attempt['time']))
    return {'horizon': horizon, 'attempts': attempts,
            'found_flag_count': sum(a['current_found_flag'] for a in attempts),
            'failure_count': sum(a['current_search_status'] == 'failed' for a in attempts),
            'stale_summary_count': sum(a['stale_summary'] for a in attempts),
            'label_semantics': DIAGNOSTIC_LABEL_SEMANTICS, 'time_meaning': DIAGNOSTIC_TIME_MEANING,
            'meaning': 'Engine mass-change stopping flag, not independently validated zero-expansion surface. Failed summary values are never current mass measurements.'}


def surfaces(records, shape_text, grid_text, basis_text, output, horizon):
    """Reconstruct only successful saved surfaces from saved harmonics, not equations."""
    grid = table(grid_text, 3)
    require(len(grid) <= 8192, 'Surface grid bound exceeded')
    lookup = {(r[0], r[1]): i for i, r in enumerate(grid)}
    require(len(lookup) == len(grid), 'Duplicate angular grid point')
    basis = {}
    for line in basis_text.splitlines():
        if not line or line.startswith('#'):
            continue
        # Upstream concatenates later derivative columns. The first seven are
        # individually separated; no derivative columns are consumed or repaired.
        parts = line.split()
        require(len(parts) >= 7, 'Malformed retained spherical basis')
        theta, phi = map(number, parts[:2]); l, m = int(parts[2]), int(parts[3])
        require(0 <= m <= l <= 32 and (theta, phi) in lookup, 'Unsupported retained harmonic/grid')
        key = (l, m, lookup[(theta, phi)])
        require(key not in basis, 'Duplicate harmonic sample')
        basis[key] = list(map(number, parts[4:7]))
    shape_rows = []
    for match in re.finditer(r'^# iter = (\d+), Time = (\S+)\s*\n([^\n]+)', shape_text, re.M):
        shape_rows.append((number(match[2]), [number(x) for x in match[3].split()]))
    fresh = [a for a in records['attempts'] if a['current_found_flag']]
    require(len(shape_rows) <= len(fresh), 'Shape without current success flag')
    for index, (time, coeff) in enumerate(shape_rows):
        attempt = fresh[index]
        require(time == attempt['time'], 'Shape is not bound to current search time')
        lmax = math.isqrt(len(coeff))-1
        require((lmax+1)**2 == len(coeff) and 0 <= lmax <= 32, 'Malformed harmonic coefficient count')
        center = attempt['center_from_rounded_verbose']
        require(center is not None, 'Missing retained surface centre')
        xyz = []
        for p, (theta, phi, _) in enumerate(grid):
            terms, offset = [], 0
            for l in range(lmax+1):
                terms.append(coeff[offset]*basis[(l, 0, p)][0]); offset += 1
                for m in range(1, l+1):
                    values = basis[(l, m, p)]
                    terms.extend((coeff[offset]*values[1], coeff[offset+1]*values[2])); offset += 2
            radius = math.fsum(terms)
            require(math.isfinite(radius) and radius > 0, 'Nonpositive retained surface radius')
            xyz.append([center[0]+radius*math.sin(theta)*math.cos(phi), center[1]+radius*math.sin(theta)*math.sin(phi), center[2]+radius*math.cos(theta)])
        path = output/f'horizons/horizon-{horizon}-surface-{index:05d}.json'
        path.parent.mkdir(parents=True, exist_ok=True)
        save(path, {'schema': 'phaseforge.nr-retained-surface.v1', 'time': time, 'horizon': horizon,
                    **diagnostic_time_metadata(time),
                    'points_xyz': xyz, 'unit': 'L', 'horizon_validated': False,
                    'method': 'Reconstructed from retained rounded harmonic coefficients, basis and verbose centre; no surface solve or interpolation of spacetime.',
                    'precision_limit': 'Saved coefficients and centres are rounded; these coordinates do not add precision.'})
        attempt['surface'] = pin(path, output)


def waveform_records(files, text):
    rows = []
    for name in sorted(files):
        match = re.fullmatch(r'waveforms/rpsi4_(real|imag)_(\d+)\.txt', name)
        if not match:
            continue
        raw = text(name); lines = raw.splitlines()
        require(lines and lines[0].startswith('# 1:time'), 'Unknown waveform header')
        labels = re.findall(r'\d+:([^\s]+)', lines[0])
        require(labels == ['time']+[str(l)+str(m) for l in range(2, 9) for m in range(-l, l+1)], 'Unsupported rPsi4 multipole ordering')
        values = table(raw, len(labels))
        times = [row[0] for row in values]
        require(all(b > a for a, b in zip(times, times[1:])), 'Unordered waveform times')
        rows.append({'path': 'native/'+name, 'part': match[1], 'filename_radius_label': int(match[2]), 'columns': labels,
                     'samples': values, 'finite': True, 'units': '1/L', 'quantity': 'rPsi4',
                     'label_semantics': DIAGNOSTIC_LABEL_SEMANTICS, 'time_meaning': DIAGNOSTIC_TIME_MEANING,
                     'time_mapping': None,
                     'sample_time_metadata': [{'native_label_time': time, 'physical_frame_time': None} for time in times],
                     'scope': 'Finite-radius multipoles with native diagnostic time labels, not strain or calibrated emitted energy; no junk removal, evolved-state time mapping, retarded-time alignment or radius convergence.'})
    return rows


def produce(run, execution_receipt, execution_receipt_sha256, output, *, engine_manifest=None, input_path=None, request_path=None, calibration=None):
    require(calibration is None, 'No verified boost-calibration authority is implemented; caller claims are rejected')
    run, receipt_path, output = Path(run), Path(execution_receipt), Path(output)
    helper_sha = decoder.sha256(Path(__file__))
    decoder_sha = decoder.sha256(Path(decoder.__file__))
    require(not output.exists() and not output.is_symlink(), 'Result must be a new immutable directory')
    require(not output.resolve().is_relative_to(run.resolve()), 'Result cannot be inside native source')
    for parent in output.parents:
        if parent.exists() or parent.is_symlink():
            info = parent.lstat()
            require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0)&0x400, 'Linked output parent')
    raw = read(receipt_path, execution_receipt_sha256); receipt = parse(raw)
    for value in (receipt.get('engine_manifest_sha256'), receipt.get('request_sha256'), receipt.get('input', {}).get('sha256')):
        hash_string(value)
    ep = Path(engine_manifest) if engine_manifest else receipt_path.parent/'engine.json'
    ip = Path(input_path) if input_path else receipt_path.parent/'input.athinput'
    qp = Path(request_path) if request_path else receipt_path.parent/'request.json'
    manifest_raw = read(ep, receipt.get('engine_manifest_sha256')); manifest = parse(manifest_raw)
    request_raw = read(qp, receipt.get('request_sha256')); request = parse(request_raw)
    input_raw = read(ip, receipt.get('input', {}).get('sha256'), 256*1024)
    require(request.get('engine_manifest_sha256') == receipt.get('engine_manifest_sha256'), 'Manifest request mismatch')
    identity(receipt, manifest, request, input_raw)
    params = decoder.parameter_blocks(input_raw.decode('utf-8'))
    target = number(params.get('time', {}).get('tlim'))
    require(target >= 0, 'Invalid target time')
    excise = number(params.get('z4c', {}).get('excise_chi', '0.0625'))
    require(excise >= 0, 'Invalid excision mask')
    inventory = receipt.get('files')
    require(isinstance(inventory, list) and 0 < len(inventory) <= MAX_MEMBERS, 'Missing bounded retained inventory')
    names, total = set(), 0
    for row in inventory:
        name = row.get('path'); path = relative(name)
        require(name not in names and type(row.get('bytes')) is int and row['bytes'] >= 0, 'Duplicate or malformed source inventory')
        names.add(name); total += row['bytes']; hash_string(row.get('sha256'))
        require(total <= MAX_TOTAL, 'Retained input exceeds 1 GiB')
        plain(run/path)
    actual = set()
    for path in run.rglob('*'):
        info = path.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0)&0x400, 'Linked native tree')
        if stat.S_ISREG(info.st_mode):
            actual.add(path.relative_to(run).as_posix())
        require(len(actual) <= MAX_MEMBERS, 'Unbounded source tree')
    require(actual == names, 'Unregistered or missing retained source')
    output.mkdir(parents=True, exist_ok=False)
    for row in inventory:
        snapshot_file(run/relative(row['path']), output/'native'/relative(row['path']), row)
    (output/'sources').mkdir()
    for name, content in [('execution-receipt.json', raw), ('engine.json', manifest_raw), ('request.json', request_raw), ('input.athinput', input_raw)]:
        with (output/'sources'/name).open('xb') as stream:
            stream.write(content)
    def text(name):
        require(name in names, 'Diagnostic absent from pinned inventory')
        return read(output/'native'/relative(name)).decode('utf-8')
    errors, decoded, grouped = [], [], {}
    native_names = sorted(name for name in names if name.endswith('.bin'))
    require(len(native_names) <= MAX_FRAMES*2, 'Native frame count exceeds bound')
    (output/'frames').mkdir()
    # Read/decode at most one native file for inventory, then at most one matched
    # metric+constraint pair for reductions. No whole-trajectory array allocation.
    for name in native_names:
        try:
            frame = decoder.read_binary(output/'native'/relative(name))
            require(len(frame['blocks']) <= MAX_BLOCKS, 'Native block bound exceeded')
            variables = frame['variables']
            kind = 'metric' if 'z4c_chi' in variables else 'constraints' if 'con_H' in variables and 'con_M' in variables else None
            require(kind is not None, 'Unsupported native channel set')
            key = frame['time']
            require(kind not in grouped.setdefault(key, {}), 'Duplicate time/channel native frame')
            require(number(frame['parameters'].get('time', {}).get('tlim')) == target, 'Native target time differs from pinned input')
            require(number(frame['parameters'].get('z4c', {}).get('excise_chi', '0.0625')) == excise, 'Native excision mask differs from pinned input')
            row = metadata(frame); row['native'] = pin(output/'native'/relative(name), output)
            row['kind'] = kind; row['units'] = UNITS
            info_path = output/f'frames/native-{len(decoded):05d}.json'; save(info_path, row)
            decoded.append({'time': frame['time'], 'scientific_time': frame['time'], 'cycle': frame['cycle'], 'kind': kind,
                            'native': row['native'], 'metadata': pin(info_path, output), 'block_count': len(frame['blocks'])})
            grouped[key][kind] = name
            del frame
        except (ValueError, KeyError, OSError) as error:
            errors.append({'path': 'native/'+name, 'error': str(error), 'status': 'unreadable_or_invalid_native_frame'})
    reductions, times, slices = [], [], []
    (output/'slices').mkdir()
    for time, pair in sorted(grouped.items()):
        if set(pair) != {'metric', 'constraints'}:
            errors.append({'time': time, 'error': 'Missing paired metric/constraint state'}); continue
        try:
            metric = decoder.read_binary(output/'native'/relative(pair['metric']))
            con = decoder.read_binary(output/'native'/relative(pair['constraints']))
            reductions.append(reduce_constraints(metric, con, excise)); times.append(time)
            try:
                view = diagnostic_slice(metric, con, pin(output/'native'/relative(pair['metric']), output),
                                        pin(output/'native'/relative(pair['constraints']), output), excise)
                data_path = output/f'slices/frame-{len(slices):05d}.json'
                encoded = (json.dumps(view, allow_nan=False, separators=(',', ':'))+'\n').encode()
                require(len(encoded) <= 32*1024**2, 'Diagnostic slice exceeds bounded JSON size')
                with data_path.open('xb') as handle:
                    handle.write(encoded)
                slices.append({'index': len(slices), 'time': time, 'scientific_time': time, 'cycle': metric['cycle'],
                               'time_unit': 'L/c', 'block_count': len(view['blocks']), 'cell_count': view['cell_count'],
                               'data': pin(data_path, output), 'plane': view['plane'], 'channel_ranges': view['channel_ranges'],
                               'source_metric': view['source_metric'], 'source_constraints': view['source_constraints']})
            except (ValueError, KeyError, OSError) as error:
                errors.append({'time': time, 'error': str(error), 'status': 'unavailable_diagnostic_slice'})
            del metric, con
        except (ValueError, KeyError, OSError) as error:
            errors.append({'time': time, 'error': str(error), 'status': 'invalid_constraint_pair'})
    basename = params.get('job', {}).get('basename', '')
    require(re.fullmatch(r'[A-Za-z0-9_.-]{1,128}', basename) is not None, 'Unsupported output basename')
    stdout = text('solver.stdout.log') if 'solver.stdout.log' in names else ''
    init = initial_diagnostics(stdout, params)
    horizons = []
    count = int(params.get('fastflow', {}).get('num_horizons', '0')); require(0 <= count <= 16, 'Horizon count exceeds bound')
    for n in range(count):
        prefix = f'{basename}.horizon_'; verbose = f'{prefix}verbose_{n}.txt'; summary = f'{prefix}summary_{n}.txt'
        if verbose not in names or summary not in names:
            horizons.append({'horizon': n, 'status': 'missing_current_search_evidence', 'attempts': [], 'horizon_validated': False}); continue
        try:
            records = horizon_records(text(verbose), text(summary), n)
            shape, grid, basis = [f'{prefix}{kind}_{n}.txt' for kind in ('shape', 'grid', 'ylm')]
            if all(name in names for name in (shape, grid, basis)):
                try:
                    surfaces(records, text(shape), text(grid), text(basis), output, n)
                except (ValueError, KeyError) as error:
                    records['surface_read_error'] = str(error)
            records['source_files'] = [pin(output/'native'/relative(name), output) for name in (verbose, summary, shape, grid, basis) if name in names]
            horizons.append(records)
        except (ValueError, KeyError) as error:
            horizons.append({'horizon': n, 'status': 'invalid_current_search_evidence', 'error': str(error), 'horizon_validated': False, 'attempts': []})
    try:
        waves = waveform_records(names, text)
    except ValueError as error:
        waves = [{'status': 'invalid_waveform_diagnostics', 'error': str(error)}]
    history = []
    histname = f'{basename}.z4c.user.hst'
    if histname in names:
        try:
            for row in table(text(histname), 11):
                require(row[-1] > 0 and row[3] >= 0 and row[4] >= 0, 'Invalid history constraint norms')
                history.append({'time_rounded': row[0], 'H_rms': math.sqrt(row[3]/row[-1]), 'M_rms': math.sqrt(row[4]/row[-1]), 'proper_volume': row[-1]})
        except ValueError as error:
            errors.append({'path': 'native/'+histname, 'error': str(error)})
    index = {'schema': 'phaseforge.nr-amr-frame-inventory.v1', 'representation': 'nr_amr_native', 'scope': SCOPE, 'units': UNITS,
             'frames': sorted(decoded, key=lambda f: (f['time'], f['kind'])), 'paired_times': times, 'interpolation': 'none', 'errors': errors}
    save(output/'frames/index.json', index)
    measurements = {'schema': 'phaseforge.nr-black-hole-measurements.v1', 'units': UNITS,
                    'initial_data': init, 'constraints': reductions, 'rounded_history': history, 'horizons': horizons, 'waveforms': waves,
                    'constraint_growth': {key: reductions[-1][key]/reductions[0][key] if reductions and reductions[0][key] > 0 else None for key in ('H_rms', 'M_rms')},
                    'physical_boost': {'status': 'unknown', 'gamma': None, 'speed_over_c': None, 'calibration': None}}
    save(output/'measurements.json', measurements)
    process_complete = receipt.get('termination_reason') == 'completed' and receipt.get('exit_code') == 0 and receipt.get('process_group_drained') is True
    target_met = bool(times and times[-1] >= target)
    execution = {key: receipt.get(key) for key in ('termination_reason', 'exit_code', 'deadline_at', 'elapsed_seconds', 'process_group_drained', 'peak_sampled_group_rss_bytes', 'retained_bytes')}
    execution.update(process_completed=process_complete, time_target=target, last_paired_native_time=times[-1] if times else None,
                     retained_time_target_met=target_met, complete=process_complete and target_met,
                     status='complete_execution' if process_complete and target_met else 'partial_execution', no_final_snapshot_fabricated=True)
    if receipt['schema'] == 'phaseforge.nr-process-receipt.v2':
        execution.update(execution_policy=receipt['execution_policy'], memory_boundary=receipt.get('memory_boundary'),
                         gpu_vram=receipt['gpu_vram'], cuda_runtime_snapshot=receipt.get('cuda_runtime_snapshot'))
    diagnostic_slices = None
    if slices:
        ranges = {name: {'min': min(f['channel_ranges'][name]['min'] for f in slices),
                         'max': max(f['channel_ranges'][name]['max'] for f in slices)} for name in SLICE_CHANNELS}
        slice_index = {'schema': 'phaseforge.nr-amr-diagnostic-slices.v1', 'representation': 'nr_amr_diagnostic_slice',
                       'engine_id': manifest['engine_id'], 'scope': SCOPE, 'units': UNITS, 'channels': SLICE_CHANNELS,
                       'primary_channel': 'chi', 'channel_ranges': ranges, 'frames': slices, 'frame_count': len(slices),
                       'initial_time': slices[0]['time'], 'final_time': slices[-1]['time'], 'time_unit': 'L/c',
                       'interpolation': 'none', 'grid_location': 'cell_center', 'axis_order': ['y', 'x'],
                       'level_semantics': 'Native logical octree level, not relative refinement depth.',
                       'execution': execution, 'physical_boost': measurements['physical_boost'],
                       'horizons': [{'horizon': h['horizon'], 'status': h.get('status'), 'found_flag_count': h.get('found_flag_count', 0),
                                     'failure_count': h.get('failure_count', 0), 'stale_summary_count': h.get('stale_summary_count', 0),
                                     'latest_status': h['attempts'][-1].get('current_search_status') if h.get('attempts') else 'missing',
                                     'horizon_validated': False} for h in horizons],
                       'fulfillment': {'requested_0_999c_collision': False, 'merger_validated': False, 'horizon_properties_validated': False}}
        save(output/'slices/index.json', slice_index)
        diagnostic_slices = pin(output/'slices/index.json', output)
    for item in inventory:
        source_pin = pin(output/'native'/relative(item['path']), output)
        require(source_pin['sha256'] == item['sha256'] and source_pin['bytes'] == item['bytes'], 'Retained snapshot mutated during processing')
    require(decoder.sha256(Path(__file__)) == helper_sha and decoder.sha256(Path(decoder.__file__)) == decoder_sha,
            'Postprocessor source changed during processing')
    artifacts = [pin(path, output) for path in sorted(output.rglob('*')) if path.is_file()]
    result = {'schema': 'phaseforge.nr-black-hole-result.v1', 'job_id': receipt['job_id'], 'engine_identity': manifest,
              'scope': SCOPE, 'units': UNITS, 'execution': execution, 'frame_inventory': pin(output/'frames/index.json', output),
              'measurements': pin(output/'measurements.json', output), 'paired_times': times, 'paired_state_count': len(times),
              'diagnostic_errors': errors, 'physical_boost': measurements['physical_boost'],
              'fulfillment': {'requested_0_999c_collision': False, 'merger_validated': False, 'horizon_properties_validated': False, 'convergence_validated': False},
              'sources': {'receipt_sha256': execution_receipt_sha256, 'manifest_sha256': receipt['engine_manifest_sha256'],
                          'input_sha256': receipt['input']['sha256'], 'request_sha256': receipt['request_sha256'],
                          'postprocessor_sha256': helper_sha, 'decoder_sha256': decoder_sha,
                          'source_commit': decoder.UPSTREAM_COMMIT, 'cpu_cuda_equivalence_tested': False}, 'artifacts': artifacts}
    if diagnostic_slices is not None:
        result['diagnostic_slices'] = diagnostic_slices
    save(output/'result.json', result)
    return result


def read_result(directory, expected_sha256):
    root = Path(directory); result = parse(read(root/'result.json', expected_sha256))
    require(result.get('schema') == 'phaseforge.nr-black-hole-result.v1', 'Unsupported retained result')
    names = set()
    for row in result['artifacts']:
        name = row['path']; path = relative(name); require(name not in names, 'Duplicate artifact pin'); names.add(name)
        actual = pin(root/path, root)
        require(actual == row, 'Retained result artifact mismatch')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('run', 'execution-receipt', 'execution-receipt-sha256', 'output'):
        parser.add_argument('--'+name, required=True)
    for name in ('engine-manifest', 'input', 'request'):
        parser.add_argument('--'+name)
    args = parser.parse_args()
    try:
        result = produce(args.run, args.execution_receipt, args.execution_receipt_sha256, args.output,
                         engine_manifest=args.engine_manifest, input_path=args.input, request_path=args.request)
        print(json.dumps({'result': str(Path(args.output)/'result.json'), 'sha256': decoder.sha256(Path(args.output)/'result.json'),
                          'execution_complete': result['execution']['complete'], 'paired_times': result['paired_times'], 'physical_boost': 'unknown'}))
        return 0 if result['execution']['complete'] else 3
    except (ValueError, KeyError, TypeError, OSError) as error:
        print(json.dumps({'error': str(error), 'result_published': False})); return 2


if __name__ == '__main__':
    raise SystemExit(main())
