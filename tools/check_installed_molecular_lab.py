"""Read-only numerical verification of ordinary-chat jobs; never prepares app answers."""
import argparse
import json
from pathlib import Path
from check_scientific_worker import artifact_test, read, sha


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--artifacts', type=Path, required=True)
    parser.add_argument('--cold', required=True)
    parser.add_argument('--hot', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'scope': 'execution and mathematical consistency, not empirical calibration', 'runs': []}
    for identity, target in [(args.cold, 90), (args.hot, 180)]:
        folder = args.artifacts / identity
        measurements = read(folder / 'measurements.json')
        tail = [r for r in measurements['series'] if r['time_ps'] >= 5 - 1e-9]
        mean = sum(r['temperature_kelvin'] for r in tail) / len(tail)
        checks = artifact_test(folder)
        report['runs'].append({'id': identity, 'target_kelvin': target, 'late_mean_kelvin': mean,
                               'relative_tolerance': .15, 'temperature_passed': abs(mean-target)/target < .15,
                               'minimum_frames': 100, 'artifacts': checks,
                               'input_sha256': sha((folder/'input.json').read_bytes()),
                               'result_sha256': sha((folder/'result.json').read_bytes())})
    report['temperature_difference_kelvin'] = report['runs'][1]['late_mean_kelvin'] - report['runs'][0]['late_mean_kelvin']
    report['passed'] = (report['temperature_difference_kelvin'] > 60 and
                        all(r['temperature_passed'] and r['artifacts']['passed'] and r['artifacts']['frame_count'] >= 100
                            for r in report['runs']))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report['passed'] else 1)


if __name__ == '__main__':
    main()
