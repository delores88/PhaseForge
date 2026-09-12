#!/usr/bin/env python3
"""Bounded, data-only AthenaK 1.1 reader for little-endian x86-64 outputs.

Format reference: IAS-Astrophysics/athenak commit
c5a0d7f9155a70149931bf0be5a4ffb673f2532a, src/outputs/binary.cpp and
vis/python/bin_convert.py. This independently implemented reader retains AMR
blocks; it never flattens, interpolates, evaluates input expressions or executes
the upstream reader. A decoded field is not proof of scientific validity.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import struct
from pathlib import Path

import numpy as np

MAX_FILE_BYTES = 256 * 1024 * 1024
MAX_HEADER_BYTES = 1024 * 1024
MAX_LINE_BYTES = 16384
MAX_VALUES = 32_000_000
MAX_BLOCKS = 65536
UPSTREAM_COMMIT = "c5a0d7f9155a70149931bf0be5a4ffb673f2532a"


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def parameter_blocks(text: str) -> dict:
    result, block = {}, None
    for raw in text.splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if line == "<par_end>":
            break
        if line.startswith("<") and line.endswith(">"):
            block = line[1:-1]
            if block in result:
                raise ValueError("duplicate parameter block")
            result[block] = {}
        elif block and "=" in line:
            key, value = (x.strip() for x in line.split("=", 1))
            if not key or not value or key in result[block]:
                raise ValueError("empty or duplicate parameter")
            result[block][key] = value
        else:
            raise ValueError("malformed parameter header")
    return result


def _exact(stream, count: int) -> bytes:
    value = stream.read(count)
    if len(value) != count:
        raise ValueError("truncated native output")
    return value


def _line(stream) -> str:
    raw = stream.readline(MAX_LINE_BYTES + 1)
    if not raw.endswith(b"\n") or len(raw) > MAX_LINE_BYTES:
        raise ValueError("invalid or oversized native header line")
    return raw.decode("utf-8", errors="strict").strip()


def _property(stream, expected: str) -> str:
    key, separator, value = _line(stream).partition("=")
    if not separator or key.strip() != expected:
        raise ValueError(f"expected header property {expected}")
    return value.strip()


def read_binary(path: str | Path) -> dict:
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError("native source must be a plain file")
    size = path.stat().st_size
    if not 1 <= size <= MAX_FILE_BYTES:
        raise ValueError("native source exceeds bounded decoder size")
    with path.open("rb") as stream:
        if _line(stream) != "Athena binary output version=1.1":
            raise ValueError("unsupported AthenaK native output version")
        if int(_property(stream, "size of preheader")) != 5:
            raise ValueError("unsupported native preheader")
        time = float(_property(stream, "time"))
        cycle = int(_property(stream, "cycle"))
        locsize = int(_property(stream, "size of location"))
        varsize = int(_property(stream, "size of variable"))
        if not math.isfinite(time) or time < 0 or cycle < 0:
            raise ValueError("invalid time or cycle")
        if locsize not in (4, 8) or varsize not in (4, 8):
            raise ValueError("unsupported numeric width")
        nvars = int(_property(stream, "number of variables"))
        names_line = _line(stream).split()
        names = names_line[1:]
        if (not names_line or names_line[0] != "variables:" or not 1 <= nvars <= 128
                or len(names) != nvars or len(set(names)) != nvars
                or any(not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]{0,63}", n) for n in names)):
            raise ValueError("invalid native variable table")
        header_size = int(_property(stream, "header offset"))
        if not 1 <= header_size <= MAX_HEADER_BYTES:
            raise ValueError("invalid native parameter header size")
        header = _exact(stream, header_size).decode("utf-8", errors="strict")
        params = parameter_blocks(header)
        try:
            shape = tuple(int(params["meshblock"][f"nx{a}"]) for a in (1, 2, 3))
            root_shape = tuple(int(params["mesh"][f"nx{a}"]) for a in (1, 2, 3))
            nghost = int(params["mesh"]["nghost"])
        except (KeyError, TypeError) as exc:
            raise ValueError("missing native grid parameters") from exc
        if any(n < 1 or n > 65536 for n in shape + root_shape) or not 0 <= nghost <= 8:
            raise ValueError("invalid native grid dimensions")
        blocks, identities, total_values = [], set(), 0
        while stream.tell() < size:
            if len(blocks) >= MAX_BLOCKS:
                raise ValueError("native block count exceeds bound")
            index = struct.unpack("<6i", _exact(stream, 24))
            logical = struct.unpack("<4i", _exact(stream, 16))
            geometry = struct.unpack("<" + ("d" if locsize == 8 else "f") * 6,
                                     _exact(stream, locsize * 6))
            if logical in identities or any(x < 0 for x in logical) or logical[3] > 30:
                raise ValueError("duplicate or invalid native block identity")
            identities.add(logical)
            dims = tuple(index[2 * a + 1] - index[2 * a] + 1 for a in range(3))
            for a in range(3):
                axis_ghost = nghost if a == 0 or shape[a] > 1 else 0
                if (dims[a] < 1 or index[2 * a] < 0
                        or index[2 * a + 1] >= shape[a] + 2 * axis_ghost
                        or not all(math.isfinite(x) for x in geometry[2*a:2*a+2])
                        or geometry[2*a] >= geometry[2*a+1]):
                    raise ValueError("invalid native block geometry/index")
            values = math.prod(dims) * nvars
            total_values += values
            if total_values > MAX_VALUES:
                raise ValueError("native array exceeds decoder allocation bound")
            data = np.frombuffer(_exact(stream, values * varsize),
                                 dtype=f"<f{varsize}").reshape((nvars,) + dims[::-1])
            if not np.isfinite(data).all():
                raise ValueError("nonfinite native field value")
            # Geometry is each full physical block; output indices retain any slice/ghost offset.
            coordinates = []
            for a in range(3):
                dx = (geometry[2*a+1] - geometry[2*a]) / shape[a]
                axis_ghost = nghost if a == 0 or shape[a] > 1 else 0
                coordinates.append(geometry[2*a] +
                    (np.arange(index[2*a], index[2*a+1] + 1) - axis_ghost + 0.5) * dx)
            blocks.append({"index": index, "logical": logical, "geometry": geometry,
                           "coordinates": coordinates,
                           "fields": {name: data[i] for i, name in enumerate(names)}})
        if not blocks:
            raise ValueError("native output has no retained mesh blocks")
    return {"schema": "phaseforge.athenak-native.v1", "path": str(path), "bytes": size,
            "sha256": sha256(path), "format": "Athena binary output version=1.1",
            "endianness": "little", "location_bytes": locsize, "variable_bytes": varsize,
            "time": time, "cycle": cycle, "variables": names, "parameters": params,
            "root_shape": root_shape, "block_shape": shape, "nghost": nghost,
            "blocks": blocks}


def export_blocks(frame: dict, destination: Path) -> dict:
    """Retain blocks/coordinates without treating AMR as a uniform display grid."""
    destination.mkdir(parents=True, exist_ok=False)
    receipt = {k: v for k, v in frame.items() if k != "blocks"}
    receipt["decoder_source_sha256"] = sha256(Path(__file__))
    receipt["upstream_format_commit"] = UPSTREAM_COMMIT
    receipt["coordinate_units"] = "unspecified code units; interpret retained physical model"
    receipt["interpolation"] = "none"
    receipt["blocks"] = []
    for i, block in enumerate(frame["blocks"]):
        path = destination / f"block-{i:06d}.npz"
        np.savez(path, x1=block["coordinates"][0], x2=block["coordinates"][1],
                 x3=block["coordinates"][2], **block["fields"])
        receipt["blocks"].append({"path": path.name, "sha256": sha256(path),
            "bytes": path.stat().st_size, "index": block["index"],
            "logical": block["logical"], "geometry": block["geometry"],
            "fields": {name: {"shape": list(values.shape), "min": float(values.min()),
                       "max": float(values.max())} for name, values in block["fields"].items()}})
    (destination / "manifest.json").write_text(json.dumps(receipt, indent=2) + "\n",
                                             encoding="utf-8")
    return receipt


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = export_blocks(read_binary(args.source), args.output)
    print(json.dumps({"time": result["time"], "blocks": len(result["blocks"]),
                      "source_sha256": result["sha256"], "output": str(args.output)}))
