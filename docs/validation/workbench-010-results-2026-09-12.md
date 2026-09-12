# Workbench 0.10 source-application results — 2026-09-12

The repaired source application completed a real numerical analysis, published existing generated numerical states to the native player without recomputation, completed a requested Blender illustration while preserving a status update, and then executed a new heat simulation when the user changed the requested output. The black-hole merger request correctly remained **unfulfilled with a capability gap**. These are bounded source-application integration results, not a claim that numerical relativity or experimentally predictive biology is implemented.

The root operator exercised the UI and actual `open_ai` / `gpt-5.6-sol` / `medium` provider workflows. A separate read-only audit captured job receipts at **2026-09-12 12:24:06 UTC**. Testing used the isolated profile `.local/validation/workbench-010/data` and UI origin `http://127.0.0.1:7442`; it did not target the original installed user profile. The audit made no provider requests, started no jobs and wrote no application data. This document does not substitute for installation or marketplace acceptance of a packaged 0.10 candidate.

## Preserved failures and repair

The first real-provider requests exposed an application contract loop that mocked-provider tests had missed:

| Request | Original session |
| --- | --- |
| Physical equal-mass black-hole merger at 0.999c | `ebad1c2e-b804-4d55-a7ac-dd44555fdf38` |
| Static quadrature of the integral of x² from 0 to 1 | `03855281-94c4-42ba-9c48-33f11747e6e5` |
| Analytic oscillator, retained states and native playback | `50050adc-aa37-48da-b419-6bfa3258b66f` |

All three initially selected the intended output kind. However, application state was appended as a user-role instruction telling the model to resolve intent and preserve a previous objective on every round, even after resolution. The model repeatedly called `set_output_intent`; some calls quoted that application sentence as user text. The saved journals retain exact quote-rejection errors, “There is no earlier objective to preserve,” and the false gap-clearing rejection caused by a rephrased gap description. The original requests had explicit 240-second budgets; the failure is retained, not reported as a successful long-running workflow.

The repair places contract state in provider instructions and distinguishes initial resolution, an actual user update, and an already resolved request. A resolved request proceeds to execution, inspection or a final explanation. Same revision/kind/capability retries return the original contract without replacing its scope, evidence or capability gap. Stale request IDs, changed kinds and changed capabilities remain rejected. Initial `preserve_objective` initializes the request when there is no earlier objective. A status update before initial scope resolution retains the original instruction and requires its exact citation; later explicit corrections still take precedence.

The two original merger/oscillator sessions encountered interrupted dispatches before a response receipt arrived. On explicit resume they returned to `paused`, retaining their pending identities and reporting that no automatic replacement request was issued:

| Original session | Pending usage ID | Saved reservation: input + output |
| --- | --- | --- |
| `ebad1c2e-b804-4d55-a7ac-dd44555fdf38` | `1d4a274a-4ac1-48b9-b705-79db9b61cdf6` | 58,865 + 12,000 tokens |
| `50050adc-aa37-48da-b419-6bfa3258b66f` | `87eb6ea8-7d21-445d-bc01-edbb82f8fe64` | 64,886 + 12,000 tokens |

A read-only SQLite receipt confirms both records remain `network_error`, with no provider response ID and `usage.reported=false`. Zero reported tokens here means **unknown usage**, not a free call or confirmed remote cancellation. New explicit merger/publication requests have distinct IDs. The quadrature session resumed its existing journal and reused its completed numerical job.

Two earlier direct field admissions also failed closed because the development seed contained an unexpected `Lib/collections/__pycache__` directory: heat job `2a1fb0d1-2d64-400d-b994-6b3f3303ad78` and flow job `44144097-8d7c-4667-9e06-5596f6530a81`. Their failures remain retained. The isolated profile was provisioned from the unchanged installed seed, verifying 2,160 files and 171,944,416 content bytes against manifest `81d70a3d337bded463bf37fa71c32e49c2b860e5e3ae12cce4aa0c4107c94dec`. Integrity checks and scientific equations were not loosened.

## Actual repaired outputs

