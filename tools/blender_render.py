# SPDX-License-Identifier: MIT OR GPL-3.0-or-later
"""Trusted PhaseForge worker. Run only by Blender --background --disable-autoexec.

Input is JSON geometry, never Python, expressions, scripts, URLs or asset paths.
Surface tessellation is an appearance model, not a molecular dynamics solver.
"""
import json
import hashlib
import math
import os
from pathlib import Path
import random
import sys
import threading
import time

import bpy
import bmesh
from mathutils import Vector, Matrix, Euler

VDW = {"H": 1.2, "C": 1.7, "N": 1.55, "O": 1.52, "F": 1.47, "P": 1.8,
       "S": 1.8, "CL": 1.75, "FE": 1.8, "ZN": 1.39, "CA": 2.31, "MG": 1.73}
PALETTE = [(0.14, 0.57, 0.53), (0.68, 0.35, 0.18), (0.30, 0.40, 0.70),
           (0.59, 0.24, 0.37), (0.47, 0.60, 0.22), (0.44, 0.28, 0.62)]
WARNINGS = []
MAX_VERTICES = 1600000
SOLVENT_RESIDUES = {"HOH", "WAT", "DOD", "H2O"}
STYLE = "studio"


def number(value, default, low, high):
    if value is None:
        return default
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError("Geometry contains a non-finite numeric parameter")
    return max(low, min(high, value))


def vector(value, default=(0, 0, 0)):
    if value is None:
        return Vector(default)
    if len(value) != 3 or any(not isinstance(x, (int, float)) or not math.isfinite(x) for x in value):
        raise ValueError("Geometry vectors require three finite coordinates")
    return Vector(value)


def watchdog(seconds, parent_pid):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if os.name != "nt" and os.getppid() != parent_pid:
            os._exit(124)
        time.sleep(.5)
    # Blender operators block Python's main thread. The external Rust supervisor
    # is the primary deadline; this also bounds an orphan after backend failure.
    os._exit(124)


def material(name, color, biological=True):
    if STYLE == "microscopy":
        grey = .38 + min(.14, sum(color[:3])*.04)
        color = (grey, grey, grey)
    mat = bpy.data.materials.new(name)
    mat.use_nodes = True
    mat.diffuse_color = (*color[:3], 1)
    nodes, links = mat.node_tree.nodes, mat.node_tree.links
    shader = nodes.get("Principled BSDF")
    shader.inputs["Base Color"].default_value = (*color[:3], 1)
    shader.inputs["Roughness"].default_value = .52 if biological else .34
    shader.inputs["Metallic"].default_value = .04 if biological else .12
    if "Subsurface Weight" in shader.inputs:
        shader.inputs["Subsurface Weight"].default_value = .09 if biological else 0
    # Fine surface contrast represents illustrative material appearance only.
    noise = nodes.new("ShaderNodeTexNoise")
    noise.inputs["Scale"].default_value = 28
    noise.inputs["Detail"].default_value = 3
    bump = nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = .17 if biological else .04
    bump.inputs["Distance"].default_value = .035
    links.new(noise.outputs["Fac"], bump.inputs["Height"])
    links.new(bump.outputs["Normal"], shader.inputs["Normal"])
    return mat


def color(value, index=0):
    if isinstance(value, str) and len(value) in (7, 9) and value.startswith("#"):
        try:
            return tuple((int(value[i:i+2], 16) / 255) ** 2.2 for i in (1, 3, 5))
        except ValueError:
            pass
    return PALETTE[index % len(PALETTE)]


def dna_parameters(parameters, node_color=None):
    """Resolve explicit illustrative geometry without silently ignoring its inputs."""
    p = parameters or {}
    def bounded(key, default, low, high, integer=False):
        value = p.get(key)
        value = default if value is None else value
        if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not low <= value <= high or (integer and value != int(value)):
            raise ValueError(f"DNA {key} must be {'an integer' if integer else 'finite'} in [{low}, {high}]")
        return int(value) if integer else value
    radius = bounded('radius', 1, 1e-8, 1e12)
    turns = bounded('turns', 4, .5, 24)
    length = bounded('length', radius * 7, 1e-8, 1e12)
    thickness = bounded('thickness', radius * .16, radius * 1e-6, radius)
    pairs = bounded('count', min(160, int(turns * 10)), 1, 160, True)
    palette = p.get('colors')
    if palette is not None:
        if not isinstance(palette, list) or not 2 <= len(palette) <= 8 or any(not isinstance(value, str) or len(value) != 7 or not value.startswith('#') or any(c not in '0123456789abcdefABCDEF' for c in value[1:]) for value in palette):
            raise ValueError('DNA colors must contain 2–8 #RRGGBB colors: two backbones, then repeating base-half colors')
        strand_colors = palette[:2]
        base_colors = palette[2:] or ['#EF537B', '#31CDBA', '#A879E3', '#F39540']
    else:
        strand_colors = [node_color or '#2878D0', '#E5B94C']
        base_colors = ['#EF537B', '#31CDBA', '#A879E3', '#F39540']
    return {'radius': radius, 'turns': turns, 'length': length, 'thickness': thickness,
            'count': pairs, 'segments': min(4096, math.ceil(turns * 128)),
            'strand_colors': strand_colors, 'base_colors': base_colors}


