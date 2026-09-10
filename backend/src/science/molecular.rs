use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use anyhow::{Context, bail};
use chrono::Utc;
use uuid::Uuid;

use crate::domain::{
    CampaignPlanRequest, CampaignStage, ComputationalCampaign, ImportStructureRequest, LigandGroup,
    MolecularAtom, MolecularBond, MolecularDiagnostics, MolecularFormat, MolecularStructure,
    QmmmRegionPlan, QmmmRegionRequest, ScientificEngineStatus,
};

const MAX_STRUCTURE_BYTES: usize = 16 * 1024 * 1024;
const MAX_ATOMS: usize = 100_000;
const MAX_INFERRED_BOND_ATOMS: usize = 12_000;

pub fn import_structure(request: ImportStructureRequest) -> anyhow::Result<MolecularStructure> {
    let name = request.name.trim();
    if name.is_empty() {
        bail!("structure name is required");
    }
    if request.content.is_empty() {
        bail!("structure content is empty");
    }
    if request.content.len() > MAX_STRUCTURE_BYTES {
        bail!("structure exceeds the 16 MiB text-import limit");
    }

    let format = resolve_format(request.format, name, &request.content)?;
    let (atoms, mut bonds, mut warnings) = match format {
        MolecularFormat::Pdb => parse_pdb(&request.content)?,
        MolecularFormat::Sdf | MolecularFormat::Mol => parse_mol_v2000(&request.content)?,
        MolecularFormat::Xyz => parse_xyz(&request.content)?,
        MolecularFormat::Auto => unreachable!("auto is resolved before parsing"),
    };
    if atoms.is_empty() {
        bail!("no atoms were found in the supplied structure");
    }
    if atoms.len() > MAX_ATOMS {
        bail!("structure contains more than {MAX_ATOMS} atoms");
    }

    if bonds.is_empty() {
        if atoms.len() <= MAX_INFERRED_BOND_ATOMS {
            bonds = infer_bonds(&atoms);
            warnings.push(
                "Connectivity was absent, so bonds were inferred from geometry. Confirm bond orders and coordination before quantitative chemistry."
                    .to_owned(),
            );
        } else {
            warnings.push(format!(
                "Connectivity was absent and bond inference was skipped above {MAX_INFERRED_BOND_ATOMS} atoms."
            ));
        }
    }
    deduplicate_bonds(&mut bonds);

    let ligands = identify_ligands(&atoms);
    let diagnostics = analyze_structure(&atoms, &bonds, &ligands, &mut warnings);

    Ok(MolecularStructure {
        id: Uuid::new_v4(),
        project_id: request.project_id,
        name: name.to_owned(),
        format,
        atoms,
        bonds,
        ligands,
        diagnostics,
        warnings,
        created_at: Utc::now(),
    })
}

