mod expression;
mod scene;

use std::collections::{BTreeSet, HashSet};

use anyhow::{Context, bail};

use crate::domain::{
    BoundarySpec, DistributionSpec, ExperimentManifest, FalsificationKind, IntegrationMethod,
    ModelSpec, SearchAlgorithm, VisualizationKind,
};

pub use expression::Expression;

const MAX_ODE_STEPS: usize = 2_000_000;
const MAX_PARTICLE_STEPS: usize = 500_000;
const MAX_SEARCH_CANDIDATES: usize = 1_000_000;

pub fn validate_manifest(manifest: &ExperimentManifest) -> anyhow::Result<Vec<String>> {
    let mut warnings = Vec::new();

    validate_identity(manifest)?;
    validate_compute(manifest)?;
    validate_integration(manifest)?;
    if let Some(scene) = &manifest.visualization.scene { scene::validate(scene)?; }

    let constant_names = validate_constants(manifest)?;
    let search_names = validate_search_structure(manifest)?;
    validate_named_analysis_items(manifest)?;

    match &manifest.model {
        ModelSpec::StateVectorOde {
            variables,
            derivatives,
        } => {
            if !matches!(
                manifest.integration.method,
                IntegrationMethod::Euler | IntegrationMethod::Rk4
            ) {
                bail!("state_vector_ode supports only Euler and RK4 integration");
            }
            if variables.is_empty() || variables.len() > 128 {
                bail!("state_vector_ode requires between 1 and 128 CPU state variables (GPU lowering supports at most 24)");
            }
            if derivatives.len() != variables.len() {
                bail!("state_vector_ode requires exactly one derivative per state variable");
            }

            let state_names = validate_names(
                variables.iter().map(|value| value.name.as_str()),
                "state variable",
            )?;
            reject_name_collisions(&state_names, &constant_names, "state variable", "constant")?;
            reject_name_collisions(&search_names, &state_names, "search variable", "state variable")?;
            reject_name_collisions(&search_names, &constant_names, "search variable", "constant")?;

            if variables.iter().any(|value| !value.initial.is_finite()) {
                bail!("state variable initial values must be finite");
            }

            let derivative_allowed = state_names
                .iter()
                .chain(constant_names.iter())
                .cloned()
                .chain(["t".to_owned()])
                .collect::<BTreeSet<_>>();
            let derivative_targets = derivatives
                .iter()
                .map(|value| value.variable.clone())
                .collect::<HashSet<_>>();
            if derivative_targets.len() != derivatives.len()
                || state_names
                    .iter()
                    .any(|name| !derivative_targets.contains(name))
            {
                bail!("derivative targets must uniquely match the state variables");
            }
            for derivative in derivatives {
                validate_expression(
                    &derivative.expression,
                    &derivative_allowed,
                    &format!("derivative for `{}`", derivative.variable),
                )?;
            }

            validate_analysis_expressions(
                manifest,
                &state_names,
                &constant_names,
                &search_names,
                false,
            )?;
            validate_ode_visualization(manifest, &state_names, &constant_names)?;
            validate_search_targets(manifest, &state_names, &constant_names, false)?;
        }
        ModelSpec::PairwiseParticles {
            dimensions,
            population,
            interaction,
            boundary,
        } => {
            if !matches!(
                manifest.integration.method,
                IntegrationMethod::Euler | IntegrationMethod::VelocityVerlet
            ) {
                bail!("pairwise_particles supports only Euler and velocity-Verlet integration");
            }
            let requested_steps = requested_steps(manifest);
            if requested_steps > MAX_PARTICLE_STEPS {
                bail!(
                    "pairwise_particles currently supports at most {MAX_PARTICLE_STEPS} steps per candidate"
                );
            }
            if !(1..=3).contains(dimensions) {
                bail!("pairwise_particles dimensions must be 1, 2, or 3");
            }
            if !(1..=2048).contains(&population.count) {
                bail!("particle count must be between 1 and 2,048");
            }
            if !population.mass.is_finite() || population.mass <= 0.0 {
                bail!("particle mass must be finite and positive");
            }

            reject_name_collisions(
                &search_names,
                &constant_names,
                "search variable",
                "constant",
            )?;
            validate_distribution(
                &population.position,
                *dimensions,
                population.count,
                "position",
            )?;
            validate_distribution(
                &population.velocity,
                *dimensions,
                population.count,
                "velocity",
            )?;
            validate_boundary(boundary, *dimensions)?;

            let force_allowed = constant_names
                .iter()
                .cloned()
                .chain([
                    "r".to_owned(),
                    "inv_r".to_owned(),
                    "mass_i".to_owned(),
                    "mass_j".to_owned(),
                    "relative_speed".to_owned(),
                    "t".to_owned(),
                ])
                .collect::<BTreeSet<_>>();
            validate_expression(
                &interaction.radial_force,
                &force_allowed,
                "pairwise radial force",
            )?;
            if !interaction.softening.is_finite() || interaction.softening < 0.0 {
                bail!("interaction softening must be finite and non-negative");
            }
            if !interaction.linear_damping.is_finite() || interaction.linear_damping < 0.0 {
                bail!("interaction linear_damping must be finite and non-negative");
            }
            if let Some(cutoff) = interaction.cutoff {
                if !cutoff.is_finite() || cutoff <= 0.0 {
                    bail!("interaction cutoff must be finite and positive when supplied");
                }
            }
            if !interaction.external_acceleration.is_empty()
                && interaction.external_acceleration.len() != *dimensions
            {
                bail!(
                    "external_acceleration must be empty or contain one expression per dimension"
                );
            }
            let external_allowed = constant_names
                .iter()
                .cloned()
                .chain([
                    "x0".to_owned(),
                    "x1".to_owned(),
                    "x2".to_owned(),
                    "v0".to_owned(),
                    "v1".to_owned(),
                    "v2".to_owned(),
                    "t".to_owned(),
                ])
                .collect::<BTreeSet<_>>();
            for (index, source) in interaction.external_acceleration.iter().enumerate() {
                validate_expression(
                    source,
                    &external_allowed,
                    &format!("external acceleration component {index}"),
                )?;
            }

            validate_analysis_expressions(
                manifest,
                &BTreeSet::new(),
                &constant_names,
                &search_names,
                true,
            )?;
            validate_search_targets(manifest, &BTreeSet::new(), &constant_names, true)?;
            validate_particle_search_targets_against_distributions(manifest)?;

            if manifest.integration.method == IntegrationMethod::VelocityVerlet {
                let force_uses_velocity = Expression::parse(&interaction.radial_force)?
                    .identifiers()
                    .contains("relative_speed");
                let external_uses_velocity = interaction.external_acceleration.iter().any(|source| {
                    Expression::parse(source)
                        .map(|expression| {
                            expression.identifiers().iter().any(|name| {
                                matches!(name.as_str(), "v0" | "v1" | "v2")
                            })
                        })
                        .unwrap_or(false)
                });
                if interaction.linear_damping > 0.0
                    || force_uses_velocity
                    || external_uses_velocity
                {
                    warnings.push(
                        "Velocity-Verlet is exact only for position-dependent acceleration. This manifest includes velocity-dependent force or damping, so the runtime uses a documented predictor-style approximation; use Euler or add an independent convergence check before interpreting results."
                            .to_owned(),
                    );
                }
            }

            if manifest.visualization.kind != VisualizationKind::ParticleCloud
                && manifest.visualization.kind != VisualizationKind::None
            {
                warnings.push(
                    "Pairwise particle results are emitted as a particle cloud; the renderer will normalize the requested visualization kind."
                        .to_owned(),
                );
            }
            if population.count > 512 && manifest.search.enabled {
                warnings.push(
                    "Searching more than 512 particles per candidate can be expensive in the current bounded CPU particle runtime."
                        .to_owned(),
                );
            }
        }
    }

    validate_visualization(manifest)?;
    validate_falsification(manifest)?;
    Ok(warnings)
}

