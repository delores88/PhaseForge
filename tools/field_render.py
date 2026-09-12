# SPDX-License-Identifier: MIT OR GPL-3.0-or-later
"""Trusted Blender export of retained scalar cells. No scientific solver runs here."""
import argparse
import bisect
from collections import OrderedDict
import hashlib
import io
import json
import math
import os
from pathlib import Path
import sys
import threading
import time

import bpy
import numpy as np
from mathutils import Vector

# Both files are shipped trusted sources copied by the backend into this export job.
sys.path.insert(0, str(Path(__file__).resolve().parent))
from trajectory_render import atomic_json, sha256, committed_digest, pinned_source_index, verify_source_bytes, finite, integer, retained_time, recorded_index, export_frame_count, vec, rgb, camera_basis, publication_metadata, publication_title


class Fields:
    def __init__(self, root, pin=None):
        self.root = root.resolve(strict=True)
        self.pin = pin
        self.index, self.index_sha256, self.pin_entry = pinned_source_index(self.root / 'fields/index.json', pin, 'frames')
        self.ny, self.nx = [integer(v, 'grid dimension', 2, 1024) for v in self.index['shape']]
        if self.index['representation'] != 'scalar_field' or self.index['axis_order'] != ['y', 'x'] or self.index['grid_location'] != 'cell_center':
            raise ValueError('Only cell-centered row-major scalar fields can be exported')
        self.lengths = [finite(v, 'field length', 1e-12, 1e12) for v in self.index['lengths_um']]
        if len(self.lengths) != 2:
            raise ValueError('A scalar field must have two physical dimensions')
        self.records = self.index['frames']
        for record in self.records:
            committed_digest(record.get('sha256'))
            committed_digest(record.get('view_sha256'))
        self.times = [finite(f['time'], 'field time', -1e12, 1e12) for f in self.records]
        if not self.times or any(b <= a for a, b in zip(self.times, self.times[1:])):
            raise ValueError('Field records must be strictly time ordered')
        self.cache = OrderedDict()
        self.hashes = {}

    def committed_path(self, path, expected):
        relative = Path(path)
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('Invalid field artifact path')
        result = (self.root / relative).resolve(strict=True)
        if not result.is_relative_to(self.root) or result.stat().st_size > 64 * 1024 * 1024:
            raise ValueError('Field artifact exceeds the source boundary or bounded cache')
        raw = result.read_bytes()
        if len(raw) > 64 * 1024 * 1024:
            raise ValueError('Field artifact exceeds the bounded cache')
        digest = verify_source_bytes(raw, expected, path, self.pin)
        self.hashes[path] = digest
        return raw

    def sample(self, timestamp):
        number = recorded_index(self.times, timestamp)
        if self.pin_entry is not None and number != self.pin_entry:
            raise ValueError('Requested time selects a different pinned field entry')
        record = self.records[number]
        if number in self.cache:
            self.cache.move_to_end(number)
            values = self.cache[number]
        else:
            numeric = self.committed_path(record['path'], record['sha256'])
            view = self.committed_path(record['view_path'], record['view_sha256'])
            values = np.load(io.BytesIO(numeric), allow_pickle=False)
            visible = json.loads(view)
            if values.shape != (self.ny, self.nx) or not np.isfinite(values).all() or visible['shape'] != [self.ny, self.nx] or visible['time'] != record['time'] or visible['step'] != record['step'] or not np.array_equal(values, np.asarray(visible['values'])):
                raise ValueError('Recorded JSON view differs from the authoritative numerical field')
            self.cache[number] = values
            while len(self.cache) > 4:
                self.cache.popitem(last=False)
        return values, {'requested_time': timestamp, 'display_time': record['time'], 'source_frame': number,
                        'step': record['step'], 'field_path': record['path'], 'field_sha256': record['sha256'],
                        'view_sha256': record['view_sha256'], 'interpolated': False}


