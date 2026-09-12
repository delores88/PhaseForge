# Unadmitted horizon-checker checkpoint

Work stopped on 2026-09-12 after the user clarified that an exact 0.999c collision is not a release gate. Dependable research software may stop unsupported or unsolved work with retained evidence and actionable options. No additional horizon study is required for this release.

`tools/nr_horizon_expansion.py` and `tools/test_nr_horizon_expansion.py` preserve an independent embedded-surface geometry/interpolation prototype. The initial 12 analytic tests passed before the stop instruction, without running an evolution or reading actual native CPU/GPU fields. The immutable tested source snapshots and logs are in `.local/validation/nr-cuda-20260912/horizon-expansion-analytic-01`; the tested helper SHA256 was `8c7ee338c1fd7cbb5c7668c41f104a07e0ce04e8b72785bdb2507f139ffee896`.

The later retained-data CLI, source/time matching and active-cell stencil extraction are an unadmitted checkpoint. They have not been validated end to end or against actual data. The CLI decoder import under isolated Python also requires review before future use. No runtime hook or product capability exposes this checker. Existing solver binaries, native outputs, diagnostic helpers and result receipts remain unchanged.

The prototype reconstructs physical metric and extrinsic curvature at native nodes, differentiates a tensor-product interpolating polynomial, and contracts the embedded surface's second fundamental form. Analytic controls covered flat spheres, the sign of nonzero extrinsic curvature, isotropic Schwarzschild surfaces, nonspherical harmonics, interpolation derivatives, and all angular nodes. These synthetic controls do not establish the correctness of actual horizon measurements or numerical convergence of a collision.

Future admission, if separately requested, must retain exact state/surface mapping, reject stale searches and incomplete same-level stencils, account for float32 native fields and printed shape precision, and apply an external resource guard. The pinned upstream FastFlow reduction omits its last angular node at both exclusive-end ranges; this prototype includes every node. No upstream source was changed by this checkpoint.
