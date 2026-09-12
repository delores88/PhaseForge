"""Independent standard-library NPY/NPZ fixtures; no numerical worker calls."""
import hashlib
import importlib.util
import io
from pathlib import Path
import stat
import struct
import subprocess
import sys
import tempfile
import unittest
import warnings
import zipfile
import json

SPEC = importlib.util.spec_from_file_location("native_continuum_arrays", Path(__file__).with_name("native-continuum-arrays.py"))
arrays = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(arrays)
EXPECTED_CHANNELS = {
    "heat_conduction_2d": ("temperature_K", "heat_flux_x_W_m2", "heat_flux_y_W_m2"),
    "navier_stokes_2d": ("vorticity_s_inv", "velocity_x_m_s", "velocity_y_m_s", "pressure_Pa", "divergence_s_inv"),
}


def npy(values=None, *, descr="<f8", shape=(16, 16), fortran=False, version=1, header=None):
    values = [row * 100 + column / 8 for row in range(16) for column in range(16)] if values is None else values
    text = header or repr({"descr": descr, "fortran_order": fortran, "shape": shape})
    start = 10 if version == 1 else 12
    encoded = text.encode("ascii")
    encoded += b" " * ((-(start + len(encoded) + 1)) % 64) + b"\n"
    return b"\x93NUMPY" + bytes((version, 0)) + struct.pack("<H" if version == 1 else "<I", len(encoded)) + encoded + struct.pack("<" + str(len(values)) + "d", *values)


def archive(entries, compression=zipfile.ZIP_STORED):
    target = io.BytesIO()
    with zipfile.ZipFile(target, "w", compression=compression) as output:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", UserWarning)
            for name, raw in entries:
                output.writestr(name, raw)
    return target.getvalue()


def entries(engine, primary=None):
    return [(name + ".npy", primary if index == 0 and primary is not None else npy([float(index)] * 256))
            for index, name in enumerate(EXPECTED_CHANNELS[engine])]


