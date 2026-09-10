"""Trusted data-only CAD/PCB worker. No generated Python or shell commands execute.

CadQuery/OpenCascade supplies actual Boolean solids and STEP/STL export. Native
KiCad supplies board parsing, design-rule checks and manufacturing file export.
"""
import argparse
import csv
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time
import zipfile

DOWNLOADABLE_ARTIFACTS = frozenset({
    'model.step', 'model.stl', 'board.kicad_pcb', 'board.glb', 'board.svg',
    'bom.csv', 'drc.json', 'manufacturing.zip', 'editable-project.zip',
})


def kicad_path():
    configured = os.environ.get('KICAD_CLI')
    if configured and Path(configured).is_file():
        return configured
    found = shutil.which('kicad-cli')
    if found:
        return found
    for root in [Path(os.environ.get('ProgramFiles', 'C:/Program Files')) / 'KiCad',
                 Path(os.environ.get('LOCALAPPDATA', '.')) / 'Programs' / 'KiCad']:
        if root.is_dir():
            for candidate in sorted(root.glob('*/bin/kicad-cli.exe'), reverse=True):
                return str(candidate)
    portable = Path(os.environ.get('LOCALAPPDATA', '.')) / 'PhaseForge' / 'engines'
    for candidate in sorted(portable.glob('kicad-*/bin/kicad-cli.exe'), reverse=True):
        return str(candidate)
    return None


def validate(recipe, schema=None):
    if schema is not None:
        from jsonschema import validate as schema_validate
        schema_validate(recipe, schema)
    def bounded(value, low, high, name):
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not low <= value <= high:
            raise ValueError(f'{name} must be finite and between {low} and {high}')
    def strings(value):
        if isinstance(value, dict):
            for child in value.values(): strings(child)
        elif isinstance(value, list):
            for child in value: strings(child)
        elif isinstance(value, str) and (len(value) > 4000 or any(ord(c) < 32 for c in value)):
            raise ValueError('Text must be bounded and contain no control characters')
        elif isinstance(value, (int, float)):
            bounded(value, -100000, 100000, 'coordinate/value')
    strings(recipe)
    if recipe.get('units') != 'mm': raise ValueError('Fabrication uses millimeters')
    if recipe.get('kind') == 'cad':
        cad = recipe.get('cad')
        if not cad or recipe.get('pcb') is not None or not 1 <= len(cad['parts']) <= 128:
            raise ValueError('CAD requires 1–128 parts and pcb:null')
        names = set()
        for i, part in enumerate(cad['parts']):
            if not part['id'].strip() or part['id'] in names: raise ValueError('Part IDs must be unique and nonempty')
            names.add(part['id'])
            if i == 0 and part['operation'] != 'add': raise ValueError('First solid must be additive')
            if part['shape'] == 'box':
                for v in part['size']: bounded(v, 0.001, 10000, 'box size')
            elif part['shape'] in ('cylinder', 'cone'):
                bounded(part['radius'], 0.001, 10000, 'radius')
                bounded(part['size'][2], 0.001, 10000, 'height')
                bounded(part['top_radius'], 0, 10000, 'top radius')
            elif part['shape'] == 'sphere': bounded(part['radius'], 0.001, 10000, 'radius')
            elif part['shape'] == 'extruded_polygon':
                bounded(part['size'][2], 0.001, 10000, 'extrusion height')
                if not 3 <= len(part['points']) <= 512: raise ValueError('Polygon requires 3–512 points')
            else: raise ValueError('Unsupported CAD shape')
        bounded(cad['fillet_radius'], 0, 100, 'fillet radius')
    elif recipe.get('kind') == 'pcb':
        pcb = recipe.get('pcb')
        if not pcb or recipe.get('cad') is not None: raise ValueError('PCB requires pcb and cad:null')
        bounded(pcb['width_mm'], 5, 1000, 'board width')
        bounded(pcb['height_mm'], 5, 1000, 'board height')
        bounded(pcb['thickness_mm'], 0.2, 10, 'board thickness')
        nets = pcb['nets']
        if len(nets) != len(set(nets)) or any(not n.strip() for n in nets): raise ValueError('Net names must be unique and nonempty')
        refs = set()
        def point(p):
            bounded(p[0], 0, pcb['width_mm'], 'board x')
            bounded(p[1], 0, pcb['height_mm'], 'board y')
        for comp in pcb['components']:
            import re
            if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,31}',comp['reference']) or comp['reference'] in refs: raise ValueError('Component references must be unique identifiers such as R1 or J2')
            refs.add(comp['reference']); point(comp['position'])
            nums = set()
            for pad in comp['pads']:
                if not pad['number'] or pad['number'] in nums: raise ValueError('Pad numbers must be unique within a component')
                nums.add(pad['number'])
                if pad['net'] and pad['net'] not in nets: raise ValueError('Pad references an undeclared net')
                for v in pad['size']: bounded(v, 0.05, 100, 'pad size')
                bounded(pad['drill'], 0, min(pad['size']), 'drill')
                if pad['kind'] == 'through_hole' and not 0 < pad['drill'] < min(pad['size']): raise ValueError('Through-hole pad needs a smaller positive drill')
                if pad['kind'] == 'smd' and pad['drill'] != 0: raise ValueError('SMD pad cannot have a drill')
                angle = math.radians(comp['rotation']); x, y = pad['position']; cx, cy = comp['position']
                point([cx + x * math.cos(angle) - y * math.sin(angle), cy + x * math.sin(angle) + y * math.cos(angle)])
        for track in pcb['tracks']:
            if track['net'] not in nets: raise ValueError('Track references an undeclared net')
            bounded(track['width_mm'], 0.05, 20, 'track width')
            if len(track['points']) < 2: raise ValueError('Track requires at least two points')
            for p in track['points']: point(p)
        for hole in pcb['mounting_holes']:
            point(hole['position']); bounded(hole['diameter_mm'], 0.1, 30, 'mounting drill')
    else: raise ValueError('Choose CAD or PCB')


