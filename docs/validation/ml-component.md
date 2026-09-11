# Surrogate numerical component evidence

This is source component evidence. It is not an installed acceptance, a fitted
scientific model, or proof of scientific acceleration. The fixed proposal remains
`b923777b120659fb5129ae4025efbd1d3a8e4930c05d27840a9436d7697ac737`.

`tools/check_ml_surrogate.py` executed 105 subprocess fixtures with the pinned
Python 3.13.15 / NumPy 2.4.6 runtime. All passed: 94 expected successful cases and
11 expected rejection cases. Evidence is retained in
`.local/science/ml-component-05/report.json`, with exact requests, outputs and
source snapshots in that directory.

Independent scalar Gaussian elimination agreed with all 72 well-conditioned
candidate configurations to a maximum 2.220446049250313e-14 Z, below the frozen
1e-10 tolerance. Fixtures also exercise singular matrices without hidden jitter,
constant targets, duplicate observations, the ninth-order calibration statistic,
role leakage, missing replicas, changed raw manifest bytes, invalid units and
sample ordering. The complete synthetic stage chain verifies selection, refit,
calibration, inference and cost finalization. Its labels and cost records are
explicitly synthetic. No scientific solver runs or scientific fitting were used
in these component fixtures.

The worker SHA-256 is
`74475774261c19d6e0bc0315789b3a79940effcbf7f79d8f7b3fed473deac892`.
The component checker SHA-256 is
`d55ded3d29653ffad8f77b1c6d17e4fb1f78ce721ffd78db6a9a130b70e6cdd3`.

`tools/check_ml_study.py` is the separate read-only evidence checker for an actual
99-run study. Its fixed pair/switch/periodic algebra self-test passed, retained in
`independent-pair-self-test.json`. The full real-study audit has not yet been run
as part of this component evidence. Actual pilot/study execution and installed
acceptance are recorded separately by the study coordinator.