pub fn plan_qmmm_region(
    structure: &MolecularStructure,
    request: QmmmRegionRequest,
) -> anyhow::Result<QmmmRegionPlan> {
    if !request.radius.is_finite() || !(0.5..=25.0).contains(&request.radius) {
        bail!("QM/MM radius must be between 0.5 and 25 angstrom");
    }
    if !(1..=10_000).contains(&request.max_atoms) {
        bail!("QM/MM max_atoms must be between 1 and 10,000");
    }
    if !(1..=20).contains(&request.multiplicity) {
        bail!("multiplicity must be between 1 and 20");
    }

    let atom_by_id = structure
        .atoms
        .iter()
        .map(|atom| (atom.id, atom))
        .collect::<HashMap<_, _>>();
    let mut seed_ids = request.atom_ids.iter().copied().collect::<BTreeSet<_>>();
    for atom_id in &seed_ids {
        if !atom_by_id.contains_key(atom_id) {
            bail!("seed atom {atom_id} was not found in the structure");
        }
    }

    if let Some(ligand_id) = request
        .ligand_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let ligand = structure
            .ligands
            .iter()
            .find(|value| value.id.eq_ignore_ascii_case(ligand_id))
            .with_context(|| format!("ligand '{ligand_id}' was not found"))?;
        seed_ids.extend(ligand.atom_ids.iter().copied());
    }

    let requested_residues = request
        .residues
        .iter()
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    for atom in &structure.atoms {
        let full = residue_key(atom).to_ascii_uppercase();
        let short = format!("{}:{}", atom.chain_id, atom.residue_id).to_ascii_uppercase();
        if requested_residues.contains(&full) || requested_residues.contains(&short) {
            seed_ids.insert(atom.id);
        }
    }

    let mut seed_positions = seed_ids
        .iter()
        .filter_map(|id| atom_by_id.get(id).map(|atom| atom.position))
        .collect::<Vec<_>>();
    if let Some(center) = request.center {
        seed_positions.push(center);
    }
    if seed_positions.is_empty() {
        bail!("select a ligand, residue, atom, or 3D center before planning a QM region");
    }

    let radius_sq = request.radius * request.radius;
    let mut selected = BTreeSet::new();
    for atom in &structure.atoms {
        if !request.include_solvent && is_solvent(&atom.residue_name) {
            continue;
        }
        if seed_positions
            .iter()
            .any(|position| squared_distance(atom.position, *position) <= radius_sq)
        {
            selected.insert(atom.id);
        }
    }

    if request.preserve_residues {
        let selected_residues = selected
            .iter()
            .filter_map(|id| atom_by_id.get(id).map(|atom| residue_key(atom)))
            .filter(|value| !value.is_empty())
            .collect::<HashSet<_>>();
        for atom in &structure.atoms {
            if selected_residues.contains(&residue_key(atom))
                && (request.include_solvent || !is_solvent(&atom.residue_name))
            {
                selected.insert(atom.id);
            }
        }
    }

    let reference = mean_position(&seed_positions);
    let mut warnings = Vec::new();
    if selected.len() > request.max_atoms {
        let mut ranked = selected
            .iter()
            .filter_map(|id| {
                atom_by_id
                    .get(id)
                    .map(|atom| (squared_distance(atom.position, reference), *id))
            })
            .collect::<Vec<_>>();
        ranked.sort_by(|left, right| left.0.total_cmp(&right.0));
        selected = ranked
            .into_iter()
            .take(request.max_atoms)
            .map(|(_, id)| id)
            .collect();
        warnings.push(format!(
            "The requested region exceeded {} atoms and was trimmed by distance. Review residue and covalent boundaries manually.",
            request.max_atoms
        ));
    }

    let selected_atom_ids = selected.into_iter().collect::<Vec<_>>();
    if selected_atom_ids.is_empty() {
        bail!("the requested QM/MM region selected no atoms");
    }
    let selected_residues = selected_atom_ids
        .iter()
        .filter_map(|id| atom_by_id.get(id).map(|atom| residue_key(atom)))
        .filter(|value| !value.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let estimated_electrons = selected_atom_ids
        .iter()
        .filter_map(|id| atom_by_id.get(id))
        .map(|atom| i64::from(atomic_number(&atom.element)))
        .sum::<i64>()
        - i64::from(request.net_charge);

    if !structure.atoms.iter().any(|atom| atom.element == "H") {
        warnings.push(
            "No explicit hydrogens were detected. Protonation and electron count must be corrected before quantum work."
                .to_owned(),
        );
    }
    let parity_is_consistent = (estimated_electrons.rem_euclid(2) == 0
        && request.multiplicity % 2 == 1)
        || (estimated_electrons.rem_euclid(2) == 1 && request.multiplicity % 2 == 0);
    if !parity_is_consistent {
        warnings.push(
            "Electron-count parity may be inconsistent with the selected multiplicity. Confirm charge and spin state."
                .to_owned(),
        );
    }
    warnings.push(
        "This is a region-selection plan, not a quantum calculation. Link atoms, embedding, basis, functional, charge, and boundary treatment still require method selection and convergence checks."
            .to_owned(),
    );

    Ok(QmmmRegionPlan {
        id: Uuid::new_v4(),
        structure_id: structure.id,
        selected_atom_ids,
        selected_residues,
        estimated_electrons,
        net_charge: request.net_charge,
        multiplicity: request.multiplicity,
        radius: request.radius,
        warnings,
        created_at: Utc::now(),
    })
}