def srgb_bytes(value, fallback):
    if value is None:
        return np.array(fallback, dtype=np.float64)
    rgb(value)  # Validate the same #RRGGBB contract as trajectory exports.
    return np.array([int(value[i:i+2], 16) for i in (1, 3, 5)], dtype=np.float64)


def field_colors(values, scale, presentation):
    lo, hi = finite(scale['min'], 'color minimum', -1e12, 1e12), finite(scale['max'], 'color maximum', -1e12, 1e12)
    if hi < lo:
        raise ValueError('Color range must be fixed and increasing')
    low = srgb_bytes(presentation.get('colorLow'), scale.get('stops', [[0, [20, 38, 80]]])[0][1])
    high = srgb_bytes(presentation.get('colorHigh'), scale.get('stops', [[1, [255, 190, 60]]])[-1][1])
    contrast = finite(presentation.get('contrast', 1), 'contrast', .5, 2)
    t = np.full_like(values, .5) if hi == lo else np.clip((values-lo)/(hi-lo), 0, 1)
    srgb = np.floor(np.clip(((low+(high-low)*t[..., None])/255-.5)*contrast*255+127.5, 0, 255)+.5)/255
    return np.where(srgb <= .04045, srgb/12.92, ((srgb+.055)/1.055)**2.4)


def emission(name, color=None, image=None):
    material = bpy.data.materials.new(name)
    material.use_nodes = True
    nodes = material.node_tree.nodes
    nodes.clear()
    output = nodes.new('ShaderNodeOutputMaterial')
    shader = nodes.new('ShaderNodeEmission')
    shader.inputs['Strength'].default_value = 1
    material.node_tree.links.new(shader.outputs[0], output.inputs['Surface'])
    if image:
        texture = nodes.new('ShaderNodeTexImage')
        texture.image = image
        texture.interpolation = 'Closest'
        material.node_tree.links.new(texture.outputs['Color'], shader.inputs['Color'])
    else:
        shader.inputs['Color'].default_value = (*color, 1)
    return material


def set_hud_visibility(objects, visible):
    if not isinstance(visible, bool):
        raise ValueError('labels must be true or false')
    for obj in objects:
        obj.hide_render = not visible
        obj.hide_viewport = not visible


