# Candidate02 horizon diagnostic review — 2026-09-12

This read-only review finds an oscillating individual-horizon iteration, a common-horizon search that aborts before its first update, and misleading persistence/time conventions in the native diagnostics. Candidate02 has **not established accurate, current individual horizons or a common horizon**. Its input, solver sources, CPU evidence and CPU/GPU comparison remain unchanged. No new calculation was launched for this review; proposed follow-ups below are not executed or admitted runs.

The requested equal-mass head-on collision at 0.999c remains unfulfilled. These low-momentum findings establish neither feasibility nor impossibility of that target.

Evidence identity:

- CPU job `74f3c493-b606-4e49-8ceb-79132d74a784`, retained under `.local/validation/nr-black-hole-build-20260912/pilot-74f3c493-b606-4e49-8ceb-79132d74a784/`.
- Input SHA256 `66dff1103d4f521b601efc79e57c1e3ac353222f937e4d97dc07fbff922d07df`; CPU binary SHA256 `969544674d82d7a8169c7da6c20992ac8efe89aae6fa32f0d8365300fd1208f6`.
- AthenaK `c5a0d7f9155a70149931bf0be5a4ffb673f2532a`; Kokkos `6739bc623081648af9e752b616d9671527922cbf`; TwoPuncturesC `ec563aeb672235b9443c330f9cde65f7246e8ea4`.
- Original receipt SHA256 `af4cf66d61be0bf262b2229a9d5d0d37df89649acba32cee8ea17509f46322aa`. The independent 30-file/array audit is `.local/validation/nr-high-boost-review-20260912/independent-pilot-review.json`, SHA256 `e2c4b1d74602d70f2caddeb51beb8d63deaf44d634accd77e4aca3eb99d20dbb`.

The 600-second deadline stopped and drained the CPU job after 35 steps. Complete fields exist only at native times 0 and 0.5126953125. The momentum-constraint RMS grew 28.50-fold; the catastrophic 100-fold guard did not trip, which supplies no accuracy or merger acceptance.

## What failed, with the native evidence retained

| Finder | Fresh native “found” searches | Failed searches | Failed rows repeating an older finite summary |
| --- | ---: | ---: | ---: |
| Individual 0 | 14 | 21 | 14 |
| Individual 1 | 13 | 22 | 4 |
| Common 2 | 0 | 35 | 0 |