def build_cad(recipe, out):
    import cadquery as cq
    result = None
    for part in recipe['cad']['parts']:
        w, d, h = part['size']; shape = part['shape']; r = part['radius']
        if shape == 'box': solid = cq.Workplane('XY').box(w, d, h)
        elif shape == 'cylinder': solid = cq.Workplane('XY').circle(r).extrude(h).translate((0, 0, -h / 2))
        elif shape == 'sphere': solid = cq.Workplane('XY').sphere(r)
        elif shape == 'cone':
            solid = cq.Workplane('XY').add(cq.Solid.makeCone(r, part['top_radius'], h)).translate((0, 0, -h / 2))
        else: solid = cq.Workplane('XY').polyline(part['points']).close().extrude(h).translate((0, 0, -h / 2))
        for axis, angle in zip([(1, 0, 0), (0, 1, 0), (0, 0, 1)], part['rotation']):
            if angle: solid = solid.rotate((0, 0, 0), axis, angle)
        solid = solid.translate(tuple(part['position']))
        result = solid if result is None else (result.union(solid) if part['operation'] == 'add' else result.cut(solid))
    if recipe['cad']['fillet_radius']:
        result = result.edges().fillet(recipe['cad']['fillet_radius'])
    shape = result.val()
    if not shape.isValid() or not shape.Solids() or shape.Volume() <= 1e-9:
        raise ValueError('CAD Boolean result is not a valid positive-volume solid')
    cq.exporters.export(result, str(out / 'model.step'))
    cq.exporters.export(result, str(out / 'model.stl'), tolerance=0.05, angularTolerance=0.1)
    box = shape.BoundingBox()
    return {'engine': 'CadQuery ' + cq.__version__, 'valid_solid': True,
            'solid_count': len(shape.Solids()), 'volume_mm3': shape.Volume(),
            'bounds_mm': [box.xlen, box.ylen, box.zlen],
            'notice': 'Geometrically valid solid. Material, loads, fit, tolerances and fabrication process still require engineering review.'}


def quoted(s):
    return json.dumps(str(s), ensure_ascii=False)


