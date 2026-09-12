# SPDX-License-Identifier: MIT OR GPL-3.0-or-later
"""Trusted, data-only Blender worker for retained numerical trajectories.

blender --background --factory-startup --disable-autoexec --python this.py --
    --input export-input.json --output export-directory

No solver is imported or executed. JSON positions are the only motion source.
The backend owns process-tree cancellation/deadlines and supplies trusted paths.
"""
import argparse
import bisect
from collections import OrderedDict
import hashlib
import json
import math
import os
from pathlib import Path
import sys
import threading
import time

import bpy
from mathutils import Vector, Matrix


def atomic_json(path, value):
    temporary = path.with_suffix(path.suffix + '.tmp')
    temporary.write_text(json.dumps(value, separators=(',', ':'), allow_nan=False), encoding='utf-8')
    temporary.replace(path)


def sha256(path):
    with path.open('rb') as handle:
        return hashlib.file_digest(handle, 'sha256').hexdigest()


def committed_digest(value):
    if not isinstance(value, str) or len(value) != 64 or any(c not in '0123456789abcdefABCDEF' for c in value):
        raise ValueError('A nonempty 64-hex committed scientific digest is required')
    return value.lower()


def pinned_source_index(index_path, pin, entry_key):
    """Freeze selection while permitting a live solver to append new records."""
    live_bytes = index_path.read_bytes()
    if len(live_bytes) > 64 * 1024 * 1024:
        raise ValueError('Source index exceeds the bounded metadata size')
    live = json.loads(live_bytes)
    if pin is None:
        return live, hashlib.sha256(live_bytes).hexdigest(), None
    if not isinstance(pin, dict):
        raise ValueError('source_pin must be an object')
    snapshot_path = Path(pin['index_snapshot_path'])
    if not snapshot_path.is_absolute() or not snapshot_path.is_file():
        raise ValueError('Pinned index snapshot must be an absolute trusted metadata file')
    snapshot_bytes = snapshot_path.read_bytes()
    if len(snapshot_bytes) > 64 * 1024 * 1024:
        raise ValueError('Pinned index exceeds the bounded metadata size')
    digest = hashlib.sha256(snapshot_bytes).hexdigest()
    if digest != committed_digest(pin.get('index_sha256')):
        raise ValueError('Pinned index snapshot digest changed')
    snapshot = json.loads(snapshot_bytes)
    mutable = {'chunks', 'frames', 'end_time', 'frame_count'}
    metadata = lambda value: {key: item for key, item in value.items() if key not in mutable}
    if metadata(snapshot) != metadata(live):
        raise ValueError('Live source immutable metadata differs from the pinned index')
    entries, current = snapshot[entry_key], live[entry_key]
    number = integer(pin.get('entry_number'), 'pinned entry', 0, len(entries) - 1)
    if number >= len(current) or current[number] != entries[number]:
        raise ValueError('Live source entry differs from the pinned scientific record')
    files = pin.get('files')
    if not isinstance(files, dict) or not files:
        raise ValueError('Pinned source files and their scientific digests are required')
    for name, expected in files.items():
        if not isinstance(name, str) or not name or '\\' in name or ':' in name or any(part in ('', '.', '..') for part in name.split('/')):
            raise ValueError('Invalid pinned source file path')
        committed_digest(expected)
    return snapshot, digest, number


def verify_source_bytes(raw, expected, relative, pin=None):
    """Hash the exact buffer that will be parsed, against index and request pin."""
    digest = hashlib.sha256(raw).hexdigest()
    if digest != committed_digest(expected):
        raise ValueError('Recorded source no longer matches its scientific digest')
    if pin is not None and digest != committed_digest(pin['files'].get(relative)):
        raise ValueError('Loaded source bytes differ from the pinned scientific digest')
    return digest


