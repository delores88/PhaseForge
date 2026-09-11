# Reviewer variant: dilute NVE density response and ballistic motion

Frozen on 2026-09-11, after the installed first-lab and interruption-recovery gate passed, and before either run below was submitted. Reviewer: the separate trajectory/viewer agent. No new variant solver output was read or produced while defining these expectations. Application code must not be changed to prepare this experiment or its answer. The separate slider endpoint repair concerns an already observed UI defect and does not alter these scientific inputs or criteria.

## Scientific question and supported scope

Does doubling mass density double the kinetic pressure while leaving ballistic mean-squared displacement unchanged in a deliberately dilute, collision-free limit of the existing periodic argon-like model?

This changes both the physical regime and the measured response from the prior dense, thermostatted 90/180 K study. It exercises ordinary-chat preparation of two new NVE computations at 200 K, a new seed, low density, and an independently calculable pressure/MSD response. It is not a renamed showcase, a visual change, or a replay. The supported claim is numerical agreement with this finite, truncated classical model. It does not establish an empirical equation of state, gas equilibration, long-time diffusion, or biological predictive validity.

The 0.1 ps horizon is selected analytically so that every pair stays beyond the declared force cutoff, allowing an exact reference without calibrating against application output. It is a scientific validation window, not a restriction on playback duration or the longer simulations already demonstrated.

## Exact ordinary-chat prompt

Submit the following text through the installed application's normal chat with the currently selected provider/model. Keep the chosen model and time budget in the run receipt. Do not inject the numerical answer table below into the agent's prompt, prepare solver directories manually, or call the scientific worker outside the app.

> Investigate the dilute ballistic limit of our real OpenMM argon-like laboratory. Run two fresh periodic 32-atom simulations with initial temperature 200 K, the same seed 46703, CPU platform with one thread, no thermostat (NVE), timestep 1 fs, 100 steps (0.1 ps), sample interval 1, and chunk size 17. Use density 0.02 g/cm³ for one and 0.04 g/cm³ for the other; leave the model's existing Lennard-Jones parameters and cutoff policy unchanged. Compare the box volumes, pressure, kinetic and potential energy, and mean-squared displacement at 0.05 and 0.1 ps. Check whether the particles stay outside the interaction cutoff and whether the observed response agrees with a finite-system ballistic/ideal-gas calculation that accounts for removing center-of-mass motion. Use the actual retained measurements and trajectories, inspect actual saved images from both new runs, and explain what the images can and cannot establish. Link the numerical jobs and distinguish mathematical agreement from validation against real argon. Do not substitute an illustration or an older run.

Both manifests must contain these exact parameters: `atom_count=32`, `temperature_kelvin=200`, `seed=46703`, `platform="CPU"`, `cpu_threads=1`, `thermostat="nve"`, `timestep_fs=1`, `steps=100`, `sample_interval=1`, `chunk_frames=17`, and the respective densities `0.02` and `0.04`. `friction_per_ps` may retain its existing default because the NVE integrator does not use it. The two jobs must have distinct fresh IDs and the same parent session.

## Reference derived before execution

Use the model's declared mass M = 39.948 g/mol (numerically 39.948 Da), sigma = 0.3405 nm and epsilon = 0.997 kJ/mol. The exact SI constants give N_A = 6.02214076 × 10²³ mol⁻¹ and R = N_A k_B / 1000 = 0.00831446261815324 kJ/(mol K). The declared initialization removes three center-of-mass velocity degrees of freedom and rescales the remaining sample to T = 200 K. Thus, for N = 32:

- K = (3N − 3) RT / 2 = **77.32450234882513 kJ/mol**.
- Mean squared speed A = 2K/(NM) = **0.12097680476623537 nm²/ps²**.
- V = NM/(N_A ρ) × 10²¹ nm³, with density ρ in g/cm³.
- With zero pair virial, P = 2K/(3V) × 10²⁵/N_A bar. Equivalently P = (N − 1) k_B T/V in consistent SI units. Using N instead of N−1 would ignore this finite-system initialization convention.
- Unwrapped position is r_i(t) = r_i(0) + v_i(0)t. MSD(t) = A t², independent of density, so MSD(0.1)/MSD(0.05) = **4**, not the factor 2 of a diffusive-time law.

| Observable | 0.02 g/cm³ | 0.04 g/cm³ |
| --- | ---: | ---: |
| Volume (nm³) | 106.13634344873732 | 53.06817172436866 |
| Cubic side (nm) | 4.734651748231893 | 3.7578955829231906 |
| Pressure (bar) | 8.065120317749026 | 16.13024063549805 |
| Kinetic / total energy (kJ/mol) | 77.32450234882513 | 77.32450234882513 |
| Potential energy (kJ/mol) | 0 | 0 |
| MSD at 0.05 ps (nm²) | 0.0003024420119155885 | 0.0003024420119155885 |
| MSD at 0.1 ps (nm²) | 0.001209768047662354 | 0.001209768047662354 |

