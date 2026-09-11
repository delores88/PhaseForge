#!/usr/bin/env python3
"""Read-only checker for the pre-registered dilute NVE reviewer variant.

Never launches a solver, writes research artifacts, or imports scientific_worker.
Expected values come from exact SI constants and the frozen analytic derivation.
"""
from __future__ import annotations
import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import math
from pathlib import Path
from urllib.request import Request, urlopen
import xml.etree.ElementTree as ET

import numpy as np
from check_scientific_worker import artifact_test

PLAN_SHA256 = '68120c4a29da439c73223a2f8d9aff2954382b2fe9c15bff8b5b827d0619275b'
FREEZE_TIME = datetime.fromisoformat('2026-09-11T15:12:56+00:00')
N, MASS, TEMPERATURE = 32, 39.948, 200.0
NA, KB = 6.02214076e23, 1.380649e-23
R = NA * KB / 1000
KINETIC = (3*N-3)*R*TEMPERATURE/2
SPEED_SQUARED = 2*KINETIC/(N*MASS)
CUTOFF = 2.5*.3405
EXPECTED = {'atom_count': N, 'temperature_kelvin': TEMPERATURE, 'seed': 46703,
            'platform': 'CPU', 'cpu_threads': 1, 'thermostat': 'nve',
            'timestep_fs': 1, 'steps': 100, 'sample_interval': 1, 'chunk_frames': 17}


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'),
                      parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=True, allow_nan=False).encode()


def retained(root, relative):
    path = (root / relative).resolve(strict=True)
    if not path.is_relative_to(root.resolve()) or not path.is_file():
        raise ValueError('Scientific artifact escapes the source job')
    return path


def reference(density):
    volume = N*MASS/NA/density*1e21
    length = volume**(1/3)
    return {'density_g_cm3': density, 'volume_nm3': volume, 'box_nm': length,
            'pressure_bar': 2*KINETIC/(3*volume)*(1e25/NA),
            'kinetic_energy_kj_mol': KINETIC, 'msd_coefficient_nm2_ps2': SPEED_SQUARED,
            'cutoff_nm': CUTOFF,
            'minimum_pair_bound_nm': length/(2*math.sqrt(2)) - math.sqrt(4*KINETIC/MASS)*.1}