fn validate_identity(manifest: &ExperimentManifest) -> anyhow::Result<()> {
    if manifest.title.trim().is_empty() {
        bail!("manifest title is required");
    }
    if manifest.question.trim().len() < 8 {
        bail!("manifest question must contain at least eight characters");
    }
    if manifest.scientific_boundary.trim().len() < 12 {
        bail!("scientific_boundary must state what the experiment cannot establish");
    }
    if manifest.hypothesis.trim().is_empty() {
        bail!("manifest hypothesis is required");
    }
    Ok(())
}

fn validate_compute(manifest: &ExperimentManifest) -> anyhow::Result<()> {
    if !(1..=86_400).contains(&manifest.compute.max_wall_seconds) {
        bail!("compute max_wall_seconds must be between 1 and 86,400");
    }
    if !(128..=262_144).contains(&manifest.compute.max_memory_mb) {
        bail!("compute max_memory_mb must be between 128 and 262,144");
    }
    if !(1..=MAX_SEARCH_CANDIDATES).contains(&manifest.compute.candidate_count) {
        bail!("candidate_count must be between 1 and {MAX_SEARCH_CANDIDATES}");
    }
    if manifest.compute.batch_size > MAX_SEARCH_CANDIDATES {
        bail!("batch_size cannot exceed {MAX_SEARCH_CANDIDATES}; use zero for automatic calibration");
    }
    Ok(())
}