def mesh(name, vertices, faces, mat):
    if len(vertices) > MAX_VERTICES:
        raise ValueError("The geometry exceeds the render vertex budget")
    data = bpy.data.meshes.new(name)
    data.from_pydata(vertices, [], faces)
    data.update()
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    if mat:
        data.materials.append(mat)
    for polygon in data.polygons:
        polygon.use_smooth = True
    return obj


def ico_template(subdivisions=2):
    bm = bmesh.new()
    bmesh.ops.create_icosphere(bm, subdivisions=subdivisions, radius=1)
    bm.verts.ensure_lookup_table()
    bm.verts.index_update()
    vertices = [tuple(v.co) for v in bm.verts]
    faces = [tuple(v.index for v in face.verts) for face in bm.faces]
    bm.free()
    return vertices, faces


ICO_VERTICES, ICO_FACES = ico_template(2)


def joined_spheres(name, atoms, mat):
    vertices, faces = [], []
    for center, radius in atoms:
        offset = len(vertices)
        vertices.extend((center[0]+v[0]*radius, center[1]+v[1]*radius, center[2]+v[2]*radius) for v in ICO_VERTICES)
        faces.extend(tuple(offset+i for i in face) for face in ICO_FACES)
    return mesh(name, vertices, faces, mat)


def molecular_surface(name, atoms, mat, coordinate_units=True, explicit_radius=None, triangle_budget=None):
    if not atoms or len(atoms) > 12000:
        raise ValueError("Molecular surfaces require 1–12000 atoms")
    centers = [vector(atom["position"]) for atom in atoms]
    lo = Vector(tuple(min(p[i] for p in centers) for i in range(3)))
    hi = Vector(tuple(max(p[i] for p in centers) for i in range(3)))
    span = max((hi-lo).length, 1e-6)
    # Imported PDB/SDF/XYZ coordinates retain their Angstrom scale. Conceptual
    # molecule nodes use the declared radius or a bounded spacing estimate.
    base = 1.0 if coordinate_units else (explicit_radius or span / max(3, len(atoms) ** (1/3)) * .42)
    radii = [VDW.get(str(atom.get("element", "C")).upper(), 1.7) * base for atom in atoms]
    obj = joined_spheres(name, list(zip(centers, radii)), mat)
    bpy.context.view_layer.objects.active = obj
    obj.select_set(True)
    remesh = obj.modifiers.new("Bounded van der Waals envelope", "REMESH")
    remesh.mode = "VOXEL"
    remesh.voxel_size = max(min(radii) * .23, (max(hi-lo) + 2*max(radii)) / 160)
    remesh.use_smooth_shade = True
    bpy.ops.object.modifier_apply(modifier=remesh.name)
    smooth = obj.modifiers.new("Surface interpolation", "SMOOTH")
    smooth.factor = .5
    smooth.iterations = 2
    bpy.ops.object.modifier_apply(modifier=smooth.name)
    triangles=sum(max(0,len(face.vertices)-2) for face in obj.data.polygons)
    if triangle_budget and triangles>triangle_budget:
        reduction=obj.modifiers.new("Declared display mesh detail budget","DECIMATE")
        reduction.ratio=triangle_budget/triangles
        bpy.ops.object.modifier_apply(modifier=reduction.name)
        obj["display_lod"]="Surface mesh decimated to its proportional triangle budget; source atom coordinates remain unchanged"
        WARNINGS.append(name+": surface tessellation reduced to fit the shared display budget; all source coordinates are retained.")
    obj.data.validate(clean_customdata=True)
    obj.data.update()
    if len(obj.data.vertices) > MAX_VERTICES:
        raise ValueError("Molecular envelope exceeds the tessellation budget")
    obj["representation"] = "Interpolated van der Waals envelope; not solvent-excluded, electron density, or simulated force field"
    obj["source_atom_count"] = len(atoms)
    obj.select_set(False)
    return obj


