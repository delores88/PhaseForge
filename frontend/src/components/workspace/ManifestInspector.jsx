import ComputeAdvice from "./ComputeAdvice";
import {
  Braces,
  CheckCircle2,
  ChevronDown,
  Clipboard,
  Cpu,
  FlaskConical,
  Gauge,
  Play,
  Search,
  ShieldCheck,
  SlidersHorizontal,
} from "lucide-react";
import { useMemo, useState } from "react";
import { compactNumber, formatDate, statusLabel } from "@/lib/format";

export default function ManifestInspector({
  manifest,
  manifests,
  onSelectManifest,
  onRun,
  running,
  onOpenImport,
}) {
  const [rawOpen, setRawOpen] = useState(false);
  const modelSummary = useMemo(() => summarizeModel(manifest?.model), [manifest]);

  if (!manifest) {
    return (
      <section className="manifestEmpty">
        <FlaskConical size={24} />
        <strong>No setup yet</strong>
        <p>
          Use Build experiment, enter equations, or import an existing setup.
        </p>
        <button type="button" className="button button--secondary" onClick={onOpenImport}>
          <Braces size={14} /> Import manifest
        </button>
      </section>
    );
  }

  return (
    <section className="manifestInspector">
      <header className="manifestHeader">
        <div>
          <div className="eyebrow"><CheckCircle2 size={13} /> Executable setup · scientific validity not assessed</div>
          <h2>{manifest.title}</h2>
          <p>{manifest.hypothesis}</p>
        </div>
        <div className="manifestHeader__actions">
          <label className="revisionSelect">
            <span>Revision</span>
            <select
              value={manifest.id}
              onChange={(event) => onSelectManifest(event.target.value)}
            >
              {manifests.map((item) => (
                <option value={item.id} key={item.id}>r{item.revision} · {formatDate(item.created_at)}</option>
              ))}
            </select>
          </label>
          <button type="button" className="button button--primary" onClick={onRun} disabled={running}>
            <Play size={14} fill="currentColor" /> {running ? "Queued" : "Run saved setup"}
          </button>
        </div>
      </header>

      <div className="manifestFacts">
        <Fact icon={FlaskConical} label="Capability" value={statusLabel(manifest.model?.kind)} />
        <Fact icon={Gauge} label="Numerics" value={`${statusLabel(manifest.integration?.method)} · Δt ${compactNumber(manifest.integration?.time_step)}`} />
        <Fact icon={Search} label="Search" value={manifest.search?.enabled ? `${statusLabel(manifest.search.algorithm)} · ${manifest.search.population} × ${manifest.search.generations}` : "Single execution"} />
        <Fact icon={Cpu} label="Compute" value={`${statusLabel(manifest.compute?.preference)} · ${manifest.compute?.candidate_count?.toLocaleString?.() || 1} candidates`} />
      </div>

      <ComputeAdvice manifest={manifest}/>
      <div className="manifestSections">
        <ManifestSection icon={ShieldCheck} title="Scientific boundary">
          <p>{manifest.scientific_boundary}</p>
        </ManifestSection>
        <ManifestSection icon={SlidersHorizontal} title="Model">
          <dl className="detailGrid">
            {modelSummary.map(([key, value]) => <div key={key}><dt>{key}</dt><dd>{value}</dd></div>)}
          </dl>
        </ManifestSection>
        <ManifestSection icon={Braces} title="Governing expressions">
          <ExpressionList manifest={manifest} />
        </ManifestSection>
        <ManifestSection icon={Gauge} title="Measurement and challenge">
          <div className="chipRows">
            <ChipRow label="Objectives" values={manifest.search?.objectives?.map((item) => item.name)} />
            <ChipRow label="Trajectory measures" values={manifest.trajectory?.map(m=>`${m.name} (${m.reducer})`)}/>
            <ChipRow label="Observables" values={manifest.observables?.map((item) => item.name)} />
            <ChipRow label="Constraints" values={manifest.constraints?.map((item) => item.name)} />
            <ChipRow label="Falsification" values={manifest.falsification?.map((item) => item.name)} />
          </div>
        </ManifestSection>
      </div>

      {!!manifest.limitations?.length && (
        <div className="limitationBox">
          <strong>Declared limitations</strong>
          <ul>{manifest.limitations.map((item) => <li key={item}>{item}</li>)}</ul>
        </div>
      )}

      <div className="rawManifest">
        <button type="button" className="rawManifest__toggle" onClick={() => setRawOpen((value) => !value)}>
          <Braces size={14} /> Raw immutable manifest <ChevronDown size={14} className={rawOpen ? "rotated" : ""} />
        </button>
        {rawOpen && (
          <div className="codeBlockWrap">
            <button
              type="button"
              className="copyButton"
              onClick={() => navigator.clipboard?.writeText(JSON.stringify(manifest, null, 2))}
              title="Copy JSON"
            >
              <Clipboard size={14} />
            </button>
            <pre>{JSON.stringify(manifest, null, 2)}</pre>
          </div>
        )}
      </div>
    </section>
  );
}

function Fact({ icon: Icon, label, value }) {
  return <article><Icon size={16} /><div><span>{label}</span><strong>{value}</strong></div></article>;
}
function ManifestSection({ icon: Icon, title, children }) {
  return <article className="manifestSection"><header><Icon size={15} /><strong>{title}</strong></header>{children}</article>;
}
function ChipRow({ label, values = [] }) {
  return <div><span>{label}</span><div>{values.length ? values.map((value) => <i key={value}>{value}</i>) : <em>None</em>}</div></div>;
}
function ExpressionList({ manifest }) {
  if (manifest.model?.kind === "state_vector_ode") {
    return <div className="expressionList">{manifest.model.derivatives?.map((item) => <code key={item.variable}>d{item.variable}/dt = {item.expression}</code>)}</div>;
  }
  if (manifest.model?.kind === "pairwise_particles") {
    return <div className="expressionList"><code>radial force = {manifest.model.interaction?.radial_force}</code>{manifest.model.interaction?.external_acceleration?.map((value, index) => <code key={`${index}-${value}`}>a[{index}] = {value}</code>)}</div>;
  }
  return <p>No executable expressions.</p>;
}
function summarizeModel(model) {
  if (!model) return [];
  if (model.kind === "state_vector_ode") {
    return [
      ["State variables", model.variables?.map((item) => item.name).join(", ") || "—"],
      ["Equations", String(model.derivatives?.length || 0)],
    ];
  }
  if (model.kind === "pairwise_particles") {
    return [
      ["Dimensions", String(model.dimensions)],
      ["Population", `${model.population?.count?.toLocaleString?.() || "—"} entities`],
      ["Boundary", statusLabel(model.boundary?.kind)],
      ["Cutoff", model.interaction?.cutoff == null ? "None" : compactNumber(model.interaction.cutoff)],
    ];
  }
  return [["Kind", model.kind || "Unknown"]];
}