fn validate_integration(manifest: &ExperimentManifest) -> anyhow::Result<()> {
    let integration = &manifest.integration;
    if !integration.start_time.is_finite()
        || !integration.end_time.is_finite()
        || !integration.time_step.is_finite()
    {
        bail!("integration times must be finite");
    }
    if integration.end_time <= integration.start_time {
        bail!("integration end_time must be greater than start_time");
    }
    if integration.time_step <= 0.0 {
        bail!("integration time_step must be positive");
    }
    if integration.output_stride == 0 {
        bail!("integration output_stride must be at least one");
    }
    if !(1..=MAX_ODE_STEPS).contains(&integration.max_steps) {
        bail!("integration max_steps must be between 1 and {MAX_ODE_STEPS}");
    }
    let steps = requested_steps(manifest);
    if steps > integration.max_steps {
        bail!(
            "integration requires {steps} steps, exceeding max_steps {}",
            integration.max_steps
        );
    }
    Ok(())
}

fn requested_steps(manifest: &ExperimentManifest) -> usize {
    ((manifest.integration.end_time - manifest.integration.start_time)
        / manifest.integration.time_step)
        .ceil()
        .max(0.0) as usize
}

fn validate_constants(manifest: &ExperimentManifest) -> anyhow::Result<BTreeSet<String>> {
    if manifest.constants.len() > 32 {
        bail!("a manifest may define at most 32 constants");
    }
    let names = validate_names(
        manifest.constants.iter().map(|value| value.name.as_str()),
        "constant",
    )?;
    for constant in &manifest.constants {
        if !constant.value.is_finite() {
            bail!("constant `{}` must have a finite value", constant.name);
        }
    }
    Ok(names)
}

fn validate_search_structure(manifest: &ExperimentManifest) -> anyhow::Result<BTreeSet<String>> {
    let names = validate_names(
        manifest.search.variables.iter().map(|value| value.name.as_str()),
        "search variable",
    )?;

    if manifest.search.enabled {
        if manifest.compute.candidate_count < 2 {bail!("enabled search requires an approved candidate_count of at least two; do not silently expand a one-candidate budget");}
        if manifest.search.algorithm == SearchAlgorithm::DifferentialEvolution && (manifest.search.population < 4 || manifest.compute.candidate_count < 4) {
            bail!("differential_evolution requires population and candidate_count >= 4");
        }
        if manifest.search.algorithm == SearchAlgorithm::None {
            bail!("search.enabled is true but search.algorithm is none");
        }
        if manifest.search.variables.is_empty() {
            bail!("enabled search requires at least one search variable");
        }
        if manifest.search.objectives.is_empty() {
            bail!("enabled search requires at least one objective");
        }
        if !(2..=100_000).contains(&manifest.search.population) {
            bail!("search population must be between 2 and 100,000");
        }
        if !(1..=10_000).contains(&manifest.search.generations) {
            bail!("search generations must be between 1 and 10,000");
        }
        if !(0.01..=0.8).contains(&manifest.search.elite_fraction) {
            bail!("search elite_fraction must be between 0.01 and 0.8");
        }
        if !manifest.search.mutation_scale.is_finite()
            || !(0.0..=2.0).contains(&manifest.search.mutation_scale)
        {
            bail!("search mutation_scale must be between 0 and 2");
        }
    } else if manifest.search.algorithm != SearchAlgorithm::None {
        bail!("disabled search must use algorithm `none`");
    }

    for variable in &manifest.search.variables {
        if !variable.minimum.is_finite()
            || !variable.maximum.is_finite()
            || variable.maximum <= variable.minimum
        {
            bail!(
                "search variable `{}` requires finite minimum < maximum",
                variable.name
            );
        }
    }
    Ok(names)
}