def curve(name, points, thickness, mat, cyclic=False):
    if len(points) < 2:
        raise ValueError("Curves need at least two points")
    data = bpy.data.curves.new(name, "CURVE")
    data.dimensions = "3D"
    data.resolution_u = 2
    data.bevel_depth = thickness
    data.bevel_resolution = 3
    spline = data.splines.new("POLY")
    spline.points.add(len(points)-1)
    for point, value in zip(spline.points, points):
        point.co = (*value, 1)
    spline.use_cyclic_u = cyclic
    obj = bpy.data.objects.new(name, data)
    bpy.context.collection.objects.link(obj)
    data.materials.append(mat)
    return obj


def sphere(name, radius, mat, subdivisions=4):
    bpy.ops.mesh.primitive_ico_sphere_add(subdivisions=subdivisions, radius=radius)
    obj = bpy.context.object
    obj.name = name
    obj.data.materials.append(mat)
    for face in obj.data.polygons:
        face.use_smooth = True
    return obj


def conceptual(node, index, bound_structure=None, triangle_budget=1500000):
    kind = node["type"]
    p = node.get("parameters") or {}
    name = str(node.get("label") or node["id"])[:120]
    radius = number(p.get("radius"), 1, 1e-8, 1e12)
    thickness = number(p.get("thickness"), radius*.055, radius*.001, radius)
    mat = material(name, color(node.get("color"), index), kind not in ("planet", "star", "black_hole", "mesh", "box"))
    before = set(bpy.context.scene.objects)
    source_metadata = None
    if bound_structure and kind in ("molecule", "protein"):
        source_metadata, surfaces = imported_geometry(bound_structure, triangle_budget)
        coords = [vertex.co for obj in surfaces for vertex in obj.data.vertices]
        low = Vector(tuple(min(v[i] for v in coords) for i in range(3)))
        high = Vector(tuple(max(v[i] for v in coords) for i in range(3)))
        center = (low+high)*.5
        factor = radius*2/max(max(high-low), 1e-6)
        for obj in surfaces:
            for vertex in obj.data.vertices:
                vertex.co = (vertex.co-center)*factor
        source_metadata["node_mapping"] = {"center_angstrom":list(center), "uniform_scale":factor, "note":"Source surface centered and uniformly fitted to the node's declared diameter; node transforms follow. Scene placement is authored, not measured."}
    elif kind == "molecule" and p.get("atoms"):
        molecular_surface(name, p["atoms"], mat, False, p.get("radius"))
    elif kind == "molecule":
        raise ValueError("Molecule rendering needs imported coordinates or explicit atoms; a generic sphere is not substituted")
    elif kind in ("mesh", "surface") and p.get("vertices"):
        indices = p.get("indices") or list(range(len(p["vertices"])))
        mesh(name, p["vertices"], [indices[i:i+3] for i in range(0, len(indices)-2, 3)], mat)
    elif kind in ("mesh", "surface"):
        raise ValueError(kind + " requires explicit vertices for Blender rendering")
    elif kind in ("curve", "streamlines", "field"):
        points = p.get("points")
        if not points:
            raise ValueError(kind + " requires explicit sampled points for Blender rendering")
        curve(name, points, thickness, mat)
    elif kind == "dna":
        dna = dna_parameters(p, node.get('color'))
        radius, turns, length, segments = dna['radius'], dna['turns'], dna['length'], dna['segments']
        for strand, phase in enumerate((0, math.pi)):
            points = [(radius*math.cos(i/segments*turns*2*math.pi+phase), radius*math.sin(i/segments*turns*2*math.pi+phase), length*(i/segments-.5)) for i in range(segments+1)]
            strand_mat = material(f'{name} backbone {strand+1}', color(dna['strand_colors'][strand]))
            obj = curve(f'{name} backbone {strand+1}', points, dna['thickness'], strand_mat)
            obj.data.bevel_resolution = 5
            obj.data.use_fill_caps = True
            obj['phaseforge_component_id'] = f'{node["id"]}/backbone-{strand+1}'
            obj['illustration_backbone_radius'] = dna['thickness']
            obj['illustration_base_pair_count'] = dna['count']
        base_materials = [material(f'{name} base color {i+1}', color(value)) for i, value in enumerate(dna['base_colors'])]
        for i in range(dna['count']):
            t = i / (dna['count']-1) if dna['count'] > 1 else .5
            angle, z = t*turns*2*math.pi, length*(t-.5)
            x, y = radius*math.cos(angle), radius*math.sin(angle)
            for half, endpoint in enumerate(((x,y,z), (-x,-y,z))):
                obj = curve(f'{name} base pair {i+1:03d} half {half+1}', [endpoint, (0,0,z)], dna['thickness']*.38, base_materials[(2*i+half)%len(base_materials)])
                obj.data.use_fill_caps = True
                obj['phaseforge_component_id'] = f'{node["id"]}/pair-{i+1:03d}/half-{half+1}'
        WARNINGS.append('DNA primitive is an authored two-strand schematic with a declared pair count and illustrative colors; it is not atomistic or sequence-specific DNA geometry.')
    elif kind == "virus":
        obj = sphere(name+" lipid envelope", radius, mat, 6)
        obj.data.materials.append(material(name+" inner membrane",(.24,.30,.33)))
        shell=obj.modifiers.new("Lipid bilayer thickness","SOLIDIFY");shell.thickness=radius*.027;shell.material_offset=1;shell.material_offset_rim=1
        bpy.ops.object.modifier_apply(modifier=shell.name)
        if p.get("cutaway") is not False:
            # Cut the finite shell, rather than discarding whole triangles: the
            # bilayer section then has a clean edge and a visible inner surface.
            bpy.ops.mesh.primitive_cube_add(size=1,location=(radius*1.04,-radius*1.04,0))
            cutter=bpy.context.object;cutter.scale=(radius*1.92,radius*1.92,radius*4)
            bpy.context.view_layer.update();bpy.context.view_layer.objects.active=obj
            section=obj.modifiers.new("Clean morphological cutaway","BOOLEAN");section.operation="DIFFERENCE";section.solver="EXACT";section.object=cutter
            bpy.ops.object.modifier_apply(modifier=section.name);bpy.data.objects.remove(cutter,do_unlink=True)
        tex = bpy.data.textures.new(name+" membrane grain", type="CLOUDS")
        tex.noise_scale = radius*.055
        displacement = obj.modifiers.new("Illustrative lipid surface", "DISPLACE")
        displacement.texture, displacement.strength = tex, radius*.022
        bpy.ops.object.modifier_apply(modifier=displacement.name)
        capsidmat=material(name+" capsid lattice",(.60,.34,.17))
        # Morphological conical lattice. This is not an atomic capsid assembly.
        for row in range(10):
            z=radius*(-.62+row*.12);cone_radius=radius*(.42-row*.029)
            for column in range(16):
                angle=(column+(row%2)*.5)*2*math.pi/16
                center=Vector((cone_radius*math.cos(angle),cone_radius*math.sin(angle),z))
                tangent=Vector((-math.sin(angle),math.cos(angle),0))
                along=Vector((-.24*math.cos(angle),-.24*math.sin(angle),1)).normalized()
                hexagon=[center+(tangent*math.cos(k*math.pi/3)+along*math.sin(k*math.pi/3))*radius*.064 for k in range(6)]
                curve(name+" capsid hexamer",hexagon,radius*.007,capsidmat,True)
        rnamat=material(name+" genome strands",(.20,.39,.62))
        for phase in (0,math.pi):
            points=[]
            for i in range(384):
                t=i/383;angle=t*18*math.pi+phase;r=radius*(.18-.09*t)
                points.append((r*math.cos(angle),r*math.sin(angle),radius*(-.55+t*.92)))
            curve(name+" illustrative packaged RNA",points,radius*.012,rnamat)
        rng = random.Random(int(number(p.get("seed"), 7, 0, 2**31-1)))
        count = int(number(p.get("count"), 72, 8, 240))
        prototype=None
        if bound_structure or p.get("atoms"):
            if bound_structure:
                source_metadata, surfaces = imported_geometry(bound_structure, min(60000,triangle_budget))
                bpy.ops.object.select_all(action='DESELECT')
                for part in surfaces:
                    part.select_set(True)
                bpy.context.view_layer.objects.active=surfaces[0]
                bpy.ops.object.join()
                template=bpy.context.object
                template.name=name+" sourced envelope protein"
            else:
                template=molecular_surface(name+" sourced envelope protein",p["atoms"],material(name+" envelope protein",PALETTE[(index+1)%len(PALETTE)]),True)
            if len(template.data.vertices)>30000:
                bpy.context.view_layer.objects.active=template
                reduction=template.modifiers.new("Bounded Env instance tessellation","DECIMATE");reduction.ratio=30000/len(template.data.vertices)
                bpy.ops.object.modifier_apply(modifier=reduction.name)
            template.data.validate(clean_customdata=True);template.data.update()
            coords=[v.co.copy() for v in template.data.vertices]
            low=Vector(tuple(min(v[i] for v in coords) for i in range(3)));high=Vector(tuple(max(v[i] for v in coords) for i in range(3)))
            centroid=(low+high)*.5;factor=radius*.25/max(max(high-low),1e-6)
            for vertex in template.data.vertices:vertex.co=(vertex.co-centroid)*factor
            prototype=template.data
            count=min(count,16,max(1,min(700000,triangle_budget//2)//max(1,len(prototype.vertices))))
            if source_metadata:
                source_metadata["node_mapping"]={"center_angstrom":list(centroid),"uniform_scale":factor,"note":"Source surface reused as an envelope-protein template; radial placements and assembly are morphological reconstruction, not measured."}
            bpy.data.objects.remove(template,do_unlink=True)
            WARNINGS.append("Supplied envelope protein coordinates are reused as a display template; placement, orientation, and virion assembly are morphological reconstruction, not measured assembly.")
        spikes = []
        source_instances = 0
        for i in range(count):
            z = 1-2*(i+.5)/count
            angle = i*math.pi*(3-math.sqrt(5))
            direction = Vector((math.sqrt(1-z*z)*math.cos(angle),math.sqrt(1-z*z)*math.sin(angle),z))
            if p.get("cutaway") is not False and direction.x>.08 and direction.y<-.08:
                continue
            if prototype:
                instance=bpy.data.objects.new(name+" Env instance",prototype);bpy.context.collection.objects.link(instance)
                instance.location=direction*radius*1.105;instance.rotation_euler=direction.to_track_quat('Z','Y').to_euler()
                instance["representation"]="Sourced coordinate geometry, illustrative assembly placement"
                source_instances += 1
                continue
            for j in range(3):
                tangent = direction.cross(Vector((0,0,1)))
                if tangent.length < .01:
                    tangent = Vector((1,0,0))
                tangent.normalize()
                bitangent = direction.cross(tangent)
                theta = j*2*math.pi/3
                tip = direction*radius*(1.16+rng.random()*.035)+(tangent*math.cos(theta)+bitangent*math.sin(theta))*radius*.035
                spikes.append((tip, radius*.058))
            spikes.append((direction*radius*1.075,radius*.038))
        if spikes:
            joined_spheres(name+" illustrative envelope proteins", spikes, material(name+" spikes", PALETTE[(index+1)%len(PALETTE)]))
        if source_metadata:
            source_metadata["instance_count"]=source_instances
            source_metadata["requested_instance_count"]=int(number(p.get("count"),72,8,240))
            WARNINGS.append("Bound molecular template tessellation and instance count are reduced to the display budget; source atoms are retained in input.json.")
        WARNINGS.append("Virus envelope, lipid bilayer thickness, capsid lattice and packaged RNA are morphological reconstruction; no cryo-EM acquisition or atomistic virion simulation is claimed.")
    elif kind == "protein":
        if p.get("atoms"):
            molecular_surface(name, p["atoms"], mat, False, p.get("radius"))
        elif p.get("points"):
            curve(name, p["points"], thickness, mat)
        else:
            raise ValueError("Protein rendering needs imported coordinates, atoms, or an explicit backbone; a generic blob is not substituted")
    elif kind == "box":
        bpy.ops.mesh.primitive_cube_add(size=radius*2)
        obj=bpy.context.object;obj.name=name;obj.data.materials.append(mat)
        bevel=obj.modifiers.new("Edge finish","BEVEL");bevel.width=radius*.04;bevel.segments=3
    elif kind == "black_hole":
        sphere(name+" illustrative event horizon",radius,material(name+" horizon",(.002,.002,.003),False))
        diskmat=material(name+" accretion disk",(.65,.19,.035),False)
        shader=diskmat.node_tree.nodes.get("Principled BSDF")
        if "Emission Color" in shader.inputs:
            shader.inputs["Emission Color"].default_value=(1,.2,.025,1);shader.inputs["Emission Strength"].default_value=1.8
        for n in range(10):
            r=radius*(1.65+n*.13)
            curve(name+" accretion ring",[(r*math.cos(i*2*math.pi/192),r*math.sin(i*2*math.pi/192),0) for i in range(192)],radius*.032,diskmat,True)
        WARNINGS.append("Black-hole geometry is illustrative; no relativistic ray tracing is claimed.")
    else:
        obj=sphere(name,radius,mat,5)
        if kind in ("planet","star"):
            tex=bpy.data.textures.new(name+" topography",type="CLOUDS");tex.noise_scale=radius*.24
            mod=obj.modifiers.new("Illustrative relief","DISPLACE");mod.texture=tex;mod.strength=radius*.025
            bpy.ops.object.modifier_apply(modifier=mod.name)
    position=vector(node.get("position"));rotation=vector(node.get("rotation"));scale=vector(node.get("scale"),(1,1,1))
    bpy.context.view_layer.update()
    transform=Matrix.Translation(position) @ Euler(rotation,'XYZ').to_matrix().to_4x4() @ Matrix.Diagonal((*scale,1))
    for obj in set(bpy.context.scene.objects)-before:
        obj.matrix_world=transform @ obj.matrix_world
        obj["phaseforge_node_id"]=node["id"]
        obj["scientific_role"]="Authored scene geometry; consult source provenance and numerical evidence separately"
        if source_metadata and (kind != "virus" or obj.get("representation") == "Sourced coordinate geometry, illustrative assembly placement"):
            obj["bound_structure_id"]=str(bound_structure["id"])
            obj["bound_structure_sha256"]=source_metadata["structure_sha256"]
    if source_metadata:
        source_metadata["node_id"]=node["id"]
        source_metadata["node_type"]=kind
        source_metadata["node_transform"]={"position":list(position),"rotation":list(rotation),"scale":list(scale)}
    return source_metadata


def imported_geometry(structure, total_triangle_budget=1500000):
    groups={}
    solvent=0
    for atom in structure["atoms"]:
        if atom.get("hetero") and str(atom.get("residue_name", "")).upper() in SOLVENT_RESIDUES:
            solvent+=1
            continue
        chain=str(atom.get("chain_id") or "structure")
        if atom.get("hetero"):
            chain="Ligand "+str(atom.get("residue_name") or atom.get("element") or "hetero")+" "+str(atom.get("residue_id") or "")
        if chain not in groups and len(groups)>=16:
            chain="other chains"
        groups.setdefault(chain,[]).append(atom)
    rendered_atoms=sum(len(atoms) for atoms in groups.values())
    objects=[]
    for index,(chain,atoms) in enumerate(groups.items()):
        triangle_budget=max(1200,int(total_triangle_budget*len(atoms)/max(1,rendered_atoms)))
        obj=molecular_surface(str(structure["name"])+" · "+chain,atoms,material("Chain "+chain,PALETTE[index%len(PALETTE)]),triangle_budget=triangle_budget)
        obj["structure_id"]=str(structure["id"])
        obj["source_units"]="angstrom"
        objects.append(obj)
    if solvent:
        WARNINGS.append(str(solvent)+" solvent atoms are hidden in the surface view; their coordinates remain in input.json.")
    fingerprint=hashlib.sha256(json.dumps(structure,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    return {"kind":"imported_structure","structure_id":structure["id"],"structure_sha256":fingerprint,"fingerprint_format":"SHA-256 of the saved structure JSON with sorted keys and compact separators","name":structure["name"],"atom_count":len(structure["atoms"]),"rendered_atom_count":len(structure["atoms"])-solvent,"hidden_solvent_atoms":solvent,"units":"angstrom","note":"Imported coordinates. Their experimental or predicted provenance remains the source record's responsibility."},objects


def build_geometry(payload):
    structure=payload.get("structure")
    if structure:
        return imported_geometry(structure)[0]
    scene=payload["request"]["scene"]
    bindings=payload.get("bound_structures") or {}
    sources=[]
    for index,node in enumerate(scene["nodes"]):
        source=conceptual(node,index,bindings.get(node["id"]),max(12000,1200000//max(1,len(bindings))))
        if source:
            sources.append(source)
    return {"kind":scene["provenance"]["kind"],"description":scene["provenance"]["description"],"units":scene.get("units","unspecified"),"bound_sources":sources}


def frame_scene(request):
    bpy.context.view_layer.update()
    objects=[o for o in bpy.context.scene.objects if o.type in ("MESH","CURVE")]
    if not objects:
        raise ValueError("No renderable geometry")
    bounds=[obj.matrix_world @ Vector(corner) for obj in objects for corner in obj.bound_box]
    lo=Vector(tuple(min(p[i] for p in bounds) for i in range(3)))
    hi=Vector(tuple(max(p[i] for p in bounds) for i in range(3)))
    center=(hi+lo)*.5
    extent=max(hi-lo)
    if not math.isfinite(extent) or extent<=1e-12:
        raise ValueError("Scene geometry has no finite visible extent")
    factor=10/extent
    transform=Matrix.Scale(factor,4) @ Matrix.Translation(-center)
    for obj in objects:
        obj.matrix_world=transform @ obj.matrix_world
    bpy.context.view_layer.update()
    authored=request.get("camera") or (request.get("scene") or {}).get("camera")
    if authored:
        target=(vector(authored["target"])-center)*factor
        position=(vector(authored["position"])-center)*factor
        lens=number(authored.get("focal_length_mm"),50,20,120)
    else:
        target=Vector((0,0,0));position=Vector((13,-18,10));lens=50
    if (target-position).length<1e-6:
        raise ValueError("Camera position and target must be distinct at the scene scale")
    bpy.ops.object.camera_add(location=position)
    camera=bpy.context.object;camera.name="Research inspection camera"
    if authored and authored.get("up") is not None:
        up=vector(authored["up"])
        if up.length<1e-9:
            raise ValueError("Camera up must be a nonzero finite source-coordinate direction")
        back=(position-target).normalized()
        right=up.normalized().cross(back)
        if right.length<1e-8:
            raise ValueError("Camera up must not be parallel to its viewing direction")
        right.normalize()
        vertical=back.cross(right).normalized()
        camera.rotation_mode='QUATERNION'
        camera.rotation_quaternion=Matrix((right,vertical,back)).transposed().to_quaternion()
    else:
        camera.rotation_euler=(target-position).to_track_quat('-Z','Y').to_euler()
    camera.data.lens=lens;camera.data.clip_end=max(1000,(position-target).length*4)
    camera.data.dof.use_dof=True;camera.data.dof.focus_distance=(position-target).length;camera.data.dof.aperture_fstop=12
    bpy.context.scene.camera=camera
    for name,location,power,size,tint in [
        ("Soft key",(3,-9,12),1600,8,(.80,.92,1)),
        ("Teal rim",(-8,3,5),2100,7,(.28,1,.80)),
        ("Warm separation",(8,5,2),1800,6,(1,.53,.30)),
        ("Front fill",(0,-9,-3),500,7,(.60,.72,1))]:
        bpy.ops.object.light_add(type="AREA",location=location)
        light=bpy.context.object;light.name=name;light.data.energy=power;light.data.shape="DISK";light.data.size=size;light.data.color=(1,1,1) if STYLE=="microscopy" else tint
        light.rotation_euler=(-light.location).to_track_quat('-Z','Y').to_euler()
    return {"source_center":list(center),"display_scale":factor,"mapping":"display = (source - source_center) * display_scale"}


def finalize_geometry():
    # Count the actual tessellation that GLB exports, including tube curves.
    bpy.ops.object.select_all(action="DESELECT")
    curves=[obj for obj in bpy.context.scene.objects if obj.type=="CURVE"]
    for obj in curves:
        obj.select_set(True)
    if curves:
        bpy.context.view_layer.objects.active=curves[0]
        bpy.ops.object.convert(target="MESH")
    bpy.ops.object.select_all(action="DESELECT")
    vertices=sum(len(obj.data.vertices) for obj in bpy.context.scene.objects if obj.type=="MESH")
    triangles=sum(max(0,len(face.vertices)-2) for obj in bpy.context.scene.objects if obj.type=="MESH" for face in obj.data.polygons)
    if vertices>MAX_VERTICES or triangles>1800000:
        raise ValueError("Geometry exceeds 1.6 million vertices or 1.8 million triangles")
    return vertices


def cycles_devices(scene, enabled=True):
    probes=[]
    if not enabled:
        scene.cycles.device="CPU"
        return {"backend":"CPU","devices":["Cycles CPU"],"fallback":"GPU use is disabled in PhaseForge settings","probes":probes}
    preferences=bpy.context.preferences.addons["cycles"].preferences
    for backend in ("OPTIX","CUDA","HIP","ONEAPI","METAL"):
        try:
            preferences.compute_device_type=backend
            preferences.get_devices()
            devices=[d for d in preferences.devices if d.type==backend]
            if not devices:
                continue
            for device in preferences.devices:
                device.use=device.type==backend
            scene.cycles.device="GPU"
            return {"backend":backend,"devices":[d.name for d in devices],"fallback":None,"probes":probes}
        except Exception as error:
            probes.append({"backend":backend,"available":False,"reason":str(error)[:180]})
    scene.cycles.device="CPU"
    return {"backend":"CPU","devices":["Cycles CPU"],"fallback":"No compatible Cycles GPU device initialized","probes":probes}


def apply_presentation(settings):
    """Presentation-only revisions; source geometry and coordinates are retained."""
    selected=set(str(value) for value in settings.get('selectedIds', []))
    hidden=set(str(value) for value in settings.get('hiddenIds', []))
    for obj in bpy.context.scene.objects:
        if obj.type != 'MESH':
            continue
        identity=str(obj.get('phaseforge_node_id', obj.name))
        obj.hide_render=identity in hidden
        for slot in obj.material_slots:
            if not slot.material or not slot.material.use_nodes:
                continue
            slot.material=slot.material.copy()
            shader=slot.material.node_tree.nodes.get('Principled BSDF')
            if shader is None:
                continue
            if identity in selected:
                tint=color(settings.get('highlightColor', '#ffd45a'))
            elif settings.get('color'):
                tint=color(settings['color'])
            else:
                tint=tuple(shader.inputs['Base Color'].default_value[:3])
            if selected and settings.get('dimOthers') and identity not in selected:
                tint=tuple(value*.22 for value in tint)
            contrast=number(settings.get('contrast'),1,.1,3)
            tint=tuple(max(0,min(1,(value-.5)*contrast+.5)) for value in tint)
            shader.inputs['Base Color'].default_value=(*tint,1)
            shader.inputs['Alpha'].default_value=number(settings.get('opacity'),1,.05,1)
            slot.material.diffuse_color=(*tint,shader.inputs['Alpha'].default_value)


def main():
    global STYLE
    arguments=sys.argv[sys.argv.index("--")+1:]
    source,out=Path(arguments[0]).resolve(),Path(arguments[1]).resolve()
    budget_seconds=max(0,int(arguments[2]));memory_budget=int(arguments[3])
    if source.stat().st_size>48*1024*1024:
        raise ValueError("Input exceeds the render data budget")
    payload=json.loads(source.read_text(encoding="utf-8"));request=payload["request"]
    STYLE=request["style"]
    # Zero is explicit timer-off under the host's process-tree supervisor.
    if budget_seconds:
        threading.Thread(target=watchdog,args=(budget_seconds,int(payload["parent_pid"])),daemon=True).start()
    bpy.ops.wm.read_factory_settings(use_empty=True)
    started=time.monotonic()
    provenance=build_geometry(payload)
    geometry=finalize_geometry()
    mapping=frame_scene(request)
    appearance=payload.get('presentation') or {}
    apply_presentation(appearance)
    scene=bpy.context.scene
    scene.render.engine="CYCLES"
    scene.cycles.samples=min(256,max(16,int(request["samples"])))
    scene.cycles.use_denoising=True
    scene.cycles.use_adaptive_sampling=True
    scene.cycles.adaptive_threshold=.025
    scene.cycles.max_bounces=6;scene.cycles.diffuse_bounces=3;scene.cycles.glossy_bounces=3;scene.cycles.transmission_bounces=3
    scene.render.threads_mode="FIXED";scene.render.threads=max(1,min(16,(os.cpu_count() or 2)-1))
    scene.render.resolution_x=min(2048,max(256,int(request["width"])))
    scene.render.resolution_y=min(2048,max(256,int(request["height"])))
    scene.render.resolution_percentage=100
    scene.render.image_settings.file_format="PNG"
    scene.render.filepath=str(out/"render.png")
    world=bpy.data.worlds.new("Scientific darkroom");world.use_nodes=True
    world.node_tree.nodes["Background"].inputs["Color"].default_value=(.012,.022,.033,1)
    if appearance.get('background'):
        world.node_tree.nodes["Background"].inputs["Color"].default_value=(*color(appearance['background']),1)
    world.node_tree.nodes["Background"].inputs["Strength"].default_value=.3 if request["style"]=="microscopy" else .6
    scene.world=world
    scene.view_settings.exposure=math.log2(number(appearance.get('exposure'),1,.05,8))
    try:
        scene.view_settings.view_transform="AgX"
    except TypeError:
        scene.view_settings.view_transform="Standard"
    actual=cycles_devices(scene,payload.get("gpu_enabled",True))
    scene["phaseforge_provenance"]=json.dumps(provenance)
    scene["phaseforge_coordinate_mapping"]=json.dumps(mapping)
    print("PHASEFORGE_RENDER "+json.dumps({"stage":"cycles","renderer":actual,"vertices":geometry}),flush=True)
    try:
        bpy.ops.render.render(write_still=True)
    except RuntimeError as error:
        if actual["backend"]=="CPU":
            raise
        actual["fallback"]="GPU render failed: "+str(error)[:600]
        actual["attempted_backend"]=actual["backend"]
        actual["backend"]="CPU";actual["devices"]=["Cycles CPU"]
        scene.cycles.device="CPU"
        bpy.ops.render.render(write_still=True)
    bpy.ops.wm.save_as_mainfile(filepath=str(out/"scene.blend"),check_existing=False)
    bpy.ops.export_scene.gltf(filepath=str(out/"scene.glb"),export_format="GLB",export_apply=True,export_extras=True,export_cameras=True,export_lights=False,export_animations=False)
    metadata={"engine":"Blender Cycles","blender_version":bpy.app.version_string,"actual_compute":actual,"elapsed_seconds":round(time.monotonic()-started,3),"width":scene.render.resolution_x,"height":scene.render.resolution_y,"samples_cap":scene.cycles.samples,"memory_budget_bytes":memory_budget,"vertices":geometry,"source":provenance,"coordinate_mapping":mapping,"warnings":WARNINGS,"scientific_scope":"A rendered surface is a visualization. It is not microscopy acquisition, a validated dynamics trajectory, binding evidence, or a claim of drug efficacy.","export_note":"PNG and BLEND retain Cycles lighting/materials; GLB carries mesh geometry, base materials, camera and provenance, and uses the viewer's lighting. Cycles area lights and procedural micro-bump shaders are not baked into GLB."}
    (out/"renderer.json").write_text(json.dumps(metadata,indent=2),encoding="utf-8")
    print("PHASEFORGE_RENDER "+json.dumps({"stage":"completed","actual_compute":actual}),flush=True)


if __name__=="__main__":
    main()
