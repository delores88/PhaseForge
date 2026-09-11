# Installed diffusion acceptance

Frozen ordinary-chat input before execution, September 11, 2026:

> Investigate how doubling diffusivity changes the decay of a spatial concentration pattern while preserving its total amount. Run two real 2D periodic diffusion experiments on a 64 by 64 cell-centered grid, 10 by 10 micrometers, dt=0.005 seconds, 512 steps, recording every 8 steps. Initial normalized concentration is 1 + 0.2*cos(2*pi*x/10)*cos(4*pi*y/10). Compare D=0.2 and D=0.4 square micrometers/second. Include a point instrument at x=0.2, y=2.4 micrometers and a region mean over x=[1.3,3.8), y=[4.2,7.1). Derive the expected Fourier amplitude response and test it against the saved fields and measurements. Inspect actual rendered images from both cases using the same fixed color scale; explain any change in contrast against the numbers. Preserve all fields for animated playback and report the model's scope and numerical limitations. Do not substitute a particle illustration or claim that this synthetic concentration is measured biological data.

The prior frozen numerical criteria in `diffusion-lab-proposal.md` apply without
revision: all retained fields versus independent discrete Fourier amplification
within 1e-10, spatial integral drift below 1e-11, scalar probes within 1e-10,
and doubled-D amplitude ratio within 1% of the continuum value at 2.56 seconds.
The component 32/64/128 refinement study is retained separately; this installed
workflow tests the ordinary chat, actual adapter, field viewer, native images,
provenance and presentation of the 64-square control/intervention pair.

No installed execution result is claimed by this preregistration. Append exact
build and session/job identities, numerical checks and observed UI behavior after
the workflow executes. A source-level worker check alone does not pass this gate.

## Installed execution, 2026-09-11

The frozen prompt was entered through ordinary chat (one typographic apostrophe
changed, with identical scientific content) in the installed Windows build dated
2026-09-11T16:21:32.328Z. Installer SHA-256:
`5dd3b93ae9812041426003b35d5b4923b4dea2feb482ee4c8fface445b502467`.
Project `21d62d26-8e20-48da-a84f-e61a1bb45095`, session
`b1ba93bf-8dbf-413c-bb50-8f732965d492`, OpenAI `gpt-5.6-sol`, medium,
15-minute limit. The session completed in 14 model rounds and 16 tool calls.

The first two attempts (`87f1b51e-8f24-4510-bda4-e94f3e8416df`,
`a662415e-b427-4b38-8d43-546829bdaee8`) failed at steps 400 and 128 with a
Windows progress-file atomic rename sharing/permission error. They remain failed
in the installed database with partial artifacts. The agent recognized this as an
execution failure and retried sequentially with unchanged numerical inputs; no
developer supplied successful outputs or changed the installed worker.

Completed control: `ca7a01a2-e971-4d14-856f-f7245ecbdb1d` (D=0.2).
Completed intervention: `03aec55f-8f90-4110-ad1f-5a53d221b9e4` (D=0.4).
Both retain all 65 scalar fields and requested point/region measurements.
`tools/check_installed_diffusion.py` checked every field against the independent
discrete Fourier solution, plus source/image hashes, exact input and lineage,
numerical probes, fixed color mapping, and actual native image dispatch followed
by completed provider acknowledgement. All checks passed in
`.local/science/installed-diffusion/report.json`.

Maximum field errors were 1.999e-15 and 1.554e-15, with maximum relative spatial
integral drift 1.422e-16. The final amplitude ratio was 0.363899989694 versus
continuum 0.363983227512 (0.0229% relative discrepancy). Both image scales remain
[0.8,1.2]. The native app displayed the measured 64-square field, animated
recorded changes across time, and maintained an eight-field bounded cache.
The fitted square/legend currently overlaps lower controls; a presentation-only
layout repair is tracked separately and must be checked in the next build.

The agent also authored and executed a new isolated NumPy instrument through the
normal generated-experiment tool: `70b96398-5f42-4ebd-9594-1f013aa289c6`.
Its Windows LPAC process completed in 535 ms with exit 0, 512 MiB memory,
60-second time and 128 MiB monitored storage limits. Source SHA-256
`34c5b0ba2ba96e144e6f5c1f345feb2035c0a7e61e96f2f20c26b789596b6a6a`.
It imports six pinned files, computes continuum and discrete responses, compares
all 65 instrument records and independently remeasures the initial/final arrays.
It does not inspect all 65 arrays; the separate developer checker does.

The written report correctly distinguishes synthetic scalar diffusion from
measured biology, but contains a symbolic error: it writes pi^2/20 beside the
correct decimal 1.9739208802; the expression must be pi^2/5. The numerical code
uses the correct sum of squared wavenumbers. An ordinary follow-up asks a real
independent specialist to audit symbolic/decimal consistency, dimensions and
execution claims against existing evidence, without rerunning science. Review
session `ebb876fe-0c48-4321-9612-2cc9dc7e127c`, specialist
`cd523140-8d1a-4364-9360-ef01031b63fa`, five-minute parent budget.
Interpretation acceptance awaits that review/correction; the numerical check
alone is not a blanket acceptance of generated prose.

The review completed through a real delegated agent, and the parent retrieved
the prior conversation plus the review output. It published the corrected
pi^2/5 expression, separated the normalized field integral from a physical amount,
and clarified endpoint-array versus complete-history checking. The specialist
properly noted it had not received the verbatim prior report or the vision
receipts; the parent reconciled those against its retained conversation. No
additional solver ran. This is a demonstrated useful review/correction, not a
claim that specialist agreement is independent numerical validation.

The installed second-domain workflow now passes with the failed executions and
the explanation correction retained. The operational file-sharing repair also
passed seven actual Windows contention tests (zero skips), including a real
129-state worker run while a reader withheld file replacement. The frozen field
checker was repeated without changed criteria and all nine groups passed:
`.local/validation/field-publication-20260911-attempt1/report.json` and
`.local/validation/diffusion-check-20260911-attempt3/report.json`.
Repair worker SHA-256:
`ee88e854487ca98bf124bf41a70fbdc992d62b88c912124dce4ca3523a5e95d7`.
The repair and viewer layout change still require the next installed-build check.
