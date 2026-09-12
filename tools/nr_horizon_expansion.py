#!/usr/bin/env python3
"""Independent retained-surface expansion diagnostic; never evolves or finds a horizon.

Geometry uses the embedded surface's second fundamental form, not FastFlow's
level-set Hessian. A finite result does not certify an apparent horizon, merger,
physical boost, or numerical convergence.

UNADMITTED RESEARCH CHECKPOINT (2026-09-12): work stopped when the user changed
the release goal to dependable research software. Only the initial analytic
geometry/interpolation controls were evaluated. The later retained-data CLI,
source/time admission and AMR patch extraction have not been admitted or tested
against actual data. This module is not integrated into the app or release gate.
"""
from __future__ import annotations
import argparse
from decimal import Decimal
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import re
import stat
import sys

import numpy as np

MAX_NATIVE = 128 * 1024**2
MAX_TEXT = 16 * 1024**2
MAX_PATCH_NODES = 262144
UPSTREAM = 'c5a0d7f9155a70149931bf0be5a4ffb673f2532a'
DECODER_SHA256 = '3f89754d60cd2daa7b8708da4bafa8334fe101fa76759d72be1b2056840cdc1f'
LABEL_SEMANTICS = 'pre_increment_mesh_time_for_final_RK_stage_state'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def array(value, shape=None):
    result = np.asarray(value, dtype=np.float64)
    require((shape is None or result.shape == shape) and np.isfinite(result).all(),
            'Missing, malformed or nonfinite numerical data')
    return result


def positive_metric(g):
    require(np.allclose(g, np.swapaxes(g, -1, -2), rtol=0, atol=1e-12), 'Metric is not symmetric')
    try:
        np.linalg.cholesky(g)
    except np.linalg.LinAlgError as exc:
        raise ValueError('Nonpositive physical spatial metric') from exc


def _multiply(a, b):
    """Value and first/second theta derivatives, independent recurrence algebra."""
    return np.array([a[0]*b[0], a[1]*b[0]+a[0]*b[1],
                     a[2]*b[0]+2*a[1]*b[1]+a[0]*b[2]])


def radial_derivatives(coefficients, lmax, theta, phi):
    require(isinstance(lmax, int) and 0 <= lmax <= 16, 'Unsupported harmonic degree')
    coefficients = array(coefficients, ((lmax+1)**2,))
    require(0 < theta < math.pi and math.isfinite(phi), 'Angular point is at a pole or invalid')
    c, s = math.cos(theta), math.sin(theta)
    cosine, sine = np.array([c, -s, -c]), np.array([s, c, -s])
    legendre = {}
    for m in range(lmax+1):
        p = np.array([1., 0., 0.])
        for j in range(1, m+1):
            p = -(2*j-1)*_multiply(p, sine)
        legendre[m, m] = p
        if m < lmax:
            legendre[m+1, m] = (2*m+1)*_multiply(cosine, p)
        for l in range(m+2, lmax+1):
            legendre[l, m] = ((2*l-1)*_multiply(cosine, legendre[l-1, m])
                              -(l+m-1)*legendre[l-2, m])/(l-m)
    # Packed normalized Condon–Shortley real harmonics: m0, cos1,sin1,... .
    result, offset = np.zeros(6), 0
    for l in range(lmax+1):
        for m in range(l+1):
            norm = math.sqrt((2*l+1)/(4*math.pi)*math.factorial(l-m)/math.factorial(l+m))
            if m:
                norm *= math.sqrt(2)
            p, pt, ptt = norm*legendre[l, m]
            cos, sin = math.cos(m*phi), math.sin(m*phi)
            result += coefficients[offset]*np.array([p*cos, pt*cos, -m*p*sin, ptt*cos, -m*pt*sin, -m*m*p*cos])
            offset += 1
            if m:
                result += coefficients[offset]*np.array([p*sin, pt*sin, m*p*cos, ptt*sin, m*pt*cos, -m*m*p*sin])
                offset += 1
    require(np.isfinite(result).all() and result[0] > 0, 'Nonpositive or nonfinite surface radius')
    return result


