"""Independent analytic expansion controls; no NR engine or retained-run replay.

Interpolation bounds below were fixed before inspecting checker output. They
test synthetic interpolation accuracy, not acceptance of any physical horizon.
"""
import math
import unittest

import numpy as np

import nr_horizon_expansion as expansion


def sphere_coefficients(radius):
    return [radius*math.sqrt(4*math.pi)]


def flat_sampler(point, curvature=0.0):
    return np.eye(3), np.zeros((3, 3, 3)), curvature*np.eye(3)


def schwarzschild_sampler(point, mass=1.0):
    x = np.asarray(point, dtype=float)
    r = float(np.linalg.norm(x))
    if r <= 0:
        raise ValueError('The isotropic puncture is excluded from the control')
    psi = 1+mass/(2*r)
    gamma = psi**4*np.eye(3)
    derivative = np.asarray([-2*mass*psi**3*x[axis]/r**3*np.eye(3) for axis in range(3)])
    return gamma, derivative, np.zeros((3, 3))


def schwarzschild_patch(count, mass=1.0):
    # Cell centers never contain the puncture for these even grid dimensions.
    spacing = 2.0/count
    origin = np.full(3, -1+spacing/2)
    coordinates = origin[0]+spacing*np.arange(count)
    x, y, z = np.meshgrid(coordinates, coordinates, coordinates, indexing='ij')
    radius = np.sqrt(x*x+y*y+z*z)
    psi = 1+mass/(2*radius)
    gamma = psi[..., None, None]**4*np.eye(3)
    return expansion.RegularPatch(origin, spacing, gamma, np.zeros_like(gamma))