def board_text(pcb, library=None):
    """Self-contained modern KiCad board; no machine-specific footprint libraries."""
    lines = ['(kicad_pcb (version 20240108) (generator "phaseforge")',
             f'(general (thickness {pcb["thickness_mm"]}))', '(paper "A4")',
             '(layers (0 "F.Cu" signal) (31 "B.Cu" signal) (36 "B.SilkS" user "b.silkscreen") (37 "F.SilkS" user "f.silkscreen") (38 "B.Mask" user) (39 "F.Mask" user) (44 "Edge.Cuts" user) (46 "B.CrtYd" user) (47 "F.CrtYd" user) (48 "B.Fab" user) (49 "F.Fab" user))',
             '(setup (pad_to_mask_clearance 0))', '(net 0 "")']
    nets = {'': 0, **{n: i + 1 for i, n in enumerate(pcb['nets'])}}
    lines.extend(f'(net {i} {quoted(n)})' for n, i in nets.items() if i)
    lines.append(f'(gr_rect (start 0 0) (end {pcb["width_mm"]} {pcb["height_mm"]}) (stroke (width 0.05) (type default)) (fill none) (layer "Edge.Cuts"))')
    for comp in pcb['components']:
        if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,31}', comp['reference']):
            raise ValueError('Component references must be safe footprint identifiers')
        start = len(lines)
        x, y = comp['position']; angle = comp['rotation']; w, d, _ = comp['body_size']
        lines += [f'(footprint {quoted("PhaseForge:"+comp["reference"])} (layer "F.Cu") (at {x} {y} {angle})',
                  f'(property "Reference" {quoted(comp["reference"])} (at 0 {-max(d/2+1.5, 2)} {angle}) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))',
                  f'(property "Value" {quoted(comp["value"])} (at 0 {max(d/2+1.5, 2)} {angle}) (layer "F.Fab") (effects (font (size 1 1) (thickness 0.15))))']
        if w > 0 and d > 0:
            lines.append(f'(fp_rect (start {-w/2} {-d/2}) (end {w/2} {d/2}) (stroke (width 0.15) (type default)) (fill none) (layer "F.SilkS"))')
            lines.append(f'(fp_rect (start {-w/2-0.25} {-d/2-0.25}) (end {w/2+0.25} {d/2+0.25}) (stroke (width 0.05) (type default)) (fill none) (layer "F.CrtYd"))')
        for pad in comp['pads']:
            px, py = pad['position']; sx, sy = pad['size']; kind = 'thru_hole' if pad['kind'] == 'through_hole' else 'smd'
            layers = '"*.Cu" "*.Mask"' if kind == 'thru_hole' else f'"{pad["layer"]}" "{pad["layer"][0]}.Mask"'
            drill = f'(drill {pad["drill"]})' if kind == 'thru_hole' else ''
            lines.append(f'(pad {quoted(pad["number"])} {kind} {pad["shape"]} (at {px} {py} {angle}) (size {sx} {sy}) {drill} (layers {layers}) (net {nets[pad["net"]]} {quoted(pad["net"])}))')
        lines.append(')')
        if library is not None:
            footprint = '\n'.join(lines[start:])
            footprint = footprint.replace(quoted('PhaseForge:'+comp['reference']), quoted(comp['reference']), 1)
            footprint = footprint.replace(f'(at {x} {y} {angle})', '(at 0 0 0)', 1)
            footprint = re.sub(r'\(net \d+ "(?:[^"\\]|\\.)*"\)', '', footprint)
            # Board coordinates do not belong in the reusable library footprint.
            footprint = footprint.replace(f'(at 0 {-max(d/2+1.5, 2)} {angle})', f'(at 0 {-max(d/2+1.5, 2)} 0)')
            footprint = footprint.replace(f'(at 0 {max(d/2+1.5, 2)} {angle})', f'(at 0 {max(d/2+1.5, 2)} 0)')
            for pad in comp['pads']:
                px, py = pad['position']; footprint = footprint.replace(f'(at {px} {py} {angle})', f'(at {px} {py} 0)')
            (library / (comp['reference']+'.kicad_mod')).write_text(footprint, encoding='utf-8')
    for i, hole in enumerate(pcb['mounting_holes']):
        x, y = hole['position']; d = hole['diameter_mm']
        hole_name = f'PhaseForgeHole{i+1}'
        references = {c['reference'] for c in pcb['components']}
        while hole_name in references: hole_name += '_'
        lines.append(f'(footprint {quoted("PhaseForge:"+hole_name)} (layer "F.Cu") (at {x} {y}) (attr board_only exclude_from_pos_files exclude_from_bom) (pad "" np_thru_hole circle (at 0 0) (size {d} {d}) (drill {d}) (layers "*.Cu" "*.Mask")))')
    for track in pcb['tracks']:
        for a, b in zip(track['points'], track['points'][1:]):
            lines.append(f'(segment (start {a[0]} {a[1]}) (end {b[0]} {b[1]}) (width {track["width_mm"]}) (layer {quoted(track["layer"])}) (net {nets[track["net"]]}))')
    for mark in pcb['silkscreen']:
        x, y = mark['position']; s = mark['size_mm']
        lines.append(f'(gr_text {quoted(mark["text"])} (at {x} {y}) (layer "F.SilkS") (effects (font (size {s} {s}) (thickness 0.15))))')
    return '\n'.join(lines + [')'])