def surface_geometry(coefficients, lmax, center, theta, phi):
    center = array(center, (3,))
    r, rt, rp, rtt, rtp, rpp = radial_derivatives(coefficients, lmax, theta, phi)
    s, c, cp, sp = math.sin(theta), math.cos(theta), math.cos(phi), math.sin(phi)
    n = np.array([s*cp, s*sp, c]); nt = np.array([c*cp, c*sp, -s]); np_ = np.array([-s*sp, s*cp, 0.])
    ntp = np.array([-c*sp, c*cp, 0.]); npp = np.array([-s*cp, -s*sp, 0.])
    tangent = np.array([rt*n+r*nt, rp*n+r*np_])
    second = np.array([[rtt*n+2*rt*nt-r*n, rtp*n+rt*np_+rp*nt+r*ntp],
                       [rtp*n+rt*np_+rp*nt+r*ntp, rpp*n+2*rp*np_+r*npp]])
    return center+r*n, tangent, second


def point_expansion(x, center, tangent, second, g, dg, K):
    g, dg, K = array(g, (3, 3)), array(dg, (3, 3, 3)), array(K, (3, 3))
    positive_metric(g)
    require(np.allclose(K, K.T, rtol=0, atol=1e-12), 'Extrinsic curvature is not symmetric')
    inv = np.linalg.inv(g)
    normal = np.cross(tangent[0], tangent[1])
    norm2 = float(normal@inv@normal)
    require(math.isfinite(norm2) and norm2 > 0, 'Degenerate surface normal')
    normal /= math.sqrt(norm2)
    require(float(normal@(x-center)) > 0, 'Surface normal does not point outward')
    q = tangent@g@tangent.T
    positive_metric(q)
    qinv = np.linalg.inv(q)
    connection = np.empty((3, 3, 3))
    for i in range(3):
        for j in range(3):
            for k in range(3):
                connection[i, j, k] = .5*sum(inv[i, l]*(dg[j, k, l]+dg[k, j, l]-dg[l, j, k]) for l in range(3))
    cov_second = second + np.einsum('ijk,aj,bk->abi', connection, tangent, tangent)
    divergence = -float(np.einsum('ab,i,abi', qinv, normal, cov_second))
    up = inv@normal
    expansion = divergence+float(up@K@up)-float(np.einsum('ij,ij', inv, K))
    area_element = math.sqrt(float(np.linalg.det(q)))
    require(math.isfinite(expansion) and math.isfinite(area_element) and area_element > 0, 'Invalid surface curvature or area')
    return expansion, area_element


def evaluate_surface(coefficients, lmax, center, metric_sampler, ntheta=10, nphi=20):
    require(isinstance(ntheta, int) and isinstance(nphi, int) and lmax+1 <= ntheta <= 64
            and 2*lmax+1 <= nphi <= 128, 'Insufficient or unbounded angular quadrature')
    center = array(center, (3,))
    mu, weights = np.polynomial.legendre.leggauss(ntheta)
    points = []
    # Both endpoints of the list are included: the native RangePolicy end bug
    # must not be reproduced by this independent quadrature.
    for p in range(nphi):
        phi = 2*math.pi*p/nphi
        for cosine, weight in zip(mu, weights):
            theta = math.acos(float(cosine))
            x, tangent, second = surface_geometry(coefficients, lmax, center, theta, phi)
            H, area = point_expansion(x, center, tangent, second, *metric_sampler(x))
            dA = float(weight)*(2*math.pi/nphi)*area/math.sin(theta)
            points.append({'xyz': x.tolist(), 'expansion': H, 'area_weight': dA})
    area = math.fsum(p['area_weight'] for p in points)
    rms = math.sqrt(math.fsum(p['expansion']**2*p['area_weight'] for p in points)/area)
    maximum = max(abs(p['expansion']) for p in points)
    radius = math.sqrt(area/(4*math.pi))
    return {'area': area, 'M_irr': math.sqrt(area/(16*math.pi)), 'areal_radius': radius,
            'expansion_rms': rms, 'max_abs_expansion': maximum,
            'expansion_integral': math.fsum(p['expansion']*p['area_weight'] for p in points),
            'dimensionless_rms': radius*rms, 'dimensionless_max': radius*maximum,
            'quadrature_points': len(points), 'points': points, 'surface_validated': False}