def publication_metadata(config, source):
    """Verify data-only publication identity supplied by the trusted coordinator.

    Descriptions are displayed as plain text, never evaluated as code or HTML.
    This receipt attests to retained bytes, not to generated-model validity.
    """
    metadata = config.get('source_metadata')
    if metadata is None:
        return None
    if not isinstance(metadata, dict) or metadata.get('kind') != 'published_simulation' or metadata.get('scientific_validation') != 'not_established_by_publication':
        raise ValueError('Invalid numerical publication metadata')
    if source.index_sha256 != committed_digest(metadata.get('index_sha256')):
        raise ValueError('Published index differs from its admitted immutable receipt')
    representation = source.index.get('representation')
    if representation != metadata.get('representation') or representation not in ('particle_trajectory', 'scalar_field'):
        raise ValueError('Published representation changed')
    model = metadata.get('model')
    if not isinstance(model, dict) or any(not isinstance(model.get(key), str) or not model[key].strip() or len(model[key]) > maximum for key, maximum in (('id', 120), ('description', 4000), ('scope', 4000))):
        raise ValueError('Published model and scope declaration missing')
    if not isinstance(model.get('limitations'), list) or not 1 <= len(model['limitations']) <= 32 or any(not isinstance(v, str) or not v.strip() or len(v) > 1000 for v in model['limitations']):
        raise ValueError('Published model limitations missing')
    for relative, key in (('source-simulation.json', 'source_sha256'), ('source-execution-manifest.json', 'source_manifest_sha256')):
        path = source.root / relative
        if path.stat().st_size > 64 * 1024 * 1024:
            raise ValueError('Publication source exceeds bounded metadata size')
        raw = path.read_bytes()
        if len(raw) > 64 * 1024 * 1024 or hashlib.sha256(raw).hexdigest() != committed_digest(metadata.get(key)):
            raise ValueError('Published original source bytes changed')
        if relative == 'source-simulation.json':
            original = json.loads(raw)
            if original.get('schema') != 'phaseforge.simulation.v1' or original.get('model') != model or original.get('representation') != representation:
                raise ValueError('Published model declaration differs from original data')
    committed_digest(metadata.get('source_code_sha256'))
    if representation == 'particle_trajectory':
        if source.index.get('boundary') != 'isolated' or source.topology.get('boundary') != 'isolated' or source.index.get('wrapping') or source.topology.get('box') or source.topology.get('box_nm'):
            raise ValueError('Published isolated particles cannot acquire a periodic boundary')
        if source.topology_sha256 != committed_digest(metadata.get('topology_sha256')):
            raise ValueError('Published topology differs from its admitted receipt')
    return metadata


def publication_title(metadata):
    if metadata is None:
        return None
    name = ' '.join(metadata['model']['id'].split())[:60]
    return f"PhaseForge | {name}\nGenerated numerical data; model validity not established"


def finite(value, name, minimum, maximum):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not minimum <= value <= maximum:
        raise ValueError(f'{name} must be a finite number in [{minimum}, {maximum}]')
    return value


def integer(value, name, minimum, maximum):
    value = finite(value, name, minimum, maximum)
    if int(value) != value:
        raise ValueError(f'{name} must be an integer')
    return int(value)


def retained_time(value, minimum, maximum):
    finite(minimum, 'trajectory start', -1e12, 1e12)
    finite(maximum, 'trajectory end', minimum, 1e12)
    tolerance = 64 * sys.float_info.epsilon * max(abs(minimum), abs(maximum), abs(maximum-minimum), sys.float_info.min)
    finite(value, 'scientific time', minimum-tolerance, maximum+tolerance)
    # Preserve distinct in-range records. Only recorded_index knows their gaps
    # and can safely snap a round-tripped timestamp near a retained state.
    return min(maximum, max(minimum, value))


def recorded_index(times, timestamp):
    """Hold a saved state; snap only floating-point round trips near a record."""
    following = bisect.bisect_right(times, timestamp)
    if following < len(times):
        candidate = times[following]
        tolerance = 64 * sys.float_info.epsilon * max(abs(candidate), abs(timestamp), sys.float_info.min)
        # Close or very large timestamps must never erase a meaningful interval.
        if following:
            tolerance = min(tolerance, (candidate - times[following - 1]) / 4)
        if candidate - timestamp <= tolerance:
            return following
    return max(0, following - 1)


def export_frame_count(config, duration, fps, mode):
    if mode == 'png':
        return 1
    # The server sends a validated integer. Legacy direct invocations use the
    # same positive half-up rule as Rust, never Python's ties-to-even round().
    expected = math.floor(duration * fps + .5)
    count = integer(config.get('frame_count', expected), 'frame count', 2, 216000)
    if count != expected:
        raise ValueError('frame_count disagrees with playback duration and fps')
    return count


def vec(value):
    if not isinstance(value, (list, tuple)) or len(value) != 3:
        raise ValueError('A camera/particle vector must contain exactly three coordinates')
    return Vector([finite(v, 'coordinate', -1e12, 1e12) for v in value])


def rgb(value, fallback='#9f8ded'):
    value = value or fallback
    if not isinstance(value, str) or len(value) != 7 or not value.startswith('#'):
        raise ValueError('Display colors must use #RRGGBB')
    color = [int(value[i:i+2], 16) / 255 for i in (1, 3, 5)]
    return tuple(v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4 for v in color)