**Black-hole capability disclosure.** Session `1495d57a-bc9b-40dd-bdf7-0e7a441d8d8e` finished its explanation in four provider rounds and three tools: intent resolution, executable catalog inspection and deliverable checking. It retained `output_intent=simulation`, `required_capability=numerical_relativity`, `status=capability_gap`, `fulfilled=false`, and an empty evidence list. Its durable session state is `paused`, not falsely completed. No substitute solver, generated model or illustration was launched by this request. The final explanation identifies the absent Einstein evolution, constraint-solving initial data, horizon finding and wave-extraction infrastructure. This is a successful routing/disclosure check, **not a solved merger**.

**Static numerical analysis.** Session `03855281-94c4-42ba-9c48-33f11747e6e5` completed after recovery, with `output_intent=analysis` and a fulfilled numerical deliverable. Its 21 rounds / 20 tools include the original loop; they are not all post-repair work. It reused completed isolated generated job `0b501c24-5c5b-4de6-b6e4-650ed8370c5c`. The retained composite trapezoidal calculation used 16 and 32 subintervals: fine estimate `0.33349609375`, analytic reference `1/3`, Richardson error estimate `0.00016276041666666666`, and actual absolute error `0.00016276041666668517`. The registered `work/result.json` SHA-256 is `b217f7072f35f350fdfa925adc2ac9400d4ed8564339f30b10ebfd01af9c7914`. No trajectory or illustration was required. This supports this calculation and its error comparison, not arbitrary generated-code scientific validity.

**Generated states published without rerunning their model.** Session `5a7f72bb-2511-41f0-b969-9b71c7a9e8c3` completed in six rounds / nine tools. It inspected already completed generated job `4f1c9a58-508e-4f78-aa43-a4b42541ef5d` and created publication `bdd69aca-4fe5-4231-b5e9-0a7488156f3e`. The raw generated job alone was explicitly rejected as a native temporal deliverable; its validated publication satisfied the request. The recorded source has 65 states of the explicitly analytic model `x(t)=cos(t)` metres, `y=z=0`, from 0 to 6.28 seconds, with a blue body and 0.06-metre display radius. The existing independent publication receipt checked all 65 positions and recorded zero difference from the analytic formula in its floating-point comparison.

| Oscillator provenance member | SHA-256 |
| --- | --- |
| Original generated code | `056c9fa4c05be85f735062b8e31d7accbb741a502617ee5cdf6ce19f718d6db4` |
| Original execution manifest | `7aaed84c6c2e49c8e845c5c441cec6dc3f2c3c93f8ecd16c25305fdafcbc4e91` |
| Published source numerical JSON | `c87592c59c2384356b2c9c8a5640077ac3f5740541317731ca14f45b3d15f782` |
| Native trajectory index | `6b14c6291052bd049582b1741c776d11cfdb1224cef5878f3ae3d6a1a21781a9` |
| Native trajectory chunk | `d65e67648a5f226b3bfd685aad43ca78b2c71b30751f12de02d2ca9f8d53470a` |

This is an educational analytic trajectory, not measured experimental data or validation of a general time integrator. Interpolated display frames do not add computed evidence. Native publication validates data and provenance; it does not confer physical applicability.

**Requested illustration and in-flight status.** UI session `d11df341-c343-4b3d-ac79-20f55a353d6f` requested a conceptual Blender still of a blue sphere with a labelled 3D coordinate triad, 512×512 pixels and 32 samples, explicitly without a physical simulation or changes to the saved flow experiment. Update `65de0e52-96f3-483e-a082-170668a2f8b1` said, “How far along is it? Please keep working on the same illustration.” The journal retained both instructions, revision 1 and `semantic_preserved_objective`; it did not turn the status question into an explanation-only task. The session completed after 19 rounds / 18 tools, with final render `e09b7d13-c53a-46f6-9a87-39d374d21a76`. Its actual 280,426-byte PNG hashes to `ffd81937fd6c12866b54578a6632c3205c28c5ce9ee000433b6155b898560812`; deliverable metadata explicitly says `rendered_still` and `scientific_simulation=false`. Intermediate label/render attempts remain retained. The root operator exercised pan/zoom and 3D orbit/tilt/pan/zoom on the saved Blender output; that UI observation is attributed to the operator, not inferred from a PNG hash.

