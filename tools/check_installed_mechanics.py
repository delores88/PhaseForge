#!/usr/bin/env python3
"""Freeze or inspect mechanics acceptance artifacts; never launch a solver/model.

prepare writes immutable reference/input snapshots. verify reads existing runs;
component-fixture scope exercises the checker without claiming installed success.
snapshot-paused retains hashes after a cooperative worker has fully stopped.
"""
from __future__ import annotations

import argparse
import copy
from datetime import datetime
import hashlib
import importlib.util
import json
import math
from pathlib import Path
import shutil
import time
import traceback

import numpy as np

TOOLS = Path(__file__).resolve().parent
SOURCE_FILES = ['tools/check_mechanics_worker.py', 'tools/check_installed_mechanics.py',
                'tools/mechanics_worker.py', 'tools/requirements-science.txt',
                'docs/validation/mechanics-lab-proposal.md',
                'docs/validation/installed-mechanics-preregistration.md']


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def read(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write(path, value):
    Path(path).write_text(json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True,
                                   allow_nan=False) + '\n', encoding='utf-8')


def require(value, message):
    if not value:
        raise AssertionError(message)


def reference(path):
    spec = importlib.util.spec_from_file_location('independent_mechanics_reference', path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def family(p, speed):
    """Validate a reviewer-selected exact Kepler/homographic family, no stepping."""
    require(speed in (1., .8), 'Reviewer speed must use one of the frozen reference speeds')
    m = np.asarray(p['masses'], dtype=float)
    q, v = [np.asarray(p[k], dtype=float) for k in ('positions', 'velocities')]
    require(len(m) in (2, 3) and q.shape == v.shape == (len(m), 3), 'Reference family requires two or three bodies')
    require(len(p['body_ids']) == len(m) == len(set(p['body_ids'])), 'Unique reference body IDs required')
    require(np.isfinite(m).all() and np.isfinite(q).all() and np.isfinite(v).all() and (m > 0).all(), 'Invalid reference arrays')
    require(abs(math.fsum(m) - 1) < 1e-14, 'Reference total mass must be one M0')
    com, drift = np.sum(m[:, None] * q, axis=0), np.sum(m[:, None] * v, axis=0)
    centered, relative_v = q - com, v - drift
    normal = np.cross(q[1] - q[0], v[1] - v[0])
    require(np.linalg.norm(normal) > 0, 'Degenerate orbital plane')
    normal /= np.linalg.norm(normal)
    require(np.max(np.abs(relative_v - speed * np.cross(normal, centered))) < 1e-12,
            'Initial velocities do not describe the declared exact family')
    require(all(abs(np.linalg.norm(q[j] - q[i]) - 1) < 1e-12 for i in range(len(m)) for j in range(i + 1, len(m))),
            'Every reference initial pair separation must be one L0')
    require(np.max(np.abs(centered @ normal)) < 1e-12, 'Bodies do not share the orbital plane')
    require(np.max(np.abs(com)) <= 1 and np.max(np.abs(drift)) <= .1,
            'Reviewer translation/drift exceed the declared reference envelope')
    require(p['steps'] == 4400 and p['sample_interval'] == 4 and p['timestep'] == 2 * math.pi / 4000,
            'Installed reference uses the frozen fine period-run cadence')
    require(p['boundary'] == 'isolated' and p['unit_system'] == 'scaled_G1' and p['min_separation'] == .05,
            'Reference physics or guard changed')
    return dict(com=com.tolist(), drift=drift.tolist(), normal=normal.tolist(), centered=centered.tolist(),
                tangent=(relative_v / speed).tolist())


def prepare(args):
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    src = out / 'source'
    for rel in SOURCE_FILES:
        dest = src / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(TOOLS.parent / rel, dest)
    ref = reference(src / 'tools/check_mechanics_worker.py')
    if args.reviewer_spec:
        selected = read(args.reviewer_spec)
        require(isinstance(selected.get('selection_provenance'), str) and selected['selection_provenance'].strip(),
                'Retain the independent reviewer selection provenance before execution')
        cases = [dict(name='installed-reviewer-variant', speed=selected['speed'], parameters=selected['parameters'],
                      selection_provenance=selected['selection_provenance'])]
        shutil.copy2(args.reviewer_spec, out / 'reviewer-selection.json')
        supplied_control = selected['control_evidence']
        accepted_path = Path(supplied_control['acceptance_report']).resolve()
        accepted = read(accepted_path)
        require(accepted['passed'] and accepted['scope'] == 'installed', 'Reviewer requires the completed installed baseline numerical acceptance')
        control = accepted['cases']['installed-binary-control']
        require(control['passed'] and control['parent_id'] and control['project_id'], 'Accepted baseline control lineage missing')
        inventory = accepted_path.parent / 'installed-binary-control-artifact-hashes.json'
        control_files = read(inventory)
        control_directory = Path(control['directory']).resolve()
        require(all(sha(ref.artifact(control_directory, path)) == value for path, value in control_files.items()),
                'Accepted baseline bytes changed before reviewer selection')
        control_evidence = dict(acceptance_report=str(accepted_path), acceptance_report_sha256=sha(accepted_path),
                                inventory=str(inventory), inventory_sha256=sha(inventory), directory=str(control_directory),
                                parent_id=control['parent_id'], project_id=control['project_id'], period_T0=control['period_T0'])
        mode = 'reviewer-selected-variant'
    else:
        cases = [dict(name=name, speed=speed, parameters=ref.parameters('binary', speed, 4000, True))
                 for name, speed in [('installed-binary-control', 1.), ('installed-binary-speed-080', .8)]]
        mode = 'ordinary-chat-controlled-pair'
        control_evidence = None
    for case in cases:
        case['reference_geometry'] = family(case['parameters'], case['speed'])
        case['expected_regular_frames'] = 1101
        case['expected_period_T0'] = 2 * math.pi * (1 if case['speed'] == 1 else (25 / 34) ** 1.5)
        case['expected_minimum_separation_L0'] = 1 if case['speed'] == 1 else 8 / 17
        write(out / (case['name'] + '-input.json'), dict(engine=ref.ENGINE, parameters=case['parameters']))
    plan = dict(schema_version=1, scope=mode, prepared_unix_s=time.time(), solver_executed=False,
                cases=cases, independent_reference_checks=ref.reference_checks(),
                criteria=dict(circular_fine_position_velocity_rms=1.25e-5, eccentric_fine_position_velocity_rms=2e-4,
                              rms_regular_prefix_end_step=4000, relative_energy_drift=2e-4, momentum_angular_com_drift=1e-11,
                              instrument_acceleration_absolute_error=1e-10, circular_separation_error=2e-4,
                              eccentric_minimum_error=5e-4, period_ratio_relative_error=.001, image_center_error_px=1,
                              per_process_budget_seconds=300),
                not_repeated=['Full three-level refinement', 'Force finite-difference fixture', 'Invalid-input suite',
                              'Uninterrupted-versus-resumed bit identity unless a separately retained matched execution is supplied'],
                control_evidence=control_evidence,
                source_sha256={rel: sha(src / rel) for rel in SOURCE_FILES})
    write(out / 'plan.json', plan)
    print(json.dumps(dict(plan=str(out / 'plan.json'), sha256=sha(out / 'plan.json'), cases=[x['name'] for x in cases], solver_executed=False)))
    return 0


def load_plan(path):
    plan = read(path)
    src = path.parent / 'source'
    for rel, value in plan['source_sha256'].items():
        require(sha(src / rel) == value, 'Preregistered source changed: ' + rel)
    require(sha(Path(__file__)) == plan['source_sha256']['tools/check_installed_mechanics.py'],
            'Executing checker differs from its frozen source; use the pinned checker or preserve a new preregistration')
    ref = reference(src / 'tools/check_mechanics_worker.py')
    return plan, ref, src / 'tools/mechanics_worker.py'


def initial_hash_parameters(ref, directory, frozen):
    """Only zero-sign spelling may differ for initial hash comparison.

    The raw manifest/input/chunks keep their exact hashes. Numerical inputs must
    still equal the preregistered arrays exactly; no magnitude tolerance is used.
    """
    actual = ref.read(directory / 'manifest.json')['input']['parameters']
    require(actual['body_ids'] == frozen['body_ids'], 'Initial body IDs differ from the frozen input')
    comparison = copy.deepcopy(frozen)
    differences = []
    for key in ('masses', 'positions', 'velocities'):
        left, right = np.asarray(actual[key], dtype=np.float64), np.asarray(frozen[key], dtype=np.float64)
        require(left.shape == right.shape and np.isfinite(left).all() and np.array_equal(left, right),
                'Initial numeric array differs from frozen input: ' + key)
        count = int(np.count_nonzero((left == 0) & (np.signbit(left) != np.signbit(right))))
        if count:
            differences.append(dict(array=key, changed_zero_signs=count))
        comparison[key] = actual[key]
    return comparison, differences


def snapshot(args):
    plan, ref, worker = load_plan(args.plan.resolve())
    case = next(x for x in plan['cases'] if x['name'] == args.case)
    directory, out = args.directory.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    checkpoint = ref.read(directory / 'checkpoint.json')
    job = read(args.job_snapshot)
    stopped = [event for event in job['events'] if event['kind'] == 'worker_stopped']
    pauses = [event for event in job['events'] if event['kind'] == 'paused']
    require(job['id'] == directory.name and job['kind'] == 'solver' and job['state'] == 'paused'
            and pauses and stopped and stopped[-1]['sequence'] > pauses[-1]['sequence'],
            'Snapshot requires a paused solver receipt after the native worker has stopped')
    process = read(args.process_receipt)
    started = [event for event in job['events'] if event['kind'] == 'solver_started']
    require(started and process['job_id'] == job['id'] and process['pid'] == started[-1]['data']['pid']
            and process['exit_code'] == 3 and process['method'] == 'Windows GetExitCodeProcess on original retained process handle',
            'Actual original-process cooperative exit3 receipt required')
    require(process['opened_unix_s'] <= process['pause_requested_unix_s'] <= process['exited_unix_s']
            and 400 <= process['observed_running_step'] <= 3600,
            'Pause observation must retain the original process handle and declared running-step window')
    step = checkpoint['step']
    require(0 < step < case['parameters']['steps'], 'Pause must retain a strictly intermediate computed step')
    require(step >= process['observed_running_step'], 'Checkpoint predates the observed progress')
    initial_parameters, zero_signs = initial_hash_parameters(ref, directory, case['parameters'])
    data = ref.inspect_artifacts(directory, initial_parameters, worker, final=step, manifest_parameters=case['parameters'])
    files = {}
    for chunk in data['index']['chunks']:
        for key in ('path', 'arrays_path'):
            files[chunk[key]] = sha(ref.artifact(directory, chunk[key]))
    for image in ref.read(directory / 'observations/index.json')['images']:
        files[image['path']] = sha(ref.artifact(directory, image['path']))
    receipt = dict(case=args.case, directory=str(directory), captured_unix_s=time.time(), step=step,
                   plan_sha256=sha(args.plan), input_sha256=checkpoint['input_sha256'],
                   immutable_prefix_sha256=files, retained_frames=len(data['frames']), solver_executed=False,
                   job_snapshot_sha256=sha(args.job_snapshot), process_receipt_sha256=sha(args.process_receipt),
                   native_exit_code=process['exit_code'])
    receipt['initial_hash_zero_signs'] = zero_signs
    write(out / 'pause-receipt.json', receipt)
    shutil.copy2(args.job_snapshot, out / 'paused-job.json')
    shutil.copy2(args.process_receipt, out / 'native-process.json')
    for name in ('checkpoint.json', checkpoint['checkpoint_path'], 'trajectory/index.json', 'measurements.json', 'observations/index.json', 'result.json', 'stdout.log', 'stderr.log'):
        if name.endswith('.log') and not (directory / name).exists():
            continue
        dest = out / name
        dest.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(directory / name, dest)
    print(json.dumps(dict(receipt=str(out / 'pause-receipt.json'), step=step, pinned_files=len(files))))
    return 0


def analytic_errors(ref, data, case):
    g, p = case['reference_geometry'], case['parameters']
    common = [i for i, step in enumerate(data['steps']) if step <= 4000 and step % 4 == 0]
    require(len(common) == 1001, 'Missing frozen common-time prefix')
    times = data['times'][common]
    scalar_q, scalar_v, residual = ref.kepler_reference(times, [[1., 0., 0.]], case['speed'])
    q0, tangent = np.asarray(g['centered']), np.asarray(g['tangent'])
    expected_q = scalar_q[:, 0, 0, None, None] * q0 + scalar_q[:, 0, 1, None, None] * tangent
    expected_q += np.asarray(g['com']) + times[:, None, None] * np.asarray(g['drift'])
    expected_v = scalar_v[:, 0, 0, None, None] * q0 + scalar_v[:, 0, 1, None, None] * tangent + np.asarray(g['drift'])
    qrms = float(np.sqrt(np.mean(np.sum((data['positions'][common] - expected_q) ** 2, axis=2))))
    vrms = float(np.sqrt(np.mean(np.sum((data['velocities'][common] - expected_v) ** 2, axis=2))))
    limit = 1.25e-5 if case['speed'] == 1 else 2e-4
    require(qrms < limit and vrms < limit, f'Frozen fine RMS tolerance: q={qrms},v={vrms}')
    return dict(position_rms_L0=qrms, velocity_rms_L0_T0=vrms, kepler_residual=residual, common_prefix_frames=len(common))


def period_in_plane(data, geometry):
    delta = data['positions'][:, 1] - data['positions'][:, 0]
    right = delta[0] / np.linalg.norm(delta[0])
    up = np.cross(np.asarray(geometry['normal']), right)
    phase = np.unwrap(np.arctan2(delta @ up, delta @ right))
    phase -= phase[0]
    crossings = np.flatnonzero(phase >= 2 * math.pi)
    require(len(crossings) and crossings[0] > 0, 'No retained full-orbit event bracket')
    i = int(crossings[0])
    t0, t1 = data['times'][i-1:i+1]
    a0, a1 = phase[i-1:i+1]
    require(a1 > a0, 'Period event must cross forward')
    return dict(period_T0=float(t0 + (2 * math.pi - a0) * (t1 - t0) / (a1 - a0)),
                bracket_T0=[float(t0), float(t1)], method='Linear interpolation of first full relative-angle advance in the preregistered orbital plane')


def verify(args):
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    report = dict(scope=args.scope, solver_executed=False, provider_called=False, native_ui_operated=False,
                  plan_sha256=sha(args.plan), mapping_sha256=sha(args.jobs), cases={}, passed=False,
                  full_installed_gate='Not decided by this numeric checker: ordinary-chat, view delivery, native interaction, interpretation and fresh reviewer evidence require their own receipts.')
    try:
        plan, ref, worker = load_plan(args.plan.resolve())
        mapping = read(args.jobs)
        require(set(mapping) == {x['name'] for x in plan['cases']}, 'Run mapping differs from preregistered case set')
        periods, cameras, lineages = {}, [], set()
        for case in plan['cases']:
            entry = mapping[case['name']]
            directory = Path(entry['directory']).resolve()
            result = dict(directory=str(directory), passed=False)
            report['cases'][case['name']] = result
            if args.scope == 'installed':
                job = read(entry['job_snapshot'])
                require(job['kind'] == 'solver' and job['state'] == 'completed' and job['id'] == directory.name,
                        'Installed job identity/status missing')
                require(job['parent_id'] and job['project_id'] and job['input']['engine'] == ref.ENGINE,
                        'Installed parent/project/engine lineage missing')
                require(datetime.fromisoformat(job['created_at']).timestamp() >= plan['prepared_unix_s'],
                        'Installed solver predates the frozen acceptance plan')
                parent = read(entry['session_snapshot'])
                require(parent['id'] == job['parent_id'] and parent['project_id'] == job['project_id']
                        and parent['kind'] == 'session' and parent['input'].get('model')
                        and parent['input'].get('provider') and parent['input'].get('reasoning_effort'),
                        'Ordinary-chat parent/model/effort receipt missing')
                require(sha(ref.artifact(directory, 'mechanics_worker.py')) == sha(worker), 'Installed worker differs from frozen source')
                result['job_snapshot_sha256'] = sha(entry['job_snapshot'])
                result['session_snapshot_sha256'] = sha(entry['session_snapshot'])
                result['parent_id'], result['project_id'] = job['parent_id'], job['project_id']
                lineages.add((job['parent_id'], job['project_id']))
                require(all(job['input']['parameters'].get(key) == value for key, value in case['parameters'].items()),
                        'Installed request parameters differ from preregistered inputs')
            extra = []
            if entry.get('pause_receipt'):
                pause = read(entry['pause_receipt'])
                pause_root = Path(entry['pause_receipt']).parent
                require(sha(pause_root / 'paused-job.json') == pause['job_snapshot_sha256']
                        and sha(pause_root / 'native-process.json') == pause['process_receipt_sha256'], 'Pause evidence bytes changed')
                paused_checkpoint = ref.read(pause_root / 'checkpoint.json')
                require(sha(ref.artifact(pause_root, paused_checkpoint['checkpoint_path'])) == paused_checkpoint['checkpoint_sha256'],
                        'Retained paused numerical checkpoint changed')
                require(pause['case'] == case['name'] and pause['plan_sha256'] == sha(args.plan)
                        and Path(pause['directory']).resolve() == directory, 'Pause lineage differs')
                require(all(sha(ref.artifact(directory, path)) == value for path, value in pause['immutable_prefix_sha256'].items()),
                        'Published prefix changed across resume')
                extra = [pause['step']]
                result['pause'] = dict(step=pause['step'], prefix_files=len(pause['immutable_prefix_sha256']), receipt_sha256=sha(entry['pause_receipt']))
            elif args.scope == 'installed' and case['name'] == 'installed-binary-control':
                raise AssertionError('The installed control requires actual cooperative pause/resume evidence')
            initial_parameters, zero_signs = initial_hash_parameters(ref, directory, case['parameters'])
            data = ref.inspect_artifacts(directory, initial_parameters, worker, extra_steps=extra, manifest_parameters=case['parameters'])
            result['initial_hash_zero_signs'] = zero_signs
            result.update(ref.invariant_errors(data, case['parameters']))
            result.update(analytic_errors(ref, data, case))
            result.update(period_in_plane(data, case['reference_geometry']))
            separation = [row['min_separation'] for row, step in zip(data['rows'], data['steps'], strict=True) if step <= 4000 and step % 4 == 0]
            result['minimum_separation_L0'] = min(separation)
            if case['speed'] == 1:
                require(max(abs(x - 1) for x in separation) < 2e-4, 'Circular distance tolerance')
            else:
                require(abs(min(separation) - 8 / 17) < 5e-4 and min(separation) < .5, 'Intervention minimum tolerance')
            result['period_relative_reference_error'] = abs(result['period_T0'] / case['expected_period_T0'] - 1)
            result['individual_period_error_scope'] = 'Diagnostic only; the preregistered acceptance threshold applies to the measured controlled ratio'
            # A legitimate off-grid pause endpoint must not move scientific image milestones.
            observed_steps = {x['step'] for x in ref.read(directory / 'observations/index.json')['images']}
            require({0, 1100, 2200, 3300, 4400}.issubset(observed_steps), 'Missing fixed regular-grid image milestones')
            result['images'] = ref.inspect_images(directory, data, require_milestones=False)
            result.update(frames=len(data['frames']), instrument_error=data['instrument_error'], acceleration_error=data['acceleration_error'], passed=True)
            periods[case['speed']] = result['period_T0']
            cameras.append(result['images']['camera'])
            files = {}
            for path in directory.rglob('*'):
                if path.is_file():
                    relative = path.relative_to(directory).as_posix()
                    files[relative] = sha(ref.artifact(directory, relative))
            write(out / (case['name'] + '-artifact-hashes.json'), files)
            result['artifact_count'] = len(files)
        if len(plan['cases']) == 2:
            if args.scope == 'installed':
                require(len(lineages) == 1, 'Controlled cases must belong to the same installed project and chat session')
            require(all(camera == cameras[0] for camera in cameras), 'Control and intervention cameras differ')
            ratio = periods[.8] / periods[1.]
            require(abs(ratio / (25 / 34) ** 1.5 - 1) <= .001, 'Frozen controlled period ratio tolerance')
            report['period_ratio'] = ratio
        else:
            control = plan['control_evidence']
            require(control and sha(control['acceptance_report']) == control['acceptance_report_sha256']
                    and sha(control['inventory']) == control['inventory_sha256'], 'Frozen baseline evidence changed')
            require(all(sha(ref.artifact(Path(control['directory']), path)) == value for path, value in read(control['inventory']).items()),
                    'Frozen baseline source bytes changed')
            if args.scope == 'installed':
                require(next(iter(lineages))[1] == control['project_id'], 'Reviewer variant must retain the baseline study project')
            speed = plan['cases'][0]['speed']
            ratio = periods[speed] / control['period_T0']
            expected = 1 if speed == 1 else (25 / 34) ** 1.5
            require(abs(ratio / expected - 1) <= .001, 'Frozen reviewer/control period-ratio tolerance')
            report['period_ratio'], report['control_evidence'] = ratio, control
        report['passed'] = True
    except Exception as error:
        report.update(failure=str(error), traceback=traceback.format_exc())
    write(out / 'report.json', report)
    print(json.dumps(dict(passed=report['passed'], scope=args.scope, solver_executed=False,
                          report=str(out / 'report.json'), failure=report.get('failure'))))
    return 0 if report['passed'] else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='mode', required=True)
    p = sub.add_parser('prepare')
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--reviewer-spec', type=Path)
    p = sub.add_parser('snapshot-paused')
    p.add_argument('--plan', type=Path, required=True)
    p.add_argument('--directory', type=Path, required=True)
    p.add_argument('--case', required=True)
    p.add_argument('--job-snapshot', type=Path, required=True)
    p.add_argument('--process-receipt', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p = sub.add_parser('verify')
    p.add_argument('--plan', type=Path, required=True)
    p.add_argument('--jobs', type=Path, required=True)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--scope', choices=['installed', 'component-fixture'], default='installed')
    args = parser.parse_args()
    output_existed = args.output.exists()
    try:
        return {'prepare': prepare, 'snapshot-paused': snapshot, 'verify': verify}[args.mode](args)
    except Exception as error:
        failure = dict(mode=args.mode, passed=False, failure=str(error), traceback=traceback.format_exc(),
                       solver_executed=False, provider_called=False)
        if not output_existed and args.output.is_dir() and not (args.output / 'entry-failure.json').exists():
            write(args.output / 'entry-failure.json', failure)
        print(json.dumps(failure))
        return 1


if __name__ == '__main__':
    raise SystemExit(main())
