# Reusable surrogate queries and evidence assembly

The source query coordinator accepts an existing completed study, an explicit
scientific or explanation intent, and temperature/density with exact K and
g/cm^3 units. An omitted protocol uses that study's frozen protocol. Both the
original and resolved request remain immutable. Query admission verifies the
source stage completion receipts and pins the model, split, calibration and final
evaluation; recovery checks those pins before continuing.

Inference runs the study's exact worker through the existing isolated generated
job path. An invalid input cannot start a solver. An explanation request returns
the decision without solver execution. A prediction requires the actual final
usefulness evaluation and pinned model identity. A scientific refusal can launch
three actual OpenMM seeds for supported CPU parameters; unknown engines,
potentials, endpoints or platforms receive an explicit unsupported decision.
New query seeds and solver IDs retain their lineage, and the original model is
not refitted or rehabilitated by a fallback.

The fallback independently reduces saved scalar pressure records. It checks
engine/source identity, potential, density/box, units, complete integer sample
ordering and the requested timestep, then reports each seed mean and the
three-replicate mean, SD and SE. Its reported physical-time window follows the
actual timestep. These describe a finite computational window and do not assert
equilibrium or empirical validity.

The initial eight Rust tests produced seven passes and one incorrect rejection
fixture: a sample interval of 101 still retained exactly 100 endpoint samples
because the worker also saves its final nonmultiple step. The fixture was
corrected to interval 200, which retains only 50 endpoint observations. The
production sampling rule was unchanged. The integration runner subsequently
passed all eight query tests and all 13 deterministic ML tests. Its comprehensive
locked backend run passed 345 unit tests and 10 integration tests, with seven
environment-gated unit tests ignored. No actual query or scientific fallback is
claimed by these deterministic receipt fixtures.

Five Python receipt/data-product tests passed with zero failures or skips:
`python tools/test_ml_check_manifest.py`. They cover exact original input byte
hashes (Unicode and floating point notation), complete synthetic 99-case receipt
assembly without changing the source database, missing/altered cases, incomplete
studies, and all 33×3 dataset means, SD/SE, roles and source ordering. The first
run exposed an unclosed SQLite connection on constructor rejection; that handle
is now closed before propagating the error. Fixtures use synthetic values and do
not run scientific engines or fit scientific models.

A separate read-only check compared five existing pilot/stage input hashes with
their original Rust-serialized database bytes; all matched. Actual study fitting,
full 99-run numerical acceptance and installed ordinary-chat query behavior
require their separate execution receipts.

Separate actual component executions passed: protocol freeze, one real OpenMM
training pilot and isolated reduction (`.local/ml/latest-report.json`), and a
registered plot of explicitly synthetic data (`.local/plt/latest-report.json`).
Neither is a completed 99-run study or installed surrogate acceptance.