class ContinuumArrayTests(unittest.TestCase):
    def test_actual_cli_returns_both_engine_channel_shapes_with_primary_header_independence(self):
        for engine in EXPECTED_CHANNELS:
            for compression in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED):
                with self.subTest(engine=engine, compression=compression), tempfile.TemporaryDirectory() as temporary:
                    root = Path(temporary)
                    primary = npy(version=2)
                    state = archive(entries(engine, npy(version=1)), compression)
                    (root / "state.npz").write_bytes(state)
                    (root / "primary.npy").write_bytes(primary)
                    state_hash = hashlib.sha256(state).hexdigest()
                    primary_hash = hashlib.sha256(primary).hexdigest()
                    run = subprocess.run([sys.executable, "-I", "-B", str(Path(arrays.__file__)), "--state", str(root / "state.npz"), "--state-sha256", state_hash,
                                          "--primary", str(root / "primary.npy"), "--primary-sha256", primary_hash, "--engine", engine], capture_output=True, text=True, timeout=10)
                    self.assertEqual(run.returncode, 0, run.stderr)
                    result = json.loads(run.stdout)
                    self.assertEqual(result["state_sha256"], state_hash)
                    self.assertEqual(result["primary_sha256"], primary_hash)
                    self.assertEqual(set(result["channels"]), set(EXPECTED_CHANNELS[engine]))
                    self.assertEqual(result["channels"][EXPECTED_CHANNELS[engine][0]][9][3], 900.375)
                    self.assertTrue(all(len(rows) == 16 and all(len(row) == 16 for row in rows) for rows in result["channels"].values()))

    def test_refuses_hash_mismatch_and_primary_channel_mismatch(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            state = archive(entries("heat_conduction_2d"))
            primary = npy([1.0] * 256)
            (root / "state").write_bytes(state)
            (root / "primary").write_bytes(primary)
            args = (root / "state", hashlib.sha256(state).hexdigest(), root / "primary", hashlib.sha256(primary).hexdigest(), "heat_conduction_2d")
            with self.assertRaisesRegex(ValueError, "Primary NPY differs"):
                arrays.read_arrays(*args)
            for index in (1, 3):
                changed = list(args)
                changed[index] = "0" * 64
                with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                    arrays.read_arrays(*changed)
            with self.assertRaisesRegex(ValueError, "complete SHA-256"):
                arrays.checked_file(root / "state", "invalid", arrays.MAX_ARCHIVE)

    def test_rejects_unsupported_array_formats_truncation_objects_and_nonfinite_values(self):
        bad = [npy(descr="|O"), npy(descr=">f8"), npy(descr="<f4"), npy(shape=(256,)), npy(shape=(16.0, 16.0)), npy(fortran=True),
               npy()[:-1], npy() + b"trailing", npy(version=4), b"\x93NUMPY\x02\x00", npy(values=[0.0] * 255),
               npy(header="{'descr':'<f8','descr':'<f8','fortran_order':False,'shape':(16,16)}"),
               npy(header="{'descr':__import__('os'),'fortran_order':False,'shape':(16,16)}")]
        bad.extend(npy([0.0] * 255 + [value]) for value in (float("nan"), float("inf"), -float("inf")))
        for raw in bad:
            with self.subTest(prefix=raw[:12]), self.assertRaises((ValueError, SyntaxError)):
                arrays.decode_npy(raw)
        self.assertEqual(arrays.decode_npy(npy(version=3))[15][15], 1501.875)

    def test_rejects_duplicate_extra_missing_link_and_unsupported_compression_members(self):
        good = entries("heat_conduction_2d")
        symlink = zipfile.ZipInfo(good[0][0])
        symlink.create_system = 3
        symlink.external_attr = (stat.S_IFLNK | 0o777) << 16
        variants = [good + [good[0]], good + [("extra.npy", npy())], good[:-1],
                    [("../" + good[0][0], good[0][1])] + good[1:], [(symlink, good[0][1])] + good[1:]]
        for rows in variants:
            with self.subTest(names=[str(row[0]) for row in rows]), self.assertRaises(ValueError):
                arrays.decode_state(archive(rows), "heat_conduction_2d")
        with self.assertRaisesRegex(ValueError, "compression"):
            arrays.decode_state(archive(good, zipfile.ZIP_BZIP2), "heat_conduction_2d")
        encrypted = bytearray(archive(good))
        local, central = encrypted.find(b"PK\x03\x04"), encrypted.find(b"PK\x01\x02")
        encrypted[local + 6] |= 1
        encrypted[central + 8] |= 1
        with self.assertRaisesRegex(ValueError, "encryption"):
            arrays.decode_state(bytes(encrypted), "heat_conduction_2d")

    def test_accepts_small_members_with_zip64_local_headers_used_by_streaming_npz_writers(self):
        target = io.BytesIO()
        with zipfile.ZipFile(target, "w") as output:
            for name, raw in entries("heat_conduction_2d"):
                with output.open(name, "w", force_zip64=True) as member:
                    member.write(raw)
        self.assertEqual(set(arrays.decode_state(target.getvalue(), "heat_conduction_2d")), set(EXPECTED_CHANNELS["heat_conduction_2d"]))

    def test_rejects_oversized_compressed_payload_archive_and_corruption(self):
        rows = entries("heat_conduction_2d")
        rows[0] = (rows[0][0], b"x" * (arrays.MAX_NPY + 1))
        with self.assertRaisesRegex(ValueError, "member exceeds"):
            arrays.decode_state(archive(rows, zipfile.ZIP_DEFLATED), "heat_conduction_2d")
        with self.assertRaisesRegex(ValueError, "1 MiB"):
            arrays.decode_state(b"x" * (arrays.MAX_ARCHIVE + 1), "heat_conduction_2d")
        raw = archive(entries("heat_conduction_2d"))
        with self.assertRaises(zipfile.BadZipFile):
            arrays.decode_state(raw[:-30], "heat_conduction_2d")
        changed = bytearray(raw)
        changed[changed.find(b"\x93NUMPY") + 150] ^= 1
        with self.assertRaises(zipfile.BadZipFile):
            arrays.decode_state(bytes(changed), "heat_conduction_2d")


if __name__ == "__main__":
    unittest.main()