def particle_radius(entity, fallback=.017):
    return finite(entity.get('display_radius', entity.get('radius_nm', entity.get('radius', fallback))), 'display radius', 1e-12, 1e12)


def normalized_position(position, center, scale):
    if not isinstance(position, (list, tuple)) or len(position) != 3:
        raise ValueError('A physical position needs three finite coordinates')
    # Blender vectors are float32. Preserve small separations after large source
    # translations by subtracting the center in Python float64 first.
    return [(finite(value, 'coordinate', -1e12, 1e12)-center[axis])*scale for axis, value in enumerate(position)]


def particle_domain(source):
    """One physical frame for all retained states; marker radii do not define physics."""
    topology, index = source.topology, source.index
    boundary = index.get('boundary', topology.get('boundary'))
    periodic = index.get('wrapping', '').startswith('periodic') or boundary == 'periodic'
    box = topology.get('box_nm', topology.get('box'))
    if boundary == 'isolated' and (periodic or box is not None):
        raise ValueError('An isolated trajectory cannot contain a periodic box or wrapping')
    units = {'position': topology.get('position_unit') or index.get('position_unit') or topology.get('units', {}).get('position') or index.get('units', {}).get('position') or 'position units',
             'time': index.get('time_unit') or index.get('units', {}).get('time') or topology.get('units', {}).get('time') or 'time units'}
    if periodic:
        if not isinstance(box, list) or len(box) != 3:
            raise ValueError('A periodic trajectory requires a positive physical box')
        return {'min': [0., 0., 0.], 'max': [finite(v, 'periodic length', 1e-12, 1e12) for v in box], 'periodic': True, 'units': units, 'basis': 'periodic cell'}
    low, high = [math.inf]*3, [-math.inf]*3
    for number, chunk in enumerate(source.chunks):
        bounds = chunk.get('bounds')
        if bounds is not None:
            if not isinstance(bounds, dict) or len(bounds.get('min', [])) != 3 or len(bounds.get('max', [])) != 3:
                raise ValueError('Invalid retained particle bounds')
            pairs = [(finite(a, 'lower bound', -1e12, 1e12), finite(b, 'upper bound', -1e12, 1e12)) for a, b in zip(bounds['min'], bounds['max'])]
            if any(a > b for a, b in pairs):
                raise ValueError('Retained particle bounds are reversed')
        else:
            if source.pin is not None:
                raise ValueError('Pinned isolated observations require committed chunk bounds')
            points = (entity['position'] for frame in source.chunk(number) for entity in frame['entities'])
            chunk_low, chunk_high = [math.inf]*3, [-math.inf]*3
            for point in points:
                for axis in range(3):
                    chunk_low[axis], chunk_high[axis] = min(chunk_low[axis], point[axis]), max(chunk_high[axis], point[axis])
            pairs = zip(chunk_low, chunk_high)
        for axis, (a, b) in enumerate(pairs):
            low[axis], high[axis] = min(low[axis], a), max(high[axis], b)
    if not all(math.isfinite(v) for v in low+high) or max(b-a for a, b in zip(low, high)) <= 0:
        raise ValueError('No nondegenerate retained particle domain is available')
    return {'min': low, 'max': high, 'periodic': False, 'units': units, 'basis': 'all retained numerical states'}


