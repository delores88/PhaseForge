# Numerical relativity source corrections

These are explicit patches for a future separately identified engine build. They do not alter the frozen engines or the retained CPU/CUDA attempts, and their presence does not establish a valid horizon or collision.

## Complete FastFlow angular reductions

`athenak-fastflow-complete-angular-reductions.patch` applies to AthenaK commit `c5a0d7f9155a70149931bf0be5a4ffb673f2532a`, `src/z4c/fastflow.cpp` SHA256 `524c8e28b417f0526753e507787138c5f9ee59897e726601a278bb7daa60d321`.

Both the radius minimum and surface-integral reductions pass `nangles-1` as a direct Kokkos `RangePolicy` exclusive end. For 200 nodes this excludes node199, including its area, expansion, spin and flow contribution. The patch changes both ends to `nangles`. The pinned Kokkos commit `6739bc623081648af9e752b616d9671527922cbf`, `core/src/Serial/Kokkos_Serial_Parallel_Range.hpp` lines89–92, explicitly loops with `i < end`. These are direct Kokkos ranges, not AthenaK's inclusive wrapper.

The patch SHA256 is `c42bbf5cc1af2701066582b4f2470383cd4156df6dee0510831143785504e9c4`. `git apply --check` passed against the unmodified pinned source on2026-09-12. It has **not been applied to or validated in an engine build**. A future build must retain the base revision, patch bytes, resulting source identity and executable identity; it must not claim to be the unmodified upstream executable.

This correction does not repair the mass-change-only horizon stopping test, validate the remaining expansion calculation, improve spatial resolution, or calibrate physical boost. Independent expansion checks must include all angular nodes and disclose the bias in historical native surface diagnostics. Existing saved results remain immutable.

Primary source: [FastFlow reductions](https://github.com/IAS-Astrophysics/athenak/blob/c5a0d7f9155a70149931bf0be5a4ffb673f2532a/src/z4c/fastflow.cpp#L942), [pinned Kokkos range reduction](https://github.com/kokkos/kokkos/blob/6739bc623081648af9e752b616d9671527922cbf/core/src/Serial/Kokkos_Serial_Parallel_Range.hpp#L86).