pub fn plan_campaign(
    structure: &MolecularStructure,
    request: CampaignPlanRequest,
    engines: &[ScientificEngineStatus],
) -> anyhow::Result<ComputationalCampaign> {
    let goal = request.goal.trim();
    if goal.len() < 8 {
        bail!("campaign goal must contain at least eight characters");
    }
    if request.candidate_count == 0 || request.candidate_count > 1_000_000 {
        bail!("candidate_count must be between 1 and 1,000,000");
    }
    let available = engines
        .iter()
        .filter(|engine| engine.available)
        .map(|engine| engine.id.as_str())
        .collect::<HashSet<_>>();
    let readiness = |required: &[&str]| {
        if required.is_empty() || required.iter().any(|id| available.contains(id)) {
            "ready"
        } else {
            "engine_required"
        }
    };
    let stage = |id: &str,
                 title: &str,
                 description: &str,
                 required: &[&str],
                 compute: &str,
                 evidence: &str| CampaignStage {
        id: id.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        required_engines: required.iter().map(|value| (*value).to_owned()).collect(),
        readiness: readiness(required).to_owned(),
        compute_profile: compute.to_owned(),
        evidence_gate: evidence.to_owned(),
    };

    let stages = vec![
        stage(
            "structure_validation",
            "Structure validation",
            "Review connectivity, missing atoms, clashes, alternate conformations, protonation, charge, and biological-assembly assumptions.",
            &[],
            "CPU; deterministic diagnostics",
            "No unresolved parser or geometry defect invalidates downstream work.",
        ),
        stage(
            "chemical_preparation",
            "Chemical preparation",
            "Create explicit protonation, tautomer, charge, solvent, ion, and force-field states instead of treating imported coordinates as simulation-ready.",
            &["openbabel", "openmm", "gromacs"],
            "CPU; parallel states",
            "Every retained state has explicit chemistry and reproducible preparation assumptions.",
        ),
        stage(
            "candidate_screening",
            "Candidate and pose screening",
            "Enumerate bounded candidate or pose variants and reject impossible chemistry before expensive dynamics. PhaseForge does not invent docking scores without an attached engine.",
            &["openbabel"],
            "CPU/GPU depending on attached engine",
            "Retained candidates satisfy structural constraints and retain uncertainty labels.",
        ),
        stage(
            "molecular_dynamics",
            "Molecular-dynamics challenge",
            "Run replicated solvated trajectories across initial velocities and preparation states, measuring stability, contacts, drift, and failure modes.",
            &["openmm", "gromacs"],
            if request.thorough {
                "Multi-GPU ensemble; hours to days"
            } else {
                "Single-GPU ensemble; hours"
            },
            "Behavior survives independent seeds, adequate equilibration, and trajectory-length checks.",
        ),
        stage(
            "higher_fidelity_scoring",
            "Higher-fidelity scoring",
            "Promote only candidates that survive dynamics into more expensive free-energy or rescoring work with explicit method limits.",
            &["openmm", "gromacs"],
            "GPU ensemble plus CPU analysis",
            "Rankings remain stable across replicas and plausible preparation alternatives.",
        ),
        stage(
            "qmmm_refinement",
            "Quantum or QM/MM refinement",
            "Evaluate selected electronic effects in a bounded region while treating the wider environment classically.",
            &["cp2k", "xtb"],
            "CPU/GPU HPC; targeted regions",
            "Conclusions survive region, charge, multiplicity, basis, and method challenges.",
        ),
        stage(
            "adversarial_validation",
            "Adversarial validation",
            "Perturb protonation, pose, solvent, temperature, force field, region boundary, and numerical settings to search for failure modes.",
            &["openmm", "gromacs", "cp2k", "xtb"],
            "Parallel uncertainty portfolio",
            "The apparent effect survives predefined attempts to destroy it.",
        ),
        stage(
            "independent_reproduction",
            "Independent reproduction",
            "Recreate decisive evidence using a second engine or independently generated setup before proposing wet-lab prioritization.",
            &["openmm", "gromacs", "cp2k", "lammps"],
            "Independent compute lane",
            "A separate implementation reaches a materially consistent conclusion.",
        ),
    ];

    let mut warnings = vec![
        "This campaign prioritizes computational evidence; it does not establish safety, efficacy, synthesizability, novelty, or clinical benefit. Controlled laboratory validation remains required."
            .to_owned(),
    ];
    if structure.diagnostics.suspicious_close_contacts > 0 {
        warnings.push("Resolve suspicious close contacts before energy-based ranking.".to_owned());
    }
    if !available.contains("openmm") && !available.contains("gromacs") {
        warnings.push("No OpenMM or GROMACS installation was detected, so dynamics stages require an external engine.".to_owned());
    }
    if request.qmmm_plan_id.is_some()
        && !available.contains("cp2k")
        && !available.contains("xtb")
    {
        warnings.push("A QM/MM plan is selected, but neither CP2K nor xTB was detected.".to_owned());
    }

    Ok(ComputationalCampaign {
        id: Uuid::new_v4(),
        structure_id: structure.id,
        goal: goal.to_owned(),
        candidate_count: request.candidate_count,
        thorough: request.thorough,
        qmmm_plan_id: request.qmmm_plan_id,
        stages,
        warnings,
        created_at: Utc::now(),
    })
}

fn resolve_format(
    requested: MolecularFormat,
    name: &str,
    content: &str,
) -> anyhow::Result<MolecularFormat> {
    if requested != MolecularFormat::Auto {
        return Ok(requested);
    }
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".pdb")
        || content
            .lines()
            .any(|line| line.starts_with("ATOM  ") || line.starts_with("HETATM"))
    {
        return Ok(MolecularFormat::Pdb);
    }
    if lower.ends_with(".sdf") || content.contains("$$$$") {
        return Ok(MolecularFormat::Sdf);
    }
    if lower.ends_with(".mol") || content.lines().nth(3).is_some_and(|line| line.contains("V2000")) {
        return Ok(MolecularFormat::Mol);
    }
    if lower.ends_with(".xyz")
        || content
            .lines()
            .next()
            .and_then(|line| line.trim().parse::<usize>().ok())
            .is_some()
    {
        return Ok(MolecularFormat::Xyz);
    }
    bail!("unable to detect molecular format; choose PDB, SDF/MOL V2000, or XYZ")
}

