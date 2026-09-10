import {
  Atom,
  CheckCircle2,
  ChevronDown,
  CircleDot,
  Cpu,
  Dna,
  FlaskConical,
  Layers3,
  RefreshCw,
  Route,
  ShieldAlert,
  Trash2,
  Upload,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "@/lib/api";
import MoleculeViewport from "./MoleculeViewport";

const number = new Intl.NumberFormat(undefined, { maximumFractionDigits: 3 });

export default function MoleculeWorkbench({
  project,
  structures,
  selectedStructureId,
  onSelectStructure,
  onImported,
  onDeleted,
  engines,
  onRefreshEngines,
}) {
  const fileRef = useRef(null);
  const [representation, setRepresentation] = useState("surface");
  const [plans, setPlans] = useState([]);
  const [campaigns, setCampaigns] = useState([]);
  const [selectedPlanId, setSelectedPlanId] = useState("");
  const [seedAtomIds, setSeedAtomIds] = useState([]);
  const [ligandId, setLigandId] = useState("");
  const [residues, setResidues] = useState("");
  const [radius, setRadius] = useState(5);
  const [maxAtoms, setMaxAtoms] = useState(400);
  const [netCharge, setNetCharge] = useState(0);
  const [multiplicity, setMultiplicity] = useState(1);
  const [preserveResidues, setPreserveResidues] = useState(true);
  const [includeSolvent, setIncludeSolvent] = useState(false);
  const [campaignGoal, setCampaignGoal] = useState(
    "Characterize this structure, identify uncertainty, and plan progressively higher-fidelity computational validation.",
  );
  const [candidateCount, setCandidateCount] = useState(64);
  const [thorough, setThorough] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

  const structure = useMemo(
    () => structures.find((item) => item.id === selectedStructureId) || structures[0] || null,
    [selectedStructureId, structures],
  );
  const selectedPlan = useMemo(
    () => plans.find((item) => item.id === selectedPlanId) || plans[0] || null,
    [plans, selectedPlanId],
  );
  const highlightedAtomIds = useMemo(
    () => selectedPlan?.selected_atom_ids || seedAtomIds,
    [seedAtomIds, selectedPlan],
  );

  const loadStructureArtifacts = useCallback(async (id) => {
    if (!id) {
      setPlans([]);
      setCampaigns([]);
      return;
    }
    try {
      const [planResponse, campaignResponse] = await Promise.all([
        api.qmmmPlans(id),
        api.campaigns(id),
      ]);
      const nextPlans = planResponse.plans || [];
      setPlans(nextPlans);
      setCampaigns(campaignResponse.campaigns || []);
      setSelectedPlanId((current) =>
        nextPlans.some((item) => item.id === current) ? current : nextPlans[0]?.id || "",
      );
    } catch (value) {
      setError(value.message);
    }
  }, []);

  useEffect(() => {
    if (!structure?.id) return;
    setLigandId(structure.ligands?.[0]?.id || "");
    setSeedAtomIds([]);
    loadStructureArtifacts(structure.id);
  }, [structure?.id, loadStructureArtifacts]);

  const importFiles = async (files) => {
    const file = Array.from(files || [])[0];
    if (!file || !project) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      if (file.size > 16 * 1024 * 1024) {
        throw new Error("Structure files are limited to 16 MiB in this release.");
      }
      const content = await file.text();
      const imported = await api.importMolecule({
        project_id: project.id,
        name: file.name,
        format: "auto",
        content,
      });
      setNotice(
        `${file.name} imported: ${imported.atoms.length.toLocaleString()} atoms and ${imported.bonds.length.toLocaleString()} bonds.`,
      );
      await onImported?.(imported);
      onSelectStructure?.(imported.id);
    } catch (value) {
      setError(value.message);
    } finally {
      setBusy(false);
    }
  };

  const createPlan = async () => {
    if (!structure) return;
    const residueValues = residues
      .split(/[,\n]/)
      .map((item) => item.trim())
      .filter(Boolean);
    if (!ligandId && !residueValues.length && !seedAtomIds.length) {
      setError(
        "Select a ligand, enter residue keys, or click one or more atoms before creating a QM/MM region.",
      );
      return;
    }
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const plan = await api.createQmmmPlan(structure.id, {
        ligand_id: ligandId || null,
        residues: residueValues,
        atom_ids: seedAtomIds,
        center: null,
        radius: Number(radius),
        include_solvent: includeSolvent,
        preserve_residues: preserveResidues,
        max_atoms: Number(maxAtoms),
        net_charge: Number(netCharge),
        multiplicity: Number(multiplicity),
      });
      setPlans((current) => [plan, ...current.filter((item) => item.id !== plan.id)]);
      setSelectedPlanId(plan.id);
      setNotice(
        `QM/MM region plan created with ${plan.selected_atom_ids.length.toLocaleString()} atoms. Review every warning before execution.`,
      );
    } catch (value) {
      setError(value.message);
    } finally {
      setBusy(false);
    }
  };

  const createCampaign = async () => {
    if (!structure) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const campaign = await api.createCampaign(structure.id, {
        goal: campaignGoal,
        qmmm_plan_id: selectedPlan?.id || null,
        candidate_count: Number(candidateCount),
        thorough,
      });
      setCampaigns((current) => [campaign, ...current.filter((item) => item.id !== campaign.id)]);
      setNotice(
        "Computational campaign planned. Stages marked blocked require an external scientific engine; PhaseForge has not fabricated results for them.",
      );
    } catch (value) {
      setError(value.message);
    } finally {
      setBusy(false);
    }
  };

  const deleteStructure = async () => {
    if (
      !structure ||
      !window.confirm(`Delete ${structure.name} and its QM/MM plans and campaigns?`)
    ) {
      return;
    }
    setBusy(true);
    try {
      await api.deleteMolecule(structure.id);
      await onDeleted?.(structure.id);
      setNotice("Structure deleted.");
    } catch (value) {
      setError(value.message);
    } finally {
      setBusy(false);
    }
  };

  const toggleSeedAtom = useCallback((atomId) => {
    setSelectedPlanId("");
    setSeedAtomIds((current) =>
      current.includes(atomId)
        ? current.filter((item) => item !== atomId)
        : [...current, atomId],
    );
  }, []);

  return (
    <div className="moleculeWorkbench">
      <section className="moleculeWorkbench__main">
        <header className="moleculeToolbar">
          <div className="moleculePicker">
            <span className="moleculePicker__mark"><Dna size={16} /></span>
            <label>
              <span>Structure</span>
              <div>
                <select
                  value={structure?.id || ""}
                  onChange={(event) => onSelectStructure?.(event.target.value)}
                  disabled={!structures.length}
                >
                  {!structures.length && <option value="">No structures imported</option>}
                  {structures.map((item) => (
                    <option key={item.id} value={item.id}>{item.name}</option>
                  ))}
                </select>
                <ChevronDown size={13} />
              </div>
            </label>
          </div>
          <div className="moleculeToolbar__actions">
            <select
              value={representation}
              onChange={(event) => setRepresentation(event.target.value)}
              aria-label="Molecular representation"
            >
              <option value="surface">Molecular surface</option>
              <option value="ribbon">Backbone ribbon</option>
              <option value="ball_and_stick">Ball and stick</option>
              <option value="space_filling">Space filling</option>
            </select>
            <input
              ref={fileRef}
              hidden
              type="file"
              accept=".pdb,.sdf,.mol,.xyz,chemical/x-pdb,chemical/x-mdl-sdfile"
              onChange={(event) => {
                importFiles(event.target.files);
                event.target.value = "";
              }}
            />
            <button
              type="button"
              className="button button--primary"
              onClick={() => fileRef.current?.click()}
              disabled={!project || busy}
            >
              <Upload size={14} /> Import structure
            </button>
            {structure && (
              <button
                type="button"
                className="iconButton moleculeDelete"
                onClick={deleteStructure}
                disabled={busy}
                title="Delete structure"
              >
                <Trash2 size={14} />
              </button>
            )}
          </div>
        </header>

        <div className="moleculeCanvasShell">
          <MoleculeViewport
            structure={structure}
            selectedAtomIds={highlightedAtomIds}
            representation={representation}
            onAtomSelect={toggleSeedAtom}
          />
        </div>

        {structure && (
          <div className="moleculeMetricStrip">
            <Metric label="Atoms" value={structure.diagnostics.atom_count.toLocaleString()} />
            <Metric label="Heavy atoms" value={structure.diagnostics.heavy_atom_count.toLocaleString()} />
            <Metric label="Mass" value={`${number.format(structure.diagnostics.molecular_mass_da)} Da`} />
            <Metric
              label="Radius of gyration"
              value={`${number.format(structure.diagnostics.radius_of_gyration)} Å`}
            />
            <Metric label="Ligands" value={structure.diagnostics.ligand_count} />
            <Metric
              label="Close contacts"
              value={structure.diagnostics.suspicious_close_contacts}
              tone={structure.diagnostics.suspicious_close_contacts ? "warning" : "normal"}
            />
          </div>
        )}
      </section>

      <aside className="moleculeWorkbench__inspector">
        {!structure ? (
          <EmptyMolecule onImport={() => fileRef.current?.click()} />
        ) : (
          <>
            <section className="moleculeInspectorSection">
              <header>
                <Atom size={14} />
                <div>
                  <strong>Normalized molecular record</strong>
                  <span>Shared by the 3D viewport and research agents</span>
                </div>
              </header>
              <dl className="moleculeFacts">
                <div><dt>Format</dt><dd>{structure.format}</dd></div>
                <div><dt>Formula</dt><dd>{structure.diagnostics.formula || "Unknown"}</dd></div>
                <div><dt>Residues</dt><dd>{structure.diagnostics.residue_count}</dd></div>
                <div><dt>Chains</dt><dd>{structure.diagnostics.chain_count}</dd></div>
                <div><dt>H-bond contacts</dt><dd>{structure.diagnostics.approximate_hbond_contacts}</dd></div>
                <div><dt>Rotatable bonds</dt><dd>{structure.diagnostics.approximate_rotatable_bonds}</dd></div>
              </dl>
              {(structure.warnings?.length > 0 || structure.diagnostics.lipinski_flags?.length > 0) && (
                <div className="moleculeWarnings">
                  {[...(structure.warnings || []), ...(structure.diagnostics.lipinski_flags || [])]
                    .map((warning, index) => (
                      <div key={`${warning}-${index}`}>
                        <ShieldAlert size={13} />
                        <span>{warning}</span>
                      </div>
                    ))}
                </div>
              )}
            </section>

            <section className="moleculeInspectorSection">
              <header>
                <Layers3 size={14} />
                <div>
                  <strong>QM/MM region planner</strong>
                  <span>Defines a candidate electronic region; it does not execute quantum chemistry</span>
                </div>
              </header>
              <div className="formGrid">
                <label className="formSpan">
                  <span>Seed ligand</span>
                  <select
                    value={ligandId}
                    onChange={(event) => {
                      setLigandId(event.target.value);
                      setSelectedPlanId("");
                    }}
                  >
                    <option value="">No ligand seed</option>
                    {(structure.ligands || []).map((ligand) => (
                      <option key={ligand.id} value={ligand.id}>
                        {ligand.name} · {ligand.chain_id || "-"}:{ligand.residue_id || "-"} · {ligand.atom_ids.length} atoms
                      </option>
                    ))}
                  </select>
                </label>
                <label className="formSpan">
                  <span>Residue keys</span>
                  <input
                    value={residues}
                    onChange={(event) => {
                      setResidues(event.target.value);
                      setSelectedPlanId("");
                    }}
                    placeholder="A:128:HIS, A:131:ASP"
                  />
                </label>
                <label>
                  <span>Radius (Å)</span>
                  <input
                    type="number"
                    min="0.5"
                    max="25"
                    step="0.5"
                    value={radius}
                    onChange={(event) => setRadius(event.target.value)}
                  />
                </label>
                <label>
                  <span>Atom ceiling</span>
                  <input
                    type="number"
                    min="1"
                    max="5000"
                    value={maxAtoms}
                    onChange={(event) => setMaxAtoms(event.target.value)}
                  />
                </label>
                <label>
                  <span>Net charge</span>
                  <input
                    type="number"
                    min="-30"
                    max="30"
                    value={netCharge}
                    onChange={(event) => setNetCharge(event.target.value)}
                  />
                </label>
                <label>
                  <span>Multiplicity</span>
                  <input
                    type="number"
                    min="1"
                    max="20"
                    value={multiplicity}
                    onChange={(event) => setMultiplicity(event.target.value)}
                  />
                </label>
              </div>
              <div className="moleculeToggles">
                <label>
                  <input
                    type="checkbox"
                    checked={preserveResidues}
                    onChange={(event) => setPreserveResidues(event.target.checked)}
                  />
                  Preserve whole residues
                </label>
                <label>
                  <input
                    type="checkbox"
                    checked={includeSolvent}
                    onChange={(event) => setIncludeSolvent(event.target.checked)}
                  />
                  Include nearby solvent
                </label>
              </div>
              {seedAtomIds.length > 0 && (
                <div className="seedAtoms">
                  <CircleDot size={13} />
                  <span>
                    {seedAtomIds.length} clicked atom{seedAtomIds.length === 1 ? "" : "s"} used as seeds
                  </span>
                  <button type="button" onClick={() => setSeedAtomIds([])}>Clear</button>
                </div>
              )}
              <button
                type="button"
                className="button button--secondary button--full"
                onClick={createPlan}
                disabled={busy}
              >
                <Layers3 size={14} /> Create region plan
              </button>
              {plans.length > 0 && (
                <label className="planPicker">
                  <span>Saved region plan</span>
                  <select
                    value={selectedPlan?.id || ""}
                    onChange={(event) => setSelectedPlanId(event.target.value)}
                  >
                    {plans.map((plan) => (
                      <option key={plan.id} value={plan.id}>
                        {plan.selected_atom_ids.length} atoms · charge {plan.net_charge} · multiplicity {plan.multiplicity}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              {selectedPlan && (
                <div className="regionSummary">
                  <strong>{selectedPlan.selected_atom_ids.length.toLocaleString()} atoms highlighted</strong>
                  <span>Estimated electrons: {selectedPlan.estimated_electrons.toLocaleString()}</span>
                  {selectedPlan.warnings.map((warning, index) => (
                    <p key={`${warning}-${index}`}><ShieldAlert size={12} /> {warning}</p>
                  ))}
                </div>
              )}
            </section>

            <section className="moleculeInspectorSection">
              <header>
                <Route size={14} />
                <div>
                  <strong>Computational campaign</strong>
                  <span>Plans evidence gates across installed open scientific engines</span>
                </div>
              </header>
              <label className="stackedField">
                <span>Scientific goal</span>
                <textarea
                  rows={4}
                  value={campaignGoal}
                  onChange={(event) => setCampaignGoal(event.target.value)}
                />
              </label>
              <div className="formGrid">
                <label>
                  <span>Candidate budget</span>
                  <input
                    type="number"
                    min="1"
                    max="1000000"
                    value={candidateCount}
                    onChange={(event) => setCandidateCount(event.target.value)}
                  />
                </label>
                <label className="campaignThorough">
                  <span>Depth</span>
                  <select
                    value={thorough ? "thorough" : "screening"}
                    onChange={(event) => setThorough(event.target.value === "thorough")}
                  >
                    <option value="screening">Screening</option>
                    <option value="thorough">Thorough</option>
                  </select>
                </label>
              </div>
              <button
                type="button"
                className="button button--primary button--full"
                onClick={createCampaign}
                disabled={busy || !campaignGoal.trim()}
              >
                <FlaskConical size={14} /> Plan campaign
              </button>
              {campaigns[0] && <Campaign campaign={campaigns[0]} />}
            </section>

            <section className="moleculeInspectorSection">
              <header>
                <Cpu size={14} />
                <div>
                  <strong>Scientific engines</strong>
                  <span>Detected locally; detection does not manufacture scientific readiness</span>
                </div>
                <button
                  type="button"
                  className="iconButton"
                  onClick={onRefreshEngines}
                  title="Refresh engine discovery"
                >
                  <RefreshCw size={13} />
                </button>
              </header>
              <div className="engineList">
                {engines.map((engine) => (
                  <div key={engine.id} className={engine.available ? "available" : "missing"}>
                    {engine.available ? <CheckCircle2 size={13} /> : <CircleDot size={13} />}
                    <div>
                      <strong>{engine.name}</strong>
                      <span>
                        {engine.available
                          ? engine.version || engine.executable || "Detected"
                          : engine.install_hint}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            </section>
          </>
        )}
      </aside>

      {(notice || error) && (
        <div className={`moleculeToast ${error ? "moleculeToast--error" : ""}`}>
          {error ? <ShieldAlert size={14} /> : <CheckCircle2 size={14} />}
          <span>{error || notice}</span>
          <button type="button" onClick={() => { setError(""); setNotice(""); }}>×</button>
        </div>
      )}
    </div>
  );
}

function Metric({ label, value, tone = "normal" }) {
  return (
    <div className={tone === "warning" ? "warning" : ""}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function EmptyMolecule({ onImport }) {
  return (
    <div className="moleculeEmpty">
      <Dna size={30} />
      <strong>Bring a molecular world into PhaseForge</strong>
      <p>
        Import a PDB, SDF/MOL V2000, or XYZ file. PhaseForge parses a normalized record,
        exposes deterministic diagnostics, and lets agents reason over the same structure you see.
      </p>
      <button type="button" className="button button--primary" onClick={onImport}>
        <Upload size={14} /> Import structure
      </button>
    </div>
  );
}

function Campaign({ campaign }) {
  return (
    <div className="campaignPlan">
      <header>
        <strong>{campaign.goal}</strong>
        <span>{campaign.candidate_count.toLocaleString()} candidate budget</span>
      </header>
      {campaign.stages.map((stage, index) => (
        <article key={stage.id}>
          <i>{index + 1}</i>
          <div>
            <strong>{stage.title}</strong>
            <p>{stage.description}</p>
            <span>
              {stage.required_engines.length
                ? stage.required_engines.join(" · ")
                : "PhaseForge local diagnostics"}
              {` · ${stage.compute_profile}`}
            </span>
          </div>
          <em className={`readiness readiness--${stage.readiness}`}>{stage.readiness}</em>
        </article>
      ))}
      {campaign.warnings.map((warning, index) => (
        <p className="campaignWarning" key={`${warning}-${index}`}>
          <ShieldAlert size={12} /> {warning}
        </p>
      ))}
    </div>
  );
}