def normalize_kicad_footprints(board_path):
    """Run inside KiCad's Python: serialize native board/library copies together.

    KiCad applies version-specific pad/text defaults while reading board files.
    Saving its own parsed objects avoids differences from hand-written library
    text without disabling library mismatch or any other design-rule check.
    """
    import pcbnew
    board_path = Path(board_path).resolve()
    library = board_path.parent / 'PhaseForge.pretty'
    if library.is_symlink() or library.resolve().parent != board_path.parent:
        raise ValueError('Footprint library must remain inside the job directory')
    library.mkdir(exist_ok=True)
    board = pcbnew.LoadBoard(str(board_path))
    seen = set()
    for footprint in board.GetFootprints():
        name = str(footprint.GetFPID().GetLibItemName())
        if not re.fullmatch(r'[A-Za-z][A-Za-z0-9_]{0,63}', name) or name in seen:
            raise ValueError('Generated footprint names must be unique safe identifiers')
        seen.add(name)
        footprint_path = library / (name + '.kicad_mod')
        if footprint_path.is_symlink() or footprint_path.resolve().parent != library.resolve():
            raise ValueError('Footprint files must remain inside the job library')
        copy = pcbnew.FOOTPRINT(footprint)
        copy.SetOrientationDegrees(0)
        copy.SetPosition(pcbnew.VECTOR2I(0, 0))
        for pad in copy.Pads():
            pad.SetNetCode(0)
        pcbnew.FootprintSave(str(library), copy)
    if not pcbnew.SaveBoard(str(board_path), board):
        raise RuntimeError('KiCad could not save the normalized board')