Every finder has 35 attempts. Both last individual rows are stale. `work/headon_low_momentum.horizon_verbose_{0,1,2}.txt` supplies per-attempt success/failure; the summary alone does not. [FastFlow resets `ah_found` each search](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/fastflow.cpp#L666), updates properties only on success at lines788–801, but prints those properties on failure too at lines367–386. Shape output is conditional on success. Preserved stale values must never count as a newly found horizon.

The first individual-0 search starts at radius0.5 and contracts, then settles into a two-cycle. Iterations60–99 alternate approximately:

| Quantity | Even iteration | Odd iteration |
| --- | ---: | ---: |
| Irreducible mass | 0.50068380 | 0.49986459 |
| Mean coordinate radius | 0.22210886 | 0.22410893 |
| Integrated expansion | −0.25169077 | +0.23194342 |

The mass change is about `8.19e-4`, above the unchanged `mass_tol=1e-6`; the search reaches100 iterations and fails. This observed limit cycle does not support extending iteration count alone as a remedy. Damping is a concrete supported experiment: [`flow_alpha_beta_const_n`](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/fastflow.cpp#L827) scales `A` linearly while `B=beta/alpha=0.5`. Changing its default1.0 to0.5 halves every modal update at fixed `lmax`; it does not change Einstein evolution. Zero is invalid because it produces0/0.

For the common finder, the first radius4 surface produces `hmean=104.07531` and immediately hits `hmean_tol=100`. All35 searches abort on this first evaluation. This is not a failed long search for a common surface and cannot establish that no such surface exists. The parameter is an absolute **integrated-expansion abort ceiling**, not a horizon-convergence tolerance. Raising this ceiling would merely permit iterations; it could not justify accepting a returned surface.

Native “found” is also insufficient: [success checks only successive irreducible-mass change](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/fastflow.cpp#L739), not a small expansion norm. Reduced damping can therefore satisfy the mass-change stop more easily without solving the horizon equation. The stored `hrms` is actually `integral(H² dA)/area`, without a square root; `hmean` is `integral(H dA)`, without division by area (lines710–712 and1368–1369). For fresh rows only, `sqrt(hrms)` is0.239765–0.284307 for finder0 and0.239819–0.259660 for finder1. Multiplying by areal radius `sqrt(area/(4*pi))` gives dimensionless residuals0.23917–0.28401 and0.23922–0.25892. These are not demonstrated near-zero-expansion surfaces; a small signed integral can conceal cancellation.

## Supported controls and resolution limitations

The requested `initial_radius_0/1=0.3` is ignored for tracked finders. [InitialGuess lines448–505](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/fastflow.cpp#L448) uses tracker mass and separation; mass0.5/separation6 gives a cold seed radius0.5, confirmed in the verbose log. `initial_radius` applies only with `use_puncture=-1`. After success, only the previous mean-radius coefficient is reused with `expand_guess`; all higher shape coefficients are reset. After failure, the next search falls back to the cold seed. Changing tracker mass to influence this guess would corrupt its declared meaning.

The finest grid spacing is0.05859375. A coordinate radius0.22–0.24 spans only3.8–4.1 finest cells; a minimum radius0.18 spans3.1. The present `ntheta=10` produces10×20=200 angular samples, and `lmax=6` gives49 real shape coefficients ([GaussLegendreGrid lines38,61–70](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/geodesic-grid/gauss_legendre.cpp#L38)). This grid and `nghost=2` have no horizon-resolution convergence evidence. Finer angular sampling cannot recover missing volume-grid derivative accuracy. Raising `lmax` changes both shape space and modal step factors, so it should not be mixed into the first damping comparison.

The retained `horizon_grid` and `horizon_ylm` files contain angles, quadrature weights and basis derivatives, not metric or extrinsic-curvature samples. Even with the shape coefficients, expansion requires the matching physical metric, spatial metric derivatives and extrinsic curvature (`fastflow.cpp`1097–1117,1237–1258). Shape coefficients are printed with `%e` and centers with `%f`; an independent reconstruction must account for that precision. The files alone are not an independent horizon verification.

## Time labels require an explicit mapping

The printed `cycle=3`/summary `iter=3` is the RK stage argument, not the evolution-cycle number. [Task ordering](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/z4c_tasks.cpp#L63) performs the RK update, converts final-stage Z4c to ADM, advances trackers, then finds/writes horizons using the still-old `pmesh->time` (lines82–93,220–223,313–341). [Driver lines559–595](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/driver/driver.cpp#L559) increment time after these tasks.

For this fixed-step run, `dt=0.0146484375`: the first horizon label0 describes a search on the first evolved state; it is not the native initial t=0 field. The final rounded label0.498047 corresponds to the completed state at0.5126953125. It is still a failed/stale horizon search. Do not use its repeated mass as a measurement of that final field.

Waveform output has the same convention. `CalcWeylScalar`/`CalcWaveForm` run at the final stage before time advances (`z4c_tasks.cpp`100–114,397–424); [WaveExtr writes old `pmesh->time`](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/z4c_wave_extr.cpp#L200). The two rPsi4 rows at each radius8/10 have raw labels0 and0.263672, corresponding here to evolved times0.0146484375 and0.2783203125. They remain near-zone plumbing output, not validated radiation.

Recommended additive metadata for horizon summaries, shapes and waves, without rewriting raw evidence:

```json
{
  "native_label_time": 0.0,
  "label_semantics": "pre_increment_mesh_time_for_final_RK_stage_state",
  "native_stage": 3,
  "physical_frame_time": null,
  "time_mapping": null,
  "fresh_search_success": false
}
```

Keep `physical_frame_time` null until a source-pinned stage/step mapping is verified; if mapped, retain source hashes, actual step interval and rounding uncertainty. Do not globally add nominal dt for adaptive/restarted runs. `fresh_search_success` applies to horizons only and must come from the corresponding verbose attempt; false means no current mass/surface measurement. Existing helper `time` fields must be labeled as native labels, not silently interpreted as field times. Frozen helper bytes used by the current pilot were not edited.

## Bounded, preregisterable follow-up proposal

1. **Finder iteration comparison:** retain candidate02 as baseline. Admit a new immutable short attempt only after CPU/GPU comparison, with both individual `flow_alpha_beta_const_n=0.5`; keep physical data, grid, gauge, `lmax=6`, `ntheta=10`,100 iterations, `mass_tol=1e-6` and abort100 unchanged. Preselect the first three completed RK states, retain exact field/horizon identity at each, and require fresh success on both objects at all selected states. If this fails, retain it; do not automatically increase iterations or try a parameter search. This is an iteration diagnostic, not sufficient mass validation.
2. **Residual and resolution acceptance:** before calling a surface a measured horizon, independently recompute outward-null expansion on the exact saved metric/extrinsic-curvature slice and returned surface, verify complete coverage, positive metric, closure and enclosure of the intended puncture. Proposed prospective gates are areal-radius-scaled expansion RMS≤`1e-3`, maximum≤`1e-2`, and no stale/substituted times. These are declared study targets, not established universal tolerances or a retroactive rejection threshold. A separate angular comparison (`ntheta=20`, fixed `lmax=6`) and then a separately budgeted factor-two local spatial refinement should reduce residuals and change irreducible masses by≤`1e-3` relatively. This is a consistency gate, not a spatial-error estimate: three matched spatial resolutions and an observed convergence regime are needed before estimating numerical uncertainty. Freeze a concrete checker, matched physical times, mesh estimate and process/storage limits before execution. Current saved outputs do not provide this independent residual check.
3. **Initial mass and boost calibration:** an actual t=0 horizon solve must be invoked on the constraint-solved initial slice; the first time-labeled0 RK diagnostic cannot replace it. Keep `M_irr=sqrt(A/(16*pi))`, spin method, horizon mass, puncture-end ADM masses, total ADM energy and prescribed momenta as distinct quantities with resolution errors. The current coordinate-rotation spin integral (`fastflow.cpp`1337–1356) needs its approximation disclosed. Validate an isolated boosted-hole control and define the asymptotic energy/momentum/velocity convention before applying it to a finite-separation binary; binding energy, junk radiation and post-junk masses matter. Puncture tracker velocity is explicitly minus the gauge shift (`compact_object_tracker.cpp`106–111), so neither it nor `P/target_M` certifies physical speed. No current measurement calibrates0.999c.

A fixed-center initial-slice test could genuinely vary `initial_radius=0.25/0.3` using `use_puncture=-1`, but loses moving-center tracking and is a separate comparison. Common-horizon recovery should follow validated individuals: either defer its search to a declared later interval, or independently preregister a larger abort ceiling while keeping strict residual/enclosure acceptance. No common-horizon parameter choice can create evidence of merger by itself.

Source hashes reread for this audit: `fastflow.cpp` `524c8e28b417f0526753e507787138c5f9ee59897e726601a278bb7daa60d321`; `z4c_tasks.cpp` `a537e45bb0b9f47a12ae9d65d5c182839c533d11afe145047be94f5b26dcb069`; `driver.cpp` `ddcde5053500beb9f62f6610d865e6070d175adb1415c14c07d9179703fad678`; `z4c_wave_extr.cpp` `bc9204bce7e51e654cc35eef3a9facb8d0cfb7ac2fca5dbfb47e289c0634edd8`.