class Trajectory:
    def __init__(self, root, pin=None):
        self.root = root.resolve(strict=True)
        self.index_path = self.root / 'trajectory/index.json'
        self.pin = pin
        self.index, self.index_sha256, self.pin_entry = pinned_source_index(self.index_path, pin, 'chunks')
        topology_bytes = (self.root / 'topology.json').read_bytes()
        self.topology_sha256 = hashlib.sha256(topology_bytes).hexdigest()
        if 'topology_sha256' in self.index and self.topology_sha256 != committed_digest(self.index['topology_sha256']):
            raise ValueError('Source topology differs from its committed index digest')
        if pin is not None:
            if self.topology_sha256 != committed_digest(pin.get('topology_sha256')):
                raise ValueError('Source topology differs from the pinned scientific digest')
            if 'topology.json' in pin['files']:
                verify_source_bytes(topology_bytes, pin['topology_sha256'], 'topology.json', pin)
        self.topology = json.loads(topology_bytes)
        self.chunks = self.index['chunks']
        if not self.chunks:
            raise ValueError('The source trajectory has no recorded states')
        for chunk in self.chunks:
            committed_digest(chunk.get('sha256'))
        self.starts = [finite(c['start_time'], 'chunk time', -1e12, 1e12) for c in self.chunks]
        if self.starts != sorted(self.starts):
            raise ValueError('Trajectory chunks are out of order')
        self.cache = OrderedDict()
        self.hashes = {}

    def chunk(self, number):
        if number in self.cache:
            self.cache.move_to_end(number)
            return self.cache[number]
        entry = self.chunks[number]
        relative = Path(entry['path'])
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('Invalid trajectory chunk path')
        path = (self.root / relative).resolve(strict=True)
        if not path.is_relative_to(self.root) or path.suffix != '.json':
            raise ValueError('Trajectory chunk escapes its source job')
        if path.stat().st_size > 64 * 1024 * 1024:
            raise ValueError('Trajectory chunk exceeds the bounded renderer cache')
        raw = path.read_bytes()
        if len(raw) > 64 * 1024 * 1024:
            raise ValueError('Trajectory chunk exceeds the bounded renderer cache')
        digest = verify_source_bytes(raw, entry.get('sha256'), entry['path'], self.pin)
        self.hashes[entry['path']] = digest
        value = json.loads(raw)
        frames = value if isinstance(value, list) else value['frames']
        previous = -math.inf
        for frame in frames:
            t = finite(frame['time'], 'frame time', -1e12, 1e12)
            if t <= previous:
                raise ValueError('Recorded frame times are not strictly increasing')
            previous = t
            ids = set()
            for entity in frame['entities']:
                if str(entity['id']) in ids:
                    raise ValueError('Duplicate particle identifier in a frame')
                ids.add(str(entity['id']))
                vec(entity['position'])
        if not frames:
            raise ValueError('Empty trajectory chunk')
        self.cache[number] = frames
        while len(self.cache) > 3:
            self.cache.popitem(last=False)
        return frames

    def sample(self, timestamp, interpolate=False):
        number = recorded_index(self.starts, timestamp)
        if self.pin_entry is not None and number != self.pin_entry:
            raise ValueError('Requested time selects a different pinned trajectory entry')
        frames = self.chunk(number)
        i = recorded_index([f['time'] for f in frames], timestamp)
        left = frames[i]
        right = frames[min(i + 1, len(frames) - 1)] if interpolate else left
        if interpolate and right is left and timestamp > left['time'] and number + 1 < len(self.chunks):
            right = self.chunk(number + 1)[0]
        alpha = max(0, min(1, (timestamp-left['time'])/(right['time']-left['time']))) if right['time'] > left['time'] else 0
        right_by_id = {str(e['id']): e for e in right['entities']}
        periodic = self.index.get('wrapping', '').startswith('periodic')
        box = self.topology.get('box_nm', [])
        positions = {}
        for entity in left['entities']:
            identifier = str(entity['id'])
            a, b = entity['position'], right_by_id.get(identifier, entity)['position']
            p = list(a)
            if interpolate and alpha:
                for axis in range(3):
                    delta = b[axis] - a[axis]
                    if periodic and len(box) == 3 and box[axis] > 0:
                        delta -= math.floor(delta / box[axis] + .5) * box[axis]
                        p[axis] = (a[axis] + delta * alpha) % box[axis]
                    else:
                        p[axis] = a[axis] + delta * alpha
            positions[identifier] = p
        return positions, {'requested_time': timestamp, 'display_time': timestamp if interpolate else left['time'],
                           'source_times': [left['time'], right['time']], 'alpha': alpha if interpolate else 0,
                           'source_frame': self.chunks[number].get('start_frame', 0) + i,
                           'interpolated': bool(interpolate and alpha)}


def material(name, color, exposure=1, emission=False, opacity=1):
    m = bpy.data.materials.new(name)
    m.diffuse_color = (*color, opacity)
    m.use_nodes = True
    shader = m.node_tree.nodes.get('Principled BSDF')
    shader.inputs['Base Color'].default_value = (*color, opacity)
    shader.inputs['Roughness'].default_value = .27
    shader.inputs['Metallic'].default_value = .1
    shader.inputs['Alpha'].default_value = opacity
    if 'Coat Weight' in shader.inputs:
        shader.inputs['Coat Weight'].default_value = .32
    if emission:
        shader.inputs['Emission Color'].default_value = (*color, 1)
        shader.inputs['Emission Strength'].default_value = exposure
    return m


