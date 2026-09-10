# Security model

## Trust boundary

AI providers can propose declarative research actions. They cannot directly execute shell commands, modify PhaseForge source, read arbitrary files, enumerate credentials, or make arbitrary network calls through the trusted runtime.

## Provider credentials

API keys are stored with the operating-system credential service. SQLite stores model ID and base URL but not the key. The settings API reports booleans for key and model readiness; it never returns credential material.

Model discovery occurs from the Rust backend using the saved key. Custom base URLs require HTTPS except for loopback development endpoints. Users must not send real provider keys to untrusted compatible proxies.

## Attachments

Chat attachments are text-only, size-bounded, and limited by extension. Supported molecular files may be parsed into normalized local records. Attachment content is included in provider context only when the user sends that message, so confidential scientific data should not be sent to a remote provider unless the user accepts that provider's terms and controls.

## Model-output boundary

Provider output is schema-parsed. Executable manifests pass trusted validation before persistence or execution. Molecular and QM/MM discussions cannot silently invoke unconfigured scientific engines.

Studio has a separate strict, data-only schema for scenes and fabrication. Source
bindings and revision parents must belong to the current project. Render and
fabrication requests validate again before starting their fixed native workers;
model-generated Python, shell commands and plugins are not accepted.

## Public source intake

Automatic catalog queries require `research_mode:true`; it defaults to false on
chat, Studio and research sessions. Explicit search/import actions authorize their
own requests. Turning the mode off stops new automatic retrieval, but already
saved evidence can still be included in later provider context.

Catalog requests and public-file downloads use fixed scientific/data hosts, bounded
responses and no redirects. Public-file URLs reject credentials, query strings,
fragments, custom ports and unlisted hosts. Allowed CSV/JSON/PDB/GLB/STL/PNG/JPEG
content must pass format-specific checks. HTML, SVG, executable files, scripts and
incoming Blender projects are rejected. GLB assets must be self-contained within
the accepted embedded-resource subset. See [the intake contract](public-research-and-studio.md)
for exact hosts and limits.

Retrieved content is untrusted evidence, never an instruction to the application or
an authority over the researcher. Original bytes and provenance are saved locally.
Checksums identify those bytes; they do not certify scientific truth or a license.
No public upload or publication is performed by this intake path.

## Resource controls

Run manifests contain finite wall-time, memory, step, candidate, batch, and output limits. Chat requests can be cancelled. Molecular imports and message attachments have explicit size and count ceilings.

## Local exposure

The default bind address is loopback. Exposing PhaseForge to a network requires additional authentication, authorization, TLS, request isolation, audit logging, and worker security that are outside this release.

## v0.3.1 controls

Every model request, including connection tests and repairs, goes through the same
local usage admission gate. Final provider-reported usage is saved before output
validation. Stopping is cooperative; providers may bill already processed tokens.
The closed response schema is checked again locally before the trusted numerical
validator. A maximum of two repair calls can be configured (default one).
No model output is granted a shell or arbitrary code execution.

This is a single-user loopback development server, not a remotely authenticated
service. Do not expose the new usage, cancellation, pricing, or existing project
write endpoints to a network without a separate authorization design.

## Trusted independent process (0.5.0)

The independent verification worker is static project-shipped Python, embedded into the
Rust binary with source hashes. No user/agent source is compiled or run. The service
launches an explicitly probed interpreter without a shell, under `-I`, with a bounded
JSON input, an isolated random working directory, output limits, cancellation and
wall/RHS caps. This is process separation and a bounded arithmetic interpreter,
not a hardened operating-system sandbox against a malicious local user. A locally
compromised Python installation remains outside the app's trust boundary.

Crossref receives only explicitly consented query text. No automatic public upload
or paper submission is added. Reviewer names are local attribution, not authenticated
signatures. Checksums verify bytes, not truth or identity. One backend instance per
database remains required.

## Native Studio processes

Blender and CAD/PCB jobs run project-shipped workers with validated JSON inputs and
separately installed local engines. The caller cannot choose an arbitrary script,
command or artifact path. Blender starts with factory settings and automatic script
execution disabled; editable `.blend` output is created by the trusted worker.
Artifact routes expose fixed filenames, not arbitrary files from disk.

Each worker has bounded queueing, elapsed time, outputs and cancellation. Windows
Job Objects supervise process trees and enforce host-memory limits; Unix workers
have process-group cancellation and memory monitoring. These controls do not
reserve GPU VRAM or sandbox a compromised native installation. Locally configured
engine executables and dependencies remain trusted software. Render/fabrication
jobs interrupted by a restart retain their files and require an explicit new job.