def check_run(job, artifacts, density, session):
    root = artifacts / job['id']
    manifest, topology, index, measurements, result = [read(root/name) for name in
        ['manifest.json', 'topology.json', 'trajectory/index.json', 'measurements.json', 'result.json']]
    ref = reference(density)
    frames, positions, velocities, forces, array_steps, array_times = [], [], [], [], [], []
    for chunk in index['chunks']:
        frames.extend(read(retained(root, chunk['path']))['frames'])
        with np.load(retained(root, chunk['arrays_path']), allow_pickle=False) as arrays:
            positions.extend(arrays['positions_unwrapped_nm'])
            velocities.extend(arrays['velocities_nm_ps'])
            forces.extend(arrays['forces_kj_mol_nm'])
            array_steps.extend(arrays['steps'].tolist())
            array_times.extend(arrays['time_ps'].tolist())
    positions, velocities, forces = map(np.asarray, (positions, velocities, forces))
    times = np.asarray([frame['time'] for frame in frames])
    rows = measurements['series']
    if positions.shape != (101, N, 3) or velocities.shape != positions.shape or forces.shape != positions.shape or len(rows) != 101:
        raise ValueError(f"{job['id']}: expected exactly 101 states of 32 position/velocity/force vectors")
    if not all(np.isfinite(array).all() for array in [positions, velocities, forces, times]):
        raise ValueError('Nonfinite authoritative numerical arrays')
    values = {key: np.asarray([row[key] for row in rows]) for key in
              ['potential_energy_kj_mol', 'pair_virial_kj_mol', 'kinetic_energy_kj_mol',
               'total_energy_kj_mol', 'temperature_kelvin', 'pressure_bar', 'msd_nm2']}
    if not all(np.isfinite(array).all() for array in values.values()):
        raise ValueError('Nonfinite measurement')
    # Independent minimum-image separation at every retained integrator state.
    pairs = np.triu_indices(N, 1)
    distances = []
    for frame in positions:
        delta = frame[pairs[0]]-frame[pairs[1]]
        delta -= ref['box_nm']*np.rint(delta/ref['box_nm'])
        distances.append(float(np.min(np.linalg.norm(delta, axis=1))))
    measured_kinetic = MASS/2*np.sum(velocities**2, axis=(1, 2))
    measured_msd = np.mean(np.sum((positions-positions[0])**2, axis=2), axis=1)
    momentum = MASS*np.sum(velocities, axis=1)
    observed = {
        'frame_count': len(frames), 'final_time_ps': float(times[-1]),
        'minimum_pair_distance_nm': min(distances),
        'max_absolute_force_kj_mol_nm': float(np.max(np.abs(forces))),
        'max_absolute_potential_kj_mol': float(np.max(np.abs(values['potential_energy_kj_mol']))),
        'max_absolute_pair_virial_kj_mol': float(np.max(np.abs(values['pair_virial_kj_mol']))),
        'max_kinetic_error_kj_mol': float(np.max(np.abs(values['kinetic_energy_kj_mol']-KINETIC))),
        'max_array_kinetic_error_kj_mol': float(np.max(np.abs(measured_kinetic-KINETIC))),
        'max_total_energy_error_kj_mol': float(np.max(np.abs(values['total_energy_kj_mol']-KINETIC))),
        'max_temperature_error_K': float(np.max(np.abs(values['temperature_kelvin']-TEMPERATURE))),
        'max_ballistic_position_error_nm': float(np.max(np.abs(positions-(positions[0]+velocities[0]*times[:, None, None])))),
        'max_velocity_change_nm_ps': float(np.max(np.abs(velocities-velocities[0]))),
        'initial_total_momentum_dalton_nm_ps': float(np.linalg.norm(momentum[0])),
        'max_momentum_drift_dalton_nm_ps': float(np.max(np.linalg.norm(momentum-momentum[0], axis=1))),
        'max_msd_error_nm2': float(np.max(np.abs(values['msd_nm2']-SPEED_SQUARED*times**2))),
        'max_array_msd_error_nm2': float(np.max(np.abs(measured_msd-SPEED_SQUARED*times**2))),
        'msd_half_nm2': float(values['msd_nm2'][50]), 'msd_final_nm2': float(values['msd_nm2'][100]),
        'msd_time_ratio': float(values['msd_nm2'][100]/values['msd_nm2'][50]),
        'max_pressure_error_bar': float(np.max(np.abs(values['pressure_bar']-ref['pressure_bar']))),
        'mean_pressure_bar': float(np.mean(values['pressure_bar'])),
    }
    system = ET.parse(root/'system.xml').getroot()
    integrator = ET.parse(root/'integrator.xml').getroot()
    force_list = system.findall('./Forces/Force')
    interaction = force_list[0] if len(force_list) == 1 else None
    normalized_input = {'engine': 'openmm_argon', 'parameters': manifest['parameters']}
    normalized_hash = hashlib.sha256(canonical(normalized_input)).hexdigest()
    model = manifest['model']
    checks = {
        'fresh_completed_cpu_solver': job['kind'] == 'solver' and job['state'] == 'completed' and result['status'] == 'completed'
            and manifest['engine'] == 'openmm_argon' and manifest['platform'] == 'CPU'
            and datetime.fromisoformat(job['created_at']) > FREEZE_TIME
            and datetime.fromisoformat(job['created_at']) > datetime.fromisoformat(session['created_at']),
        'exact_parameters': all(manifest['parameters'].get(key) == value and job['input']['parameters'].get(key) == value
                                for key, value in {**EXPECTED, 'density_g_cm3': density}.items()),
        'same_parent': job['parent_id'] == session['id'] == job['input']['source_session_id'],
        'normalized_input_hash': normalized_hash == manifest['input_sha256'] == result['input_sha256'],
        'retained_worker_hash': digest(root/'scientific_worker.py') == manifest['worker_sha256'],
        'exact_steps_and_state_counts': [frame['step'] for frame in frames] == array_steps == [row['step'] for row in rows] == list(range(101))
            and index['frame_count'] == result['frame_count'] == 101 and result['completed_steps'] == 100,
        'all_state_times': np.array_equal(times, array_times) and np.array_equal(times, [row['time_ps'] for row in rows])
            and bool(np.all(np.diff(times)>0)) and float(np.max(np.abs(times-np.arange(101)*.001))) <= 1e-12,
        'topology': topology['position_unit'] == index['position_unit'] == 'nm' and index['time_unit'] == 'ps'
            and all([entity['id'] for entity in frame['entities']] == [f'ar-{i:04d}' for i in range(N)] for frame in frames)
            and len(topology['entities']) == N and all(entity['element'] == 'Ar' and entity['mass_dalton'] == MASS for entity in topology['entities']),
        'geometry': max(abs(float(side)-ref['box_nm']) for side in model['box_nm']+topology['box_nm']) <= 1e-10
            and abs(float(np.prod(model['box_nm']))-ref['volume_nm3']) <= 1e-8,
        'declared_force_model': model['mass_dalton'] == MASS and model['sigma_nm'] == .3405 and model['epsilon_kj_mol'] == .997
            and model['boundary'] == 'cubic periodic' and model['charges'] == 0 and model['dispersion_correction'] is False
            and abs(model['cutoff_nm']-CUTOFF) <= 1e-14 and abs(model['switch_nm']-.8*CUTOFF) <= 1e-14,
        'serialized_integrator': integrator.get('type') == 'VerletIntegrator' and float(integrator.get('stepSize')) == .001,
        'serialized_force': interaction is not None and interaction.get('type') == 'NonbondedForce'
            and interaction.get('method') == '2' and interaction.get('dispersionCorrection') == '0'
            and interaction.get('useSwitchingFunction') == '1' and abs(float(interaction.get('cutoff'))-CUTOFF) <= 1e-14
            and len(interaction.findall('./Particles/Particle')) == N
            and all(float(p.get('q')) == 0 and float(p.get('eps')) == .997 and float(p.get('sig')) == .3405 for p in interaction.findall('./Particles/Particle')),
        'pair_distance_bound': min(distances) >= ref['minimum_pair_bound_nm']-1e-8,
        'zero_force': observed['max_absolute_force_kj_mol_nm'] <= 1e-10,
        'zero_potential': observed['max_absolute_potential_kj_mol'] <= 1e-10,
        'zero_virial': observed['max_absolute_pair_virial_kj_mol'] <= 1e-10,
        'kinetic_reference': max(observed['max_kinetic_error_kj_mol'], observed['max_array_kinetic_error_kj_mol']) <= 1e-5,
        'total_energy_reference': observed['max_total_energy_error_kj_mol'] <= 1e-5,
        'temperature_reference': observed['max_temperature_error_K'] <= 2e-5,
        'ballistic_positions': observed['max_ballistic_position_error_nm'] <= 2e-8,
        'constant_velocities': observed['max_velocity_change_nm_ps'] <= 1e-8,
        'zero_initial_com': observed['initial_total_momentum_dalton_nm_ps'] <= 1e-6,
        'momentum_conservation': observed['max_momentum_drift_dalton_nm_ps'] <= 1e-6,
        'ballistic_msd': max(observed['max_msd_error_nm2'], observed['max_array_msd_error_nm2']) <= 1e-10,
        'quadratic_msd_ratio': abs(observed['msd_time_ratio']-4) <= 2e-6,
        'pressure_reference': observed['max_pressure_error_bar'] <= 2e-5,
    }
    artifacts_check = artifact_test(root)
    checks['independent_artifact_pressure_msd_rdf_images'] = artifacts_check['passed']
    receipts = {'files': {name: digest(root/name) for name in ['input.json', 'worker-input.json', 'manifest.json',
                'result.json', 'topology.json', 'trajectory/index.json', 'measurements.json', 'system.xml', 'integrator.xml']},
                'worker_sha256': manifest['worker_sha256'], 'engine_version': manifest['engine_version'],
                'platform_properties': manifest['platform_properties'], 'chunks': index['chunks']}
    return {'id': job['id'], 'created_at': job['created_at'], 'reference': ref, 'observed': observed,
            'checks': checks, 'passed': all(checks.values()), 'artifact_checks': artifacts_check, 'receipts': receipts}, values['msd_nm2']


