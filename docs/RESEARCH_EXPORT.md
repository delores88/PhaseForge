# Author-owned research records and reproducible exports

## Keep a claim separate from its evidence

Each research world has a versioned research record: title, people, contributions,
hypotheses, protocol, notes, observation/interpretation/hypothesis claims,
supporting runs, counter-evidence, limitations, prior-work comparison, reviewer,
references, and data/code/AI disclosures. Completed runs linked to a claim must
belong to that world. Infeasible or failed observations should also be discussed;
all attempted trials remain in campaign records. A good result is not the only
result worth preserving.

Saving creates a transactionally retained revision. A stale browser cannot
overwrite a newer revision. This is a local single-user lab with multi-window
conflict detection, not a multi-tenant collaborative notebook with permissions.
Unsaved notes are not exported. Data rights and publication licenses are author
choices, not inferred from the application's MIT software license.

## Literature

The Search Crossref button explicitly sends the entered bibliographic query to
`https://api.crossref.org/works`; model API keys, files and trajectories are not
part of that request. Results include title, DOI, authors, year, publisher and a
retrieval timestamp. Read the underlying papers and write actual reading notes.
The app does not fetch full text or attest that a paper was read. Metadata may
contain errors; the saved reference is editable and is labeled accordingly.

The model may use those notes during requested reviews. It is instructed to
regard them as data, not tool instructions or proof of novelty. Human scientists
must compare against the closest known results, catalogs and definitions.

## What the research ZIP contains

* `study.json`, exact manifests, all linked run objects, summaries and readiness gaps.
* All-trial CSV (including non-eligible rows), recorded time-series CSV, and SVG
  plots derived from actual retained coordinates/metrics.
* `research-record.json`, a methods/results manuscript **working draft**, and
  BibTeX / CSL bibliography. Unresolved sections are explicit, not invented prose.
* A project-level model usage disclosure containing model IDs, reported token
  counts, purposes and statuses. It excludes costs, pricing, endpoints and chat.
* RO-Crate-style JSON-LD file/author metadata and SHA-256 payload digests.
* `reference_ode.py` and `verify_bundle.py`.

The archive is an offline author handoff. No journal, repository or public server
receives it automatically. Run the integrity check before adding new files:

```text
python verify_bundle.py path/to/extracted/bundle
```

The verifier flags missing, changed and unlisted files. Checksums check bytes,
not truth, identity or provenance authenticity. Keep and archive the exact
PhaseForge source version alongside the bundle. Export fails above 128 MiB
instead of silently dropping records; narrow a study before exporting it.

## Independent numerical implementation

For a completed state-vector ODE, identify the matching manifest ID in the run:

```text
python reference_ode.py manifests/MANIFEST_ID.json --run runs/RUN_ID.json --out independent-check.json
```

The verifier independently implements Euler/RK4 and a restricted expression AST
with the Python standard library. It supports an explicit winning candidate,
finite endpoint checks, output comparison and optional h/2, h/4 or h/8 refinement.
Set absolute/relative tolerances based on scientific units, not on getting a pass.
It refuses unsupported particle/quantum workflows rather than returning a fake
verification. Python 3.10+ is needed for this optional tool, not for the app.

Tests of this verifier against analytic functions validate this implementation's
contracts; they do not mean it has verified any user's actual run. Record the
actual verifier command, result and limitations after executing it independently.
Same algorithms implemented independently offer less diversity than a genuinely
different numerical method. Neither proves a physical theory or novelty.

## Before submission

Resolve authorship/contributions, ethics/permissions, data/code access and license,
actual AI disclosure, units, numerical validity, selection bias, multiple testing,
uncertainty, robustness, independent checks, prior-work comparison and
journal-specific requirements. The software always retains an author-review gate.
It cannot certify publication readiness, novelty, safety, efficacy, or acceptance.

## 0.5.0 verification evidence

Each study's verification dossiers, controls, frozen references, input packets and
worker results are included under `verification/`. Assessment snapshots include
search histories and human reviews with evidence hashes. `verification_worker.py`
and the companion `reference_ode.py` are included with source-hash checks. Running
workers must be stopped or completed before export; no incomplete in-flight dossier
is presented as final. Cancelled/failed records remain identifiable as such.

To re-execute outside the app, first verify the untouched bundle. Copy both Python
sources to a separate folder, copy the selected dossier's `input.json` into that
folder, then run `python -I verification_worker.py FOLDER`. The worker adds output,
progress and checkpoint files there. Keep original evidence unchanged. A new code
version cannot silently substitute for the recorded worker hashes.