fn parse_pdb(content: &str) -> anyhow::Result<(Vec<MolecularAtom>, Vec<MolecularBond>, Vec<String>)> {
    let mut atoms = Vec::new();
    let mut serial_to_id = HashMap::new();
    let mut pending_bonds = Vec::new();
    let mut warnings = Vec::new();
    let mut model_index = 0usize;
    let mut in_first_model = true;
    let mut saw_model = false;
    let mut skipped_alternates = 0usize;

    for line in content.lines() {
        let record = column(line, 0, 6).trim();
        if record == "MODEL" {
            saw_model = true;
            model_index += 1;
            in_first_model = model_index == 1;
            continue;
        }
        if record == "ENDMDL" && in_first_model {
            break;
        }
        if saw_model && !in_first_model {
            continue;
        }
        if record == "ATOM" || record == "HETATM" {
            let alternate = column(line, 16, 17).trim();
            if !alternate.is_empty() && alternate != "A" && alternate != "1" {
                skipped_alternates += 1;
                continue;
            }
            let serial = column(line, 6, 11).trim().parse::<i32>().ok();
            let atom_name = column(line, 12, 16).trim().to_owned();
            let residue_name = column(line, 17, 20).trim().to_owned();
            let chain_id = column(line, 21, 22).trim().to_owned();
            let residue_id = format!(
                "{}{}",
                column(line, 22, 26).trim(),
                column(line, 26, 27).trim()
            );
            let position = [
                parse_pdb_number(line, 30, 38, "x coordinate")?,
                parse_pdb_number(line, 38, 46, "y coordinate")?,
                parse_pdb_number(line, 46, 54, "z coordinate")?,
            ];
            let element_field = column(line, 76, 78).trim();
            let element = if element_field.is_empty() {
                infer_pdb_element(&atom_name, &residue_name, record == "ATOM")
            } else {
                normalize_element(element_field)
            };
            let id = atoms.len() as u32 + 1;
            if let Some(serial) = serial {
                serial_to_id.insert(serial, id);
            }
            atoms.push(MolecularAtom {
                id,
                serial,
                name: atom_name,
                element,
                position,
                residue_name,
                residue_id,
                chain_id,
                hetero: record == "HETATM",
                formal_charge: parse_pdb_charge(column(line, 78, 80)),
            });
        } else if record == "CONECT" {
            let values = line
                .get(6..)
                .unwrap_or_default()
                .split_whitespace()
                .filter_map(|value| value.parse::<i32>().ok())
                .collect::<Vec<_>>();
            if let Some((source, targets)) = values.split_first() {
                for target in targets {
                    pending_bonds.push((*source, *target));
                }
            }
        }
    }

    if skipped_alternates > 0 {
        warnings.push(format!(
            "Skipped {skipped_alternates} alternate-location atom record(s); the default/first conformer was retained."
        ));
    }
    if saw_model {
        warnings.push("Only the first coordinate model in the PDB file was imported.".to_owned());
    }

    let bonds = pending_bonds
        .into_iter()
        .filter_map(|(source, target)| {
            Some((*serial_to_id.get(&source)?, *serial_to_id.get(&target)?))
        })
        .filter(|(left, right)| left != right)
        .map(|(atom_a, atom_b)| MolecularBond {
            atom_a,
            atom_b,
            order: 1.0,
            source: "pdb_conect".to_owned(),
        })
        .collect::<Vec<_>>();
    Ok((atoms, bonds, warnings))
}

fn parse_mol_v2000(
    content: &str,
) -> anyhow::Result<(Vec<MolecularAtom>, Vec<MolecularBond>, Vec<String>)> {
    let lines = content.lines().collect::<Vec<_>>();
    if lines.len() < 4 {
        bail!("SDF/MOL content is missing the V2000 counts line");
    }
    let counts = lines[3];
    if !counts.contains("V2000") {
        bail!("only SDF/MOL V2000 records are currently supported");
    }
    let atom_count = column(counts, 0, 3)
        .trim()
        .parse::<usize>()
        .context("invalid V2000 atom count")?;
    let bond_count = column(counts, 3, 6)
        .trim()
        .parse::<usize>()
        .context("invalid V2000 bond count")?;
    if atom_count == 0 || atom_count > MAX_ATOMS {
        bail!("V2000 atom count is outside the supported range");
    }
    if lines.len() < 4 + atom_count + bond_count {
        bail!("SDF/MOL record ended before all declared atoms and bonds were read");
    }

    let mut atoms = Vec::with_capacity(atom_count);
    for (index, line) in lines[4..4 + atom_count].iter().enumerate() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 {
            bail!("invalid V2000 atom line {}", index + 1);
        }
        atoms.push(MolecularAtom {
            id: index as u32 + 1,
            serial: Some(index as i32 + 1),
            name: fields[3].to_owned(),
            element: normalize_element(fields[3]),
            position: [
                fields[0].parse().context("invalid SDF x coordinate")?,
                fields[1].parse().context("invalid SDF y coordinate")?,
                fields[2].parse().context("invalid SDF z coordinate")?,
            ],
            residue_name: String::new(),
            residue_id: String::new(),
            chain_id: String::new(),
            hetero: true,
            formal_charge: None,
        });
    }

    let mut bonds = Vec::with_capacity(bond_count);
    for (index, line) in lines[4 + atom_count..4 + atom_count + bond_count]
        .iter()
        .enumerate()
    {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 3 {
            bail!("invalid V2000 bond line {}", index + 1);
        }
        let atom_a = fields[0]
            .parse::<u32>()
            .context("invalid SDF bond origin")?;
        let atom_b = fields[1]
            .parse::<u32>()
            .context("invalid SDF bond target")?;
        if atom_a == 0
            || atom_b == 0
            || atom_a as usize > atom_count
            || atom_b as usize > atom_count
            || atom_a == atom_b
        {
            bail!("SDF bond references an invalid atom index");
        }
        bonds.push(MolecularBond {
            atom_a,
            atom_b,
            order: fields[2].parse::<f32>().unwrap_or(1.0).clamp(1.0, 4.0),
            source: "sdf_v2000".to_owned(),
        });
    }

    Ok((
        atoms,
        bonds,
        vec![
            "V2000 stereochemistry, aromaticity, charge, and property blocks are only partially interpreted in this release. Use a dedicated cheminformatics engine before making chemical claims."
                .to_owned(),
        ],
    ))
}