def image_checks(artifacts, session, jobs):
    journal = read(artifacts/session['id']/'journal.json')
    supplied = set()
    for item in journal.get('items', []):
        content = item.get('content')
        for part in content if isinstance(content, list) else []:
            url = part.get('image_url', '')
            if part.get('type') == 'input_image' and url.startswith('data:image/png;base64,'):
                supplied.add(hashlib.sha256(base64.b64decode(url.split(',', 1)[1], validate=True)).hexdigest())
    receipts = []
    for call_id, tool in journal['tools'].items():
        if tool['name'] != 'observe_frame':
            continue
        evidence = tool.get('output', {}).get('evidence', {})
        identity = evidence.get('job_id')
        if identity not in jobs:
            continue
        path = retained(artifacts/identity, evidence['path'])
        image_hash = digest(path)
        declaration = next((row for row in read(artifacts/identity/'observations/index.json')['images'] if row['path'] == evidence['path']), None)
        receipts.append({'call_id': call_id, 'job_id': identity, 'path': evidence['path'], 'sha256': image_hash,
                         'native_input_bytes_match': image_hash in supplied,
                         'tool_and_saved_source_match': evidence.get('sha256') == image_hash and declaration is not None and declaration['sha256'] == image_hash,
                         'source': declaration})
    return {'passed': all(any(row['job_id'] == identity and row['native_input_bytes_match'] and row['tool_and_saved_source_match'] for row in receipts) for identity in jobs),
            'receipts': receipts, 'journal_sha256': digest(artifacts/session['id']/'journal.json')}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--session', required=True)
    parser.add_argument('--low', required=True)
    parser.add_argument('--high', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--installed-build', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.is_relative_to(args.artifacts.resolve()) or output.exists():
        raise ValueError('Choose a new report outside the scientific artifacts; previous reports are immutable')
    plan = Path(__file__).parents[1]/'docs/validation/reviewer-variant.md'
    if digest(plan) != PLAN_SHA256:
        raise ValueError('The frozen reviewer criteria have changed')
    def get(path):
        with urlopen(Request(args.base_url+path, headers={'Origin': args.base_url}), timeout=30) as response:
            return json.load(response)
    session = get(f'/api/laboratory/jobs/{args.session}')
    jobs = [get(f'/api/laboratory/jobs/{identity}') for identity in [args.low, args.high]]
    inventory = get(f"/api/laboratory/jobs?project_id={session['project_id']}")
    inventory = inventory['jobs'] if isinstance(inventory, dict) else inventory
    plan_text = plan.read_text(encoding='utf-8')
    prompt = next(line[2:] for line in plan_text.splitlines() if line.startswith('> Investigate'))
    run_reports, msd = [], []
    for job, density in zip(jobs, [.02, .04], strict=True):
        report, values = check_run(job, args.artifacts, density, session)
        run_reports.append(report); msd.append(values)
    ratio = run_reports[1]['observed']['mean_pressure_bar']/run_reports[0]['observed']['mean_pressure_bar']
    difference = run_reports[1]['observed']['mean_pressure_bar']-run_reports[0]['observed']['mean_pressure_bar']
    delta_msd = float(np.max(np.abs(msd[1]-msd[0])))
    images = image_checks(args.artifacts, session, [args.low, args.high])
    journal = read(args.artifacts/session['id']/'journal.json')
    launches = [{'call_id': call_id, 'arguments': tool['arguments'], 'output': tool['output']}
                for call_id, tool in journal['tools'].items() if tool['name'] == 'launch_experiment']
    build = read(args.installed_build)
    checks = {'session_completed': session['state'] == 'completed', 'exact_frozen_chat_prompt': session['input']['content'] == prompt,
              'session_after_freeze': datetime.fromisoformat(session['created_at']) > FREEZE_TIME,
              'distinct_fresh_ids': args.low != args.high and all(job['parent_id'] == session['id'] for job in jobs),
              'ordinary_tool_launch_receipts': all(any(row['output'].get('job_id') == identity for row in launches) for identity in [args.low, args.high]),
              'all_numerical_run_checks': all(row['passed'] for row in run_reports),
              'pressure_ratio': abs(ratio-2) <= 2e-6, 'pressure_difference': abs(difference-reference(.02)['pressure_bar']) <= 3e-5,
              'density_independent_msd': delta_msd <= 1e-10, 'actual_native_image_inputs': images['passed'],
              'answer_identifies_both_jobs': all(identity in session.get('result', {}).get('answer', '') for identity in [args.low, args.high])}
    failed_attempts = [{'id': job['id'], 'state': job['state'], 'created_at': job['created_at'], 'input': job['input'], 'error': job['error']}
                       for job in inventory if job['parent_id'] == session['id'] and job['kind'] == 'solver' and job['state'] != 'completed']
    report = {'schema_version': 1, 'checked_at': datetime.now(timezone.utc).isoformat(), 'passed': all(checks.values()), 'checks': checks,
              'scope': 'Installed execution, analytic numerical agreement, artifact integrity and native image-input receipts; empirical validity is not tested.',
              'criteria_sha256': PLAN_SHA256, 'checker_sha256': digest(__file__), 'session_id': session['id'],
              'provider_model_budget': {key: session['input'].get(key) for key in ['provider', 'model', 'reasoning_effort', 'time_limit_seconds']},
              'installed_build': build, 'installed_build_sha256': digest(args.installed_build), 'runs': run_reports,
              'comparison': {'mean_pressure_ratio': ratio, 'mean_pressure_difference_bar': difference, 'maximum_density_msd_difference_nm2': delta_msd},
              'image_evidence': images, 'launch_receipts': launches, 'preserved_failed_attempts': failed_attempts,
              'answer': session.get('result', {}).get('answer'),
              'interpretation_review': 'Read the retained answer and actual images separately; numeric checks do not automatically validate prose.'}
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, allow_nan=False), encoding='utf-8')
    print(json.dumps({'passed': report['passed'], 'checks': checks, 'runs': [{'id': row['id'], 'observed': row['observed'], 'failed_checks': [key for key, value in row['checks'].items() if not value]} for row in run_reports], 'comparison': report['comparison'], 'report': str(output)}, indent=2))
    raise SystemExit(0 if report['passed'] else 1)


if __name__ == '__main__':
    main()