class HorizonExpansionAnalyticTests(unittest.TestCase):
    def evaluate(self, radius, sampler=flat_sampler, center=(0., 0., 0.), **kwargs):
        return expansion.evaluate_surface(sphere_coefficients(radius), 0, center, sampler, **kwargs)

    def assert_complete_quadrature(self, result, count=200):
        self.assertEqual(result['quadrature_points'], count)
        self.assertEqual(len(result['points']), count)
        self.assertFalse(result['surface_validated'])
        self.assertTrue(all(math.isfinite(row['area_weight']) and row['area_weight'] > 0
                            for row in result['points']))
        self.assertTrue(all(math.isfinite(row['expansion']) for row in result['points']))
        self.assertAlmostEqual(math.fsum(row['area_weight'] for row in result['points']),
                               result['area'], delta=1e-11*result['area'])
        self.assertEqual(len({tuple(row['xyz']) for row in result['points']}), count)

    def test_flat_spheres_have_outward_positive_curvature_and_full_quadrature(self):
        for radius, center in ((.5, (0., 0., 0.)), (1.3, (2., -3., .75)), (2., (-4., 1., 3.))):
            with self.subTest(radius=radius, center=center):
                result = self.evaluate(radius, center=center)
                self.assert_complete_quadrature(result)
                self.assertAlmostEqual(result['area'], 4*math.pi*radius**2, delta=1e-10*radius**2)
                self.assertAlmostEqual(result['M_irr'], radius/2, delta=1e-10*radius)
                self.assertAlmostEqual(result['expansion_rms'], 2/radius, delta=1e-9/radius)
                self.assertAlmostEqual(result['max_abs_expansion'], 2/radius, delta=1e-9/radius)
                self.assertAlmostEqual(result['dimensionless_rms'], 2., delta=1e-9)
                self.assertAlmostEqual(result['dimensionless_max'], 2., delta=1e-9)
                for point in result['points']:
                    self.assertAlmostEqual(math.dist(point['xyz'], center), radius, delta=1e-10*radius)
                    self.assertAlmostEqual(point['expansion'], 2/radius, delta=1e-9/radius)

    def test_nonzero_extrinsic_curvature_sign_is_Kss_minus_traceK(self):
        # Algebraic controls; constant K=k*g is not claimed to solve constraints.
        for radius, k in ((.75, .125), (2., 1.)):
            with self.subTest(radius=radius, k=k):
                result = self.evaluate(radius, sampler=lambda point: flat_sampler(point, k))
                expected = 2/radius-2*k
                self.assertAlmostEqual(result['expansion_rms'], abs(expected), delta=1e-9)
                for point in result['points']:
                    self.assertAlmostEqual(point['expansion'], expected, delta=1e-9)

    def test_isotropic_schwarzschild_horizon_area_mass_and_zero_expansion(self):
        for mass in (1., 2.3):
            with self.subTest(mass=mass):
                result = self.evaluate(mass/2, sampler=lambda point: schwarzschild_sampler(point, mass))
                self.assert_complete_quadrature(result)
                self.assertAlmostEqual(result['area'], 16*math.pi*mass**2, delta=1e-9*mass**2)
                self.assertAlmostEqual(result['M_irr'], mass, delta=1e-10*mass)
                self.assertLess(result['expansion_rms'], 1e-9/mass)
                self.assertLess(result['max_abs_expansion'], 1e-9/mass)
                self.assertLess(result['dimensionless_rms'], 1e-8)

    def test_isotropic_schwarzschild_inside_and_outside_have_opposite_signs(self):
        mass = 1.
        a = mass/2
        for radius in (.35, .75):
            with self.subTest(radius=radius):
                expected = 2*radius*(radius-a)/(radius+a)**3
                psi = 1+mass/(2*radius)
                result = self.evaluate(radius, sampler=schwarzschild_sampler)
                self.assertAlmostEqual(result['area'], 4*math.pi*radius**2*psi**4, delta=1e-9)
                self.assertAlmostEqual(result['expansion_rms'], abs(expected), delta=1e-9)
                for point in result['points']:
                    self.assertAlmostEqual(point['expansion'], expected, delta=1e-9)

    def test_real_l1_harmonic_phase_and_nonspherical_mean_curvature(self):
        # R(n)=R0+epsilon*(axis dot n). The two principal curvatures below
        # follow directly from a surface of revolution, independently of the
        # checker's harmonic derivatives and embedded-surface contraction.
        radius, epsilon = 1.1, .1
        scale = epsilon*math.sqrt(4*math.pi/3)
        cases = ((2, [radius*math.sqrt(4*math.pi), scale, 0., 0.]),
                 (0, [radius*math.sqrt(4*math.pi), 0., -scale, 0.]),
                 (1, [radius*math.sqrt(4*math.pi), 0., 0., -scale]))
        for axis, coefficients in cases:
            with self.subTest(axis=axis):
                result = expansion.evaluate_surface(coefficients, 1, (0., 0., 0.), flat_sampler)
                self.assert_complete_quadrature(result)
                for point in result['points']:
                    r = math.sqrt(math.fsum(value*value for value in point['xyz']))
                    mu = point['xyz'][axis]/r
                    self.assertAlmostEqual(r, radius+epsilon*mu, delta=1e-10)
                    slope2 = epsilon**2*(1-mu*mu)
                    denominator = math.sqrt(r*r+slope2)
                    meridional = (r*r+2*slope2+r*epsilon*mu)/denominator**3
                    azimuthal = (r+epsilon*mu)/(r*denominator)
                    self.assertAlmostEqual(point['expansion'], meridional+azimuthal, delta=1e-8)

    def test_custom_quadrature_uses_every_requested_node(self):
        result = self.evaluate(1., ntheta=12, nphi=24)
        self.assert_complete_quadrature(result, 288)
        self.assertAlmostEqual(result['area'], 4*math.pi, delta=1e-10)

    def test_incomplete_nonfinite_or_nonpositive_spacetime_is_rejected(self):
        bad_samples = [(np.eye(3), np.zeros((3, 3, 3))),
                       (np.diag([1., 1., -1.]), np.zeros((3, 3, 3)), np.zeros((3, 3))),
                       (np.eye(3), np.full((3, 3, 3), float('nan')), np.zeros((3, 3))),
                       (np.eye(3), np.zeros((3, 3, 3)), np.full((3, 3), float('inf')))]
        for index, values in enumerate(bad_samples):
            with self.subTest(index=index), self.assertRaises((ValueError, TypeError)):
                self.evaluate(1., sampler=lambda point: values)

    def test_invalid_shape_and_explicit_quadrature_bounds_are_rejected(self):
        invalid = [([], 0, 10, 20), ([1.], 1, 10, 20), ([-1.], 0, 10, 20),
                   ([float('nan')], 0, 10, 20), ([1.]*324, 17, 10, 20),
                   ([1.], 0, 65, 20), ([1.], 0, 10, 129)]
        for coefficients, lmax, ntheta, nphi in invalid:
            with self.subTest(lmax=lmax, ntheta=ntheta, nphi=nphi), self.assertRaises(ValueError):
                expansion.evaluate_surface(coefficients, lmax, (0., 0., 0.), flat_sampler,
                                           ntheta=ntheta, nphi=nphi)