def _weights(u, degree):
    nodes = list(range(degree+1))
    values, derivatives = [], []
    for j in nodes:
        others = [k for k in nodes if k != j]
        denominator = math.prod(j-k for k in others)
        values.append(math.prod(u-k for k in others)/denominator)
        derivatives.append(sum(math.prod(u-k for k in others if k != removed) for removed in others)/denominator)
    return np.array(values), np.array(derivatives)


class RegularPatch:
    """Physical tensors on complete same-level cell centres, axes x,y,z.

    Tensor-product polynomial interpolation and derivatives of that same
    polynomial. No ghosts, extrapolation or implicit AMR prolongation.
    """
    def __init__(self, origin, spacing, gamma, K):
        self.origin = array(origin, (3,))
        self.spacing = array([spacing]*3 if np.isscalar(spacing) else spacing, (3,))
        require((self.spacing > 0).all(), 'Nonpositive lattice spacing')
        self.gamma, self.K = array(gamma), array(K)
        require(self.gamma.ndim == 5 and self.gamma.shape[-2:] == (3, 3)
                and self.K.shape == self.gamma.shape, 'Invalid regular-patch tensor shape')
        self.shape = self.gamma.shape[:3]
        require(min(self.shape) >= 4 and math.prod(self.shape) <= MAX_PATCH_NODES, 'Patch size is unsupported')
        positive_metric(self.gamma)

    def sample(self, point, order=3):
        require(order in (3, 5), 'Only degree3/5 tensor interpolation is supported')
        coordinate = (array(point, (3,))-self.origin)/self.spacing
        start = np.floor(coordinate).astype(int)-order//2
        require(all(0 <= i and i+order < n for i, n in zip(start, self.shape)), 'Incomplete interpolation stencil; extrapolation is forbidden')
        wd = [_weights(float(u-i), order) for u, i in zip(coordinate, start)]
        w = [p[0] for p in wd]
        patch = tuple(slice(i, i+order+1) for i in start)
        g = np.einsum('a,b,c,abcij->ij', *w, self.gamma[patch])
        K = np.einsum('a,b,c,abcij->ij', *w, self.K[patch])
        dg = []
        for axis in range(3):
            dw = list(w); dw[axis] = wd[axis][1]/self.spacing[axis]
            dg.append(np.einsum('a,b,c,abcij->ij', *dw, self.gamma[patch]))
        positive_metric(g)
        return g, np.asarray(dg), K


def _printed(value):
    require(re.fullmatch(r'[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?', value) is not None, 'Invalid printed time')
    number = float(value)
    require(math.isfinite(number) and number >= 0, 'Invalid printed time')
    error = 0.0 if number == 0 else float(Decimal(10)**Decimal(value).as_tuple().exponent)/2
    return number, error


