use super::evidence;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    time::Instant,
};

use anyhow::{Context, bail};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rand_distr::{Distribution, Normal};
use rayon::prelude::*;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{
    domain::{
        BoundarySpec, ComputeRequest, DistributionSpec, ExperimentManifest, FalsificationKind,
        IntegrationMethod, IntegrationSpec, ModelSpec, ObjectiveGoal, PairInteractionSpec,
        SearchAlgorithm, VisualizationKind,
    },
    sandbox::Expression,
};

use super::{
    ExecutionOutput, FalsificationResult, InternalOutcome, ProgressCallback, TimeSeries,
    VisualEntity, VisualFrame, VisualizationOutput,
    search::{Candidate, PopulationEngine, generation_summary, sort_candidates},
};

#[derive(Clone)]
struct ParticleProgram {
    dimensions: usize,
    count: usize,
    base_mass: f64,
    position: DistributionSpec,
    velocity: DistributionSpec,
    interaction: PairInteractionSpec,
    boundary: BoundarySpec,
    force: Expression,
    external: Vec<Expression>,
    constant_names: Vec<String>,
    constant_values: Vec<f64>,
    objectives: Vec<(ObjectiveGoal, f64, Expression)>,
    constraints: Vec<(f64, Expression)>,
    observables: Vec<Expression>,
}

#[derive(Clone)]
struct ParticleCandidateInput {
    mass: f64,
    position: DistributionSpec,
    velocity: DistributionSpec,
    interaction: PairInteractionSpec,
    constants: Vec<f64>,
    seed: u64,
    position_noise: f64,
    velocity_noise: f64,
}

struct ParticleReport {
    constraint_results: Vec<evidence::ConstraintResult>,
    score: f64,
    metrics: BTreeMap<String, f64>,
    series: Vec<TimeSeries>,
    visualization: VisualizationOutput,
    steps: usize,
}

pub(crate) fn execute(
    manifest: &ExperimentManifest,
    compute: &ComputeRequest,
    cancellation: &CancellationToken,
    progress: &ProgressCallback,
) -> anyhow::Result<InternalOutcome> {
    execute_checkpointed(manifest, compute, cancellation, progress, None)
}