def normalize_with_kicad(cli, board, deadline):
    bundled = Path(cli).resolve().parent / ('python.exe' if os.name == 'nt' else 'python3')
    interpreter = str(bundled) if bundled.is_file() else (sys.executable if importlib.util.find_spec('pcbnew') else shutil.which('python3'))
    if not interpreter:
        raise RuntimeError('KiCad Python bindings are required to normalize generated footprint libraries')
    proc = subprocess.run([interpreter, str(Path(__file__).resolve()), '--normalize-kicad-footprints', str(board)],
                          capture_output=True, text=True, timeout=max(1, deadline-time.monotonic()),
                          creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
    if proc.returncode:
        raise RuntimeError('KiCad footprint normalization failed: ' + (proc.stdout + proc.stderr)[-4000:])


def build_pcb(recipe, out, deadline):
    pcb = recipe['pcb']; board = out / 'board.kicad_pcb'
    library = out/'PhaseForge.pretty'; library.mkdir(exist_ok=True)
    board.write_text(board_text(pcb,library), encoding='utf-8')
    (out/'fp-lib-table').write_text('(fp_lib_table (version 7) (lib (name "PhaseForge") (type "KiCad") (uri "${KIPRJMOD}/PhaseForge.pretty") (options "") (descr "Embedded generated footprints")))',encoding='utf-8')
    with (out / 'bom.csv').open('w', newline='', encoding='utf-8') as f:
        writer = csv.writer(f); writer.writerow(['Reference', 'Value', 'Pads', 'X_mm', 'Y_mm', 'Rotation_deg'])
        for c in pcb['components']: writer.writerow([c['reference'], c['value'], len(c['pads']), *c['position'], c['rotation']])
    result = {'engine': 'PhaseForge declarative PCB / KiCad', 'drc_status': 'not_run', 'manufacturing_files': False,
              'notice': 'Editable board design. Electrical function, component ratings and schematic parity have not been verified.'}
    cli = kicad_path()
    if not cli:
        result['notice'] += ' Install KiCad to run native design-rule checks and export Gerbers/drills.'
        return result
    normalize_with_kicad(cli, board, deadline)
    def run(args, allowed=(0,)):
        proc = subprocess.run([cli, *args], capture_output=True, text=True,
                              timeout=max(1, deadline-time.monotonic()),
                              creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
        if proc.returncode not in allowed:
            raise RuntimeError('KiCad failed: ' + (proc.stdout + proc.stderr)[-4000:])
        return proc
    run(['pcb', 'drc', '--format', 'json', '--exit-code-violations', '-o', str(out/'drc.json'), str(board)], (0, 5))
    report = json.loads((out/'drc.json').read_text(encoding='utf-8'))
    violations = report.get('violations', []) + report.get('unconnected_items', []) + report.get('schematic_parity', [])
    result.update(drc_status='issues_found' if violations else 'passed', drc_issue_count=len(violations))
    with zipfile.ZipFile(out/'editable-project.zip','w',zipfile.ZIP_DEFLATED) as archive:
        editable = [board,out/'fp-lib-table',out/'bom.csv',*library.iterdir()]
        if (out/'board.kicad_pro').is_file(): editable.append(out/'board.kicad_pro')
        for f in editable: archive.write(f,str(f.relative_to(out)))
    # Retain a viewable native 3D board even when DRC reports unresolved issues.
    run(['pcb', 'export', 'glb', '--board-only', '-o', str(out/'board.glb'), str(board)])
    run(['pcb', 'export', 'svg', '--layers', 'F.Cu,B.Cu,Edge.Cuts,F.SilkS', '--mode-single', '-o', str(out/'board.svg'), str(board)])
    if not violations:
        manufacture = out/'manufacturing'; manufacture.mkdir(exist_ok=True)
        run(['pcb', 'export', 'gerbers', '-o', str(manufacture), str(board)])
        run(['pcb', 'export', 'drill', '-o', str(manufacture), str(board)])
        with zipfile.ZipFile(out/'manufacturing.zip', 'w', zipfile.ZIP_DEFLATED) as archive:
            for f in sorted(manufacture.iterdir()):
                if f.is_file(): archive.write(f, f.name)
        result['manufacturing_files'] = True
    else: result['notice'] += ' Manufacturing export withheld until the reported DRC issues are corrected.'
    return result


def main():
    parser = argparse.ArgumentParser(); parser.add_argument('--input'); parser.add_argument('--output'); parser.add_argument('--schema'); parser.add_argument('--max-seconds', type=int, default=300); parser.add_argument('--probe', action='store_true'); parser.add_argument('--normalize-kicad-footprints')
    args = parser.parse_args()
    if args.normalize_kicad_footprints:
        normalize_kicad_footprints(args.normalize_kicad_footprints); return
    if args.probe:
        print(json.dumps({'cadquery_available': importlib.util.find_spec('cadquery') is not None, 'kicad_available': bool(kicad_path())})); return
    if not 1 <= args.max_seconds <= 1800: raise ValueError('Worker deadline must be 1–1800 seconds')
    watchdog = threading.Timer(args.max_seconds, lambda: os._exit(124)); watchdog.daemon = True; watchdog.start()
    deadline = time.monotonic() + args.max_seconds
    recipe = json.loads(Path(args.input).read_text(encoding='utf-8'))
    validate(recipe, json.loads(Path(args.schema).read_text(encoding='utf-8')))
    out = Path(args.output); out.mkdir(parents=True, exist_ok=True)
    result = build_cad(recipe, out) if recipe['kind'] == 'cad' else build_pcb(recipe, out, deadline)
    # Advertise only the actual API downloads. KiCad's local preferences/project
    # auxiliaries remain in the editable ZIP instead of producing broken links.
    result['units'] = 'mm'; result['files'] = sorted(p.name for p in out.iterdir() if p.is_file() and p.name in DOWNLOADABLE_ARTIFACTS)
    (out/'result.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result)); watchdog.cancel()


if __name__ == '__main__':
    main()