def line(name, points, width, mat):
    data = bpy.data.curves.new(name, 'CURVE')
    data.dimensions = '3D'
    data.bevel_depth = width
    data.bevel_resolution = 2
    spline = data.splines.new('POLY')
    spline.points.add(len(points) - 1)
    for vertex, point in zip(spline.points, points):
        vertex.co = (*point, 1)
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    data.materials.append(mat)
    return obj


def camera_basis(position, target, up):
    back = (position - target).normalized()
    right = up.cross(back)
    if right.length < 1e-8:
        up = Vector((0, 0, 1)) if abs(back.z) < .9 else Vector((1, 0, 0))
        right = up.cross(back)
    right.normalize()
    vertical = back.cross(right).normalized()
    return Matrix((right, vertical, back)).transposed().to_quaternion()


def main(config, output):
    started = time.monotonic()
    source = Trajectory(Path(config['source_directory']), config.get('source_pin'))
    source_metadata = publication_metadata(config, source)
    presentation = config.get('presentation') or {}
    mode = config.get('mode', 'video')
    if mode not in ('video', 'png'):
        raise ValueError('Choose a video export or PNG observation')
    width = integer(config.get('width', 1920), 'width', 320, 3840)
    height = integer(config.get('height', 1080), 'height', 180, 2160)
    if width % 2 or height % 2:
        raise ValueError('H.264 export requires even pixel dimensions')
    fps = integer(config.get('fps', 30), 'fps', 1, 60)
    duration = finite(config.get('playback_duration_seconds', 30), 'playback duration', .1, 86400)
    count = export_frame_count(config, duration, fps, mode)
    start = retained_time(config.get('start_time', source.index['start_time']), source.index['start_time'], source.index['end_time'])
    end = retained_time(config.get('end_time', source.index['end_time']), source.index['start_time'], source.index['end_time'])
    if mode == 'video' and end <= start:
        raise ValueError('Select an increasing scientific time range')
    interpolation = bool(presentation.get('interpolate', False))
    if len(source.topology['entities']) > 20000:
        raise ValueError('This renderer supports up to 20,000 retained particles')
    stop = threading.Event()
    def cancelled():
        while not stop.wait(.25):
            if (output / 'cancel.request').exists():
                atomic_json(output / 'progress.json', {'state': 'cancelled', 'fraction': 0, 'message': 'Export cancelled; incomplete files are not downloadable'})
                os._exit(130)
    threading.Thread(target=cancelled, daemon=True).start()
    atomic_json(output / 'progress.json', {'state': 'preparing', 'fraction': 0, 'frame_count': count})
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.resolution_x, scene.render.resolution_y, scene.render.resolution_percentage = width, height, 100
    scene.render.fps = fps
    scene.render.image_settings.color_mode = 'RGB'
    scene.render.film_transparent = False
    scene.render.use_file_extension = True
    scene.render.use_lock_interface = True
    engine = config.get('renderer', 'eevee')
    if engine not in ('eevee', 'cycles'):
        raise ValueError('Choose eevee or cycles rendering')
    scene.render.engine = 'BLENDER_EEVEE_NEXT' if engine == 'eevee' else 'CYCLES'
    devices = []
    if engine == 'cycles':
        scene.cycles.samples = integer(config.get('samples', 32), 'samples', 8, 512)
        scene.cycles.use_denoising = True
        preferences = bpy.context.preferences.addons['cycles'].preferences
        for backend in ('OPTIX', 'CUDA', 'HIP', 'ONEAPI'):
            try:
                preferences.compute_device_type = backend
                preferences.get_devices()
                devices = [d.name for d in preferences.devices if d.type != 'CPU']
                if devices:
                    for d in preferences.devices:
                        d.use = d.type != 'CPU'
                    scene.cycles.device = 'GPU'
                    break
            except (TypeError, RuntimeError):
                continue
    elif hasattr(scene, 'eevee'):
        scene.eevee.taa_render_samples = integer(config.get('samples', 64), 'samples', 8, 128)
    scene.view_settings.view_transform = 'AgX'
    scene.view_settings.exposure = math.log2(finite(presentation.get('exposure', 1.1), 'exposure', .15, 3))
    contrast = finite(presentation.get('contrast', 1), 'contrast', .5, 2)
    if abs(contrast - 1) > .02:
        # Blender AgX has named contrast looks; retain that explicit mapping in provenance.
        look = 'AgX - Medium High Contrast' if contrast > 1 else 'AgX - Medium Low Contrast'
        try:
            scene.view_settings.look = look
        except TypeError:
            raise RuntimeError('This Blender build does not support the requested AgX contrast look')
    scene.world = bpy.data.worlds.new('Laboratory environment')
    scene.world.use_nodes = True
    background = rgb(presentation.get('background'), '#080f20')
    scene.world.node_tree.nodes['Background'].inputs['Color'].default_value = (*background, 1)
    scene.world.node_tree.nodes['Background'].inputs['Strength'].default_value = .3
    domain = particle_domain(source)
    center_values = [(a+b)/2 for a,b in zip(domain['min'],domain['max'])]
    extent_values = [b-a for a,b in zip(domain['min'],domain['max'])]
    box, extent = source.topology.get('box_nm', source.topology.get('box')), Vector(extent_values)
    center, scale = Vector(center_values), 4/max(extent_values)
    camera_config = presentation.get('camera') or {}
    fov = math.radians(finite(camera_config.get('fov', 38), 'camera field of view', 5, 100))
    angle = min(fov/2, math.atan(math.tan(fov/2)*width/height))
    radius = (extent.length/2+max(particle_radius(entity) for entity in source.topology['entities']))*scale
    fit_distance = 13*max(1,height/width) if domain['periodic'] else radius/math.sin(angle)*1.1
    position = Vector(normalized_position(camera_config['position'], center_values, scale)) if camera_config.get('position') else Vector((.8, .48, 1)).normalized() * fit_distance
    target = Vector(normalized_position(camera_config['target'], center_values, scale)) if camera_config.get('target') else Vector()
    if (position - target).length < .0001:
        raise ValueError('The camera position must differ from its target')
    camera_data = bpy.data.cameras.new('Scientific camera')
    camera = bpy.data.objects.new('Scientific camera', camera_data)
    bpy.context.collection.objects.link(camera)
    camera.location = position
    camera.rotation_mode = 'QUATERNION'
    camera.rotation_quaternion = camera_basis(position, target, vec(camera_config.get('up', [0, 1, 0])))
    camera_data.sensor_fit = 'VERTICAL'
    camera_data.angle_y = fov
    distance = (position-target).length
    camera_data.clip_start, camera_data.clip_end = max(1e-6, distance/10000), max(100, distance*10)
    scene.camera = camera
    for name, position, power, color, size in [('Key', (3, 6, 5), 950, '#e8edff', 5), ('Cyan rim', (-5, 2, -4), 1100, '#70dcef', 4), ('Violet fill', (2, -3, 3), 450, '#bc9bff', 4)]:
        data = bpy.data.lights.new(name, 'AREA')
        data.energy, data.color, data.shape, data.size = power, rgb(color), 'DISK', size
        obj = bpy.data.objects.new(name, data)
        bpy.context.collection.objects.link(obj)
        obj.location = position
        obj.rotation_euler = (-obj.location).to_track_quat('-Z', 'Y').to_euler()
    base_color = rgb(presentation.get('color'))
    opacity = finite(presentation.get('opacity', 1), 'particle opacity', .05, 1)
    normal = material('Particle display color', base_color, opacity=opacity)
    highlighted = material('Selected particle', rgb('#ffd275'), opacity=opacity)
    dimmed = material('Unselected particles', tuple(v*.16 for v in base_color), opacity=opacity)
    materials = {}
    selected = set(map(str, presentation.get('selectedIds', [])))
    hidden = set(map(str, presentation.get('hiddenIds', [])))
    bpy.ops.mesh.primitive_uv_sphere_add(segments=32, ring_count=20, radius=1)
    template = bpy.context.object
    geometry = template.data
    for polygon in geometry.polygons:
        polygon.use_smooth = True
    bpy.data.objects.remove(template, do_unlink=True)
    objects = {}
    for entity in source.topology['entities']:
        identifier = str(entity['id'])
        if identifier in hidden:
            continue
        obj = bpy.data.objects.new(identifier, geometry)
        bpy.context.collection.objects.link(obj)
        radius = particle_radius(entity, .17 if domain['periodic'] else .017)
        obj.scale = (radius*scale,) * 3
        # Material slots are object-linked so stable IDs can be highlighted without changing shared geometry.
        if not geometry.materials:
            geometry.materials.append(normal)
        obj.material_slots[0].link = 'OBJECT'
        display_color = rgb(presentation.get('color') or entity.get('color'))
        if selected and presentation.get('dimOthers') and identifier not in selected:
            display_color = tuple(v*.16 for v in display_color)
        if display_color not in materials:
            materials[display_color] = material(f'Body color {len(materials)}', display_color, opacity=opacity)
        obj.material_slots[0].material = highlighted if identifier in selected else materials[display_color]
        obj['source_entity_id'] = identifier
        obj['radius_semantics'] = source.topology.get('radius_semantics', 'Display radius')
        objects[identifier] = obj
    if domain['periodic'] and presentation.get('showBox', True):
        wire = material('Periodic cell boundary', rgb('#6c8da9'), emission=True)
        corners = [Vector((box[0]*x, box[1]*y, box[2]*z)) for x in (0, 1) for y in (0, 1) for z in (0, 1)]
        for a in range(8):
            for b in range(a+1, 8):
                if bin(a ^ b).count('1') == 1:
                    line('Periodic cell', [(corners[a]-center)*scale, (corners[b]-center)*scale], .006, wire)
    label = None
    if config.get('labels', True):
        font = bpy.data.curves.new('Measured state label', 'FONT')
        font.size = .026
        label = bpy.data.objects.new('Measured state label', font)
        bpy.context.collection.objects.link(label)
        label.parent = camera
        half_height = math.tan(camera_data.angle_y / 2)
        hud_depth = max(camera_data.clip_start*4, min(.01, distance*.001))
        label.location = Vector((-half_height*width/height+.025, half_height-.045, -1))*hud_depth
        label.scale = (hud_depth,)*3
        font.materials.append(material('Label ink', rgb('#e4edfa'), exposure=1.2, emission=True))
    receipts = []
    def apply_frame(frame):
        timestamp = start if frame <= 1 else end if frame >= count else start + (end-start)*(frame-1)/max(1, count-1)
        positions, receipt = source.sample(timestamp, interpolation)
        for identifier, obj in objects.items():
            obj.hide_render = identifier not in positions
            if identifier in positions:
                obj.location = normalized_position(positions[identifier], center_values, scale)
        if label:
            elements = {entity.get('element') for entity in source.topology['entities']}
            kind = f"{next(iter(elements))} particles" if len(elements)==1 and None not in elements else 'bodies'
            spatial = f"cell {box[0]:.4g} {domain['units']['position']}" if domain['periodic'] else f"isolated positions [{domain['units']['position']}]"
            scientific_time = f"t/T0 = {receipt['display_time']:.6g}" if domain['units']['time']=='T0' else f"t = {receipt['display_time']:.6g} {domain['units']['time']}"
            heading = publication_title(source_metadata) or f"PhaseForge  |  {len(positions)} {kind}"
            label.data.body = f"{heading}\n{scientific_time}  |  {spatial}\n{'Interpolated display' if receipt['interpolated'] else 'Recorded numerical state'}"
        return receipt
    scene.frame_start, scene.frame_end = 1, count
    progress_phase = {'name': 'preview', 'previews': 0, 'completed': 0, 'movie_started': None}
    @bpy.app.handlers.persistent
    def frame_pre(scene, *unused):
        apply_frame(scene.frame_current)
    @bpy.app.handlers.persistent
    def render_post(scene, *unused):
        elapsed = time.monotonic()-started
        if progress_phase['name'] == 'preview':
            progress_phase['previews'] += 1
            preview_count = 2 if mode == 'video' else 1
            atomic_json(output / 'progress.json', {'state': 'preparing', 'phase': 'endpoint_previews',
                        'fraction': .05*min(progress_phase['previews'], preview_count)/preview_count,
                        'frame': 0, 'frame_count': count, 'elapsed_seconds': elapsed,
                        'message': 'Rendering endpoint previews; video frames have not started'})
        else:
            completed = max(progress_phase['completed'], min(count, scene.frame_current))
            progress_phase['completed'] = completed
            movie_elapsed = time.monotonic()-progress_phase['movie_started']
            atomic_json(output / 'progress.json', {'state': 'rendering', 'phase': 'video_frames',
                        'fraction': .05+.90*completed/count, 'frame': completed, 'frame_count': count,
                        'elapsed_seconds': elapsed, 'estimated_remaining_seconds': movie_elapsed/completed*(count-completed),
                        'estimate_basis': 'completed movie frames and elapsed movie-rendering throughput'})
    bpy.app.handlers.frame_change_pre.append(frame_pre)
    bpy.app.handlers.render_post.append(render_post)
    scene.render.image_settings.file_format = 'PNG'
    scene.frame_set(1)
    scene.render.filepath = str(output / 'first-frame.png')
    bpy.ops.render.render(write_still=True)
    receipts.append({'video_frame': 1, **apply_frame(1)})
    if mode == 'video':
        scene.frame_set(count)
        scene.render.filepath = str(output / 'last-frame.png')
        bpy.ops.render.render(write_still=True)
        receipts.append({'video_frame': count, **apply_frame(count)})
    bpy.ops.wm.save_as_mainfile(filepath=str(output / 'scene.blend'), check_existing=False)
    if mode == 'video':
        if not bpy.app.build_options.codec_ffmpeg:
            raise RuntimeError('This Blender build has no FFmpeg encoder')
        scene.render.image_settings.file_format = 'FFMPEG'
        scene.render.ffmpeg.format = 'MPEG4'
        scene.render.ffmpeg.codec = 'H264'
        scene.render.ffmpeg.constant_rate_factor = 'HIGH'
        scene.render.ffmpeg.ffmpeg_preset = 'GOOD'
        scene.render.ffmpeg.audio_codec = 'NONE'
        scene.render.filepath = str(output / 'encoding.mp4')
        progress_phase['name'] = 'video'
        progress_phase['movie_started'] = time.monotonic()
        bpy.ops.render.render(animation=True)
        atomic_json(output / 'progress.json', {'state': 'verifying', 'phase': 'encoded_video_check',
                    'fraction': .97, 'frame': count, 'frame_count': count, 'elapsed_seconds': time.monotonic()-started})
        videos = list(output.glob('encoding*.mp4'))
        if len(videos) != 1 or videos[0].stat().st_size < 1000:
            raise RuntimeError('The encoder did not produce a complete MP4')
        artifact = output / 'simulation.mp4'
        videos[0].replace(artifact)
        # Read via Blender's movie decoder as a basic worker check. Independent ffprobe/ffplay acceptance is separate.
        clip = bpy.data.movieclips.load(str(artifact), check_existing=False)
        decoded = {'decoder': 'Blender FFmpeg movie clip', 'width': clip.size[0], 'height': clip.size[1], 'frame_count': clip.frame_duration, 'fps': float(clip.fps)}
        if tuple(clip.size) != (width, height) or clip.frame_duration != count:
            raise RuntimeError(f'Encoded movie dimensions/frame count failed decoder verification: {decoded}')
        bpy.data.movieclips.remove(clip)
    else:
        artifact = output / 'first-frame.png'
        decoded = {'decoder': 'Blender PNG render', 'width': width, 'height': height, 'frame_count': 1}
    result = {'schema_version': 1, 'state': 'completed', 'filename': artifact.name, 'width': width, 'height': height,
              'fps': fps, 'duration_seconds': count/fps if mode == 'video' else 0, 'frame_count': count,
              'start_time': start, 'end_time': end, 'time_unit': domain['units']['time'], 'position_unit': domain['units']['position'], 'display_domain': domain,
              'renderer': f'Blender {bpy.app.version_string} {engine}', 'devices': devices, 'codec': 'H.264 / MP4' if mode == 'video' else 'PNG',
              'worker_sha256': sha256(Path(__file__)), 'display_transform': {'view': scene.view_settings.view_transform, 'look': scene.view_settings.look},
              'sha256': sha256(artifact), 'size_bytes': artifact.stat().st_size, 'elapsed_seconds': time.monotonic()-started,
              'source_index_sha256': source.index_sha256, 'source_topology_sha256': source.topology_sha256,
              'source_chunks': source.hashes, 'presentation': presentation, 'endpoint_frames': receipts,
              'labels': config.get('labels', True),
              'interpolation': ('minimum-image periodic interpolation of retained positions' if domain['periodic'] else 'linear interpolation of retained isolated positions') if interpolation else 'hold previous recorded state',
              'source_metadata': source_metadata,
              'scientific_rerun': False, 'decode_check': decoded, 'independent_player_check': 'not performed by this worker'}
    atomic_json(output / 'result.json', result)
    atomic_json(output / 'progress.json', {'state': 'completed', 'fraction': 1, 'frame': count, 'frame_count': count, 'elapsed_seconds': result['elapsed_seconds']})
    stop.set()
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--input', required=True)
    parser.add_argument('--output', required=True)
    arguments = parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else sys.argv[1:])
    export_directory = Path(arguments.output).resolve()
    export_directory.mkdir(parents=True, exist_ok=True)
    try:
        payload = json.loads(Path(arguments.input).read_text(encoding='utf-8'))
        main(payload, export_directory)
    except Exception as error:
        atomic_json(export_directory / 'progress.json', {'state': 'failed', 'fraction': 0, 'error': str(error)})
        raise