fn parse_xyz(content: &str) -> anyhow::Result<(Vec<MolecularAtom>, Vec<MolecularBond>, Vec<String>)> {
    let mut lines = content.lines();
    let atom_count = lines
        .next()
        .context("XYZ file is empty")?
        .trim()
        .parse::<usize>()
        .context("invalid XYZ atom count")?;
    if atom_count == 0 || atom_count > MAX_ATOMS {
        bail!("XYZ atom count is outside the supported range");
    }
    let _comment = lines.next();
    let mut atoms = Vec::with_capacity(atom_count);
    for index in 0..atom_count {
        let line = lines
            .next()
            .with_context(|| format!("XYZ ended at atom {}", index + 1))?;
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 4 {
            bail!("invalid XYZ atom line {}", index + 1);
        }
        atoms.push(MolecularAtom {
            id: index as u32 + 1,
            serial: Some(index as i32 + 1),
            name: fields[0].to_owned(),
            element: normalize_element(fields[0]),
            position: [
                fields[1].parse().context("invalid XYZ x coordinate")?,
                fields[2].parse().context("invalid XYZ y coordinate")?,
                fields[3].parse().context("invalid XYZ z coordinate")?,
            ],
            residue_name: String::new(),
            residue_id: String::new(),
            chain_id: String::new(),
            hetero: true,
            formal_charge: None,
        });
    }
    Ok((
        atoms,
        Vec::new(),
        vec![
            "XYZ does not encode connectivity, bond order, residue identity, formal charge, or protonation."
                .to_owned(),
        ],
    ))
}

fn infer_bonds(atoms: &[MolecularAtom]) -> Vec<MolecularBond> {
    let mut bonds = Vec::new();
    for left_index in 0..atoms.len() {
        for right_index in left_index + 1..atoms.len() {
            let left = &atoms[left_index];
            let right = &atoms[right_index];
            if left.element == "H" && right.element == "H" {
                continue;
            }
            let distance = squared_distance(left.position, right.position).sqrt();
            let threshold =
                (1.25 * (covalent_radius(&left.element) + covalent_radius(&right.element)))
                    .clamp(0.7, 2.35);
            if distance >= 0.35 && distance <= threshold {
                bonds.push(MolecularBond {
                    atom_a: left.id,
                    atom_b: right.id,
                    order: 1.0,
                    source: "geometry_inferred".to_owned(),
                });
            }
        }
    }
    bonds
}

fn deduplicate_bonds(bonds: &mut Vec<MolecularBond>) {
    let mut seen = HashSet::new();
    bonds.retain(|bond| {
        if bond.atom_a == bond.atom_b {
            return false;
        }
        let key = ordered_pair(bond.atom_a, bond.atom_b);
        seen.insert(key)
    });
}

fn identify_ligands(atoms: &[MolecularAtom]) -> Vec<LigandGroup> {
    let mut groups = BTreeMap::<String, Vec<u32>>::new();
    for atom in atoms {
        if !atom.hetero || is_solvent(&atom.residue_name) || is_common_ion(&atom.residue_name) {
            continue;
        }
        let key = residue_key(atom);
        groups.entry(key).or_default().push(atom.id);
    }
    groups
        .into_iter()
        .filter(|(_, atom_ids)| atom_ids.len() >= 3)
        .map(|(id, atom_ids)| {
            let atom = atom_ids
                .first()
                .and_then(|atom_id| atoms.iter().find(|value| value.id == *atom_id));
            LigandGroup {
                id,
                name: atom.map(|value| value.residue_name.clone()).unwrap_or_default(),
                chain_id: atom.map(|value| value.chain_id.clone()).unwrap_or_default(),
                residue_id: atom.map(|value| value.residue_id.clone()).unwrap_or_default(),
                atom_ids,
            }
        })
        .collect()
}

