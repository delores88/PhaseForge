"""Read-only validation of a deliberately interrupted installed solver job."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
from urllib.request import Request, urlopen


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--before', type=Path, required=True)
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--base-url', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    before = read(args.before)
    def job(identity):
        request = Request(f'{args.base_url}/api/laboratory/jobs/{identity}', headers={'Origin': args.base_url})
        with urlopen(request) as response:
            return json.load(response)
    solver = job(before['solver']['id'])
    session = job(before['session']['id'])
    root = args.artifacts / solver['id']
    index = read(root / 'trajectory/index.json')
    checkpoint = read(root / 'checkpoint.json')
    result = read(root / 'result.json')
    image_bytes = (root / 'observations/frame-00100000.png').read_bytes()
    journal = read(args.artifacts / session['id'] / 'journal.json')
    image_supplied = any(part.get('type') == 'input_image' and
                         part.get('image_url') == 'data:image/png;base64,' + base64.b64encode(image_bytes).decode()
                         for item in journal.get('items', []) for part in (item.get('content') if isinstance(item.get('content'), list) else []))
    prefix_unchanged = all(index['chunks'][i] == chunk and digest(root / chunk['path']) == chunk['sha256']
                           and digest(root / chunk['arrays_path']) == chunk['arrays_sha256']
                           for i, chunk in enumerate(before['index']['chunks']))
    steps, times, hashes_ok = [], [], True
    for chunk in index['chunks']:
        hashes_ok &= digest(root / chunk['path']) == chunk['sha256']
        hashes_ok &= digest(root / chunk['arrays_path']) == chunk['arrays_sha256']
        frames = read(root / chunk['path'])['frames']
        steps.extend(frame['step'] for frame in frames)
        times.extend(frame['time'] for frame in frames)
    checks = {
        'same_solver_completed': solver['state'] == 'completed',
        'session_completed': session['state'] == 'completed',
        'immutable_input_unchanged': solver['input'] == before['solver']['input'],
        'absolute_deadline_unchanged': session['deadline_at'] == before['session']['deadline_at'],
        'input_hash_unchanged': checkpoint['input_sha256'] == before['checkpoint']['input_sha256'] == result['input_sha256'],
        'committed_prefix_unchanged': prefix_unchanged,
        'original_checkpoint_retained': digest(root / before['checkpoint']['checkpoint_path']) == before['checkpoint']['checkpoint_sha256'],
        'all_chunk_hashes_valid': hashes_ok,
        'exact_5001_steps': steps == list(range(0, 100001, 20)) and index['frame_count'] == 5001,
        'monotone_100ps_timeline': all(b > a for a, b in zip(times, times[1:])) and abs(times[-1] - 100) < 1e-7,
        'final_checkpoint': checkpoint['step'] == 100000 and digest(root / checkpoint['checkpoint_path']) == checkpoint['checkpoint_sha256'],
        'provider_request_retrieved': any(event['kind'] == 'reconciling' for event in session['events']),
        'final_native_image_observed': image_supplied and any(event['kind'] == 'tool_completed' and
             event.get('data', {}).get('output', {}).get('evidence', {}).get('sha256') == hashlib.sha256(image_bytes).hexdigest()
             for event in session['events']),
    }
    report = {'passed': all(checks.values()), 'checks': checks, 'solver_id': solver['id'], 'session_id': session['id'],
              'interrupted_committed_step': before['checkpoint']['step'], 'retained_prefix_frames': before['index']['frame_count'],
              'final_frame_count': len(steps), 'final_time_ps': times[-1], 'original_deadline': session['deadline_at'],
              'result_sha256': digest(root / 'result.json'), 'scope': 'execution recovery and retained numerical provenance'}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report['passed'] else 1)


if __name__ == '__main__':
    main()
