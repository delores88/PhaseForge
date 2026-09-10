# Public research and artifact design

`research_mode` is an optional boolean on chat turns, research sessions and Studio design requests. It defaults to false. Enabling it authorizes bounded public queries using the current brief: biomedical literature through Europe PMC, experimental molecular structures through RCSB PDB when relevant, and NASA images for astronomy or space topics. A session records this intake before its specialists work in each cycle. Turning the mode off prevents new automatic catalog requests; already saved evidence remains available.

Automatic discovery uses the three public catalogs below and makes at most two searches, bounded entry-metadata requests and one asset import per cycle/turn. Search matches remain candidates whose relevance and provenance require interpretation. Explicit public-file imports additionally accept supported data and CAD meshes from the listed hosts; this does not enable unrestricted web browsing, full-paper retrieval or executable assets.

## Source intake

- `GET /api/projects/{id}/assets` returns saved `assets` and `searches`.
- `POST /api/projects/{id}/assets/search` accepts `{"catalog":"rcsb","query":"HIV protease","limit":5}`. Catalog choices are `rcsb`, `literature` and `nasa`.
- `POST /api/projects/{id}/assets/import` accepts a catalog accession, for example `{"catalog":"rcsb","accession":"1HSG"}`. Literature search already saves its source records; it does not import full papers.
- The same import endpoint accepts `{"catalog":"public_file","accession":"https://raw.githubusercontent.com/owner/repository/main/data.csv"}` for an explicitly selected file. `public_file` is an import option, not a search catalog.
- `GET /api/assets/{id}/content` returns the saved original structure, image, data or mesh bytes.

RCSB imports currently support four-character PDB IDs. Their actual coordinates become local molecular structures; a source binding reuses these coordinates instead of asking a language model to invent them. Downloads are limited to 8 MiB, and the molecular importer supports up to 100,000 atoms. NASA imports accept only static PNG/JPEG files within a 32-megapixel decoding budget. Every asset retains its source URL, SHA-256, source description and provenance, with the original bytes in local storage. Provider context includes source metadata; NASA pixels are not currently sent as model vision input.

When RCSB catalog metadata is unavailable, new imports fall back to the downloaded
PDB header's title, experimental method and reported resolution. Continued header
lines are joined; unavailable resolution stays unknown. Richer catalog metadata
and previously saved asset records are preserved.

Fetches use fixed catalog hosts and routes with redirects disabled. Legacy NASA image links are upgraded to HTTPS only for its exact asset host. Retrieved text is untrusted evidence, never application instructions. NASA records can describe observations, composites or artist illustrations; source images and deposited structures do not establish simulation or clinical validity.

Public-file URLs require HTTPS on exactly `raw.githubusercontent.com`, `zenodo.org`, `www.ebi.ac.uk`, `ftp.ebi.ac.uk`, `files.rcsb.org`, `data.eso.org`, or `www.eso.org`. Credentials, query strings, fragments, custom ports, redirects and other hosts are rejected with an explanation. Supported extensions are CSV, JSON, PDB, STL, GLB, PNG and JPEG; the bytes must also pass the corresponding data/image/geometry checks. HTML, SVG, scripts, executables and Blender project files are not accepted.

The general byte limit is 8 MiB. CSV profiling retains its existing 1 MiB, 20,000-row and 128-column limits; original CSV bytes remain saved with the profile. JSON/CSV/PDB contribute bounded, untrusted text previews to model context. STL requires complete finite triangle records and at most 150,000 triangles. GLB requires embedded buffers and PNG/JPEG textures, bounded accessors, and an acyclic scene hierarchy; external/data URLs, sparse accessors and compressed decoder extensions require a self-contained expanded export. GLB/STL originals use the same content endpoint and can open in the local model viewer. A file's public host does not establish its accuracy, license or manufacturing suitability.

The adapters follow the official [RCSB Search API](https://search.rcsb.org/), [RCSB Data API](https://data.rcsb.org/), [RCSB file download services](https://www.rcsb.org/docs/programmatic-access/file-download-services-overview), and [NASA image API](https://images.nasa.gov/docs/images.nasa.gov_api_docs.pdf).

## Studio design

`POST /api/projects/{id}/studio/design` accepts `prompt`, `kind` (`scene`, `cad`, or `pcb`), optional `provider`, `model`, `reasoning_effort`, `request_id`, `research_mode`, and up to eight selected `asset_ids`. It uses the existing usage limits, model selection, background polling and request cancellation. Rendering or fabrication does not create an unrelated numerical experiment.

For revisions, supply `parent_design_id` and optionally a `failure_report` of up to 12,000 characters. The parent must exist in the same project; this is checked before public retrieval or a paid model call. The provider receives the exact saved prior design and the supplied diagnostic report. A successful revision becomes a new record with its parent ID, revision number and report, preserving the original. The model is not told that the correction passed a render or fabrication test before those workers actually run again.

The saved response contains a readable `message`, a `design`, source `assets` and the public-research receipt. The strict design contains `version`, `title`, `kind`, nullable `scene` and `fabrication`, `source_refs`, and `asset_bindings`. A molecular binding has `asset_id`, nullable `node_id`, and `representation` (`surface`, `ribbon`, or `ball_and_stick`); the returned asset's `molecule_id` resolves its full structure. Unknown sources and cross-project bindings are rejected.

`GET /api/projects/{id}/studio/designs` lists saved designs. The trusted rendering and fabrication workers validate and export these declarative specifications separately. A saved design is not a claim of manufacturing readiness, correct electrical operation or scientifically validated dynamics.

Output-limit exhaustion can use the configured repair allowance to request a
complete compact design under the same selected token ceiling. It never continues
truncated JSON, silently raises the limit or discards the billed first attempt.
Zero repairs disables this recovery; all attempts retain cancellation and usage
admission. See [usage controls](USAGE_AND_COST.md).

Paused session controls also accept optional `research_mode` on resume. Omitting it preserves the previous choice; changing it applies to subsequent discovery stages without reissuing completed work.

## Inspect and execute locally

In **Studio**, select the artifact kind and sources, then **Build design**. Inspect
the saved scene or specification before choosing **Render in Blender** or **Build
engineering files**. The current design can be revised with a new brief and the
latest worker failure report; the original remains in saved history. An imported
GLB/STL can also open directly in the interactive viewer without a model call.

Blender renders a supplied structure or scene and exports PNG, `.blend` and GLB.
CadQuery/OpenCascade exports checked solids as STEP/STL. KiCad produces editable
boards, native DRC and previews; manufacturing output is gated on its actual checks.
Native jobs retain their inputs, reports and artifacts separately from design
generation. They have bounded queues, deadlines and cancellation; restart marks
unfinished jobs interrupted and preserves their files for explicit reruns.

These optional engines are locally installed software, not part of the PhaseForge
desktop bundle. See [Blender setup and limits](BLENDER_RENDERING.md),
[CAD/PCB setup and checks](CAD_PCB_ENGINES.md), and
[interactive model inspection](SCIENTIFIC_MODEL_VIEWER.md). Current native-engine
execution evidence is Windows x64; it does not establish Linux or ARM64 availability.