fn analyze_structure(
    atoms: &[MolecularAtom],
    bonds: &[MolecularBond],
    ligands: &[LigandGroup],
    warnings: &mut Vec<String>,
) -> MolecularDiagnostics {
    let mut composition = BTreeMap::<String, usize>::new();
    let mut total_mass = 0.0;
    let mut weighted = [0.0; 3];
    let mut bounding_min = [f64::INFINITY; 3];
    let mut bounding_max = [f64::NEG_INFINITY; 3];
    for atom in atoms {
        *composition.entry(atom.element.clone()).or_default() += 1;
        let mass = atomic_mass(&atom.element);
        total_mass += mass;
        for axis in 0..3 {
            weighted[axis] += atom.position[axis] * mass;
            bounding_min[axis] = bounding_min[axis].min(atom.position[axis]);
            bounding_max[axis] = bounding_max[axis].max(atom.position[axis]);
        }
    }
    let center = if total_mass > 0.0 {
        [
            weighted[0] / total_mass,
            weighted[1] / total_mass,
            weighted[2] / total_mass,
        ]
    } else {
        mean_position(&atoms.iter().map(|atom| atom.position).collect::<Vec<_>>())
    };
    let radius_of_gyration = if total_mass > 0.0 {
        (atoms
            .iter()
            .map(|atom| atomic_mass(&atom.element) * squared_distance(atom.position, center))
            .sum::<f64>()
            / total_mass)
            .sqrt()
    } else {
        0.0
    };

    let adjacency = build_adjacency(atoms, bonds);
    let explicit_hydrogen = atoms.iter().any(|atom| atom.element == "H");
    if !explicit_hydrogen {
        warnings.push(
            "Hydrogens are not explicit; donor, acceptor, protonation, mass, and contact diagnostics are screening estimates."
                .to_owned(),
        );
    }
    let donor_ids = atoms
        .iter()
        .filter(|atom| {
            matches!(atom.element.as_str(), "N" | "O" | "S")
                && adjacency.get(atom.id as usize - 1).map_or(0, Vec::len) > 0
        })
        .map(|atom| atom.id)
        .collect::<HashSet<_>>();
    let acceptor_ids = atoms
        .iter()
        .filter(|atom| {
            matches!(atom.element.as_str(), "N" | "O" | "S" | "F" | "Cl" | "Br")
                && atom.formal_charge.unwrap_or(0) <= 0
        })
        .map(|atom| atom.id)
        .collect::<HashSet<_>>();
    let bonded = bonds
        .iter()
        .map(|bond| ordered_pair(bond.atom_a, bond.atom_b))
        .collect::<HashSet<_>>();
    let approximate_hbond_contacts = count_pairs(atoms, &donor_ids, &acceptor_ids, 2.2, 3.6, &bonded);
    let all_ids = atoms.iter().map(|atom| atom.id).collect::<HashSet<_>>();
    let suspicious_close_contacts = count_pairs(atoms, &all_ids, &all_ids, 0.01, 0.70, &bonded) / 2;
    let approximate_rotatable_bonds = bonds
        .iter()
        .filter(|bond| {
            (bond.order - 1.0).abs() < 0.2
                && adjacency.get(bond.atom_a as usize - 1).map_or(0, Vec::len) > 1
                && adjacency.get(bond.atom_b as usize - 1).map_or(0, Vec::len) > 1
        })
        .count();
    let approximate_ring_bonds = estimate_cycle_edges(atoms.len(), bonds);
    let residues = atoms
        .iter()
        .map(residue_key)
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    let chains = atoms
        .iter()
        .map(|atom| atom.chain_id.trim().to_owned())
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    let heavy_atom_count = atoms.iter().filter(|atom| atom.element != "H").count();
    let mut lipinski_flags = Vec::new();
    if atoms.len() <= 300 && residues.len() <= 3 {
        if total_mass > 500.0 {
            lipinski_flags.push("Approximate molecular mass exceeds 500 Da".to_owned());
        }
        if donor_ids.len() > 5 {
            lipinski_flags.push("Potential hydrogen-bond donors exceed 5".to_owned());
        }
        if acceptor_ids.len() > 10 {
            lipinski_flags.push("Potential hydrogen-bond acceptors exceed 10".to_owned());
        }
        if approximate_rotatable_bonds > 10 {
            lipinski_flags.push("Approximate rotatable-bond count exceeds 10".to_owned());
        }
        lipinski_flags.push(
            "LogP is not calculated; this is not a complete Rule-of-Five assessment".to_owned(),
        );
    }
    if suspicious_close_contacts > 0 {
        warnings.push(format!(
            "Detected {suspicious_close_contacts} suspicious nonbonded close-contact pair(s)."
        ));
    }

    MolecularDiagnostics {
        atom_count: atoms.len(),
        heavy_atom_count,
        bond_count: bonds.len(),
        residue_count: residues.len(),
        chain_count: chains.len(),
        ligand_count: ligands.len(),
        formula: hill_formula(&composition),
        molecular_mass_da: total_mass,
        center_of_mass: center,
        bounding_min,
        bounding_max,
        radius_of_gyration,
        potential_hbond_donors: donor_ids.len(),
        potential_hbond_acceptors: acceptor_ids.len(),
        approximate_hbond_contacts,
        approximate_rotatable_bonds,
        approximate_ring_bonds,
        suspicious_close_contacts,
        lipinski_flags,
    }
}