**Explicit still-to-simulation switch.** The next ordinary UI request in the same project selected a playable heat simulation instead. Session `666af4a1-d8f3-4fb5-b6ba-34ebf8d30269` completed in seven rounds / seven tools, with `output_intent=simulation`, capability `heat_conduction_2d`, and solver `c0e86028-024c-45e9-9f49-ef269a4a479e` as its fulfilled evidence. The solver retained 21 states on a 32×32 periodic grid over a 0.01-metre square, at times 0–0.4 seconds, using 40 steps of 0.01 seconds and sampling every two steps. Mean temperature remained 300 K; the requested Fourier amplitude changed from approximately 20 K to `19.11668228344545` K; recorded relative thermal-energy drift was zero. Default material coefficients were explicitly treated as a numerical example. Index SHA-256: `77c3f7ebaf2200ca9a2e210c539562e4a1da9b5a4d5599c9ebf5f94931250c6c`. Final authoritative temperature-array SHA-256: `545be711f8b5f2bc9cba2d24c5b9e7b6c5fd1fb3928a7dd9fe1b9fca83e01d91`. This case did not use a Blender still to fulfill the simulation request.

Direct API heat job `ce8734f2-2f72-4082-ac08-aeeff3d1d6a5` and incompressible-flow job `3d7fa697-fefa-46d1-a1cf-7b6c1685ac11` also completed with saved physical fields and measurements. Their scope remains constant-property periodic 2D conduction and unforced periodic 2D incompressible Newtonian flow. No claim is made for general material calibration, walls, free surfaces, compressible shocks, 3D turbulence, or replacement of a wet-lab experiment.

The root operator also set the heat playback duration to two seconds, pressed Play, and observed the final player time `0.4 / 0.4 s`. This is an actual UI observation in addition to the saved numerical-state receipts; the display duration does not change the simulation's physical time.

## Verification and retained evidence

The final command `cargo test --locked --no-default-features --lib -- --nocapture` passed **400 tests, 0 failures, 11 ignored**, in 18.61 seconds of test execution. The ignored native/scientific cases were not silently counted as executed by this suite. The earlier full-suite result (394 passed, four mocked-provider contract failures, 11 ignored) remains in `backend-tests.log`; fixture repairs now perform real intent tool turns rather than bypassing completion gates. Focused repaired paths separately passed intent 8, data 5 and steering 6 tests.

Local evidence is retained under `.local/validation/workbench-010/` (not embedded into this tracked document):

| Receipt | SHA-256 |
| --- | --- |
| `results-receipt-20260912T122406Z.json` — compact read-only API/job/journal snapshot, 19 jobs: 14 completed, two failed, three paused; artifact pins included | `c9f25fd3ee077b0869977f9946461af5b6751865dd95dd26ff1859b629e169db` |
| `uncertain-reservation-receipt-20260912T1227Z.json` — read-only SQLite capture at 12:25:28 UTC of the two interrupted calls | `a86b4d9b121a54115622f210e8df0ad5feccfb152f2e6210ed136542bb7ad71c` |
| `actual-publication-verification.json` — pre-existing all-state oscillator comparison | `d74e74ed164b2a131c455e63ca95a718cc55eb21328daee529b11d1a3b703d44` |
| `actual-fields-status.json` — direct heat/flow execution receipts | `2890f2476b493131f38125a85f2ba0c4e8d572651e5c5f87cfd12435e7ebed88` |
| `navigation-read-verification.json` — operator navigation/read-state observations | `c892f2f59c05b1afa5c2ba21c380253971e23cfa9969362f05b605959e859bfb` |
| `backend-lib-final-20260912T062019881.log` — final full library suite | `4fe39ea7856b62862ed9e3c2bd8d0cd2a63691385759dc416d54fb11bbc966e6` |

The original job directories and provider/tool receipts remain available beneath `data/artifacts/laboratory/<job-id>/`. The evidence capture deliberately omits credentials and bulk generated code; it retains identities and hashes. Backend deliverable checks verify retained execution evidence; their endpoint checks are not a replacement for all-state numerical validation, convergence studies or real-world calibration. These few successful semantic cases also do not establish a measured general routing success rate. A separately packaged Windows candidate still requires its own release checks.