def main(config, output):
    started = time.monotonic()
    source = Fields(Path(config['source_directory']), config.get('source_pin'))
    source_metadata = publication_metadata(config, source)
    presentation = config.get('presentation') or {}
    mode = config.get('mode', 'video')
    if mode not in ('video', 'png'):
        raise ValueError('Choose video or PNG')
    width, height = integer(config.get('width', 1920), 'width', 320, 3840), integer(config.get('height', 1080), 'height', 180, 2160)
    if width % 2 or height % 2:
        raise ValueError('H.264 dimensions must be even')
    fps = integer(config.get('fps', 30), 'fps', 1, 60)
    duration = finite(config.get('playback_duration_seconds', 30), 'playback duration', .1, 86400)
    count = export_frame_count(config, duration, fps, mode)
    start = retained_time(config.get('start_time', source.times[0]), source.times[0], source.times[-1])
    end = retained_time(config.get('end_time', source.times[-1]), source.times[0], source.times[-1])
    if mode == 'video' and end <= start:
        raise ValueError('Select an increasing physical time range')
    stop = threading.Event()
    def cancelled():
        while not stop.wait(.25):
            if (output / 'cancel.request').exists():
                atomic_json(output / 'progress.json', {'state':'cancelled','fraction':0,'message':'Partial export retained; not downloadable'})
                os._exit(130)
    threading.Thread(target=cancelled, daemon=True).start()
    atomic_json(output / 'progress.json', {'state':'preparing','fraction':0,'frame_count':count})
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.render.resolution_x, scene.render.resolution_y, scene.render.resolution_percentage = width, height, 100
    scene.render.fps = fps
    scene.render.image_settings.color_mode = 'RGB'
    engine = config.get('renderer', 'eevee')
    if engine not in ('eevee','cycles'):
        raise ValueError('Choose Eevee or Cycles')
    scene.render.engine = 'BLENDER_EEVEE_NEXT' if engine == 'eevee' else 'CYCLES'
    samples = integer(config.get('samples',64), 'samples', 8, 128 if engine == 'eevee' else 512)
    if engine == 'eevee' and hasattr(scene, 'eevee'):
        scene.eevee.taa_render_samples = samples
    elif engine == 'cycles':
        scene.cycles.samples = samples
        scene.cycles.use_denoising = False  # Do not smooth a quantitative cell texture.
    scene.view_settings.view_transform = 'Standard'
    scene.view_settings.look = 'None'
    scene.view_settings.exposure = 0
    scene.view_settings.gamma = 1
    scene.world = bpy.data.worlds.new('Field background')
    scene.world.use_nodes = True
    scene.world.node_tree.nodes['Background'].inputs['Color'].default_value = (*rgb(presentation.get('background'),'#080f20'),1)
    scene.world.node_tree.nodes['Background'].inputs['Strength'].default_value = 1
    lx, ly = source.lengths
    scale = 4/max(lx,ly)
    center = Vector((lx/2,ly/2,0))
    pixels = bpy.data.images.new('Committed scalar cells',width=source.nx,height=source.ny,alpha=True,float_buffer=True)
    pixels.colorspace_settings.name = 'Non-Color'
    bpy.ops.mesh.primitive_plane_add(size=2)
    plane = bpy.context.object
    plane.name = 'Recorded concentration field — row zero is low y'
    plane.scale = (lx*scale/2,ly*scale/2,1)
    plane.data.materials.append(emission('Quantitative fixed-scale cells',image=pixels))
    camera_settings = presentation.get('camera') or {}
    data = bpy.data.cameras.new('Field camera')
    camera = bpy.data.objects.new('Field camera', data)
    bpy.context.collection.objects.link(camera)
    data.sensor_fit = 'VERTICAL'
    fov = math.radians(finite(camera_settings.get('fov',35),'field of view',10,90))
    data.angle_y = fov
    default_distance = math.hypot(lx,ly)*scale/2 / math.sin(min(fov/2,math.atan(math.tan(fov/2)*width/height)))*1.12
    position = (vec(camera_settings['position'])-center)*scale if camera_settings.get('position') else Vector((0,0,default_distance))
    target = (vec(camera_settings['target'])-center)*scale if camera_settings.get('target') else Vector()
    if (position-target).length < 1e-6:
        raise ValueError('Camera position must differ from its target')
    camera.location = position
    camera.rotation_mode = 'QUATERNION'
    camera.rotation_quaternion = camera_basis(position,target,vec(camera_settings.get('up',[0,1,0])))
    data.clip_start, data.clip_end = max(1e-6,(position-target).length/10000),max(100,(position-target).length*10)
    scene.camera = camera
    ink = emission('Label ink',rgb('#e4edfa'))
    # Keep annotations inside the near-camera frustum even after Fit cell.
    # Scaling their geometry and offsets by the same depth preserves pixel size.
    hud_depth = max(data.clip_start * 4, min(.01, (position-target).length * .001))
    def text(name, position, size):
        curve = bpy.data.curves.new(name,'FONT');curve.size=size
        obj = bpy.data.objects.new(name,curve);bpy.context.collection.objects.link(obj);obj.parent=camera;obj.location=Vector(position)*hud_depth;obj.scale=(hud_depth,)*3;curve.materials.append(ink)
        return obj
    half = math.tan(fov/2)
    hud_objects = []
    for name,y in [('Title backing',half-.045),('Legend backing',-half+.06)]:
        bpy.ops.mesh.primitive_plane_add(size=2)
        backing=bpy.context.object;backing.name=name;backing.parent=camera
        backing.location=Vector((0,y,-1))*hud_depth*1.08
        backing.scale=(half*width/height*hud_depth*1.1,.055*hud_depth*1.08,1)
        backing.data.materials.append(emission(name,rgb('#08101f')))
        hud_objects.append(backing)
    label = text('Saved numerical field label',(-half*width/height+.025,half-.045,-1),.024)
    legend = text('Fixed numerical color scale',(-half*width/height+.025,-half+.06,-1),.02)
    lo,hi=source.index['color_scale']['min'],source.index['color_scale']['max']
    legend.data.body=f"{lo:.6g}  ->  {hi:.6g}   {source.index['field_name']} [{source.index['field_unit']}]\nFixed scale | {lx:g} x {ly:g} um | row 0 = low y"
    gradient = bpy.data.images.new('Fixed legend colors',width=256,height=1,alpha=True,float_buffer=True)
    gradient.colorspace_settings.name='Non-Color'
    colors=field_colors(np.linspace(lo,hi,256)[None,:],source.index['color_scale'],presentation)
    gradient.pixels.foreach_set(np.concatenate([colors,np.ones((1,256,1))],axis=2).astype(np.float32).ravel());gradient.update();gradient.pack()
    bpy.ops.mesh.primitive_plane_add(size=2)
    bar=bpy.context.object;bar.name='Quantitative legend';bar.parent=camera;bar.location=Vector((-half*width/height+.19,-half+.085,-1))*hud_depth;bar.scale=(.165*hud_depth,.008*hud_depth,1);bar.data.materials.append(emission('Legend colors',image=gradient))
    hud_objects.extend([label, legend, bar])
    set_hud_visibility(hud_objects, config.get('labels', True))
    selected=presentation.get('selectedCell')
    if selected is not None:
        x=integer(selected.get('x'),'selected x',0,source.nx-1);y=integer(selected.get('y'),'selected y',0,source.ny-1)
        from trajectory_render import line
        x0=(x*lx/source.nx-lx/2)*scale;y0=(y*ly/source.ny-ly/2)*scale;dx=lx/source.nx*scale;dy=ly/source.ny*scale
        line('Selected recorded cell',[Vector((x0,y0,.002)),Vector((x0+dx,y0,.002)),Vector((x0+dx,y0+dy,.002)),Vector((x0,y0+dy,.002)),Vector((x0,y0,.002))],.002,emission('Cell highlight',rgb('#ffffff')))
    def apply_frame(frame):
        timestamp=start if frame<=1 else end if frame>=count else start+(end-start)*(frame-1)/(count-1)
        values,receipt=source.sample(timestamp)
        colors=field_colors(values,source.index['color_scale'],presentation)
        pixels.pixels.foreach_set(np.concatenate([colors,np.ones((source.ny,source.nx,1))],axis=2).astype(np.float32).ravel());pixels.update()
        heading=publication_title(source_metadata) or f"PhaseForge | computed scalar field | {source.nx} x {source.ny} cells"
        label.data.body=f"{heading}\nt = {receipt['display_time']:.6g} {source.index['time_unit']} | exact recorded state"
        return receipt
    scene.frame_start,scene.frame_end=1,count
    phase={'name':'preview','previews':0,'completed':0,'movie_started':None}
    @bpy.app.handlers.persistent
    def frame_pre(scene,*unused):apply_frame(scene.frame_current)
    @bpy.app.handlers.persistent
    def render_post(scene,*unused):
        if phase['name']=='preview':
            phase['previews']+=1;fraction=.05*min(phase['previews'],2 if mode=='video' else 1)/(2 if mode=='video' else 1);completed=0
        else:
            completed=max(phase['completed'],min(count,scene.frame_current));phase['completed']=completed;fraction=.05+.9*completed/count
        atomic_json(output/'progress.json',{'state':'rendering' if phase['name']=='video' else 'preparing','fraction':fraction,'frame':completed,'frame_count':count,'elapsed_seconds':time.monotonic()-started})
    bpy.app.handlers.frame_change_pre.append(frame_pre);bpy.app.handlers.render_post.append(render_post)
    scene.render.image_settings.file_format='PNG'
    receipts=[]
    for frame,name in [(1,'first-frame.png')]+([(count,'last-frame.png')] if mode=='video' else []):
        scene.frame_set(frame);scene.render.filepath=str(output/name);bpy.ops.render.render(write_still=True);receipts.append({'video_frame':frame,**apply_frame(frame)})
    pixels.pack();bpy.ops.wm.save_as_mainfile(filepath=str(output/'scene.blend'),check_existing=False)
    if mode=='video':
        if not bpy.app.build_options.codec_ffmpeg:raise RuntimeError('Blender FFmpeg encoder is unavailable')
        scene.render.image_settings.file_format='FFMPEG';scene.render.ffmpeg.format='MPEG4';scene.render.ffmpeg.codec='H264';scene.render.ffmpeg.constant_rate_factor='HIGH';scene.render.ffmpeg.ffmpeg_preset='GOOD';scene.render.ffmpeg.audio_codec='NONE';scene.render.filepath=str(output/'encoding.mp4')
        phase['name']='video';phase['movie_started']=time.monotonic();bpy.ops.render.render(animation=True)
        atomic_json(output/'progress.json',{'state':'verifying','fraction':.97,'frame':count,'frame_count':count})
        videos=list(output.glob('encoding*.mp4'))
        if len(videos)!=1 or videos[0].stat().st_size<1000:raise RuntimeError('No complete MP4 was encoded')
        artifact=output/'simulation.mp4';videos[0].replace(artifact)
        clip=bpy.data.movieclips.load(str(artifact),check_existing=False)
        decoded={'decoder':'Blender FFmpeg movie clip','width':clip.size[0],'height':clip.size[1],'frame_count':clip.frame_duration,'fps':float(clip.fps)}
        if tuple(clip.size)!=(width,height) or clip.frame_duration!=count:raise RuntimeError(f'Encoded field dimensions/frame count failed: {decoded}')
        bpy.data.movieclips.remove(clip)
    else:
        artifact=output/'first-frame.png';decoded={'decoder':'Blender PNG render','width':width,'height':height,'frame_count':1}
    result={'schema_version':1,'state':'completed','filename':artifact.name,'width':width,'height':height,'fps':fps,'duration_seconds':count/fps if mode=='video' else 0,'frame_count':count,'start_time':start,'end_time':end,'time_unit':source.index['time_unit'],'renderer':f'Blender {bpy.app.version_string} {engine}','worker_sha256':sha256(Path(__file__)),'helper_sha256':sha256(Path(__file__).with_name('trajectory_render.py')),'sha256':sha256(artifact),'size_bytes':artifact.stat().st_size,'elapsed_seconds':time.monotonic()-started,'source_index_sha256':source.index_sha256,'source_fields':source.hashes,'presentation':presentation,'endpoint_frames':receipts,'representation':'scalar_field','color_scale':source.index['color_scale'],'display_transform':{'view':'Standard','look':'None','color_mapping':'sRGB linear two-stop interpolation; fixed range; nearest cell; contrast shared with legend'},'interpolation':'hold previous recorded numerical field','scientific_rerun':False,'decode_check':decoded,'independent_player_check':'not performed by this worker'}
    result['labels'] = config.get('labels', True)
    result['source_metadata'] = source_metadata
    atomic_json(output/'result.json',result);atomic_json(output/'progress.json',{'state':'completed','fraction':1,'frame':count,'frame_count':count,'elapsed_seconds':result['elapsed_seconds']});stop.set();return result


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--input',required=True);parser.add_argument('--output',required=True)
    arguments=parser.parse_args(sys.argv[sys.argv.index('--')+1:] if '--' in sys.argv else sys.argv[1:]);output=Path(arguments.output).resolve();output.mkdir(parents=True,exist_ok=True)
    try:main(json.loads(Path(arguments.input).read_text(encoding='utf-8')),output)
    except Exception as error:
        atomic_json(output/'progress.json',{'state':'failed','fraction':0,'error':str(error)});raise
