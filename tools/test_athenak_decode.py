"""Small format/analytic fixtures; these never run an NR engine."""
import json
import math
from pathlib import Path
import struct
import tempfile
import unittest

import numpy as np

from athenak_decode import export_blocks, parameter_blocks, read_binary
from check_athenak_gauge import check_run, exact_at, measure

FIXTURE = Path(__file__).with_name("athenak_gauge_wave.athinput").read_text(encoding="utf-8")
NAMES = ("chi gxx gxy gxz gyy gyz gzz Khat Axx Axy Axz Ayy Ayz Azz "
         "Gamx Gamy Gamz Theta alpha betax betay betaz").split()


def analytic_fields(time):
    data = np.zeros((22, 4, 4, 32), dtype="<f4")
    for i in range(32):
        r = exact_at((i + .5) / 32, time)
        chi = r["gxx"] ** (-1 / 3)
        trace = r["Kxx"] / r["gxx"]
        v = dict(chi=chi, alpha=r["alpha"], Khat=trace)
        for axis in "xyz":
            v[f"g{axis}{axis}"] = chi * r[f"g{axis}{axis}"]
            v[f"A{axis}{axis}"] = chi * (r[f"K{axis}{axis}"] - trace*r[f"g{axis}{axis}"]/3)
        for n, name in enumerate(NAMES):
            data[n, :, :, i] = v.get(name, 0)
    return data


def write_native(path, time=0., data=None, names=None, header=FIXTURE, index=None, logical=None):
    data = analytic_fields(time) if data is None else data
    names = ["z4c_" + n for n in NAMES] if names is None else names
    h = header.encode("utf-8")
    prefix = ("Athena binary output version=1.1\n  size of preheader=5\n"
              f"  time={time:.17e}\n  cycle={round(time*1000)}\n"
              "  size of location=8\n  size of variable=4\n"
              f"  number of variables={len(names)}\n  variables: {' '.join(names)}\n"
              f"  header offset={len(h)}\n").encode("ascii")
    b = struct.pack("<6i", *(index or (2, 33, 2, 5, 2, 5)))
    b += struct.pack("<4i", *(logical or (0, 0, 0, 0)))
    b += struct.pack("<6d", 0, 1, 0, 1, 0, 1)
    path.write_bytes(prefix + h + b + data.astype("<f4").tobytes())


class NativeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.path = self.root / "sample.bin"

    def tearDown(self):
        self.temp.cleanup()

    def test_real_format_float32_data_float64_coordinates_and_no_interpolation(self):
        write_native(self.path, .1)
        f = read_binary(self.path)
        self.assertEqual(f["variable_bytes"], 4)
        self.assertEqual(f["location_bytes"], 8)
        self.assertEqual(list(f["blocks"][0]["coordinates"][0])[:2], [.015625, .046875])
        self.assertTrue(measure(f)["passed"])
        receipt = export_blocks(f, self.root / "decoded")
        with np.load(self.root / "decoded" / receipt["blocks"][0]["path"], allow_pickle=False) as saved:
            np.testing.assert_array_equal(saved["z4c_alpha"], f["blocks"][0]["fields"]["z4c_alpha"])
        self.assertEqual(receipt["interpolation"], "none")

    def test_truncated_header_and_payload_rejected(self):
        write_native(self.path)
        original = self.path.read_bytes()
        for end in (7, 80, len(original) - 1):
            self.path.write_bytes(original[:end])
            with self.assertRaises(ValueError):
                read_binary(self.path)

    def test_unknown_format_duplicate_names_nonfinite_rejected(self):
        write_native(self.path)
        self.path.write_bytes(self.path.read_bytes().replace(b"version=1.1", b"version=9.9"))
        with self.assertRaisesRegex(ValueError, "version"):
            read_binary(self.path)
        names = ["z4c_" + n for n in NAMES]
        names[-1] = names[0]
        write_native(self.path, names=names)
        with self.assertRaisesRegex(ValueError, "variable table"):
            read_binary(self.path)
        data = analytic_fields(0)
        data[0, 0, 0, 0] = float("nan")
        write_native(self.path, data=data)
        with self.assertRaisesRegex(ValueError, "nonfinite"):
            read_binary(self.path)

    def test_hostile_allocation_and_negative_block_indices_rejected(self):
        write_native(self.path, index=(2, 2_000_000_000, 2, 5, 2, 5))
        with self.assertRaisesRegex(ValueError, "geometry/index"):
            read_binary(self.path)
        write_native(self.path, logical=(-1, 0, 0, 0))
        with self.assertRaisesRegex(ValueError, "block identity"):
            read_binary(self.path)

    def test_slice_keeps_original_coordinate_frame_but_benchmark_refuses(self):
        data = analytic_fields(.1)[:, 2:3, 1:2, 7:8]
        write_native(self.path, .1, data=data, index=(9, 9, 3, 3, 4, 4))
        frame = read_binary(self.path)
        self.assertEqual(float(frame["blocks"][0]["coordinates"][0][0]), 7.5/32)
        self.assertEqual(float(frame["blocks"][0]["coordinates"][1][0]), 1.5/4)
        with self.assertRaisesRegex(ValueError, "sliced"):
            measure(frame)

    def test_duplicate_parameters_not_silently_overridden(self):
        with self.assertRaisesRegex(ValueError, "duplicate"):
            parameter_blocks("<z4c>\na=1\na=2\n")

    def test_degenerate_axes_have_no_fictitious_ghost_coordinate_offset(self):
        header = FIXTURE.replace("nx2 = 4", "nx2 = 1").replace("nx3 = 4", "nx3 = 1")
        write_native(self.path, data=analytic_fields(0)[:, :1, :1, :], header=header,
                     index=(2, 33, 0, 0, 0, 0))
        frame = read_binary(self.path)
        self.assertEqual(float(frame["blocks"][0]["coordinates"][1][0]), .5)
        self.assertEqual(float(frame["blocks"][0]["coordinates"][2][0]), .5)

    def test_analytic_wave_sign_and_extrinsic_curvature(self):
        zero = exact_at(0., 0.)
        quarter = exact_at(.25, 0.)
        self.assertAlmostEqual(zero["Kxx"], -.01*math.pi)
        self.assertAlmostEqual(quarter["alpha"]**2, .99)
        self.assertAlmostEqual(exact_at(.35, .1)["gxx"], quarter["gxx"])

    def _write_run(self, static=False):
        for seq, t in enumerate((0., .05, .1)):
            write_native(self.root / f"gauge-{seq}.bin", t, data=analytic_fields(0 if static else t))
            write_native(self.root / f"con-{seq}.bin", t,
                         data=np.zeros((1, 4, 4, 32)), names=["con_H"])

    def test_temporal_analytic_reference_and_missing_frames(self):
        self._write_run()
        result = check_run(self.root)
        self.assertTrue(result["passed"])
        self.assertGreater(result["alpha_change_linf"], .001)
        (self.root / "con-1.bin").unlink()
        with self.assertRaisesRegex(ValueError, "missing temporal"):
            check_run(self.root)

    def test_frozen_animation_fails_even_with_valid_time_headers(self):
        self._write_run(static=True)
        with self.assertRaisesRegex(ValueError, "did not evolve"):
            check_run(self.root)

    def test_wrong_gauge_cannot_claim_analytic_benchmark(self):
        write_native(self.path, header=FIXTURE.replace("lapse_oplog = 0", "lapse_oplog = 2"))
        with self.assertRaisesRegex(ValueError, "lapse_oplog"):
            measure(read_binary(self.path))


if __name__ == "__main__":
    unittest.main()