def match_terminal_surface(verbose, summary, shape, stdout, final_time, final_cycle, horizon):
    """Bind only the final completed step; never add a nominal dt to old labels.

    Caller separately verifies pinned source convention, fresh nonrestart input,
    receipt, and the actual terminal native header. Printed values are intervals.
    """
    require(math.isfinite(final_time) and final_time > 0 and isinstance(final_cycle, int)
            and 1 <= final_cycle <= 128 and horizon in (0, 1), 'Invalid terminal state selection')
    steps = re.findall(r'^elapsed=\S+ cycle=(\d+) time=(\S+) dt=(\S+)\s*$', stdout, re.M)
    require([int(r[0]) for r in steps] == list(range(final_cycle+1)), 'Missing or duplicate contiguous evolution cycles')
    require(stdout.count('Terminating on time limit') == 1, 'No unique terminal time-limit marker')
    time_intervals = [_printed(r[1]) for r in steps]
    require(time_intervals[0][0] == 0 and all(b[0] > a[0] for a, b in zip(time_intervals, time_intervals[1:])),
            'Evolution labels do not describe a fresh increasing attempt')
    require(abs(time_intervals[-1][0]-final_time) <= time_intervals[-1][1]+1e-14, 'Final native time differs from terminal cycle')
    # Check every recorded step interval, including the shortened final step.
    for index, ((time, error), (next_time, next_error)) in enumerate(zip(time_intervals, time_intervals[1:])):
        dt, dt_error = _printed(steps[index][2])
        require(dt > 0 and abs(time+dt-next_time) <= error+dt_error+next_error+1e-14,
                'Recorded step interval does not reach the next cycle')
    sections = [section for section in re.split(r'(?=^time=)', verbose, flags=re.M) if section.startswith('time=')]
    rows = [line.split() for line in summary.splitlines() if line.strip() and not line.startswith('#')]
    require(len(sections) == final_cycle and len(rows) == final_cycle, 'Horizon search count does not match completed RK steps')
    fresh = []
    for index, (section, row) in enumerate(zip(sections, rows)):
        match = re.match(r'time=(\S+), cycle=(\d+)\s*\n', section)
        require(match is not None and int(match[2]) == 3 and len(row) == 12 and row[0] == '3', 'Horizon stage or row shape differs')
        vt, ve = _printed(match[1]); st, se = _printed(row[1]); tt, te = time_intervals[index]
        require(abs(st-tt) <= se+te+1e-14 and abs(vt-tt) <= ve+te+1e-14, 'Horizon label belongs to a different evolution step')
        found = re.search(rf'^Found horizon {horizon}\s*$', section, re.M) is not None
        require(not (found and re.search(r'^Failed', section, re.M)), 'Ambiguous horizon search outcome')
        if found:
            fresh.append(index)
    require(fresh and fresh[-1] == final_cycle-1, 'Final horizon search is stale, failed or interrupted')
    values = array([float(v) for v in rows[-1][2:]], (10,))
    require(values[0] > 0 and values[5] > 0, 'Final native surface properties are nonpositive')
    shapes = list(re.finditer(r'^# iter = (\d+), Time = (\S+)\s*\n([^\n]+)', shape, re.M))
    require(len(shapes) == len(fresh), 'Missing or extra fresh harmonic surfaces')
    for record, index in zip(shapes, fresh):
        require(record[1] == '3' and _printed(record[2])[0] == _printed(rows[index][1])[0], 'Harmonic surface has a mismatched time or stage')
    center = re.search(r'center = \(([^,]+), ([^,]+), ([^)]+)\)', sections[-1])
    require(center is not None, 'Missing final retained surface center')
    center = array([float(v) for v in center.groups()], (3,)).tolist()
    coefficients = array([float(v) for v in shapes[-1][3].split()])
    lmax = math.isqrt(len(coefficients))-1
    require(0 <= lmax <= 16 and len(coefficients) == (lmax+1)**2, 'Invalid harmonic coefficient count')
    return {'coefficients': coefficients.tolist(), 'lmax': lmax, 'center': center,
            'time_mapping': {'schema': 'phaseforge.nr-final-stage-time-map.v1',
                'native_label_time': float(rows[-1][1]), 'physical_frame_time': final_time,
                'physical_cycle': final_cycle, 'iteration_stage': 3, 'horizon_search_ordinal': final_cycle-1,
                'fresh_surface_ordinal': len(fresh)-1, 'label_semantics': LABEL_SEMANTICS,
                'previous_mesh_time_printed': steps[-2][1], 'last_step_dt_printed': steps[-2][2],
                'method': 'Pinned final-RK task order, contiguous fresh-start cycle/search sequences, completed terminal native header, and overlapping printed-time intervals; no nominal-step or nearest-time mapping.',
                'source_commit': UPSTREAM, 'rounded_labels_are_not_exact_times': True}}


