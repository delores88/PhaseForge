# PhaseForge 0.3.0 release notes

PhaseForge 0.3.0 is a complete standalone release based on the working 0.2.4 generic runtime.

## Provider configuration

- API keys can be stored without a model ID.
- Provider status distinguishes key readiness, model readiness, and complete chat readiness.
- Live model discovery is available after credential storage for OpenAI and Anthropic.
- Model selection is a separate save operation.
- A custom model-ID path remains available for compatible endpoints and account aliases.

## Research chat

- Persistent searchable research-world history.
- Rename, switch, create, and delete workflows.
- Resizable 10%-to-60% drawer and collapse/restore state.
- Draft persistence, attachments, cancellation, copy, reuse, and edit-and-branch actions.
- Builder, Explorer, Falsifier, Theorist, Biomolecular Architect, and QM/MM Planner roles.
- Selected molecular structure included in agent context.

## Scientific and molecular capabilities

- PDB, MOL/SDF V2000, and XYZ structure import.
- Normalized atom, bond, residue, chain, ligand, coordinate, and warning records.
- Interactive shared Three.js molecular viewport.
- Deterministic structure and geometry diagnostics.
- Candidate QM/MM-region planning and electron-count sanity checks.
- Local scientific-engine discovery.
- Staged computational campaign planning with explicit evidence gates and unavailable-engine states.

## Windows packaging

- Windows text helpers are complete copy-and-paste PowerShell programs.
- No PowerShell wrapper files are included.
- Installers perform the actual Cargo build/tests and Next.js production build.
