"""Read small, hash-bound continuum acceptance arrays without NumPy or pickle."""
import argparse
import ast
import hashlib
import io
import json
import math
from pathlib import Path
import re
import stat
import struct
import sys
import zipfile

MAX_ARCHIVE = 1024 * 1024
MAX_NPY = 8192
MAX_HEADER = 4096
CHANNELS = {
    "heat_conduction_2d": ("temperature_K", "heat_flux_x_W_m2", "heat_flux_y_W_m2"),
    "navier_stokes_2d": ("vorticity_s_inv", "velocity_x_m_s", "velocity_y_m_s", "pressure_Pa", "divergence_s_inv"),
}


def checked_file(path, expected, limit):
    if not isinstance(expected, str) or re.fullmatch(r"[0-9a-fA-F]{64}", expected) is None:
        raise ValueError("A complete SHA-256 pin is required")
    with Path(path).open("rb") as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ValueError("Retained array file exceeds its bounded size")
    actual = hashlib.sha256(raw).hexdigest()
    if actual != expected.lower():
        raise ValueError("Retained array SHA-256 mismatch")
    return raw, actual


def decode_npy(raw):
    if len(raw) > MAX_NPY or raw[:6] != b"\x93NUMPY" or len(raw) < 10:
        raise ValueError("Invalid or oversized NPY member")
    version = raw[6:8]
    if version == b"\x01\x00":
        start, size = 10, struct.unpack_from("<H", raw, 8)[0]
    elif version in (b"\x02\x00", b"\x03\x00") and len(raw) >= 12:
        start, size = 12, struct.unpack_from("<I", raw, 8)[0]
    else:
        raise ValueError("Unsupported NPY version")
    end = start + size
    if not 0 < size <= MAX_HEADER or end > len(raw) or end % 16:
        raise ValueError("Invalid bounded NPY header length/alignment")
    header = raw[start:end]
    if not header.endswith(b"\n"):
        raise ValueError("NPY header lacks its newline terminator")
    parsed = ast.parse(header.decode("utf-8" if version == b"\x03\x00" else "latin1").strip(), mode="eval")
    if not isinstance(parsed.body, ast.Dict) or len(list(ast.walk(parsed))) > 64:
        raise ValueError("NPY header must be a small literal dictionary")
    keys = [key.value if isinstance(key, ast.Constant) else None for key in parsed.body.keys]
    if len(keys) != 3 or set(keys) != {"descr", "fortran_order", "shape"}:
        raise ValueError("NPY header fields must be exact and unique")
    values = ast.literal_eval(parsed)
    shape = values["shape"]
    if (values["descr"] != "<f8" or values["fortran_order"] is not False or not isinstance(shape, tuple)
            or shape != (16, 16) or any(type(dimension) is not int for dimension in shape)):
        raise ValueError("Expected 16x16 little-endian float64 C-order NPY")
    if len(raw) - end != 16 * 16 * 8:
        raise ValueError("NPY payload is truncated or has trailing data")
    flat = struct.unpack_from("<256d", raw, end)
    if not all(math.isfinite(value) for value in flat):
        raise ValueError("NPY contains nonfinite numerical values")
    return [list(flat[row * 16:(row + 1) * 16]) for row in range(16)]


def decode_state(raw, engine):
    if engine not in CHANNELS:
        raise ValueError("Unsupported continuum engine")
    if len(raw) > MAX_ARCHIVE:
        raise ValueError("NPZ exceeds 1 MiB")
    expected = {name + ".npy" for name in CHANNELS[engine]}
    with zipfile.ZipFile(io.BytesIO(raw)) as archive:
        members = archive.infolist()
        names = [member.filename for member in members]
        if len(members) > 5 or len(names) != len(set(names)) or set(names) != expected:
            raise ValueError("NPZ must contain exactly the unique declared channel members")
        channels = {}
        for member in members:
            mode = (member.external_attr >> 16) & 0xFFFF
            if member.is_dir() or stat.S_IFMT(mode) not in (0, stat.S_IFREG) or member.external_attr & 0x410:
                raise ValueError("NPZ links and directories are not allowed")
            if member.flag_bits & 1 or member.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                raise ValueError("NPZ encryption or compression method is not allowed")
            if not 0 < member.file_size <= MAX_NPY or not 0 <= member.compress_size <= MAX_ARCHIVE:
                raise ValueError("NPZ member exceeds its bounded size")
            with archive.open(member, "r") as stream:
                payload = stream.read(MAX_NPY + 1)
            if len(payload) != member.file_size or len(payload) > MAX_NPY:
                raise ValueError("NPZ member length mismatch")
            channels[member.filename[:-4]] = decode_npy(payload)
    return channels


def read_arrays(state, state_sha256, primary, primary_sha256, engine):
    state_raw, state_hash = checked_file(state, state_sha256, MAX_ARCHIVE)
    primary_raw, primary_hash = checked_file(primary, primary_sha256, MAX_NPY)
    channels = decode_state(state_raw, engine)
    primary_values = decode_npy(primary_raw)
    if primary_values != channels[CHANNELS[engine][0]]:
        raise ValueError("Primary NPY differs from the declared primary state channel")
    return {"channels": channels, "state_sha256": state_hash, "primary_sha256": primary_hash}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--state", required=True)
    parser.add_argument("--state-sha256", required=True)
    parser.add_argument("--primary", required=True)
    parser.add_argument("--primary-sha256", required=True)
    parser.add_argument("--engine", choices=tuple(CHANNELS), required=True)
    args = parser.parse_args()
    try:
        result = read_arrays(args.state, args.state_sha256, args.primary, args.primary_sha256, args.engine)
    except (OSError, ValueError, SyntaxError, zipfile.BadZipFile, RuntimeError, EOFError, NotImplementedError) as error:
        print(json.dumps({"error": str(error)}), file=sys.stderr)
        return 2
    print(json.dumps(result, allow_nan=False, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