fn validate_named_analysis_items(manifest: &ExperimentManifest) -> anyhow::Result<()> {
    let observables = validate_names(
        manifest.observables.iter().map(|value| value.name.as_str()),
        "observable",
    )?;
    let objectives = validate_names(
        manifest.search.objectives.iter().map(|value| value.name.as_str()),
        "objective",
    )?;
    let constraints = validate_names(
        manifest.constraints.iter().map(|value| value.name.as_str()),
        "constraint",
    )?;
    reject_name_collisions(&observables, &objectives, "observable", "objective")?;
    reject_name_collisions(&observables, &constraints, "observable", "constraint")?;
    reject_name_collisions(&objectives, &constraints, "objective", "constraint")?;
    Ok(())
}

fn validate_names<'a>(
    names: impl Iterator<Item = &'a str>,
    label: &str,
) -> anyhow::Result<BTreeSet<String>> {
    let mut unique = BTreeSet::new();
    for raw in names {
        let name = raw.trim();
        if !valid_identifier(name) {
            bail!("{label} name `{name}` is not a valid expression identifier");
        }
        if !unique.insert(name.to_owned()) {
            bail!("duplicate {label} name `{name}`");
        }
    }
    Ok(unique)
}

fn reject_name_collisions(
    left: &BTreeSet<String>,
    right: &BTreeSet<String>,
    left_label: &str,
    right_label: &str,
) -> anyhow::Result<()> {
    let collisions = left.intersection(right).cloned().collect::<Vec<_>>();
    if !collisions.is_empty() {
        bail!(
            "{left_label} names collide with {right_label} names: {}",
            collisions.join(", ")
        );
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|value| value.is_ascii_alphanumeric() || value == '_')
}

fn validate_expression(
    source: &str,
    allowed: &BTreeSet<String>,
    label: &str,
) -> anyhow::Result<()> {
    let expression =
        Expression::parse(source).with_context(|| format!("invalid {label} expression"))?;
    let unknown = expression
        .identifiers()
        .difference(allowed)
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        bail!("{label} references unknown variables: {}", unknown.join(", "));
    }
    Ok(())
}

fn validate_analysis_expressions(
    manifest: &ExperimentManifest,
    state_names: &BTreeSet<String>,
    constant_names: &BTreeSet<String>,
    search_names: &BTreeSet<String>,
    particles: bool,
) -> anyhow::Result<()> {
    let mut allowed = constant_names.clone();
    allowed.extend(search_names.iter().cloned());
    allowed.extend([
        "t".to_owned(),
        "elapsed_time".to_owned(),
        "steps".to_owned(),
        "sample_count".to_owned(),
    ]);

    if particles {
        let metric_names = particle_metric_names();
        allowed.extend(metric_names.iter().cloned());
        for name in metric_names {
            allowed.insert(format!("initial_{name}"));
            allowed.insert(format!("delta_{name}"));
            allowed.insert(format!("abs_delta_{name}"));
        }
    } else {
        for name in state_names {
            allowed.insert(name.clone());
            for prefix in [
                "initial",
                "final",
                "minimum",
                "maximum",
                "mean",
                "delta",
                "abs_delta",
            ] {
                allowed.insert(format!("{prefix}_{name}"));
            }
        }
    }

    if particles && !manifest.trajectory.is_empty() { bail!("trajectory reducers require the ODE runtime; particle summaries are not equivalent"); }
    if manifest.trajectory.len() > 64 { bail!("at most 64 trajectory measures"); }
    let names=validate_names(manifest.trajectory.iter().map(|m|m.name.as_str()), "trajectory measure")?;
    let mut reserved=allowed.clone();
    for name in &names {
        if !reserved.insert(name.clone()) {bail!("trajectory name collides with a runtime identifier");}
    }
    let mut metadata=BTreeSet::new();
    for measure in &manifest.trajectory {
        if matches!(measure.reducer, crate::domain::TrajectoryReducer::FirstBelow | crate::domain::TrajectoryReducer::FirstAbove) {
            let key=format!("{}_observed",measure.name);
            if !reserved.insert(key.clone()) {bail!("first-passage metadata collides with a declared identifier");}
            metadata.insert(key);
        }
    }
    let instantaneous=state_names.iter().chain(constant_names.iter()).cloned()
        .chain(["t".to_owned()]).chain(state_names.iter().map(|s|format!("initial_{s}"))).collect::<BTreeSet<_>>();
    for measure in &manifest.trajectory {
        if !measure.threshold.is_finite() || !measure.hysteresis.is_finite() || measure.hysteresis<0.0 {bail!("trajectory thresholds must be finite and hysteresis nonnegative");}
        validate_expression(&measure.expression,&instantaneous,"trajectory expression")?;
    }
    allowed=reserved;
    for observable in &manifest.observables {
        if names.contains(&observable.name) && observable.expression.trim()!=observable.name {bail!("an observable cannot overwrite a trajectory measurement with another expression");}
        if metadata.contains(&observable.name) && observable.expression.trim()!=observable.name {bail!("observable cannot overwrite first-passage censoring metadata");}

        validate_expression(
            &observable.expression,
            &allowed,
            &format!("observable `{}`", observable.name),
        )?;
    }
    for objective in &manifest.search.objectives {
        validate_expression(
            &objective.expression,
            &allowed,
            &format!("objective `{}`", objective.name),
        )?;
        if !objective.weight.is_finite() || objective.weight < 0.0 {
            bail!(
                "objective `{}` weight must be finite and non-negative",
                objective.name
            );
        }
    }
    for constraint in &manifest.constraints {
        validate_expression(
            &constraint.expression,
            &allowed,
            &format!("constraint `{}`", constraint.name),
        )?;
        if !constraint.tolerance.is_finite() || constraint.tolerance < 0.0 {
            bail!(
                "constraint `{}` tolerance must be finite and non-negative",
                constraint.name
            );
        }
    }
    Ok(())
}

