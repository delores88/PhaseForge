# ML evidence plot component validation

Date: 2026-09-11. This is a rendering component check using explicitly synthetic data. It does not establish scientific validity or useful acceleration for an actual ML study.

`tools/ml_plot_worker.py` accepts only local JSON data and uses Pillow plus the Python standard library. The supervised coordinator supplies three immutable copies: a completed evaluation, its frozen split, and one completed three-seed out-of-distribution (OOD) solver shard. No model fitting, solver execution, network access, arbitrary code, or OOD prediction occurs in this worker.

## Coordinator contract

```text
python ml_plot_worker.py --input <plot-input.json> --output <output-directory>
```

```json
{
  "schema_version": 1,
  "evaluation": {"path": "imports/evaluation.json", "sha256": "<64 lowercase hex characters>"},
  "split": {"path": "imports/split.json", "sha256": "<64 lowercase hex characters>"},
  "ood": {"path": "imports/ood.json", "sha256": "<64 lowercase hex characters>"}
}
```

Paths resolve against the input file directory; the trusted coordinator may also supply absolute local paths. Each source is limited to 4 MiB. Its bytes are read once, checked against the supplied SHA-256, and parsed from those same bytes. Non-finite values, unexpected dimensions, identity changes, mismatched frozen condition membership, inconsistent three-seed uncertainty, and contradictory gate summaries are rejected. The current adapter is deliberately specific to the frozen 32-condition argon pressure study.

Outputs are `plot.png` (1600 × 1000), `plot.json`, and `result.json`. The result includes hashes and sizes of the PNG and mapping JSON. The mapping retains the exact input hashes, model and study identity, axis domains, all eight held-out data points, all error-bar endpoint pixel coordinates, the 32 frozen condition IDs, and the three-seed OOD measurement with `prediction: null`.

The first panel uses measured pressure on the horizontal axis and predicted pressure on the vertical axis. Horizontal intervals are three-seed mean uncertainty, using t(0.975, df=2) = 4.302653; vertical intervals are saved calibrated model intervals. The second panel shows the entire frozen condition split and the actual OOD fallback measurement at 180 K and 0.55 g/cm³. Both the individual saved gates and the overall pass or rejection remain visible. Synthetic data receive a prominent synthetic fixture label.

## Checks and retained evidence

`python tools/test_ml_plot_worker.py` passed three test methods. Independent affine equations verify point centers and both uncertainty-bar endpoints against the published pixel mappings; the actual PNG has the expected colors at those positions. Additional cases reject altered raw hashes, NaN, changed study or condition identity, missing cases or seed rows, duplicate seed IDs, inconsistent standard error or calibrated bounds, and contradictory gate status.

Actual component-05 fixtures were copied to local imports and rendered through the worker CLI:

| Variant | Evaluation source | Actual output |
| --- | --- | --- |
| Synthetic pass | `.local/science/ml-component-05/stage-finalize-synthetic-costs/evaluation.json` | `.local/render-acceptance/ml-plot-component-01/output/plot.png` |
| Synthetic rejection | `.local/science/ml-component-05/stage-wide-completed-rejection/evaluation.json` | `.local/render-acceptance/ml-plot-component-rejection-01/output/plot.png` |

Both variants use the exact frozen split from `.local/science/ml-component-05/membership-freeze/split.json`. The fixture had no OOD file, so the component runner constructed an explicitly synthetic three-seed OOD shard with pressures 90, 95, and 100 bar. Its displayed mean is 95 bar, sample SD 5 bar, and SE 5/√3 bar. This synthetic fallback is not an observation from a scientific experiment.

Both 1600 × 1000 PNGs were opened and visually inspected. Condition labels, uncertainty bars, legends, source hashes and rejection gates are readable. The rejection PNG SHA-256 is `e3e0bf5fdfbba5cd983e51e0dfbfb8f77fa02d506ad5bac5a68a1b9d3d64e86c`; its exact data-to-pixel mapping SHA-256 is `b7048bcb5f2be63266cdda951c6a335082c7120b2f60ed7eb466d0f13621863d`.

An actual completed study must still run through the supervised adapter, retain its own immutable inputs, and receive independent numerical and image checks. These synthetic component receipts are not installed-app or actual-study acceptance.

## Laboratory chart UI integration

The normal Scientific lab result selector and sidebar now open `study_plot` and `ml_study` results through `LaboratoryPlotViewer`. A selected study resolves its completed plot child without changing the selected job or taking focus. A chart that finishes just before its coordinator continues refreshing that parent until the final dataset links are published. Other views remain where the user left them. Chart context is available to ordinary chat; only solver viewers receive the numerical camera-capture callback.

The viewer downloads and verifies the PNG against its completed receipt SHA-256 before displaying the original pixels. It provides pointer drag and keyboard pan, cursor-anchored wheel zoom, zoom buttons, fit/reset, fullscreen and an accessible numerical table. Downloads expose the PNG, data-to-pixel mapping, all 99 seeds and their NPZ dataset, frozen split/protocol, evaluation/gates, model/model card, and full study receipt. View zoom does not resample the stored artifact or change numeric data. Invalid receipt/image hashes fail visibly; transient refresh errors do not replace an already displayed image.

Eleven targeted frontend tests pass across `laboratory-plot.test.mjs` and `workbenchRequests.test.mjs`, including cross-project context isolation, completed-only download links, unsafe artifact-path rejection, PNG digest verification and independent zoom-anchor/fit equations. The frontend production build passed after integration.

An isolated fixture bundles the actual React component and its CSS with the saved synthetic rejection PNG. It substitutes read-only fixture job responses; it does not create or modify research in the installed app. Actual decoded images were inspected at 900 px and 520 px pane widths. At 520 px the image bounds were 495 × 309.375 px inside a 518.667 × 406.333 px viewport; its bottom ended at y = 507.854 while the footer began at y = 556.333, so the data and controls do not overlap. Nine expected artifact download links were present. Zoom changed the rendered scale from 0.309375 to 0.386719; one right-arrow pan changed x by −40 px; Fit/reset returned x = 12 and scale = 0.309375. The original decoded dimensions remained 1600 × 1000. Evidence: `.local/viewer-acceptance/ml-chart-520.png`, `.local/viewer-acceptance/ml-chart-900.png`, and `.local/viewer-acceptance/ml-chart-layout.json`.

This verifies the new source component using synthetic evidence. The fourth installed checkpoint must still verify real study routing and downloads against the supervised backend.