class RegularPatchInterpolationTests(unittest.TestCase):
    @staticmethod
    def polynomial_patch():
        origin = np.array([-.6, -.8, -1.])
        spacing = np.array([.2, .25, .3])
        axes = [origin[axis]+spacing[axis]*np.arange(8) for axis in range(3)]
        x, y, z = np.meshgrid(*axes, indexing='ij')
        gamma = np.zeros(x.shape+(3, 3))
        gamma[..., 0, 0] = 2+.2*x+.1*y*y+.03*z**3
        gamma[..., 1, 1] = 1.5+.1*y
        gamma[..., 2, 2] = 2+.07*z
        gamma[..., 0, 1] = gamma[..., 1, 0] = .02*x*y
        gamma[..., 1, 2] = gamma[..., 2, 1] = .01*y*z
        curvature = np.zeros_like(gamma)
        curvature[..., 0, 0] = .3+.02*x*x
        curvature[..., 1, 1] = -.1+.01*y**3
        curvature[..., 2, 2] = .07+.02*z
        return expansion.RegularPatch(origin, spacing, gamma, curvature)

    def test_cubic_and_quintic_reproduce_tensor_values_and_cartesian_derivative_axes(self):
        patch = self.polynomial_patch()
        point = np.array([.13, .07, .11])
        x, y, z = point
        expected_g = np.array([[2+.2*x+.1*y*y+.03*z**3, .02*x*y, 0.],
                               [.02*x*y, 1.5+.1*y, .01*y*z], [0., .01*y*z, 2+.07*z]])
        expected_dg = np.zeros((3, 3, 3))
        expected_dg[0, 0, 0] = .2
        expected_dg[0, 0, 1] = expected_dg[0, 1, 0] = .02*y
        expected_dg[1, 0, 0] = .2*y
        expected_dg[1, 1, 1] = .1
        expected_dg[1, 0, 1] = expected_dg[1, 1, 0] = .02*x
        expected_dg[1, 1, 2] = expected_dg[1, 2, 1] = .01*z
        expected_dg[2, 0, 0] = .09*z*z
        expected_dg[2, 2, 2] = .07
        expected_dg[2, 1, 2] = expected_dg[2, 2, 1] = .01*y
        expected_k = np.diag([.3+.02*x*x, -.1+.01*y**3, .07+.02*z])
        for order in (3, 5):
            with self.subTest(order=order):
                gamma, derivative, curvature = patch.sample(point, order=order)
                np.testing.assert_allclose(gamma, expected_g, rtol=0, atol=1e-11)
                np.testing.assert_allclose(derivative, expected_dg, rtol=0, atol=1e-10)
                np.testing.assert_allclose(curvature, expected_k, rtol=0, atol=1e-11)

    def test_missing_stencil_and_extrapolation_are_rejected(self):
        gamma = np.broadcast_to(np.eye(3), (4, 4, 4, 3, 3)).copy()
        patch = expansion.RegularPatch((0., 0., 0.), .25, gamma, np.zeros_like(gamma))
        for point, order in (((.4, .4, .4), 5), ((-.01, .4, .4), 3),
                             ((.76, .4, .4), 3), ((.4, .4, .4), 2)):
            with self.subTest(point=point, order=order), self.assertRaises(ValueError):
                patch.sample(point, order=order)

    def test_nonfinite_or_nonpositive_patch_is_rejected(self):
        good = np.broadcast_to(np.eye(3), (6, 6, 6, 3, 3)).copy()
        bad = good.copy()
        bad[2, 2, 2, 0, 0] = float('nan')
        negative = good.copy()
        negative[2, 2, 2, 2, 2] = -1.
        for gamma, spacing in ((bad, .1), (negative, .1), (good, 0.), (good, (.1, -.1, .1))):
            with self.subTest(spacing=spacing), self.assertRaises(ValueError):
                patch = expansion.RegularPatch((0., 0., 0.), spacing, gamma, np.zeros_like(gamma))
                patch.sample((.25, .25, .25), order=3)

    def test_schwarzschild_sampled_patch_refines_under_preregistered_bounds(self):
        # Frozen before implementation output: degree3 on N=16,24,32 cells.
        # Targets assess only the synthetic interpolator and geometry pipeline.
        results = []
        for count in (16, 24, 32):
            patch = schwarzschild_patch(count)
            result = expansion.evaluate_surface(sphere_coefficients(.5), 0, (0., 0., 0.),
                                                lambda point: patch.sample(point, order=3))
            results.append(result)
        coarse, _, fine = results
        self.assertLess(fine['expansion_rms'], .03)
        self.assertLess(fine['max_abs_expansion'], .1)
        self.assertLess(abs(fine['area']/(16*math.pi)-1), .008)
        self.assertLess(fine['expansion_rms'], .6*coarse['expansion_rms'])
        self.assertTrue(all(result['surface_validated'] is False for result in results))


if __name__ == '__main__':
    unittest.main()
