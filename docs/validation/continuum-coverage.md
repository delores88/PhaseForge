# Continuum coverage and fixed numerical checks

This is source-level component work. It does not constitute installed application
acceptance, material calibration, or wet-lab replacement. Reference formulae and
tolerances below were specified before running the new worker checker.

`heat_conduction_2d` advances rho*cp*T_t = k*(T_xx+T_yy), with constant positive
density/heat capacity and nonnegative conductivity, on a periodic rectangle.
Lengths are metres, temperature kelvin, conductivity W/(m K), heat capacity
J/(kg K), time seconds, and energy is per unit out-of-plane depth (J/m).
Initial data can be a Fourier perturbation, a minimum-image Gaussian, or an
explicitly imported finite nonnegative numeric array. The five-point FTCS scheme
rejects alpha*dt*(dx^-2+dy^-2)>0.5. It does not silently shorten timesteps.

`navier_stokes_2d` advances the full unforced nonlinear vorticity transport
equation omega_t+u*omega_x+v*omega_y=nu*laplacian(omega). Fourier inversion of
-laplacian(psi)=omega gives u=psi_y, v=-psi_x. Classical RK4 advances the
resolved modes with strict abs(mode)<N/3 filtering of state and nonlinear RHS at
every stage. Initial conditions can be Taylor-Green or a sum of up to 16 resolved
streamfunction sine modes, including nonlinear mode interactions. The actual
velocity CFL must remain <=0.5 at every stage; nu*dt*kmax^2 must also be <=0.5.
Pressure is an actual diagnostic Poisson solve in the same retained modes, with
a mean-zero gauge. Density scales pressure; kinematic viscosity controls motion.

Both engines save strictly ordered physical time states, numerical .npy plus
exact JSON scalar views, and a per-frame .npz containing all computed channels.
Temperature has both heat flux components. Flow has both velocity components,
vorticity, pressure, and divergence. Each file has SHA256 provenance. The scalar
viewer receives explicitly converted micrometre coordinates for compatibility;
all calculations and retained physical channels use SI. Display pixels and
fixed colour clipping do not alter numerical values.

Checkpoint publication commits one atomic document containing the complete
retained-index snapshots and an immutable per-step state reference. On recovery,
all retained numerical and image hashes, input, worker, shared I/O source,
initial data and NumPy identity are verified; public index aliases can be rebuilt
after interruption. Changing parameters requires a new job, not an old-state
restart. Cancellation is cooperative and retains the last complete state.

## Checks frozen before execution

The independent checker does not import the worker or its numerical functions.
It reads retained arrays and computes its own formulae/invariants.

- Heat: cos(kx*x)cos(ky*y) amplitude is A exp(-alpha*(kx^2+ky^2)*t).
  Exact FTCS multiplier is 1-4*alpha*dt*(sin(pi*mx/nx)^2/dx^2 +
  sin(pi*my/ny)^2/dy^2). Raw-array error versus the discrete solution <=1e-10 K;
  relative energy drift <=1e-12. At fixed physical horizon, doubling grid
  resolution and quartering dt must reduce continuum L2 error by a factor >3.5.
  Zero conductivity leaves the state exactly unchanged; increased conductivity
  causes the independently predicted stronger decay.
- Taylor-Green on a unit square: u=U*sin(kx)*cos(ky)*exp(-2*nu*k^2*t),
  v=-U*cos(kx)*sin(ky)*exp(-2*nu*k^2*t), omega=2*U*k*sin(kx)*sin(ky)*exp(-2*nu*k^2*t).
  Where the doubled pressure modes are resolved, p=rho*U^2/4*(cos(2kx)+cos(2ky))*exp(-4*nu*k^2*t).
  Independent velocity/vorticity/pressure errors <=1e-8 for the small-step case;
  divergence max <=1e-10 1/s, mean velocity/pressure <=1e-12 in their SI units.
  Mode-3 temporal refinement at dt=0.02,0.01,0.005 s, horizon 0.1 s,
  nu=0.01 m^2/s must reduce continuum error by a factor >12 at each refinement.
- Nonlinear initial modes with different Laplacian eigenvalues: a one-step
  finite difference at dt=1e-6 s must match the independently differentiated
  analytic nonlinear RHS within relative L2 1e-4. A zero-viscosity interaction
  run must conserve kinetic energy and enstrophy within relative 1e-8 over the
  fixed small test horizon. This prevents Fourier-decay-only code passing as
  nonlinear Navier-Stokes.
- All registered NPY/JSON/NPZ/image pins, exact channel/view equality, units,
  frame order/endpoints and source identity must match. A genuine subprocess
  cancellation/resume must agree with an uninterrupted run within 1e-12 for heat
  and 1e-10 for flow; corrupted retained bytes must be rejected. Off-grid and
  unsupported boundary/CFL/shape requests must fail rather than be adjusted.

No stationarity, turbulent fidelity, empirical material validity, or arbitrary
geometry claim follows from these numerical tests. Walls/inlets/obstacles,
free surfaces, compressible or 3D flow, source-driven/radiative heat transfer,
temperature-dependent material parameters, and biological transport are outside
these adapters. User requests requiring them need a compatible engine or an
explicitly accepted simplified model.

## Black-hole collisions

A full merger requires an actual numerical-relativity engine, constraint-valid
initial data, spacetime evolution, mesh refinement, horizon finding, waveform
extraction, and convergence/constraint diagnostics. None is implemented here.
Newtonian orbital particles or a Blender merger illustration cannot fulfil that
experiment request.

The Einstein Toolkit provides a real binary-black-hole merger example using
TwoPunctures initial data, McLachlan spacetime evolution and horizon/waveform
analysis. Its documented GW150914-like example uses 98 GB RAM and roughly
8,700 core-hours, with large output storage; those are that example's resources,
not a universal minimum. [Official binary-black-hole example](https://www.einsteintoolkit.org/gallery/bbh/index.html).
Its current tutorial offers a hosted Jupyter environment or a local Docker
image. This establishes a plausible separately provisioned compute route, not
native compatibility with this app's bundled Windows NumPy interpreter.
[Official current tutorial](https://einsteintoolkit.org/documentation/new-user-tutorial).
Windows would need a validated Linux/container/remote execution arrangement,
compiler/MPI dependencies, resource admission and retained solver provenance.
No engine was installed or merger run for this coverage work.