The cutoff in both boxes is 2.5 sigma = **0.85125 nm**. A complete 32-site FCC cell has nearest separation L/(2√2): 1.6739521788657568 nm and 1.328616724837981 nm respectively. For any pair, |v_i−v_j| ≤ √(2 Σ|v|²) = √(4K/M) = 2.782537601729591 nm/ps. Over 0.1 ps, the periodic distance therefore cannot fall below **1.3956984186927976 nm** and **1.0503629646650219 nm**, respectively. Both exceed the cutoff. This bound proves the collision-free reference is self-consistent for the whole interval without depending on a favorable observed seed or on checking only sample endpoints. No long-range dispersion correction is present in the declared model.

## Frozen tolerances and failure conditions

An independent read-only checker must not import `scientific_worker.py`, use the app's summaries as reference answers, or replace measured values. It may reuse the independent pair loop and artifact-integrity routines in `tools/check_scientific_worker.py`, while calculating the analytic values above separately from constants. Check all 101 saved states in each job, including the initial state.

1. **Execution and provenance:** both fresh jobs complete with the exact parameters above. Retain installed-build receipt, parent session/tool receipts, manifest/input hashes, chunk and array hashes, result hashes, and the frozen plan hash. Each run has exactly steps 0–100 and 101 strictly increasing times ending at 0.1 ps within 1e-12 ps. JSON viewer positions equal wrapped numerical coordinates; topology IDs/units, chunk hashes and actual PNG source hashes pass the existing independent artifact checks. Any substituted engine, different parameters, reused old job, fabricated state, invalid hash, missing state or duplicate timestamp fails.
2. **Geometry and no interactions:** side length matches the table within 1e-10 nm and volume within 1e-8 nm³. Independent minimum-image pair distances stay above the applicable conservative bound minus 1e-8 nm. Maximum absolute force is ≤1e-10 kJ/(mol nm), absolute potential energy and pair virial are each ≤1e-10 kJ/mol. A nonzero interaction beyond those tolerances fails this reference; do not silently reinterpret the run as an interacting gas.
3. **Conservation and ballistic trajectory:** K and total energy each match the analytic value within 1e-5 kJ/mol at every state; temperature is within 2e-5 K of 200 K. Each unwrapped coordinate agrees with r(0)+v(0)t within 2e-8 nm; velocity components remain within 1e-8 nm/ps of their initial values; total momentum drift is ≤1e-6 Da nm/ps. All-state MSD agrees with A t² within 1e-10 nm², and the 0.1/0.05 ps MSD ratio is within 2e-6 of 4. Density must not change MSD by more than 1e-10 nm² at corresponding times.
4. **Distinguishable density response:** every pressure sample agrees with its table value within 2e-5 bar. The ratio of mean pressure at 0.04 density to mean pressure at 0.02 density is within 2e-6 of 2, and the pressure difference is within 3e-5 bar of 8.065120317749026 bar. A merely plausible discussion without these measured values does not pass.
5. **Interpretation and visual evidence:** the session must actually supply at least one retained PNG from each fresh job to its vision model and keep the associated source receipt. Its explanation must compare measured pressure and MSD with a defensible calculation, identify the short-time noninteracting cutoff regime, and avoid inferring pressure, diffusion, equilibrium, or empirical argon validity solely from an image. Auto-fit can make different cell sizes look similar; use physical units/coordinates and numerical data to establish density. Viewer/export presentation remains separate from physical time.

Preserve any failed result and the original tolerances. If a general application defect requires a code change for this study to execute, this variant becomes diagnostic evidence; freeze and execute another scientifically meaningful variant afterward. Do not count a repaired rerun of the same case as the fresh generalization gate.

## Reference sources

- [OpenMM NonbondedForce theory](https://docs.openmm.org/latest/userguide/theory/02_standard_forces.html#lennard-jones-interaction): declared cutoff makes pair energy/force zero outside it; switching and dispersion correction are distinct model choices.
- [OpenMM integrator theory](https://docs.openmm.org/latest/userguide/theory/04_integrators.html): Verlet time staggering and energy conventions; when force is zero, the velocity offset vanishes.
- [NIST SI defining constants](https://www.nist.gov/pml/special-publication-330/sp-330-section-2): exact k_B and N_A used for the independent unit conversion.

The formulas, no-interaction bound and numerical table above were calculated directly from constants and the declared initialization, without executing OpenMM or reading outputs for these proposed runs. Upstream theory pages were checked on 2026-09-11; their live documentation version is newer than the pinned installed runtime, so the actual manifest/system/integrator must also confirm the stated force and integrator settings.