fn build_adjacency(atoms: &[MolecularAtom], bonds: &[MolecularBond]) -> Vec<Vec<u32>> {
    let mut adjacency = vec![Vec::new(); atoms.len()];
    for bond in bonds {
        if let Some(row) = adjacency.get_mut(bond.atom_a as usize - 1) {
            row.push(bond.atom_b);
        }
        if let Some(row) = adjacency.get_mut(bond.atom_b as usize - 1) {
            row.push(bond.atom_a);
        }
    }
    adjacency
}

fn count_pairs(
    atoms: &[MolecularAtom],
    left_ids: &HashSet<u32>,
    right_ids: &HashSet<u32>,
    minimum: f64,
    maximum: f64,
    excluded: &HashSet<(u32, u32)>,
) -> usize {
    if atoms.len() > MAX_INFERRED_BOND_ATOMS {
        return 0;
    }
    let min_sq = minimum * minimum;
    let max_sq = maximum * maximum;
    let mut count = 0usize;
    for left in atoms {
        if !left_ids.contains(&left.id) {
            continue;
        }
        for right in atoms {
            if left.id == right.id
                || !right_ids.contains(&right.id)
                || excluded.contains(&ordered_pair(left.id, right.id))
            {
                continue;
            }
            let distance_sq = squared_distance(left.position, right.position);
            if distance_sq >= min_sq && distance_sq <= max_sq {
                count += 1;
            }
        }
    }
    count
}

fn estimate_cycle_edges(atom_count: usize, bonds: &[MolecularBond]) -> usize {
    if atom_count == 0 {
        return 0;
    }
    let mut parent = (0..=atom_count).collect::<Vec<_>>();
    fn root(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }
    let mut cycles = 0usize;
    for bond in bonds {
        let left = bond.atom_a as usize;
        let right = bond.atom_b as usize;
        if left > atom_count || right > atom_count {
            continue;
        }
        let left_root = root(&mut parent, left);
        let right_root = root(&mut parent, right);
        if left_root == right_root {
            cycles += 1;
        } else {
            parent[left_root] = right_root;
        }
    }
    cycles
}

fn parse_pdb_number(line: &str, start: usize, end: usize, label: &str) -> anyhow::Result<f64> {
    column(line, start, end)
        .trim()
        .parse::<f64>()
        .with_context(|| format!("invalid PDB {label}"))
}

fn parse_pdb_charge(value: &str) -> Option<i32> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    if text.ends_with('+') || text.ends_with('-') {
        let sign = if text.ends_with('-') { -1 } else { 1 };
        let magnitude = text[..text.len().saturating_sub(1)]
            .parse::<i32>()
            .unwrap_or(1);
        Some(sign * magnitude)
    } else {
        text.parse().ok()
    }
}

fn column(value: &str, start: usize, end: usize) -> &str {
    value.get(start..end.min(value.len())).unwrap_or_default()
}

fn infer_pdb_element(atom_name: &str, residue_name: &str, polymer_atom: bool) -> String {
    let upper = atom_name
        .trim_start_matches(|value: char| value.is_ascii_digit())
        .to_ascii_uppercase();
    if polymer_atom && is_standard_polymer_residue(residue_name) {
        return upper
            .chars()
            .next()
            .map(|value| normalize_element(&value.to_string()))
            .unwrap_or_else(|| "X".to_owned());
    }
    const TWO_LETTER: &[&str] = &[
        "CL", "BR", "NA", "MG", "AL", "SI", "CA", "SC", "TI", "CR", "MN", "FE",
        "CO", "NI", "CU", "ZN", "GA", "GE", "AS", "SE", "SR", "MO", "RU", "RH",
        "PD", "AG", "CD", "IN", "SN", "SB", "TE", "CS", "BA", "PT", "AU", "HG", "PB",
    ];
    if upper.len() >= 2 && TWO_LETTER.contains(&&upper[..2]) {
        normalize_element(&upper[..2])
    } else {
        upper
            .chars()
            .next()
            .map(|value| normalize_element(&value.to_string()))
            .unwrap_or_else(|| "X".to_owned())
    }
}

fn normalize_element(value: &str) -> String {
    let cleaned = value
        .trim()
        .chars()
        .filter(|character| character.is_ascii_alphabetic())
        .take(2)
        .collect::<String>();
    let mut chars = cleaned.chars();
    let Some(first) = chars.next() else {
        return "X".to_owned();
    };
    match chars.next() {
        Some(second) => format!(
            "{}{}",
            first.to_ascii_uppercase(),
            second.to_ascii_lowercase()
        ),
        None => first.to_ascii_uppercase().to_string(),
    }
}

fn is_standard_polymer_residue(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "ALA"
            | "ARG"
            | "ASN"
            | "ASP"
            | "CYS"
            | "GLN"
            | "GLU"
            | "GLY"
            | "HIS"
            | "ILE"
            | "LEU"
            | "LYS"
            | "MET"
            | "PHE"
            | "PRO"
            | "SER"
            | "THR"
            | "TRP"
            | "TYR"
            | "VAL"
            | "A"
            | "C"
            | "G"
            | "U"
            | "DA"
            | "DC"
            | "DG"
            | "DT"
    )
}