def patch_from_frame(frame, points, order):
    require(frame['block_count'] == 624 if 'block_count' in frame else len(frame['blocks']) == 624, 'Unexpected native block count')
    require(frame['block_shape'] == (8, 8, 8) and frame['root_shape'] == (32, 32, 32)
            and frame['nghost'] == 2 and frame['variable_bytes'] == 4, 'Unsupported native grid or precision')
    p = frame['parameters']
    require(p['mesh_refinement']['refinement'] == 'static' and float(p['z4c'].get('chi_psi_power', '-4')) == -4,
            'Only static mesh with chi=psi^-4 is admitted')
    level = max(block['logical'][3] for block in frame['blocks'])
    require(level == 4, 'Unexpected finest native level')
    mesh = p['mesh']
    spacing = np.array([(float(mesh[f'x{a}max'])-float(mesh[f'x{a}min']))/int(mesh[f'nx{a}'])/2**level for a in (1, 2, 3)])
    origin = np.array([float(mesh[f'x{a}min']) for a in (1, 2, 3)])+spacing/2
    coordinate = (array(points)-origin)/spacing
    starts = np.floor(coordinate).astype(int)-order//2
    lo, hi = starts.min(axis=0), (starts+order).max(axis=0)
    shape = tuple((hi-lo+1).tolist())
    require(min(shape) >= order+1 and math.prod(shape) <= MAX_PATCH_NODES, 'Native stencil patch exceeds bound')
    gamma = np.full(shape+(3, 3), np.nan); K = np.full_like(gamma, np.nan)
    filled = np.zeros(shape, dtype=bool); selected = []
    axes = ('xx', 'xy', 'xz', 'yy', 'yz', 'zz')
    pairs = ((0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2))
    for block in frame['blocks']:
        require(tuple(block['index']) == (2, 9, 2, 9, 2, 9), 'Native frame omits active cells or includes ghosts')
        if block['logical'][3] != level:
            continue
        integer = []
        for a in range(3):
            u = (block['coordinates'][a]-origin[a])/spacing[a]
            require(np.allclose(u, np.rint(u), atol=1e-9, rtol=0), 'Fine cells are off the declared lattice')
            integer.append(np.rint(u).astype(int))
        local = [np.flatnonzero((u >= low) & (u <= high)) for u, low, high in zip(integer, lo, hi)]
        if any(len(v) == 0 for v in local):
            continue
        destination = np.ix_(*(u[v]-low for u, v, low in zip(integer, local, lo)))
        require(not filled[destination].any(), 'Overlapping native interpolation cells')
        fields = block['fields']
        def values(name):
            require(name in fields, f'Missing native field {name}')
            return array(fields[name]).transpose(2, 1, 0)[np.ix_(*local)]
        chi = values('z4c_chi')
        require((chi > 0).all(), 'Nonpositive native chi; no surface-node excision is allowed')
        trace = values('z4c_Khat')+2*values('z4c_Theta')
        g = np.empty(chi.shape+(3, 3)); k = np.empty_like(g)
        for name, (i, j) in zip(axes, pairs):
            component = values('z4c_g'+name)/chi
            curvature = values('z4c_A'+name)/chi+trace*component/3
            g[..., i, j] = g[..., j, i] = component
            k[..., i, j] = k[..., j, i] = curvature
        gamma[destination] = g; K[destination] = k; filled[destination] = True
        selected.append(list(block['logical']))
    require(filled.all(), 'Missing same-level active-cell stencil; AMR prolongation and extrapolation are forbidden')
    patch = RegularPatch(origin+lo*spacing, spacing, gamma, K)
    patch.provenance = {'degree': order, 'nodes_per_axis': order+1, 'shape_xyz': list(shape),
        'origin_cell_center': patch.origin.tolist(), 'spacing': spacing.tolist(), 'native_level': level,
        'global_cell_index_min': lo.tolist(), 'global_cell_index_max': hi.tolist(), 'native_blocks': selected,
        'metric_reconstruction': 'At native active nodes before interpolation: g=tilde_g/chi; K=tilde_A/chi+(Khat+2*Theta)*g/3. Tensor trace is computed from g and K, not assumed.',
        'derivatives': 'Analytic first derivatives of the same tensor-product interpolating polynomial of physical g.',
        'amr_prolongation': False, 'extrapolation': False, 'excised_surface_nodes': 0}
    return patch


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def _plain(path):
    for p in (path, *path.parents):
        meta = p.lstat()
        require(not stat.S_ISLNK(meta.st_mode) and not getattr(meta, 'st_file_attributes', 0)&0x400, 'Linked source or parent')
    meta = path.stat()
    require(stat.S_ISREG(meta.st_mode) and meta.st_nlink == 1, 'Source is not an independent plain file')
    return meta