pub(crate) fn execute_checkpointed(
    manifest: &ExperimentManifest,
    compute: &ComputeRequest,
    cancellation: &CancellationToken,
    progress: &ProgressCallback,
    checkpoint: Option<crate::compute::recovery::CheckpointStore>,
) -> anyhow::Result<InternalOutcome> {
    let program = compile_program(manifest)?;
    let started = Instant::now();
    let search_enabled = manifest.search.enabled
        && manifest.search.algorithm != SearchAlgorithm::None
        && !manifest.search.variables.is_empty();
    let population_size = if search_enabled {
        manifest
            .search
            .population
            .min(compute.candidate_count.max(2))
            .max(2)
    } else {
        1
    };
    if search_enabled && manifest.search.algorithm == SearchAlgorithm::DifferentialEvolution && population_size < 4 {
        bail!("differential evolution requires at least four evaluated candidates; increase the compute candidate allocation");
    }
    let generations = if search_enabled {
        manifest.search.generations.max(1)
    } else {
        1
    };

    let mut engine = PopulationEngine::new(manifest.search.seed);
    let mut population = if search_enabled {
        engine.initial(&manifest.search, population_size)
    } else {
        vec![Candidate::unevaluated(Vec::new())]
    };
    let mut best = Candidate::unevaluated(Vec::new());
    let mut history = Vec::new();

    let mut first_generation = 0;
    if let Some(saved) = checkpoint.as_ref().map(|store| store.load(population_size, generations)).transpose()?.flatten() {
        first_generation = saved.next_generation;
        population = saved.population;
        best = saved.best;
        history = saved.history;
        engine = PopulationEngine::restore(saved.engine)?;
    }

    for generation in first_generation..generations {
        if cancellation.is_cancelled() {
            bail!("run cancelled");
        }
        if started.elapsed().as_secs() > compute.max_wall_seconds {
            bail!("run exceeded its configured wall-time budget");
        }
        progress(
            0.05 + 0.7 * generation as f32 / generations.max(1) as f32,
            &format!(
                "Evaluating particle candidate generation {}/{}",
                generation + 1,
                generations
            ),
        );

        // Every candidate is evaluated against the same generated ensemble. This prevents
        // random initial conditions from being mistaken for parameter quality.
        let inputs = population
            .iter()
            .map(|candidate| {
                materialize_candidate(
                    &program,
                    manifest,
                    &candidate.values,
                    manifest.search.seed,
                )
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let completed = std::sync::atomic::AtomicUsize::new(0);
        let width = if compute.batch_size == 0 { rayon::current_num_threads() } else {
            compute.batch_size.min(rayon::current_num_threads())
        }.max(1);
        let mut scores = Vec::with_capacity(inputs.len());
        for wave in inputs.chunks(width) {
            if cancellation.is_cancelled() { bail!("run cancelled"); }
            crate::compute::recovery::check_memory_headroom(
                crate::compute::advisor::worker_memory_bytes(manifest).saturating_mul(wave.len() as u64))?;
            scores.extend(wave.par_iter()
            .map(|input| {
                if cancellation.is_cancelled() {
                    return f64::INFINITY;
                }
                let score = simulate(
                    &program,
                    manifest,
                    input,
                    &manifest.integration,
                    false,
                    cancellation,
                    None,
                )
                .map(|report| report.score)
                .unwrap_or(f64::INFINITY);
                let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                progress(0.05 + 0.7 * (generation as f32 + done as f32 / inputs.len().max(1) as f32) / generations as f32,
                    &format!("Particle candidates: {}/{} in generation {}/{}", done, inputs.len(), generation+1, generations));
                score
            })
            .collect::<Vec<_>>());
        }
        for (candidate, score) in population.iter_mut().zip(scores) {
            candidate.score = if score.is_finite() {
                score
            } else {
                f64::INFINITY
            };
        }
        engine.select(&manifest.search, &mut population);
        sort_candidates(&mut population);
        history.push(generation_summary(generation + 1, &population));
        if population
            .first()
            .map(|candidate| candidate.score < best.score)
            .unwrap_or(false)
        {
            best = population[0].clone();
        }
        if generation + 1 < generations {
            population = engine.next(&manifest.search, &population, population_size);
        }
        if cancellation.is_cancelled() { bail!("run cancelled"); }
        if let Some(store) = &checkpoint {
            store.save(crate::compute::recovery::SearchCheckpoint {
                version: 1, manifest_hash: String::new(), population_size,
                next_generation: generation + 1, population: population.clone(), best: best.clone(),
                history: history.clone(), engine: engine.checkpoint(), used_gpu: false, used_cpu: true, warnings: vec![],
            })?;
        }
    }

    if !best.score.is_finite() {
        bail!(
            "all particle candidates failed or produced non-finite objective values; revise the equations, timestep, bounds, or constraints"
        );
    }

    progress(0.78, "Replaying the strongest particle candidate");
    let winning_input = materialize_candidate(
        &program,
        manifest,
        &best.values,
        manifest.search.seed,
    )?;
    let callback = progress.clone();
    let replay_progress: ProgressCallback = std::sync::Arc::new(move |fraction, _| {
        callback(0.78 + 0.1 * fraction, "Replaying particle ensemble (f64)");
    });
    let report = simulate(
        &program,
        manifest,
        &winning_input,
        &manifest.integration,
        true,
        cancellation,
        Some(&replay_progress),
    )?;

    progress(0.9, "Running configured falsification passes");
    let falsification = run_falsification(
        &program,
        manifest,
        &winning_input,
        &report.metrics,
        cancellation,
        progress,
    )?;
    let best_candidate = manifest
        .search
        .variables
        .iter()
        .zip(&best.values)
        .map(|(variable, value)| (variable.name.clone(), *value))
        .collect::<BTreeMap<_, _>>();
    let mut numerical = BTreeMap::new();
    numerical.insert("resumed_after_generation".to_owned(), json!(first_generation));
    numerical.insert("candidate_batch_size".to_owned(), json!(compute.batch_size));
    numerical.insert("pair_search".to_owned(), json!(if program.count >= 64 && program.interaction.cutoff.is_some() {
        "Exact cutoff with spatial neighbor cells (falls back to all pairs for unrepresentable coordinates)"
    } else { "Exact all pairs" }));
    numerical.insert("checkpoint_scope".to_owned(), json!("Completed search generations; final replay and falsification restart from the selected candidate."));
    numerical.insert(
        "integration_method".to_owned(),
        json!(manifest.integration.method),
    );
    numerical.insert(
        "time_step".to_owned(),
        json!(manifest.integration.time_step),
    );
    numerical.insert("steps".to_owned(), json!(report.steps));
    numerical.insert("particles".to_owned(), json!(program.count));
    numerical.insert("dimensions".to_owned(), json!(program.dimensions));
    numerical.insert("precision".to_owned(), json!("f64"));
    numerical.insert(
        "candidate_initial_condition_policy".to_owned(),
        json!("common deterministic ensemble per generation"),
    );

    numerical.insert("requested_end_time".to_owned(), json!(manifest.integration.end_time));
    numerical.insert("reached_end_time".to_owned(), json!(evidence::reached_end(&report.metrics, manifest.integration.start_time, manifest.integration.end_time)));
    numerical.insert("actual_end_time".to_owned(), json!(report.metrics.get("elapsed_time").map(|t| t + manifest.integration.start_time)));
    progress(1.0, "Complete");
    Ok(InternalOutcome {
        backend_used: "Rayon CPU / f64 pairwise runtime".to_owned(),
        output: ExecutionOutput {
            evidence_version: 2,
            constraint_results: report.constraint_results,
            summary: if search_enabled {
                format!(
                    "Evaluated {} particle universes across {} generation(s) and retained bounded evidence for the strongest candidate.",
                    population_size.saturating_mul(generations),
                    generations
                )
            } else {
                "Integrated the authored pairwise particle universe using deterministic f64 arithmetic."
                    .to_owned()
            },
            capability_id: "pairwise_particles".to_owned(),
            metrics: report.metrics,
            objective_score: (!program.objectives.is_empty()).then_some(report.score),
            best_candidate,
            search_history: history,
            series: report.series,
            visualization: report.visualization,
            falsification,
            warnings: vec![
                "Pairwise numerical compute currently uses the bounded CPU runtime. Hardware GPU discovery remains active for rendering and compatible state-vector search workloads."
                    .to_owned(),
            ],
            numerical,
        },
    })
}

fn compile_program(manifest: &ExperimentManifest) -> anyhow::Result<ParticleProgram> {
    let ModelSpec::PairwiseParticles {
        dimensions,
        population,
        interaction,
        boundary,
    } = &manifest.model
    else {
        bail!("manifest does not contain a pairwise particle model");
    };
    Ok(ParticleProgram {
        dimensions: *dimensions,
        count: population.count,
        base_mass: population.mass,
        position: population.position.clone(),
        velocity: population.velocity.clone(),
        interaction: interaction.clone(),
        boundary: boundary.clone(),
        force: Expression::parse(&interaction.radial_force)?,
        external: interaction
            .external_acceleration
            .iter()
            .map(|source| Expression::parse(source))
            .collect::<anyhow::Result<Vec<_>>>()?,
        constant_names: manifest
            .constants
            .iter()
            .map(|value| value.name.clone())
            .collect(),
        constant_values: manifest
            .constants
            .iter()
            .map(|value| value.value)
            .collect(),
        objectives: manifest
            .search
            .objectives
            .iter()
            .map(|value| {
                Ok((
                    value.goal,
                    value.weight,
                    Expression::parse(&value.expression)?,
                ))
            })
            .collect::<anyhow::Result<Vec<_>>>()?,
        constraints: manifest
            .constraints
            .iter()
            .map(|value| Ok((value.tolerance, Expression::parse(&value.expression)?)))
            .collect::<anyhow::Result<Vec<_>>>()?,
        observables: manifest
            .observables
            .iter()
            .map(|value| Expression::parse(&value.expression))
            .collect::<anyhow::Result<Vec<_>>>()?,
    })
}

fn materialize_candidate(
    program: &ParticleProgram,
    manifest: &ExperimentManifest,
    values: &[f64],
    seed: u64,
) -> anyhow::Result<ParticleCandidateInput> {
    let mut input = ParticleCandidateInput {
        mass: program.base_mass,
        position: program.position.clone(),
        velocity: program.velocity.clone(),
        interaction: program.interaction.clone(),
        constants: program.constant_values.clone(),
        seed,
        position_noise: 0.0,
        velocity_noise: 0.0,
    };
    if values.len() != manifest.search.variables.len()
        && !(values.is_empty() && !manifest.search.enabled)
    {
        bail!("candidate value count does not match search variables");
    }
    for (variable, value) in manifest.search.variables.iter().zip(values) {
        apply_particle_target(program, &mut input, &variable.target, *value)?;
    }
    Ok(input)
}

fn apply_particle_target(
    program: &ParticleProgram,
    input: &mut ParticleCandidateInput,
    target: &str,
    value: f64,
) -> anyhow::Result<()> {
    if let Some(name) = target.strip_prefix("constant:") {
        let index = program
            .constant_names
            .iter()
            .position(|candidate| candidate == name)
            .with_context(|| format!("unknown constant target `{name}`"))?;
        input.constants[index] = value;
        return Ok(());
    }
    match target {
        "population.mass" => input.mass = value,
        "interaction.softening" => input.interaction.softening = value,
        "interaction.linear_damping" => input.interaction.linear_damping = value,
        _ => {
            let (distribution_name, field) = target
                .split_once('.')
                .with_context(|| format!("unsupported particle target `{target}`"))?;
            let distribution = match distribution_name {
                "position" => &mut input.position,
                "velocity" => &mut input.velocity,
                _ => bail!("unsupported particle target `{target}`"),
            };
            apply_distribution_target(distribution, field, value)?;
        }
    }
    Ok(())
}

fn apply_distribution_target(
    distribution: &mut DistributionSpec,
    field: &str,
    value: f64,
) -> anyhow::Result<()> {
    match field {
        "radius" => {
            let DistributionSpec::Sphere { radius, .. } = distribution else {
                bail!("radius target requires a sphere distribution");
            };
            *radius = value;
        }
        "thickness" => {
            let DistributionSpec::Sphere { thickness, .. } = distribution else {
                bail!("thickness target requires a sphere distribution");
            };
            *thickness = value;
        }
        "spacing" => {
            let DistributionSpec::Grid { spacing, .. } = distribution else {
                bail!("spacing target requires a grid distribution");
            };
            *spacing = value;
        }
        "jitter" => {
            let DistributionSpec::Grid { jitter, .. } = distribution else {
                bail!("jitter target requires a grid distribution");
            };
            *jitter = value;
        }
        _ => {
            let (vector_name, axis_name) = field
                .split_once('.')
                .with_context(|| format!("unsupported distribution target `{field}`"))?;
            let axis = parse_axis(axis_name)?;
            match (distribution, vector_name) {
                (DistributionSpec::Uniform { min, .. }, "min") => set_axis(min, axis, value)?,
                (DistributionSpec::Uniform { max, .. }, "max") => set_axis(max, axis, value)?,
                (DistributionSpec::Normal { mean, .. }, "mean") => set_axis(mean, axis, value)?,
                (DistributionSpec::Normal { std_dev, .. }, "std_dev") => {
                    set_axis(std_dev, axis, value)?
                }
                _ => bail!("target `{field}` is incompatible with the selected distribution"),
            }
        }
    }
    Ok(())
}

fn parse_axis(value: &str) -> anyhow::Result<usize> {
    match value {
        "x" | "0" => Ok(0),
        "y" | "1" => Ok(1),
        "z" | "2" => Ok(2),
        _ => bail!("unknown vector axis `{value}`"),
    }
}

fn set_axis(values: &mut [f64], axis: usize, value: f64) -> anyhow::Result<()> {
    let target = values
        .get_mut(axis)
        .with_context(|| format!("axis {axis} is outside the configured dimensionality"))?;
    *target = value;
    Ok(())
}

fn particle_target_value(
    program: &ParticleProgram,
    input: &ParticleCandidateInput,
    target: &str,
) -> Option<f64> {
    if let Some(name) = target.strip_prefix("constant:") {
        return program
            .constant_names
            .iter()
            .position(|candidate| candidate == name)
            .and_then(|index| input.constants.get(index))
            .copied();
    }
    match target {
        "population.mass" => Some(input.mass),
        "interaction.softening" => Some(input.interaction.softening),
        "interaction.linear_damping" => Some(input.interaction.linear_damping),
        _ => {
            let (distribution_name, field) = target.split_once('.')?;
            let distribution = match distribution_name {
                "position" => &input.position,
                "velocity" => &input.velocity,
                _ => return None,
            };
            distribution_target_value(distribution, field)
        }
    }
}

fn distribution_target_value(distribution: &DistributionSpec, field: &str) -> Option<f64> {
    match (distribution, field) {
        (DistributionSpec::Sphere { radius, .. }, "radius") => Some(*radius),
        (DistributionSpec::Sphere { thickness, .. }, "thickness") => Some(*thickness),
        (DistributionSpec::Grid { spacing, .. }, "spacing") => Some(*spacing),
        (DistributionSpec::Grid { jitter, .. }, "jitter") => Some(*jitter),
        _ => {
            let (vector_name, axis_name) = field.split_once('.')?;
            let axis = parse_axis(axis_name).ok()?;
            match (distribution, vector_name) {
                (DistributionSpec::Uniform { min, .. }, "min") => min.get(axis).copied(),
                (DistributionSpec::Uniform { max, .. }, "max") => max.get(axis).copied(),
                (DistributionSpec::Normal { mean, .. }, "mean") => mean.get(axis).copied(),
                (DistributionSpec::Normal { std_dev, .. }, "std_dev") => {
                    std_dev.get(axis).copied()
                }
                _ => None,
            }
        }
    }
}

fn simulate(
    program: &ParticleProgram,
    manifest: &ExperimentManifest,
    input: &ParticleCandidateInput,
    integration: &IntegrationSpec,
    capture: bool,
    cancellation: &CancellationToken,
    step_progress: Option<&ProgressCallback>,
) -> anyhow::Result<ParticleReport> {
    let mut random = ChaCha20Rng::seed_from_u64(input.seed);
    let mut positions = sample_distribution(
        &input.position,
        program.dimensions,
        program.count,
        &mut random,
    )?;
    let mut velocities = sample_distribution(
        &input.velocity,
        program.dimensions,
        program.count,
        &mut random,
    )?;
    add_noise(&mut positions, input.position_noise, &mut random)?;
    add_noise(&mut velocities, input.velocity_noise, &mut random)?;

    let initial_positions = positions.clone();
    let initial_velocities = velocities.clone();
    let initial_metrics = particle_base_metrics(
        program,
        input,
        &initial_positions,
        &initial_velocities,
    )?;

    let requested_steps = ((integration.end_time - integration.start_time) / integration.time_step)
        .ceil()
        .max(1.0) as usize;
    let steps = requested_steps.min(integration.max_steps);
    let max_frames = manifest.visualization.max_frames.max(1);
    let frame_stride = integration
        .output_stride
        .max((steps + max_frames.saturating_sub(2)) / max_frames.saturating_sub(1).max(1)).max(1);
    let capture_visuals = capture && manifest.visualization.kind != VisualizationKind::None;
    let mut time = integration.start_time;
    let mut completed_steps = 0usize;
    let mut frames = Vec::new();
    let mut series_points = manifest
        .observables
        .iter()
        .map(|_| Vec::<[f64; 2]>::new())
        .collect::<Vec<_>>();

    if capture {
        let context = particle_analysis_context(
            program,
            manifest,
            input,
            &positions,
            &velocities,
            &initial_metrics,
            time,
            integration.start_time,
            completed_steps,
        )?;
        capture_particle_output(
            program,
            manifest,
            &positions,
            &velocities,
            time,
            &context,
            &mut series_points,
            &mut frames,
            capture_visuals,
        )?;
    }

    let mut acceleration = accelerations(program, input, &positions, &velocities, time)?;
    for step in 0..steps {
        if step % 32 == 0 && cancellation.is_cancelled() {
            bail!("run cancelled");
        }
        let dt = (integration.end_time - time).min(integration.time_step);
        if dt <= 0.0 {
            break;
        }
        match integration.method {
            IntegrationMethod::Euler => {
                for particle in 0..program.count {
                    for axis in 0..program.dimensions {
                        velocities[particle][axis] += acceleration[particle][axis] * dt;
                        positions[particle][axis] += velocities[particle][axis] * dt;
                    }
                }
                apply_boundaries(&program.boundary, &mut positions, &mut velocities)?;
                time += dt;
                acceleration = accelerations(program, input, &positions, &velocities, time)?;
            }
            IntegrationMethod::VelocityVerlet => {
                for particle in 0..program.count {
                    for axis in 0..program.dimensions {
                        positions[particle][axis] += velocities[particle][axis] * dt
                            + 0.5 * acceleration[particle][axis] * dt * dt;
                    }
                }
                apply_boundaries(&program.boundary, &mut positions, &mut velocities)?;
                let next_acceleration =
                    accelerations(program, input, &positions, &velocities, time + dt)?;
                for particle in 0..program.count {
                    for axis in 0..program.dimensions {
                        velocities[particle][axis] +=
                            0.5 * (acceleration[particle][axis] + next_acceleration[particle][axis])
                                * dt;
                    }
                }
                acceleration = next_acceleration;
                time += dt;
            }
            IntegrationMethod::Rk4 => bail!("RK4 is not enabled for pairwise particle state"),
        }
        completed_steps += 1;
        if completed_steps % (steps / 100).max(128) == 0 || completed_steps == steps {
            if let Some(callback) = step_progress { callback(completed_steps as f32 / steps as f32, "integration"); }
        }
        if positions
            .iter()
            .flatten()
            .chain(velocities.iter().flatten())
            .any(|value| !value.is_finite())
        {
            bail!("particle state became non-finite at integration step {step}");
        }
        if capture && (completed_steps % frame_stride == 0 || completed_steps == steps) {
            let context = particle_analysis_context(
                program,
                manifest,
                input,
                &positions,
                &velocities,
                &initial_metrics,
                time,
                integration.start_time,
                completed_steps,
            )?;
            capture_particle_output(
                program,
                manifest,
                &positions,
                &velocities,
                time,
                &context,
                &mut series_points,
                &mut frames,
                capture_visuals,
            )?;
        }
    }

    let context = particle_analysis_context(
        program,
        manifest,
        input,
        &positions,
        &velocities,
        &initial_metrics,
        time,
        integration.start_time,
        completed_steps,
    )?;
    let score = score_context(program, &context)?;
    let mut metrics = context
        .iter()
        .map(|(name, value)| (name.clone(), *value))
        .collect::<BTreeMap<_, _>>();
    for (observable, expression) in manifest.observables.iter().zip(&program.observables) {
        let value = expression.eval(&context)?;
        if !value.is_finite() {
            bail!("observable `{}` produced a non-finite value", observable.name);
        }
        metrics.insert(observable.name.clone(), value);
    }
    let series = manifest
        .observables
        .iter()
        .zip(series_points)
        .map(|(observable, points)| TimeSeries {
            name: observable.name.clone(),
            unit: observable.unit.clone(),
            points,
        })
        .collect();

    Ok(ParticleReport {
        constraint_results: evidence::constraints(&manifest.constraints, &context),
        score,
        metrics,
        series,
        visualization: VisualizationOutput {
            kind: if capture_visuals {
                "particle_cloud".to_owned()
            } else {
                "none".to_owned()
            },
            point_size: manifest.visualization.point_size,
            trails: manifest.visualization.trails,
            frames,
        },
        steps: completed_steps,
    })
}

fn add_noise(
    values: &mut [Vec<f64>],
    magnitude: f64,
    random: &mut ChaCha20Rng,
) -> anyhow::Result<()> {
    if magnitude == 0.0 {
        return Ok(());
    }
    let distribution = Normal::new(0.0, magnitude.abs())?;
    for vector in values {
        for value in vector {
            *value += distribution.sample(random);
        }
    }
    Ok(())
}

fn sample_distribution(
    distribution: &DistributionSpec,
    dimensions: usize,
    count: usize,
    random: &mut ChaCha20Rng,
) -> anyhow::Result<Vec<Vec<f64>>> {
    match distribution {
        DistributionSpec::Explicit { values } => Ok(values.clone()),
        DistributionSpec::Uniform { min, max } => Ok((0..count)
            .map(|_| {
                (0..dimensions)
                    .map(|axis| random.gen_range(min[axis]..=max[axis]))
                    .collect()
            })
            .collect()),
        DistributionSpec::Normal { mean, std_dev } => {
            let distributions = (0..dimensions)
                .map(|axis| Normal::new(mean[axis], std_dev[axis]))
                .collect::<Result<Vec<_>, _>>()?;
            Ok((0..count)
                .map(|_| {
                    distributions
                        .iter()
                        .map(|distribution| distribution.sample(random))
                        .collect()
                })
                .collect())
        }
        DistributionSpec::Grid {
            extent,
            spacing,
            jitter,
        } => {
            let center = extent
                .iter()
                .map(|value| (*value as f64 - 1.0) * 0.5)
                .collect::<Vec<_>>();
            Ok((0..count)
                .map(|index| {
                    let mut remainder = index;
                    (0..dimensions)
                        .map(|axis| {
                            let coordinate = remainder % extent[axis];
                            remainder /= extent[axis];
                            (coordinate as f64 - center[axis]) * spacing
                                + random.gen_range(-*jitter..=*jitter)
                        })
                        .collect()
                })
                .collect())
        }
        DistributionSpec::Sphere { radius, thickness } => {
            let unit_normal = Normal::new(0.0, 1.0)?;
            Ok((0..count)
                .map(|_| {
                    let mut vector = (0..dimensions)
                        .map(|_| unit_normal.sample(random))
                        .collect::<Vec<f64>>();
                    let norm = vector
                        .iter()
                        .map(|value| value * value)
                        .sum::<f64>()
                        .sqrt()
                        .max(1.0e-12);
                    let sampled_radius =
                        (radius + random.gen_range(-0.5..=0.5) * thickness).max(0.0);
                    for value in &mut vector {
                        *value = *value / norm * sampled_radius;
                    }
                    vector
                })
                .collect())
        }
    }
}

/// Exact neighbor lookup for finite-range forces. Candidate indices retain the
/// original ascending order, preserving deterministic floating-point accumulation.
struct NeighborCells {
    cells: HashMap<[i64; 3], Vec<usize>>,
    keys: Vec<[i64; 3]>,
    periodic_counts: Option<[i64; 3]>,
    dimensions: usize,
}

impl NeighborCells {
    fn new(program: &ParticleProgram, input: &ParticleCandidateInput, positions: &[Vec<f64>]) -> Option<Self> {
        let cutoff = input.interaction.cutoff.filter(|c| c.is_finite() && *c > 0.0)?;
        if program.count < 64 { return None; }
        let mut counts = [1_i64; 3];
        let mut widths = [cutoff; 3];
        let mut origins = [0.0; 3];
        let periodic = if let BoundarySpec::Periodic { min, max } = &program.boundary {
            for axis in 0..program.dimensions {
                counts[axis] = ((max[axis] - min[axis]) / cutoff).floor().clamp(1.0, 1_000_000_000.0) as i64;
                widths[axis] = (max[axis] - min[axis]) / counts[axis] as f64;
                origins[axis] = min[axis];
            }
            Some(counts)
        } else { None };
        let mut cells: HashMap<[i64; 3], Vec<usize>> = HashMap::new();
        let mut keys = Vec::with_capacity(positions.len());
        for (index, position) in positions.iter().enumerate() {
            let mut key = [0_i64; 3];
            for axis in 0..program.dimensions {
                let coordinate = ((position[axis] - origins[axis]) / widths[axis]).floor();
                if !coordinate.is_finite() || coordinate.abs() >= (i64::MAX / 2) as f64 { return None; }
                key[axis] = coordinate as i64;
                if let Some(counts) = periodic { key[axis] = key[axis].rem_euclid(counts[axis]); }
            }
            cells.entry(key).or_default().push(index);
            keys.push(key);
        }
        Some(Self { cells, keys, periodic_counts: periodic, dimensions: program.dimensions })
    }

    fn following(&self, left: usize) -> Vec<usize> {
        let mut neighbors = Vec::new();
        let mut visited = HashSet::new();
        for x in -1..=1 {
            for y in if self.dimensions >= 2 { -1..=1 } else { 0..=0 } {
                for z in if self.dimensions >= 3 { -1..=1 } else { 0..=0 } {
                    let offsets = [x, y, z];
                    let mut key = self.keys[left];
                    for axis in 0..self.dimensions {
                        key[axis] += offsets[axis];
                        if let Some(counts) = self.periodic_counts { key[axis] = key[axis].rem_euclid(counts[axis]); }
                    }
                    if visited.insert(key) {
                        if let Some(indices) = self.cells.get(&key) {
                            neighbors.extend(indices.iter().copied().filter(|right| *right > left));
                        }
                    }
                }
            }
        }
        neighbors.sort_unstable();
        neighbors
    }
}

fn accelerations(
    program: &ParticleProgram,
    input: &ParticleCandidateInput,
    positions: &[Vec<f64>],
    velocities: &[Vec<f64>],
    time: f64,
) -> anyhow::Result<Vec<Vec<f64>>> {
    accelerations_impl(program, input, positions, velocities, time, true)
}

fn accelerations_impl(
    program: &ParticleProgram,
    input: &ParticleCandidateInput,
    positions: &[Vec<f64>],
    velocities: &[Vec<f64>],
    time: f64,
    use_neighbors: bool,
) -> anyhow::Result<Vec<Vec<f64>>> {
    let mut acceleration = vec![vec![0.0; program.dimensions]; program.count];
    let constants = program
        .constant_names
        .iter()
        .zip(&input.constants)
        .map(|(name, value)| (name.clone(), *value))
        .collect::<HashMap<_, _>>();

    let neighbors = if use_neighbors { NeighborCells::new(program, input, positions) } else { None };
    let mut pair_context = constants.clone();
    for name in ["r", "inv_r", "mass_i", "mass_j", "relative_speed", "t"] {
        pair_context.insert(name.to_owned(), 0.0);
    }
    for left in 0..program.count {
        let following = neighbors.as_ref().map(|cells| cells.following(left));
        let pairs: Box<dyn Iterator<Item = usize>> = match following {
            Some(indices) => Box::new(indices.into_iter()),
            None => Box::new((left + 1)..program.count),
        };
        for right in pairs {
            let displacement = displacement_with_boundary(
                &program.boundary,
                &positions[left],
                &positions[right],
            )?;
            let raw_distance = displacement
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt();
            if input
                .interaction
                .cutoff
                .map(|cutoff| raw_distance > cutoff)
                .unwrap_or(false)
            {
                continue;
            }
            let distance = (raw_distance * raw_distance
                + input.interaction.softening * input.interaction.softening)
                .sqrt()
                .max(1.0e-12);
            let relative_speed = (0..program.dimensions)
                .map(|axis| velocities[right][axis] - velocities[left][axis])
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt();
            for (name, value) in [("r", distance), ("inv_r", 1.0 / distance), ("mass_i", input.mass),
                ("mass_j", input.mass), ("relative_speed", relative_speed), ("t", time)] {
                *pair_context.get_mut(name).expect("pair context initialized") = value;
            }
            let force = program.force.eval(&pair_context)?;
            if !force.is_finite() {
                bail!("pairwise force expression produced a non-finite value");
            }
            for axis in 0..program.dimensions {
                let component = force * displacement[axis] / distance;
                acceleration[left][axis] += component / input.mass;
                acceleration[right][axis] -= component / input.mass;
            }
        }
    }

    for particle in 0..program.count {
        if !program.external.is_empty() {
            let mut context = constants.clone();
            context.insert("t".to_owned(), time);
            for axis in 0..3 {
                context.insert(
                    format!("x{axis}"),
                    positions[particle].get(axis).copied().unwrap_or(0.0),
                );
                context.insert(
                    format!("v{axis}"),
                    velocities[particle].get(axis).copied().unwrap_or(0.0),
                );
            }
            for axis in 0..program.dimensions {
                let value = program.external[axis].eval(&context)?;
                if !value.is_finite() {
                    bail!("external acceleration component {axis} became non-finite");
                }
                acceleration[particle][axis] += value;
            }
        }
        for axis in 0..program.dimensions {
            acceleration[particle][axis] -=
                input.interaction.linear_damping * velocities[particle][axis];
        }
    }
    Ok(acceleration)
}

fn displacement_with_boundary(
    boundary: &BoundarySpec,
    left: &[f64],
    right: &[f64],
) -> anyhow::Result<Vec<f64>> {
    let mut displacement = right
        .iter()
        .zip(left)
        .map(|(right, left)| right - left)
        .collect::<Vec<_>>();
    if let BoundarySpec::Periodic { min, max } = boundary {
        for axis in 0..displacement.len() {
            let width = max[axis] - min[axis];
            displacement[axis] -= width * (displacement[axis] / width).round();
        }
    }
    Ok(displacement)
}

fn apply_boundaries(
    boundary: &BoundarySpec,
    positions: &mut [Vec<f64>],
    velocities: &mut [Vec<f64>],
) -> anyhow::Result<()> {
    match boundary {
        BoundarySpec::Open => Ok(()),
        BoundarySpec::Periodic { min, max } => {
            for position in positions.iter_mut() {
                for axis in 0..position.len() {
                    let width = max[axis] - min[axis];
                    position[axis] = min[axis] + (position[axis] - min[axis]).rem_euclid(width);
                }
            }
            Ok(())
        }
        BoundarySpec::Reflective { min, max } => {
            for (position, velocity) in positions.iter_mut().zip(velocities.iter_mut()) {
                for axis in 0..position.len() {
                    let width = max[axis] - min[axis];
                    let folded = (position[axis] - min[axis]).rem_euclid(2.0 * width);
                    if folded <= width {
                        position[axis] = min[axis] + folded;
                    } else {
                        position[axis] = max[axis] - (folded - width);
                        velocity[axis] = -velocity[axis];
                    }
                }
            }
            Ok(())
        }
    }
}

fn particle_base_metrics(
    program: &ParticleProgram,
    input: &ParticleCandidateInput,
    positions: &[Vec<f64>],
    velocities: &[Vec<f64>],
) -> anyhow::Result<HashMap<String, f64>> {
    let divisor = program.count.max(1) as f64;
    let mut center = vec![0.0; program.dimensions];
    let mut momentum = vec![0.0; program.dimensions];
    let mut kinetic_energy = 0.0;
    let mut mean_speed = 0.0;
    for (position, velocity) in positions.iter().zip(velocities) {
        for axis in 0..program.dimensions {
            center[axis] += position[axis];
            momentum[axis] += input.mass * velocity[axis];
        }
        let speed_squared = velocity.iter().map(|value| value * value).sum::<f64>();
        mean_speed += speed_squared.sqrt();
        kinetic_energy += 0.5 * input.mass * speed_squared;
    }
    for value in &mut center {
        *value /= divisor;
    }
    mean_speed /= divisor;

    let mut spread = vec![0.0; program.dimensions];
    let radii = positions
        .iter()
        .map(|position| {
            let mut squared = 0.0;
            for axis in 0..program.dimensions {
                let delta = position[axis] - center[axis];
                squared += delta * delta;
                spread[axis] += delta * delta;
            }
            squared.sqrt()
        })
        .collect::<Vec<_>>();
    for value in &mut spread {
        *value = (*value / divisor).sqrt();
    }

    let mean_radius = radii.iter().sum::<f64>() / radii.len().max(1) as f64;
    let rms_radius = (radii.iter().map(|value| value * value).sum::<f64>()
        / radii.len().max(1) as f64)
        .sqrt();
    let max_radius = radii.iter().copied().fold(0.0, f64::max);
    let momentum_norm = momentum
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    let mut minimum_distance = f64::INFINITY;
    for left in 0..program.count {
        for right in (left + 1)..program.count {
            let distance = displacement_with_boundary(
                &program.boundary,
                &positions[left],
                &positions[right],
            )?
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
            minimum_distance = minimum_distance.min(distance);
        }
    }
    if !minimum_distance.is_finite() {
        minimum_distance = 0.0;
    }

    let mut context = HashMap::from([
        ("particle_count".to_owned(), program.count as f64),
        ("mean_radius".to_owned(), mean_radius),
        ("rms_radius".to_owned(), rms_radius),
        ("max_radius".to_owned(), max_radius),
        ("mean_speed".to_owned(), mean_speed),
        ("kinetic_energy".to_owned(), kinetic_energy),
        ("momentum_norm".to_owned(), momentum_norm),
        ("minimum_distance".to_owned(), minimum_distance),
    ]);
    for axis in 0..3 {
        let suffix = ["x", "y", "z"][axis];
        context.insert(
            format!("center_of_mass_{suffix}"),
            center.get(axis).copied().unwrap_or(0.0),
        );
        context.insert(
            format!("spread_{suffix}"),
            spread.get(axis).copied().unwrap_or(0.0),
        );
    }
    Ok(context)
}

#[allow(clippy::too_many_arguments)]
fn particle_analysis_context(
    program: &ParticleProgram,
    manifest: &ExperimentManifest,
    input: &ParticleCandidateInput,
    positions: &[Vec<f64>],
    velocities: &[Vec<f64>],
    initial_metrics: &HashMap<String, f64>,
    time: f64,
    start_time: f64,
    completed_steps: usize,
) -> anyhow::Result<HashMap<String, f64>> {
    let current = particle_base_metrics(program, input, positions, velocities)?;
    let mut context = current.clone();
    context.insert("t".to_owned(), time);
    context.insert("elapsed_time".to_owned(), time - start_time);
    context.insert("steps".to_owned(), completed_steps as f64);
    context.insert("sample_count".to_owned(), completed_steps.saturating_add(1) as f64);
    for (name, value) in &current {
        let initial = initial_metrics.get(name).copied().unwrap_or(*value);
        let delta = *value - initial;
        context.insert(format!("initial_{name}"), initial);
        context.insert(format!("delta_{name}"), delta);
        context.insert(format!("abs_delta_{name}"), delta.abs());
    }
    for (name, value) in program.constant_names.iter().zip(&input.constants) {
        context.insert(name.clone(), *value);
    }
    for variable in &manifest.search.variables {
        if let Some(value) = particle_target_value(program, input, &variable.target) {
            context.insert(variable.name.clone(), value);
        }
    }
    Ok(context)
}

fn score_context(program: &ParticleProgram, context: &HashMap<String, f64>) -> anyhow::Result<f64> {
    let mut score = 0.0;
    for (goal, weight, expression) in &program.objectives {
        let value = expression.eval(context)?;
        if !value.is_finite() {
            return Ok(f64::INFINITY);
        }
        score += weight
            * match goal {
                ObjectiveGoal::Minimize => value,
                ObjectiveGoal::Maximize => -value,
            };
    }
    for (tolerance, expression) in &program.constraints {
        let value = expression.eval(context)?;
        if !value.is_finite() {
            return Ok(f64::INFINITY);
        }
        let violation = (value - tolerance).max(0.0);
        score += 1.0e6 * violation * violation;
    }
    Ok(if score.is_finite() {
        score
    } else {
        f64::INFINITY
    })
}

#[allow(clippy::too_many_arguments)]
fn capture_particle_output(
    program: &ParticleProgram,
    manifest: &ExperimentManifest,
    positions: &[Vec<f64>],
    velocities: &[Vec<f64>],
    time: f64,
    context: &HashMap<String, f64>,
    series_points: &mut [Vec<[f64; 2]>],
    frames: &mut Vec<VisualFrame>,
    capture_visuals: bool,
) -> anyhow::Result<()> {
    for (index, expression) in program.observables.iter().enumerate() {
        let value = expression.eval(context)?;
        if value.is_finite() {
            series_points[index].push([time, value]);
        }
    }
    if capture_visuals && frames.len() < manifest.visualization.max_frames {
        frames.push(VisualFrame {
            time,
            entities: positions
                .iter()
                .zip(velocities)
                .enumerate()
                .map(|(index, (position, velocity))| VisualEntity {
                    radius: None,
                    id: format!("p{index}"),
                    position: [
                        position.first().copied().unwrap_or(0.0),
                        position.get(1).copied().unwrap_or(0.0),
                        position.get(2).copied().unwrap_or(0.0),
                    ],
                    velocity: Some([
                        velocity.first().copied().unwrap_or(0.0),
                        velocity.get(1).copied().unwrap_or(0.0),
                        velocity.get(2).copied().unwrap_or(0.0),
                    ]),
                    scalar: None,
                })
                .collect(),
        });
    }
    Ok(())
}

fn run_falsification(
    program: &ParticleProgram,
    manifest: &ExperimentManifest,
    winning_input: &ParticleCandidateInput,
    baseline_metrics: &BTreeMap<String, f64>,
    cancellation: &CancellationToken,
    progress: &ProgressCallback,
) -> anyhow::Result<Vec<FalsificationResult>> {
    let mut results = Vec::new();
    for test in &manifest.falsification {
        let mut trials = Vec::with_capacity(test.repetitions);
        let mut note = String::new();
        for repetition in 0..test.repetitions {
            if cancellation.is_cancelled() {
                bail!("run cancelled");
            }
            let mut input = winning_input.clone();
            let mut integration = manifest.integration.clone();
            note = match test.kind {
                FalsificationKind::ResolutionLadder => {
                    integration=evidence::refinement(&manifest.integration,repetition,500000)?;
                    "Distinct resolution ladder h/2, h/4, ...; same candidate, full horizon. Not a proof of convergence order.".to_owned()
                }
                FalsificationKind::StepHalving => {
                    integration.time_step *= 0.5;
                    integration.max_steps = integration.max_steps.saturating_mul(2).min(500_000);
                    "Replayed the candidate at half the integration step.".to_owned()
                }
                FalsificationKind::SeedReplication => {
                    input.seed = input.seed.wrapping_add(repetition as u64 + 1);
                    "Regenerated the ensemble from independent deterministic seeds.".to_owned()
                }
                FalsificationKind::InitialPerturbation => {
                    // Same ensemble seed: isolate additive noise, rather than regenerating the universe.
                    match test.target.as_deref() {
                        Some("position") => input.position_noise = test.magnitude.abs() * (repetition + 1) as f64,
                        Some("velocity") => input.velocity_noise = test.magnitude.abs() * (repetition + 1) as f64,
                        Some(target) => bail!("unsupported initial perturbation target `{target}`"),
                        None => bail!("initial perturbation requires a target"),
                    }
                    format!(
                        "Added deterministic Gaussian noise with magnitude {} to the generated {} ensemble.",
                        test.magnitude.abs(),
                        test.target.as_deref().unwrap_or("initial")
                    )
                }
                FalsificationKind::ParameterPerturbation => {
                    let target = test
                        .target
                        .as_deref()
                        .context("parameter perturbation requires a target")?;
                    let baseline = particle_target_value(program, &input, target)
                        .with_context(|| format!("unknown parameter perturbation target `{target}`"))?;
                    apply_particle_target(program, &mut input, target, baseline + evidence::perturbation(test.magnitude, repetition))?;
                    format!("Perturbed `{target}` by {} before replay.", test.magnitude)
                }
            };
            let evaluated = simulate(
                program,
                manifest,
                &input,
                &integration,
                false,
                cancellation,
                None,
            );

            let delta = match test.kind {
                FalsificationKind::InitialPerturbation => Some(test.magnitude.abs() * (repetition + 1) as f64),
                FalsificationKind::ParameterPerturbation => Some(evidence::perturbation(test.magnitude, repetition)),
                _ => None,
            };
            let (metrics,constraints,reached)=match evaluated {
                Ok(value)=>{let complete=evidence::reached_end(&value.metrics,integration.start_time,integration.end_time);(value.metrics,value.constraint_results,complete)},
                Err(e)=>{if cancellation.is_cancelled(){bail!("run cancelled");}(BTreeMap::new(),vec![evidence::ConstraintResult{name:"challenge_execution".into(),expression:String::new(),value:None,tolerance:0.0,status:"inconclusive".into(),note:format!("Challenge did not complete: {e:#}").chars().take(2000).collect()}],false)},
            };
            trials.push(evidence::ChallengeTrial {
                repetition:repetition+1,perturbation:delta,time_step:integration.time_step,reached_end_time:reached,
                metrics,constraint_results:constraints,
            });
            let total: usize = manifest.falsification.iter().map(|t| t.repetitions).sum();
            let done: usize = manifest.falsification.iter().take(results.len()).map(|t|t.repetitions).sum::<usize>() + repetition + 1;
            progress(0.9 + 0.08 * done as f32 / total.max(1) as f32,
                &format!("Validating {}: trial {}/{}", test.name, repetition + 1, test.repetitions));
        }
        results.push(evidence::compare(test, baseline_metrics, trials, note));
    }
    Ok(results)
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use crate::{domain::ExperimentManifestDraft, compute::recovery::CheckpointStore, persistence::Database};
    use std::sync::Arc;
    use uuid::Uuid;

    fn fixture() -> ExperimentManifest {
        let mut draft: ExperimentManifestDraft = serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
        draft.model = serde_json::from_value(json!({"kind":"pairwise_particles","dimensions":3,
            "population":{"count":64,"mass":1.0,
                "position":{"kind":"grid","extent":[4,4,4],"spacing":1.0,"jitter":0.0},
                "velocity":{"kind":"normal","mean":[0,0,0],"std_dev":[0.1,0.1,0.1]}},
            "interaction":{"radial_force":"-1/(r*r)","cutoff":1.1,"softening":0.1,"linear_damping":0.0,"external_acceleration":[]},
            "boundary":{"kind":"open"}})).unwrap();
        draft.integration.method = IntegrationMethod::VelocityVerlet;
        draft.integration.end_time = 0.04;
        draft.integration.time_step = 0.01;
        draft.visualization.max_frames = 3;
        draft.visualization.kind = VisualizationKind::ParticleCloud;
        draft.observables = serde_json::from_value(json!([{"name":"energy","expression":"kinetic_energy","unit":"model"}])).unwrap();
        draft.search.enabled = true;
        draft.search.algorithm = SearchAlgorithm::DifferentialEvolution;
        draft.search.variables = serde_json::from_value(json!([{"name":"damping","target":"interaction.linear_damping","minimum":0.0,"maximum":0.2}])).unwrap();
        draft.search.objectives = serde_json::from_value(json!([{"name":"kinetic","expression":"kinetic_energy","goal":"minimize","weight":1.0}])).unwrap();
        draft.search.population = 4;
        draft.search.generations = 3;
        draft.compute.candidate_count = 4;
        draft.compute.batch_size = 2;
        ExperimentManifest::from_draft(Uuid::nil(), None, 1, draft, "particle runtime test")
    }

    #[test]
    fn cutoff_cells_match_all_pairs_exactly_in_open_and_periodic_domains() {
        for boundary in [BoundarySpec::Open, BoundarySpec::Periodic { min: vec![-2.0;3], max: vec![2.0;3] },
            BoundarySpec::Periodic { min: vec![-0.7;3], max: vec![0.7;3] }] {
            let manifest = fixture();
            let mut program = compile_program(&manifest).unwrap();
            program.boundary = boundary;
            let input = materialize_candidate(&program, &manifest, &[0.0], 7).unwrap();
            let positions = (0..64).map(|i| vec![(i%4) as f64*0.6-0.9, ((i/4)%4) as f64*0.6-0.9, (i/16) as f64*0.6-0.9]).collect::<Vec<_>>();
            let velocities = vec![vec![0.0;3];64];
            assert!(NeighborCells::new(&program, &input, &positions).is_some());
            let expected = accelerations_impl(&program, &input, &positions, &velocities, 0.0, false).unwrap();
            let actual = accelerations_impl(&program, &input, &positions, &velocities, 0.0, true).unwrap();
            assert_eq!(actual, expected);
            for axis in 0..3 {
                assert!(actual.iter().map(|v|v[axis]).sum::<f64>().abs() < 1e-10, "equal and opposite forces must conserve momentum");
            }
        }
    }

    #[test]
    fn particle_checkpoint_replays_identically_with_a_smaller_batch_and_retains_final_frame() {
        let manifest = fixture();
        let temp = tempfile::tempdir().unwrap();
        let db = Database::open(&temp.path().join("particles.sqlite3")).unwrap();
        let store = CheckpointStore::new(db, Uuid::new_v4(), &manifest).unwrap();
        let noop: ProgressCallback = Arc::new(|_,_|{});
        let baseline = execute(&manifest, &manifest.compute, &CancellationToken::new(), &noop).unwrap();
        let cancellation = CancellationToken::new();
        let target = cancellation.clone();
        let interrupt: ProgressCallback = Arc::new(move |_, phase| {
            if phase == "Evaluating particle candidate generation 2/3" { target.cancel(); }
        });
        assert!(execute_checkpointed(&manifest, &manifest.compute, &cancellation, &interrupt, Some(store.clone())).is_err());
        assert_eq!(store.load(4,3).unwrap().unwrap().next_generation, 1);
        let mut allocation = manifest.compute.clone();
        allocation.batch_size = 1;
        let resumed = execute_checkpointed(&manifest, &allocation, &CancellationToken::new(), &noop, Some(store)).unwrap();
        assert_eq!(resumed.output.metrics, baseline.output.metrics);
        assert_eq!(resumed.output.best_candidate, baseline.output.best_candidate);
        assert_eq!(serde_json::to_value(resumed.output.search_history).unwrap(), serde_json::to_value(baseline.output.search_history).unwrap());
        assert!(resumed.output.visualization.frames.len() <= manifest.visualization.max_frames);
        assert_eq!(resumed.output.visualization.frames.last().unwrap().time, manifest.integration.end_time);
    }
}