fn is_solvent(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "HOH" | "WAT" | "SOL" | "TIP3" | "TIP3P"
    )
}

fn is_common_ion(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "NA" | "CL" | "K" | "CA" | "MG" | "ZN" | "FE" | "MN" | "CU" | "CO" | "NI"
    )
}

fn residue_key(atom: &MolecularAtom) -> String {
    if atom.residue_name.trim().is_empty() && atom.residue_id.trim().is_empty() {
        String::new()
    } else {
        format!(
            "{}:{}:{}",
            if atom.chain_id.trim().is_empty() {
                "_"
            } else {
                atom.chain_id.trim()
            },
            if atom.residue_id.trim().is_empty() {
                "_"
            } else {
                atom.residue_id.trim()
            },
            atom.residue_name.trim()
        )
    }
}

fn mean_position(values: &[[f64; 3]]) -> [f64; 3] {
    if values.is_empty() {
        return [0.0; 3];
    }
    let mut sum = [0.0; 3];
    for value in values {
        for axis in 0..3 {
            sum[axis] += value[axis];
        }
    }
    [
        sum[0] / values.len() as f64,
        sum[1] / values.len() as f64,
        sum[2] / values.len() as f64,
    ]
}

fn hill_formula(composition: &BTreeMap<String, usize>) -> String {
    let mut order = Vec::new();
    if composition.contains_key("C") {
        order.push("C".to_owned());
        if composition.contains_key("H") {
            order.push("H".to_owned());
        }
    }
    for element in composition.keys() {
        if !order.contains(element) {
            order.push(element.clone());
        }
    }
    order
        .into_iter()
        .filter_map(|element| composition.get(&element).map(|count| (element, *count)))
        .map(|(element, count)| {
            if count == 1 {
                element
            } else {
                format!("{element}{count}")
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn ordered_pair(left: u32, right: u32) -> (u32, u32) {
    if left < right {
        (left, right)
    } else {
        (right, left)
    }
}

fn squared_distance(left: [f64; 3], right: [f64; 3]) -> f64 {
    (left[0] - right[0]).powi(2)
        + (left[1] - right[1]).powi(2)
        + (left[2] - right[2]).powi(2)
}

fn covalent_radius(element: &str) -> f64 {
    match element {
        "H" => 0.31,
        "B" => 0.84,
        "C" => 0.76,
        "N" => 0.71,
        "O" => 0.66,
        "F" => 0.57,
        "P" => 1.07,
        "S" => 1.05,
        "Cl" => 1.02,
        "Br" => 1.20,
        "I" => 1.39,
        "Fe" => 1.24,
        "Zn" => 1.22,
        "Mg" => 1.30,
        "Ca" => 1.74,
        _ => 0.77,
    }
}

fn atomic_mass(element: &str) -> f64 {
    match element {
        "H" => 1.008,
        "He" => 4.0026,
        "Li" => 6.94,
        "B" => 10.81,
        "C" => 12.011,
        "N" => 14.007,
        "O" => 15.999,
        "F" => 18.998,
        "Na" => 22.990,
        "Mg" => 24.305,
        "Al" => 26.982,
        "Si" => 28.085,
        "P" => 30.974,
        "S" => 32.06,
        "Cl" => 35.45,
        "K" => 39.098,
        "Ca" => 40.078,
        "Mn" => 54.938,
        "Fe" => 55.845,
        "Co" => 58.933,
        "Ni" => 58.693,
        "Cu" => 63.546,
        "Zn" => 65.38,
        "Br" => 79.904,
        "I" => 126.904,
        _ => 0.0,
    }
}

fn atomic_number(element: &str) -> u32 {
    match element {
        "H" => 1,
        "He" => 2,
        "Li" => 3,
        "B" => 5,
        "C" => 6,
        "N" => 7,
        "O" => 8,
        "F" => 9,
        "Na" => 11,
        "Mg" => 12,
        "Al" => 13,
        "Si" => 14,
        "P" => 15,
        "S" => 16,
        "Cl" => 17,
        "K" => 19,
        "Ca" => 20,
        "Mn" => 25,
        "Fe" => 26,
        "Co" => 27,
        "Ni" => 28,
        "Cu" => 29,
        "Zn" => 30,
        "Br" => 35,
        "I" => 53,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polymer_alpha_carbon_is_not_parsed_as_calcium() {
        assert_eq!(infer_pdb_element("CA", "ALA", true), "C");
        assert_eq!(infer_pdb_element("CA", "CA", false), "Ca");
    }

    #[test]
    fn generic_xyz_import_generates_connectivity() {
        let structure = import_structure(ImportStructureRequest {
            project_id: None,
            name: "triad.xyz".to_owned(),
            format: MolecularFormat::Auto,
            content: "3\ntriad\nO 0 0 0\nH 0.95 0 0\nH -0.24 0.92 0\n".to_owned(),
        })
        .expect("structure");
        assert_eq!(structure.diagnostics.atom_count, 3);
        assert!(structure.bonds.len() >= 2);
    }
}