def read_pinned(path, expected, maximum=MAX_TEXT):
    require(re.fullmatch('[0-9a-f]{64}', expected or '') is not None, 'Missing exact SHA256')
    before = _plain(path)
    require(before.st_size <= maximum, 'Source exceeds size bound')
    with path.open('rb') as stream:
        raw = stream.read(maximum+1)
    after = _plain(path)
    require((before.st_size, before.st_mtime_ns, before.st_ino) == (after.st_size, after.st_mtime_ns, after.st_ino)
            and len(raw) == before.st_size and digest(raw) == expected, 'Source changed or differs from its pin')
    return raw


def _json(raw):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, 'Duplicate JSON key')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=unique, parse_constant=lambda _: (_ for _ in ()).throw(ValueError('Nonfinite JSON constant')))


def _relative(name):
    require(isinstance(name, str) and '\\' not in name and ':' not in name, 'Unsafe artifact path')
    path = PurePosixPath(name)
    require(name and str(path) == name and not path.is_absolute() and all(p not in ('.', '..') for p in path.parts), 'Unsafe artifact path')
    return Path(*path.parts)


def _save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('xb') as stream:
        stream.write((json.dumps(value, allow_nan=False, indent=2)+'\n').encode())


def retained_check(diagnostics, result_sha256, horizon, output, order=3, ntheta=10, nphi=20):
    """One terminal surface only; all arrays and code remain data-only inputs."""
    diagnostics, output = Path(diagnostics), Path(output)
    require(diagnostics.is_absolute() and output.is_absolute(), 'Absolute input/output directories are required')
    require(not output.exists() and not output.is_symlink(), 'Output must be a new immutable directory')
    require(not output.resolve().is_relative_to(diagnostics.resolve()), 'Output cannot be inside source diagnostics')
    for parent in output.parents:
        if parent.exists():
            meta = parent.lstat()
            require(not stat.S_ISLNK(meta.st_mode) and not getattr(meta, 'st_file_attributes', 0)&0x400, 'Linked output parent')
    output.mkdir(parents=True)
    retained = []
    try:
        raw = read_pinned(diagnostics/'result.json', result_sha256, 8*1024**2)
        result = _json(raw)
        (output/'selected-result.json').write_bytes(raw)
        require(result['schema'] == 'phaseforge.nr-black-hole-result.v1'
                and result['engine_identity']['engine_id'] in ('athenak_two_punctures_serial', 'athenak_two_punctures_cuda')
                and result['engine_identity']['source']['athenak'] == UPSTREAM, 'Unsupported diagnostic source identity')
        require(result['execution']['complete'] is True and result['execution']['process_completed'] is True
                and result['execution']['retained_time_target_met'] is True, 'Only a complete terminal state can be selected')
        inventory = result['artifacts']; require(len(inventory) <= 4096, 'Artifact inventory exceeds bound')
        descriptors = {}
        for row in inventory:
            name = row['path']; _relative(name)
            require(name.lower() not in descriptors, 'Duplicate retained artifact')
            descriptors[name.lower()] = row
        def source(name, maximum=MAX_TEXT):
            descriptor = descriptors.get(name.lower())
            require(descriptor is not None and descriptor['path'] == name, 'Source is outside exact retained inventory')
            raw = read_pinned(diagnostics/_relative(name), descriptor['sha256'], maximum)
            require(len(raw) == descriptor['bytes'], 'Source length differs')
            target = output/'sources'/_relative(name)
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open('xb') as stream:
                stream.write(raw)
            retained.append(dict(descriptor)); return raw
        receipt_raw = source('sources/execution-receipt.json')
        receipt = _json(receipt_raw); input_raw = source('sources/input.athinput')
        manifest_raw = source('sources/engine.json'); manifest = _json(manifest_raw)
        request_raw = source('sources/request.json'); request = _json(request_raw)
        require(digest(receipt_raw) == result['sources']['receipt_sha256'] and receipt['job_id'] == result['job_id']
                and receipt['termination_reason'] == 'completed' and receipt['exit_code'] == 0
                and receipt['process_group_drained'] is True and receipt['checkpoint_resume_supported'] is False,
                'Terminal process or fresh-attempt provenance is missing')
        require(manifest == result['engine_identity'] and digest(manifest_raw) == receipt['engine_manifest_sha256']
                and digest(request_raw) == receipt['request_sha256'] and digest(input_raw) == receipt['input']['sha256']
                and receipt['executable']['sha256'] == manifest['executable']['sha256']
                and request['input']['sha256'] == digest(input_raw), 'Original execution/input/source pins differ')
        decoder_path = Path(__file__).with_name('athenak_decode.py')
        read_pinned(decoder_path, DECODER_SHA256, 128*1024)
        import athenak_decode as decoder
        require(Path(decoder.__file__).resolve() == decoder_path.resolve(), 'Unexpected decoder import origin')
        parameters = decoder.parameter_blocks(input_raw.decode('utf-8'))
        target = float(parameters['time']['tlim']); basename = parameters['job']['basename']
        require(parameters['time']['integrator'] == 'rk3' and parameters['mesh_refinement']['refinement'] == 'static'
                and re.fullmatch('[A-Za-z0-9_.-]{1,128}', basename), 'Unsupported integration/input convention')
        ff = parameters['fastflow']
        require(horizon in (0, 1) and ff[f'use_puncture_{horizon}'] == str(horizon)
                and float(ff[f'start_time_{horizon}']) == 0 and float(ff[f'stop_time_{horizon}']) >= target
                and ff[f'wait_until_punc_are_close_{horizon}'] == 'false', 'Horizon scheduling is not one search per completed step')
        index = _json(source('frames/index.json'))
        require(index['paired_times'] == result['paired_times'] and index['paired_times'][0] == 0
                and index['paired_times'][-1] == target, 'Terminal native coverage differs from original target')
        frames = [f for f in index['frames'] if f['kind'] == 'metric' and f['time'] == target]
        require(len(frames) == 1, 'Terminal metric state is not unique')
        selected = frames[0]; native = selected['native']
        require(descriptors.get(native['path'].lower()) == native, 'Native field descriptor differs from inventory')
        native_raw = source(native['path'], MAX_NATIVE)
        # The decoded file is a verified private copy, unaffected by source writes.
        del native_raw
        frame = decoder.read_binary(output/'sources'/_relative(native['path']))
        require(frame['time'] == target and frame['cycle'] == selected['cycle'] and frame['sha256'] == native['sha256'], 'Decoded native header differs')
        for name in ('time', 'mesh', 'meshblock', 'mesh_refinement', 'z4c', 'fastflow'):
            for key, value in parameters[name].items():
                require(frame['parameters'][name].get(key) == value, 'Native frame changed original parameters')
        prefix = f'native/{basename}.horizon_'
        verbose = source(prefix+f'verbose_{horizon}.txt').decode('utf-8')
        summary = source(prefix+f'summary_{horizon}.txt').decode('utf-8')
        shape = source(prefix+f'shape_{horizon}.txt').decode('utf-8')
        stdout = source('native/solver.stdout.log').decode('utf-8')
        selection = match_terminal_surface(verbose, summary, shape, stdout, target, frame['cycle'], horizon)
        require(selection['lmax'] == int(ff['lmax']), 'Retained harmonic degree differs from input')
        mu, _ = np.polynomial.legendre.leggauss(ntheta)
        points = [surface_geometry(selection['coefficients'], selection['lmax'], selection['center'], math.acos(float(u)), 2*math.pi*p/nphi)[0]
                  for p in range(nphi) for u in mu]
        patch = patch_from_frame(frame, points, order)
        measured = evaluate_surface(selection['coefficients'], selection['lmax'], selection['center'],
                                    lambda point: patch.sample(point, order), ntheta, nphi)
        _save(output/'points.json', measured.pop('points'))
        report = {'schema': 'phaseforge.nr-independent-expansion.v1', 'status': 'evaluated_retained_surface',
            'job_id': result['job_id'], 'horizon': horizon, 'diagnostic_result_sha256': result_sha256,
            'source_files': retained, 'selected_surface': selection, 'interpolation': patch.provenance,
            'quadrature': {'ntheta': ntheta, 'nphi': nphi, 'all_nodes_included': True}, 'measurements': measured,
            'units': {'length': 'L', 'area': 'L^2', 'mass': 'c^2 L/G', 'expansion': '1/L', 'si_mapping': None},
            'precision_limits': ['Native fields are float32; polynomial derivatives amplify serialization and discretization errors.',
                'Harmonic coefficients have about7 significant printed digits and centers6 decimal places.',
                'Same-level degree3/5 interpolation is an independent reconstruction, not the solver ghost-zone derivative operator.',
                'Native FastFlow omits its last angular node; its reported mass/residual need not agree with all-node quadrature.'],
            'horizon_validated': False, 'merger_validated': False, 'physical_boost_validated': False,
            'numerical_convergence_validated': False,
            'checker_sha256': digest(Path(__file__).read_bytes()), 'decoder_sha256': DECODER_SHA256}
        _save(output/'report.json', report)
        return report
    except Exception as error:
        _save(output/'failure.json', {'schema': 'phaseforge.nr-independent-expansion-failure.v1',
            'error': str(error), 'retained_sources': retained, 'horizon_validated': False,
            'evolution_executed': False})
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--diagnostics', required=True, type=Path)
    parser.add_argument('--result-sha256', required=True)
    parser.add_argument('--horizon', required=True, type=int, choices=(0, 1))
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--order', type=int, choices=(3, 5), default=3)
    parser.add_argument('--ntheta', type=int, choices=(10, 20), default=10)
    parser.add_argument('--nphi', type=int, choices=(20, 40), default=20)
    args = parser.parse_args()
    try:
        result = retained_check(args.diagnostics, args.result_sha256, args.horizon, args.output, args.order, args.ntheta, args.nphi)
        print(json.dumps({'status': result['status'], 'horizon_validated': False, 'report': str(args.output/'report.json')}))
        return 0
    except Exception as error:
        print(json.dumps({'status': 'rejected', 'error': str(error), 'evolution_executed': False}), file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