fn particle_metric_names() -> BTreeSet<String> {
    [
        "particle_count",
        "mean_radius",
        "rms_radius",
        "max_radius",
        "mean_speed",
        "kinetic_energy",
        "momentum_norm",
        "minimum_distance",
        "center_of_mass_x",
        "center_of_mass_y",
        "center_of_mass_z",
        "spread_x",
        "spread_y",
        "spread_z",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn validate_ode_visualization(
    manifest: &ExperimentManifest,
    state_names: &BTreeSet<String>,
    constant_names: &BTreeSet<String>,
) -> anyhow::Result<()> {
    if manifest.visualization.kind == VisualizationKind::None {
        return Ok(());
    }
    let allowed = state_names
        .iter()
        .chain(constant_names.iter())
        .cloned()
        .chain(["t".to_owned()])
        .collect::<BTreeSet<_>>();
    for (axis, expression) in [
        ("x", manifest.visualization.x.as_str()),
        ("y", manifest.visualization.y.as_str()),
        ("z", manifest.visualization.z.as_str()),
    ] {
        if expression.trim().is_empty() {
            continue;
        }
        validate_expression(expression, &allowed, &format!("visualization {axis}"))?;
    }
    for mapping in &manifest.visualization.entities {
        for expression in [&mapping.x, &mapping.y, &mapping.z, &mapping.vx, &mapping.vy, &mapping.vz, &mapping.radius] {
            if !expression.trim().is_empty() {
                validate_expression(expression, &allowed, "visual entity expression")?;
            }
        }
        if mapping.x.trim().is_empty() || mapping.y.trim().is_empty() {
            bail!("visual entities require explicit x and y expressions");
        }
    }
    Ok(())
}

fn validate_visualization(manifest: &ExperimentManifest) -> anyhow::Result<()> {
    if manifest.visualization.entities.len() > 128 { bail!("at most 128 explicit visual entities are supported"); }
    let mut ids = std::collections::HashSet::new();
    for entity in &manifest.visualization.entities {
        if entity.id.trim().is_empty() || !ids.insert(entity.id.clone()) { bail!("visual entity IDs must be nonempty and unique"); }
    }
    if manifest.visualization.max_frames == 0 || manifest.visualization.max_frames > 20_000 {
        bail!("visualization max_frames must be between 1 and 20,000");
    }
    if !manifest.visualization.point_size.is_finite()
        || manifest.visualization.point_size <= 0.0
        || manifest.visualization.point_size > 10.0
    {
        bail!("visualization point_size must be finite and between 0 and 10");
    }
    Ok(())
}

fn validate_search_targets(
    manifest: &ExperimentManifest,
    state_names: &BTreeSet<String>,
    constant_names: &BTreeSet<String>,
    particles: bool,
) -> anyhow::Result<()> {
    for variable in &manifest.search.variables {
        let target = variable.target.trim();
        if let Some(name) = target.strip_prefix("constant:") {
            if !constant_names.contains(name) {
                bail!("search target `{target}` does not identify a declared constant");
            }
            continue;
        }
        if !particles {
            if let Some(name) = target.strip_prefix("initial:") {
                if !state_names.contains(name) {
                    bail!("search target `{target}` does not identify a state variable");
                }
                continue;
            }
        } else if particle_target_shape(target).is_some() {
            continue;
        }
        bail!("unsupported search target `{target}`");
    }
    Ok(())
}

fn particle_target_shape(target: &str) -> Option<(&str, &str, Option<usize>)> {
    match target {
        "population.mass" => return Some(("population", "mass", None)),
        "interaction.softening" => return Some(("interaction", "softening", None)),
        "interaction.linear_damping" => {
            return Some(("interaction", "linear_damping", None));
        }
        "position.radius" | "position.thickness" | "position.spacing" | "position.jitter"
        | "velocity.radius" | "velocity.thickness" | "velocity.spacing" | "velocity.jitter" => {
            let (distribution, field) = target.split_once('.')?;
            return Some((distribution, field, None));
        }
        _ => {}
    }

    let mut parts = target.split('.');
    let distribution = parts.next()?;
    let field = parts.next()?;
    let axis = parts.next()?;
    if parts.next().is_some() || !matches!(distribution, "position" | "velocity") {
        return None;
    }
    if !matches!(field, "min" | "max" | "mean" | "std_dev") {
        return None;
    }
    let axis = match axis {
        "x" | "0" => 0,
        "y" | "1" => 1,
        "z" | "2" => 2,
        _ => return None,
    };
    Some((distribution, field, Some(axis)))
}

fn validate_particle_search_targets_against_distributions(
    manifest: &ExperimentManifest,
) -> anyhow::Result<()> {
    let ModelSpec::PairwiseParticles {
        dimensions,
        population,
        ..
    } = &manifest.model
    else {
        return Ok(());
    };

    for variable in &manifest.search.variables {
        let target = variable.target.as_str();
        if target.starts_with("constant:") {
            continue;
        }
        match target {
            "population.mass" if variable.minimum <= 0.0 => {
                bail!("population.mass search bounds must remain positive");
            }
            "interaction.softening" | "interaction.linear_damping"
                if variable.minimum < 0.0 =>
            {
                bail!("search target `{target}` cannot use negative bounds");
            }
            "population.mass" | "interaction.softening" | "interaction.linear_damping" => {
                continue;
            }
            _ => {}
        }

        let Some((which, field, axis)) = particle_target_shape(target) else {
            continue;
        };
        let distribution = if which == "position" {
            &population.position
        } else {
            &population.velocity
        };
        if let Some(axis) = axis {
            if axis >= *dimensions {
                bail!(
                    "search target `{}` references an axis outside the {}D model",
                    variable.target,
                    dimensions
                );
            }
        }
        let supported = match (distribution, field) {
            (DistributionSpec::Uniform { min: _, max }, "min") => {
                let axis = axis.context("uniform minimum target requires an axis")?;
                if variable.maximum >= max[axis] {
                    bail!(
                        "search target `{target}` can cross its fixed maximum {}; lower the variable maximum",
                        max[axis]
                    );
                }
                true
            }
            (DistributionSpec::Uniform { min, max }, "max") => {
                let axis = axis.context("uniform maximum target requires an axis")?;
                if variable.minimum <= min[axis] {
                    bail!(
                        "search target `{target}` can cross its fixed minimum {}; raise the variable minimum",
                        min[axis]
                    );
                }
                let _ = max;
                true
            }
            (DistributionSpec::Normal { .. }, "mean") => true,
            (DistributionSpec::Normal { .. }, "std_dev") => {
                if variable.minimum < 0.0 {
                    bail!("search target `{target}` cannot use negative bounds");
                }
                true
            }
            (DistributionSpec::Grid { .. }, "spacing") => {
                if variable.minimum <= 0.0 {
                    bail!("search target `{target}` must remain positive");
                }
                true
            }
            (DistributionSpec::Grid { .. }, "jitter")
            | (DistributionSpec::Sphere { .. }, "radius" | "thickness") => {
                if variable.minimum < 0.0 {
                    bail!("search target `{target}` cannot use negative bounds");
                }
                true
            }
            _ => false,
        };
        if !supported {
            bail!(
                "search target `{}` is incompatible with the selected {} distribution",
                variable.target,
                which
            );
        }
    }
    Ok(())
}

fn validate_falsification(manifest: &ExperimentManifest) -> anyhow::Result<()> {
    if manifest.falsification.len() > 64 {
        bail!("a manifest may define at most 64 falsification passes");
    }
    let _ = validate_names(
        manifest
            .falsification
            .iter()
            .map(|value| value.name.as_str()),
        "falsification",
    )?;

    let (state_names, constant_names) = match &manifest.model {
        ModelSpec::StateVectorOde { variables, .. } => (
            variables
                .iter()
                .map(|value| value.name.as_str())
                .collect::<HashSet<_>>(),
            manifest
                .constants
                .iter()
                .map(|value| value.name.as_str())
                .collect::<HashSet<_>>(),
        ),
        ModelSpec::PairwiseParticles { .. } => (HashSet::new(), HashSet::new()),
    };

    if manifest.falsification.iter().try_fold(0usize, |sum,t|sum.checked_add(t.repetitions)).unwrap_or(usize::MAX)>4096 {
        bail!("a manifest may execute at most 4,096 challenge trials");
    }
    for test in &manifest.falsification {
        let mut metric_names=HashSet::new();
        if test.checks.iter().any(|c| !metric_names.insert(&c.metric)) { bail!("a challenge may specify each metric only once"); }
        if test.checks.len() > 64 { bail!("at most 64 checks per challenge"); }
        for check in &test.checks {
            if check.metric.trim().is_empty() || !["stable", "change", "decrease", "increase"].contains(&check.expectation.as_str()) {
                bail!("challenge checks need a metric and a valid expectation");
            }
            if !check.absolute_tolerance.is_finite() || check.absolute_tolerance < 0.0 ||
               !check.relative_tolerance.is_finite() || check.relative_tolerance < 0.0 {
                bail!("challenge tolerances must be finite and nonnegative");
            }
        }
        if !test.magnitude.is_finite() {
            bail!("falsification `{}` magnitude must be finite", test.name);
        }
        if !(1..=1000).contains(&test.repetitions) {
            bail!(
                "falsification `{}` repetitions must be between 1 and 1,000",
                test.name
            );
        }
        if test.kind == FalsificationKind::ResolutionLadder {
            if !(2..=4).contains(&test.repetitions) {bail!("resolution_ladder needs 2–4 distinct levels, starting with h/2 and h/4");}
            let cap=if matches!(&manifest.model,ModelSpec::PairwiseParticles{..}) {MAX_PARTICLE_STEPS}else{MAX_ODE_STEPS};
            crate::science::evidence::refinement(&manifest.integration,test.repetitions-1,cap)?;
        }
        match (&manifest.model, test.kind) {
            (_, FalsificationKind::ResolutionLadder) => {}
            (_, FalsificationKind::StepHalving | FalsificationKind::SeedReplication) => {}
            (ModelSpec::StateVectorOde { .. }, FalsificationKind::InitialPerturbation) => {
                let target = test
                    .target
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .context(format!("falsification `{}` requires a target", test.name))?;
                let target = target.strip_prefix("initial:").unwrap_or(target);
                if !state_names.contains(target) {
                    bail!(
                        "falsification `{}` initial target `{target}` is not a state variable",
                        test.name
                    );
                }
            }
            (ModelSpec::StateVectorOde { .. }, FalsificationKind::ParameterPerturbation) => {
                let target = test
                    .target
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .context(format!("falsification `{}` requires a target", test.name))?;
                let target = target.strip_prefix("constant:").unwrap_or(target);
                if !constant_names.contains(target) {
                    bail!(
                        "falsification `{}` parameter target `{target}` is not a constant",
                        test.name
                    );
                }
            }
            (ModelSpec::PairwiseParticles { .. }, FalsificationKind::InitialPerturbation) => {
                let target = test.target.as_deref().map(str::trim).unwrap_or_default();
                if !matches!(target, "position" | "velocity") {
                    bail!(
                        "falsification `{}` initial target must be `position` or `velocity`",
                        test.name
                    );
                }
                if test.magnitude < 0.0 {
                    bail!(
                        "falsification `{}` initial perturbation magnitude must be non-negative",
                        test.name
                    );
                }
            }
            (ModelSpec::PairwiseParticles { .. }, FalsificationKind::ParameterPerturbation) => {
                let target = test
                    .target
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .context(format!("falsification `{}` requires a target", test.name))?;
                let valid_constant = target
                    .strip_prefix("constant:")
                    .map(|name| manifest.constants.iter().any(|value| value.name == name))
                    .unwrap_or(false);
                if !valid_constant && particle_target_shape(target).is_none() {
                    bail!(
                        "falsification `{}` parameter target `{target}` is not supported",
                        test.name
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_distribution(
    distribution: &DistributionSpec,
    dimensions: usize,
    count: usize,
    label: &str,
) -> anyhow::Result<()> {
    match distribution {
        DistributionSpec::Explicit { values } => {
            if values.len() != count {
                bail!("explicit {label} distribution must contain one vector per particle");
            }
            if values.iter().any(|value| {
                value.len() != dimensions || value.iter().any(|component| !component.is_finite())
            }) {
                bail!(
                    "explicit {label} vectors must match dimensions and contain finite values"
                );
            }
        }
        DistributionSpec::Uniform { min, max } => {
            validate_vector_pair(min, max, dimensions, label, true)?;
        }
        DistributionSpec::Normal { mean, std_dev } => {
            if mean.len() != dimensions || std_dev.len() != dimensions {
                bail!("normal {label} mean and std_dev must match dimensions");
            }
            if mean.iter().any(|value| !value.is_finite())
                || std_dev
                    .iter()
                    .any(|value| !value.is_finite() || *value < 0.0)
            {
                bail!("normal {label} parameters must be finite and std_dev non-negative");
            }
        }
        DistributionSpec::Grid {
            extent,
            spacing,
            jitter,
        } => {
            if extent.len() != dimensions || extent.iter().any(|value| *value == 0) {
                bail!("grid {label} extent must match dimensions and be positive");
            }
            if extent.iter().product::<usize>() < count {
                bail!("grid {label} extent does not contain enough cells for the population");
            }
            if !spacing.is_finite() || *spacing <= 0.0 || !jitter.is_finite() || *jitter < 0.0 {
                bail!("grid {label} spacing must be positive and jitter non-negative");
            }
        }
        DistributionSpec::Sphere { radius, thickness } => {
            if !radius.is_finite()
                || *radius < 0.0
                || !thickness.is_finite()
                || *thickness < 0.0
            {
                bail!("sphere {label} radius and thickness must be finite and non-negative");
            }
        }
    }
    Ok(())
}

fn validate_vector_pair(
    minimum: &[f64],
    maximum: &[f64],
    dimensions: usize,
    label: &str,
    ordered: bool,
) -> anyhow::Result<()> {
    if minimum.len() != dimensions || maximum.len() != dimensions {
        bail!("{label} bounds must match dimensions");
    }
    for (minimum, maximum) in minimum.iter().zip(maximum) {
        if !minimum.is_finite() || !maximum.is_finite() || (ordered && maximum <= minimum) {
            bail!("{label} bounds must be finite with minimum < maximum");
        }
    }
    Ok(())
}

fn validate_boundary(boundary: &BoundarySpec, dimensions: usize) -> anyhow::Result<()> {
    match boundary {
        BoundarySpec::Open => Ok(()),
        BoundarySpec::Periodic { min, max } | BoundarySpec::Reflective { min, max } => {
            validate_vector_pair(min, max, dimensions, "boundary", true)
        }
    }
}

/*
The mutable research layer is intentionally a mathematical DSL, not native host
code. A later extension can compile richer agent-authored modules to WebAssembly
without ambient network, filesystem, process, environment, or credential access.
The current expression runtime already supplies fuel-like numerical limits,
seeded randomness, validated identifiers, bounded memory, and immutable manifests.
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifier_rules_reject_paths_and_shell_metacharacters() {
        assert!(valid_identifier("state_1"));
        assert!(!valid_identifier("state.value"));
        assert!(!valid_identifier("x;exit"));
    }

    #[test]
    fn expression_validation_rejects_unknown_symbols() {
        let allowed = BTreeSet::from(["x".to_owned(), "t".to_owned()]);
        assert!(validate_expression("x + t", &allowed, "test").is_ok());
        assert!(validate_expression("x + hidden", &allowed, "test").is_err());
    }

    #[test]
    fn particle_target_parser_accepts_vector_axes() {
        assert_eq!(
            particle_target_shape("position.min.x"),
            Some(("position", "min", Some(0)))
        );
        assert_eq!(
            particle_target_shape("velocity.std_dev.2"),
            Some(("velocity", "std_dev", Some(2)))
        );
    }
}
