use super::evidence;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use anyhow::{Context, bail};
use async_trait::async_trait;
use rayon::prelude::*;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{
    domain::{
        ComputePreference, ComputeRequest, ExperimentManifest, FalsificationKind,
        IntegrationMethod, IntegrationSpec, ModelSpec, ObjectiveGoal, SearchAlgorithm,
        VisualizationKind,
    },
    sandbox::Expression,
};

use super::{
    ExecutionOutput, FalsificationResult, InternalOutcome, ProgressCallback, TimeSeries,
    VisualEntity, VisualFrame, VisualizationOutput,
    search::{
        Candidate, PopulationEngine, SearchGeneration, generation_summary, sort_candidates,
    },
};

#[derive(Clone)]
pub(crate) struct CompiledObjective {
    pub goal: ObjectiveGoal,
    pub weight: f64,
    pub expression: Expression,
}

#[derive(Clone)]
pub(crate) struct CompiledConstraint {
    pub tolerance: f64,
    pub expression: Expression,
}

#[derive(Clone)]
pub(crate) struct OdeProgram {
    pub state_names: Vec<String>,
    pub initial_state: Vec<f64>,
    pub derivatives: Vec<Expression>,
    pub constant_names: Vec<String>,
    pub constant_values: Vec<f64>,
    pub objectives: Vec<CompiledObjective>,
    pub constraints: Vec<CompiledConstraint>,
}

#[derive(Debug, Clone)]
pub(crate) struct OdeCandidateInput {
    pub state: Vec<f64>,
    pub constants: Vec<f64>,
}

#[async_trait]
pub(crate) trait OdeBatchEvaluator: Send + Sync {
    fn take_execution_notes(&self) -> Vec<String> { Vec::new() }
    fn label(&self) -> String;
    fn compatibility(&self, program: &OdeProgram, manifest: &ExperimentManifest)
        -> anyhow::Result<()>;
    async fn evaluate(
        &self,
        program: &OdeProgram,
        manifest: &ExperimentManifest,
        candidates: &[OdeCandidateInput],
        compute: &ComputeRequest,
        cancellation: &CancellationToken,
    ) -> anyhow::Result<Vec<f64>>;
}

pub(crate) async fn execute(
    manifest: ExperimentManifest,
    compute: ComputeRequest,
    gpu: Option<Arc<dyn OdeBatchEvaluator>>,
    cancellation: CancellationToken,
    progress: ProgressCallback,
) -> anyhow::Result<InternalOutcome> {
    execute_checkpointed(manifest, compute, gpu, cancellation, progress, None).await
}

pub(crate) async fn execute_checkpointed(
    manifest: ExperimentManifest,
    compute: ComputeRequest,
    gpu: Option<Arc<dyn OdeBatchEvaluator>>,
    cancellation: CancellationToken,
    progress: ProgressCallback,
    checkpoint: Option<crate::compute::recovery::CheckpointStore>,
) -> anyhow::Result<InternalOutcome> {
    let program = compile_program(&manifest)?;
    let mut warnings = Vec::new();
    let mut history = Vec::<SearchGeneration>::new();
    let mut best = Candidate::unevaluated(Vec::new());
    let mut backend_used = "Rayon CPU / f64".to_owned();
    let mut used_gpu = false;
    let mut used_cpu = false;
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

    let mut first_generation = 0;
    if let Some(saved) = checkpoint.as_ref().map(|store| store.load(population_size, generations)).transpose()?.flatten() {
        first_generation = saved.next_generation;
        population = saved.population;
        best = saved.best;
        history = saved.history;
        engine = PopulationEngine::restore(saved.engine)?;
        used_gpu = saved.used_gpu;
        used_cpu = saved.used_cpu;
        warnings = saved.warnings;
        warnings.push(format!("Resumed after {first_generation} completed generation(s); any interrupted generation and the final trajectory are replayed."));
    }

    let mut active_gpu = match gpu {
        Some(evaluator) if compute.preference != ComputePreference::Cpu && search_enabled => {
            match evaluator.compatibility(&program, &manifest) {
                Ok(()) => Some(evaluator),
                Err(error) => {
                    warnings.push(format!(
                        "The manifest could not be lowered to generic GPU compute: {error:#}. The same candidate population will run on CPU."
                    ));
                    None
                }
            }
        }
        _ => None,
    };
    if compute.preference == ComputePreference::Gpu && active_gpu.is_none() {
        warnings.push(
            "GPU execution was requested, but no compatible hardware lowerer was available; CPU execution remained enabled."
                .to_owned(),
        );
    }

    for generation in first_generation..generations {
        if cancellation.is_cancelled() {
            bail!("run cancelled");
        }
        if started.elapsed().as_secs() > compute.max_wall_seconds {
            bail!("run exceeded its configured wall-time budget");
        }

        let phase = if search_enabled {
            format!("Evaluating candidate generation {}/{}", generation + 1, generations)
        } else {
            "Integrating experiment".to_owned()
        };
        progress(
            0.06 + 0.68 * generation as f32 / generations.max(1) as f32,
            &phase,
        );

        let inputs = population
            .iter()
            .map(|candidate| materialize_candidate(&program, &manifest, &candidate.values))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let scores = if let Some(evaluator) = active_gpu.as_ref() {
            match evaluator
                .evaluate(&program, &manifest, &inputs, &compute, &cancellation)
                .await
            {
                Ok(scores) if scores.len() == population.len() => {
                    warnings.extend(evaluator.take_execution_notes());
                    used_gpu = true;
                    backend_used = format!("{} / f32 search + CPU f64 replay", evaluator.label());
                    scores
                }
                Ok(_) => {
                    used_cpu = true;
                    warnings.push(
                        "GPU candidate evaluation returned an unexpected result count; the generation was replayed on CPU."
                            .to_owned(),
                    );
                    active_gpu = None;
                    cpu_scores(program.clone(), manifest.clone(), inputs, cancellation.clone(), progress.clone(), generation, generations, compute.batch_size).await?
                }
                Err(error) => {
                    if cancellation.is_cancelled() { bail!("run cancelled"); }
                    used_cpu = true;
                    warnings.push(format!(
                        "GPU candidate evaluation failed and was replayed on CPU: {error:#}"
                    ));
                    active_gpu = None;
                    cpu_scores(program.clone(), manifest.clone(), inputs, cancellation.clone(), progress.clone(), generation, generations, compute.batch_size).await?
                }
            }
        } else {
            used_cpu = true;
            cpu_scores(program.clone(), manifest.clone(), inputs, cancellation.clone(), progress.clone(), generation, generations, compute.batch_size).await?
        };

        for (candidate, score) in population.iter_mut().zip(scores) {
            candidate.score = score;
        }
        engine.select(&manifest.search, &mut population);
        sort_candidates(&mut population);
        history.push(generation_summary(generation + 1, &population));
        if best.score > population[0].score {
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
                history: history.clone(), engine: engine.checkpoint(), used_gpu, used_cpu, warnings: warnings.clone(),
            })?;
        }
    }

    if !best.score.is_finite() {
        bail!("no candidate completed with a finite objective score; inspect the equations, bounds, and timestep");
    }

    progress(0.78, "Replaying the strongest candidate in f64");
    let winning_input = materialize_candidate(&program, &manifest, &best.values)?;
    let replay_program = program.clone();
    let replay_manifest = manifest.clone();
    let replay_input = winning_input.clone();
    let replay_cancellation = cancellation.clone();
    let replay_callback = progress.clone();
    let replay_progress: ProgressCallback = Arc::new(move |fraction, _| {
        replay_callback(0.78 + 0.10 * fraction, "Replaying verified candidate (f64)");
    });
    let report = tokio::task::spawn_blocking(move || {
        simulate(
            &replay_program,
            &replay_manifest,
            &replay_input,
            &replay_manifest.integration,
            true,
            &replay_cancellation,
            Some(&replay_progress),
        )
    })
    .await
    .context("state-vector replay worker panicked")??;

    progress(0.9, "Running configured falsification passes");
    let challenge_program = program.clone();
    let challenge_manifest = manifest.clone();
    let challenge_input = winning_input.clone();
    let challenge_metrics = report.metrics.clone();
    let challenge_cancel = cancellation.clone();
    let challenge_progress = progress.clone();
    let falsification = tokio::task::spawn_blocking(move || run_falsification(
        &challenge_program, &challenge_manifest, &challenge_input, &challenge_metrics,
        &challenge_cancel, &challenge_progress,
    )).await.context("challenge worker panicked")??;

    let best_candidate = manifest
        .search
        .variables
        .iter()
        .zip(&best.values)
        .map(|(variable, value)| (variable.name.clone(), *value))
        .collect::<BTreeMap<_, _>>();

    let mut numerical = BTreeMap::new();
    numerical.insert("trajectory_contract".to_owned(),json!({"measures":manifest.trajectory,
        "sampling":"every fixed integration step, independent of output stride",
        "extrema":"sampled, not exact continuous bounds","duration":"piecewise-linear threshold interpolation",
        "first_passage":"censored at horizon unless NAME_observed=1; initial occupancy counts as time zero, not an entry crossing",
        "collision_physics":false}));
    numerical.insert("integration_method".to_owned(), json!(manifest.integration.method));
    numerical.insert("time_step".to_owned(), json!(manifest.integration.time_step));
    numerical.insert("steps".to_owned(), json!(report.steps));
    numerical.insert("state_dimension".to_owned(), json!(program.state_names.len()));
    numerical.insert("search_precision".to_owned(), json!(match (used_gpu, used_cpu) {
        (true, true) => "mixed f32/f64", (true, false) => "f32", _ => "f64",
    }));
    numerical.insert("resumed_after_generation".to_owned(), json!(first_generation));
    numerical.insert("checkpoint_scope".to_owned(), json!("Completed search generations; final replay and falsification restart from the selected candidate."));
    if used_gpu && used_cpu {
        backend_used = "Mixed GPU f32 + Rayon CPU f64 search / CPU f64 replay".to_owned();
    } else if used_gpu && first_generation == generations {
        backend_used = "Checkpointed GPU f32 search / CPU f64 replay".to_owned();
    }
    numerical.insert("replay_precision".to_owned(), json!("f64"));
    numerical.insert("visual_entity_mapping".to_owned(), json!(if !manifest.visualization.entities.is_empty() {
        "explicit manifest entities"
    } else if report.visualization.frames.first().map(|f|f.entities.len()>1).unwrap_or(false) {
        "inferred coordinate families; verify axes or author explicit entities"
    } else { "declared single state-space trace (not necessarily physical geometry)" }));

    numerical.insert("requested_end_time".to_owned(), json!(manifest.integration.end_time));
    numerical.insert("reached_end_time".to_owned(), json!(evidence::reached_end(&report.metrics, manifest.integration.start_time, manifest.integration.end_time)));
    numerical.insert("actual_end_time".to_owned(), json!(report.metrics.get("elapsed_time").map(|t| t + manifest.integration.start_time)));
    progress(1.0, "Complete");
    Ok(InternalOutcome {
        backend_used,
        output: ExecutionOutput {
            evidence_version: 2,
            constraint_results: report.constraint_results,
            summary: if search_enabled {
                format!(
                    "Evaluated {} candidate states across {} generation(s), then replayed the strongest candidate using deterministic f64 integration.",
                    population_size.saturating_mul(generations),
                    generations
                )
            } else {
                "Integrated the authored state-vector system using deterministic f64 arithmetic."
                    .to_owned()
            },
            capability_id: "state_vector_ode".to_owned(),
            metrics: report.metrics,
            objective_score: (!program.objectives.is_empty()).then_some(report.score),
            best_candidate,
            search_history: history,
            series: report.series,
            visualization: report.visualization,
            falsification,
            warnings,
            numerical,
        },
    })
}

async fn cpu_scores(
    program: OdeProgram, manifest: ExperimentManifest, inputs: Vec<OdeCandidateInput>,
    cancellation: CancellationToken, progress: ProgressCallback, generation: usize, generations: usize, batch_size: usize,
) -> anyhow::Result<Vec<f64>> {
    tokio::task::spawn_blocking(move || {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let done = AtomicUsize::new(0);
        let count = inputs.len().max(1);
        let width=if batch_size==0{rayon::current_num_threads().max(1)}else{batch_size.min(rayon::current_num_threads()).max(1)};
        let mut scores=Vec::with_capacity(inputs.len());
        for wave in inputs.chunks(width){
            if cancellation.is_cancelled(){break;}
            crate::compute::recovery::check_memory_headroom(
                crate::compute::advisor::worker_memory_bytes(&manifest).saturating_mul(wave.len() as u64))?;
            let values=wave.par_iter().map(|input| {
            if cancellation.is_cancelled() { return f64::INFINITY; }
            let cb = progress.clone();
            let hook: ProgressCallback = Arc::new(move |fraction, _| {
                cb(0.06 + 0.68 * (generation as f32 + fraction) / generations as f32, "Integrating authored equations");
            });
            let result = simulate(&program, &manifest, input, &manifest.integration, false,
                &cancellation, if count == 1 { Some(&hook) } else { None })
                .map(|report| report.score).unwrap_or(f64::INFINITY);
            let completed = done.fetch_add(1, Ordering::Relaxed) + 1;
            progress(0.06 + 0.68 * (generation as f32 + completed as f32 / count as f32) / generations as f32,
                &format!("Candidate generation {}/{}: {}/{} evaluated", generation + 1, generations, completed, count));
            result
            }).collect::<Vec<_>>();scores.extend(values);
        }
        scores.resize(inputs.len(),f64::INFINITY);Ok(scores)
    }).await.context("state-vector CPU batch worker panicked")?
}

pub(crate) fn compile_program(manifest: &ExperimentManifest) -> anyhow::Result<OdeProgram> {
    let ModelSpec::StateVectorOde {
        variables,
        derivatives,
    } = &manifest.model
    else {
        bail!("manifest does not contain a state-vector model");
    };

    let state_names = variables.iter().map(|value| value.name.clone()).collect::<Vec<_>>();
    let initial_state = variables.iter().map(|value| value.initial).collect::<Vec<_>>();
    let derivative_by_name = derivatives
        .iter()
        .map(|value| (value.variable.as_str(), value.expression.as_str()))
        .collect::<HashMap<_, _>>();
    let derivatives = state_names
        .iter()
        .map(|name| {
            Expression::parse(
                derivative_by_name
                    .get(name.as_str())
                    .with_context(|| format!("missing derivative for `{name}`"))?,
            )
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let constant_names = manifest
        .constants
        .iter()
        .map(|value| value.name.clone())
        .collect::<Vec<_>>();
    let constant_values = manifest.constants.iter().map(|value| value.value).collect();
    let objectives = manifest
        .search
        .objectives
        .iter()
        .map(|value| {
            Ok(CompiledObjective {
                goal: value.goal,
                weight: value.weight,
                expression: Expression::parse(&value.expression)?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let constraints = manifest
        .constraints
        .iter()
        .map(|value| {
            Ok(CompiledConstraint {
                tolerance: value.tolerance,
                expression: Expression::parse(&value.expression)?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(OdeProgram {
        state_names,
        initial_state,
        derivatives,
        constant_names,
        constant_values,
        objectives,
        constraints,
    })
}

fn materialize_candidate(
    program: &OdeProgram,
    manifest: &ExperimentManifest,
    values: &[f64],
) -> anyhow::Result<OdeCandidateInput> {
    let mut state = program.initial_state.clone();
    let mut constants = program.constant_values.clone();
    if values.len() != manifest.search.variables.len() {
        if !(values.is_empty() && !manifest.search.enabled) {
            bail!("candidate value count does not match search variables");
        }
    }
    for (variable, value) in manifest.search.variables.iter().zip(values) {
        if let Some(name) = variable.target.strip_prefix("initial:") {
            let index = program
                .state_names
                .iter()
                .position(|candidate| candidate == name)
                .with_context(|| format!("unknown initial-state search target `{name}`"))?;
            state[index] = *value;
        } else if let Some(name) = variable.target.strip_prefix("constant:") {
            let index = program
                .constant_names
                .iter()
                .position(|candidate| candidate == name)
                .with_context(|| format!("unknown constant search target `{name}`"))?;
            constants[index] = *value;
        } else {
            bail!("unsupported state-vector search target `{}`", variable.target);
        }
    }
    Ok(OdeCandidateInput { state, constants })
}

struct SimulationReport {
    constraint_results: Vec<evidence::ConstraintResult>,
    score: f64,
    metrics: BTreeMap<String, f64>,
    series: Vec<TimeSeries>,
    visualization: VisualizationOutput,
    steps: usize,
}

fn simulate(
    program: &OdeProgram,
    manifest: &ExperimentManifest,
    input: &OdeCandidateInput,
    integration: &IntegrationSpec,
    capture: bool,
    cancellation: &CancellationToken,
    step_progress: Option<&ProgressCallback>,
) -> anyhow::Result<SimulationReport> {
    let requested_steps = ((integration.end_time - integration.start_time) / integration.time_step)
        .ceil()
        .max(1.0) as usize;
    let steps = requested_steps.min(integration.max_steps);
    let mut state = input.state.clone();
    let initial = state.clone();
    let mut minimum = state.clone();
    let mut maximum = state.clone();
    let mut sums = state.clone();
    let mut samples = 1usize;
    let mut completed_steps = 0usize;
    let mut time = integration.start_time;

    let max_frames = manifest.visualization.max_frames.max(1);
    let frame_stride = integration.output_stride.max((steps + max_frames.saturating_sub(2)) / max_frames.saturating_sub(1).max(1)).max(1);
    let observable_programs = manifest
        .observables
        .iter()
        .map(|value| Expression::parse(&value.expression))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let visual_programs = if capture && manifest.visualization.kind != VisualizationKind::None {
        Some(compile_visual_entities(program, manifest)?)
    } else { None };
    let mut series_points = manifest
        .observables
        .iter()
        .map(|_| Vec::<[f64; 2]>::new())
        .collect::<Vec<_>>();
    let mut frames = Vec::new();
    let measure_context=|values:&[f64],at:f64| {
        let mut c=instantaneous_context(program,values,&input.constants,at);
        for (name,v) in program.state_names.iter().zip(&initial){c.insert(format!("initial_{name}"),*v);}
        c
    };
    let mut tracker=super::trajectory::Tracker::new(&manifest.trajectory,&measure_context(&state,time),time)?;

    if capture {
        let mut context = analysis_context(
            program,
            manifest,
            input,
            &initial,
            &state,
            &minimum,
            &maximum,
            &sums,
            samples,
            time,
            integration.start_time,
            completed_steps,
        );
        tracker.extend(&mut context);
        capture_ode_output(
            &state,
            time,
            &context,
            &observable_programs,
            &mut series_points,
            visual_programs.as_ref(),
            &mut frames,
        )?;
    }

    for step in 0..steps {
        if step % 128 == 0 && cancellation.is_cancelled() {
            bail!("run cancelled");
        }
        let dt = (integration.end_time - time).min(integration.time_step);
        if dt <= 0.0 {
            break;
        }
        state = match integration.method {
            IntegrationMethod::Euler => {
                let derivative = derivatives(program, &state, &input.constants, time)?;
                state
                    .iter()
                    .zip(derivative)
                    .map(|(value, derivative)| value + dt * derivative)
                    .collect()
            }
            IntegrationMethod::Rk4 => rk4_step(program, &state, &input.constants, time, dt)?,
            IntegrationMethod::VelocityVerlet => {
                bail!("velocity-Verlet is not valid for a generic first-order state vector")
            }
        };
        if state.iter().any(|value| !value.is_finite()) {
            bail!("state became non-finite at integration step {step}");
        }
        time += dt;
        completed_steps += 1;
        if completed_steps % (steps / 100).max(128) == 0 || completed_steps == steps {
            if let Some(callback) = step_progress { callback(completed_steps as f32 / steps as f32, "integration"); }
        }
        for index in 0..state.len() {
            minimum[index] = minimum[index].min(state[index]);
            maximum[index] = maximum[index].max(state[index]);
            sums[index] += state[index];
        }
        samples += 1;
        if !manifest.trajectory.is_empty(){tracker.observe(&measure_context(&state,time),time)?;}

        if capture && (completed_steps % frame_stride == 0 || completed_steps == steps) {
            let mut context = analysis_context(
                program,
                manifest,
                input,
                &initial,
                &state,
                &minimum,
                &maximum,
                &sums,
                samples,
                time,
                integration.start_time,
                completed_steps,
            );
            tracker.extend(&mut context);
        capture_ode_output(
                &state,
                time,
                &context,
                &observable_programs,
                &mut series_points,
                visual_programs.as_ref(),
                &mut frames,
            )?;
        }
    }

    let mut context = analysis_context(
        program,
        manifest,
        input,
        &initial,
        &state,
        &minimum,
        &maximum,
        &sums,
        samples,
        time,
        integration.start_time,
        completed_steps,
    );
    tracker.extend(&mut context);
    let score = score_context(program, &context)?;
    let mut metrics = tracker.values();
    for (name, value) in &context {
        if name.starts_with("final_")
            || name.starts_with("delta_")
            || name.starts_with("abs_delta_")
            || matches!(name.as_str(), "elapsed_time" | "steps" | "sample_count")
        {
            metrics.insert(name.clone(), *value);
        }
    }
    for (observable, expression) in manifest.observables.iter().zip(&observable_programs) {
        metrics.insert(observable.name.clone(), expression.eval(&context)?);
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
    let visualization = VisualizationOutput {
        kind: format!("{:?}", manifest.visualization.kind).to_ascii_lowercase(),
        point_size: manifest.visualization.point_size,
        trails: manifest.visualization.trails,
        frames,
    };

    Ok(SimulationReport {
        constraint_results: evidence::constraints(&manifest.constraints, &context),
        score,
        metrics,
        series,
        visualization,
        steps: completed_steps,
    })
}

fn derivatives(
    program: &OdeProgram,
    state: &[f64],
    constants: &[f64],
    time: f64,
) -> anyhow::Result<Vec<f64>> {
    let context = instantaneous_context(program, state, constants, time);
    program
        .derivatives
        .iter()
        .map(|expression| expression.eval(&context))
        .collect()
}

fn rk4_step(
    program: &OdeProgram,
    state: &[f64],
    constants: &[f64],
    time: f64,
    dt: f64,
) -> anyhow::Result<Vec<f64>> {
    let k1 = derivatives(program, state, constants, time)?;
    let s2 = state
        .iter()
        .zip(&k1)
        .map(|(state, value)| state + 0.5 * dt * value)
        .collect::<Vec<_>>();
    let k2 = derivatives(program, &s2, constants, time + 0.5 * dt)?;
    let s3 = state
        .iter()
        .zip(&k2)
        .map(|(state, value)| state + 0.5 * dt * value)
        .collect::<Vec<_>>();
    let k3 = derivatives(program, &s3, constants, time + 0.5 * dt)?;
    let s4 = state
        .iter()
        .zip(&k3)
        .map(|(state, value)| state + dt * value)
        .collect::<Vec<_>>();
    let k4 = derivatives(program, &s4, constants, time + dt)?;

    Ok((0..state.len())
        .map(|index| {
            state[index]
                + dt * (k1[index] + 2.0 * k2[index] + 2.0 * k3[index] + k4[index])
                    / 6.0
        })
        .collect())
}

fn instantaneous_context(
    program: &OdeProgram,
    state: &[f64],
    constants: &[f64],
    time: f64,
) -> HashMap<String, f64> {
    let mut context = HashMap::with_capacity(state.len() + constants.len() + 1);
    context.insert("t".to_owned(), time);
    for (name, value) in program.state_names.iter().zip(state) {
        context.insert(name.clone(), *value);
    }
    for (name, value) in program.constant_names.iter().zip(constants) {
        context.insert(name.clone(), *value);
    }
    context
}

#[allow(clippy::too_many_arguments)]
fn analysis_context(
    program: &OdeProgram,
    manifest: &ExperimentManifest,
    input: &OdeCandidateInput,
    initial: &[f64],
    current: &[f64],
    minimum: &[f64],
    maximum: &[f64],
    sums: &[f64],
    samples: usize,
    time: f64,
    start_time: f64,
    completed_steps: usize,
) -> HashMap<String, f64> {
    let mut context = instantaneous_context(program, current, &input.constants, time);
    context.insert("elapsed_time".to_owned(), time - start_time);
    context.insert("steps".to_owned(), completed_steps as f64);
    context.insert("sample_count".to_owned(), samples as f64);
    for index in 0..program.state_names.len() {
        let name = &program.state_names[index];
        let delta = current[index] - initial[index];
        context.insert(format!("initial_{name}"), initial[index]);
        context.insert(format!("final_{name}"), current[index]);
        context.insert(format!("minimum_{name}"), minimum[index]);
        context.insert(format!("maximum_{name}"), maximum[index]);
        context.insert(format!("mean_{name}"), sums[index] / samples.max(1) as f64);
        context.insert(format!("delta_{name}"), delta);
        context.insert(format!("abs_delta_{name}"), delta.abs());
    }
    insert_search_aliases(program, manifest, input, &mut context);
    context
}

fn insert_search_aliases(
    program: &OdeProgram,
    manifest: &ExperimentManifest,
    input: &OdeCandidateInput,
    context: &mut HashMap<String, f64>,
) {
    for variable in &manifest.search.variables {
        let value = if let Some(name) = variable.target.strip_prefix("initial:") {
            program
                .state_names
                .iter()
                .position(|candidate| candidate == name)
                .and_then(|index| input.state.get(index))
                .copied()
        } else if let Some(name) = variable.target.strip_prefix("constant:") {
            program
                .constant_names
                .iter()
                .position(|candidate| candidate == name)
                .and_then(|index| input.constants.get(index))
                .copied()
        } else {
            None
        };
        if let Some(value) = value {
            context.insert(variable.name.clone(), value);
        }
    }
}

fn score_context(program: &OdeProgram, context: &HashMap<String, f64>) -> anyhow::Result<f64> {
    let mut score = 0.0;
    for objective in &program.objectives {
        let value = objective.expression.eval(context)?;
        if !value.is_finite() {
            return Ok(f64::INFINITY);
        }
        let oriented = match objective.goal {
            ObjectiveGoal::Minimize => value,
            ObjectiveGoal::Maximize => -value,
        };
        score += objective.weight * oriented;
    }
    for constraint in &program.constraints {
        let value = constraint.expression.eval(context)?;
        if !value.is_finite() {
            return Ok(f64::INFINITY);
        }
        let violation = (value - constraint.tolerance).max(0.0);
        score += 1.0e6 * violation * violation;
    }
    Ok(if score.is_finite() {
        score
    } else {
        f64::INFINITY
    })
}

fn parse_optional_axis(source: &str) -> anyhow::Result<Option<Expression>> {
    if source.trim().is_empty() {
        Ok(None)
    } else {
        Ok(Some(Expression::parse(source)?))
    }
}

struct CompiledVisualEntity {
    radius: Option<Expression>,
    id: String,
    axes: [Option<Expression>;3],
    velocities: [Option<Expression>;3],
}

fn compile_visual_entities(program: &OdeProgram, manifest: &ExperimentManifest) -> anyhow::Result<Vec<CompiledVisualEntity>> {
    use crate::domain::VisualMapping;
    let mut mappings = manifest.visualization.entities.clone();
    // Coordinate-family inference is not a scientific template. It only recognizes
    // matching variable names when the declared axes already select such a family.
    if mappings.is_empty() {
        if let (Some(x_suffix), Some(y_suffix)) = (manifest.visualization.x.strip_prefix('x'), manifest.visualization.y.strip_prefix('y')) {
            if !x_suffix.is_empty() && x_suffix == y_suffix && program.state_names.contains(&manifest.visualization.x) {
                for x in &program.state_names {
                    if let Some(suffix) = x.strip_prefix('x') {
                        let y = format!("y{suffix}");
                        if !suffix.is_empty() && program.state_names.contains(&y) {
                            let existing = |name: String| if program.state_names.contains(&name) { name } else { String::new() };
                            mappings.push(VisualMapping { radius:String::new(), id:format!("entity{suffix}"), x:x.clone(), y,
                                z:existing(format!("z{suffix}")), vx:existing(format!("vx{suffix}")),
                                vy:existing(format!("vy{suffix}")), vz:existing(format!("vz{suffix}")) });
                        }
                    }
                }
            }
        }
    }
    if mappings.is_empty() {
        let axis = |source: &str, index: usize| if source.trim().is_empty() { program.state_names.get(index).cloned().unwrap_or_else(||"0".to_owned()) } else {source.to_owned()};
        // An omitted Z denotes a plane, not the third state variable (which might be velocity).
        mappings.push(VisualMapping { radius:String::new(), id:"state".to_owned(), x:axis(&manifest.visualization.x,0),
            y:axis(&manifest.visualization.y,1), z:if manifest.visualization.z.trim().is_empty(){"0".to_owned()}else{manifest.visualization.z.clone()},
            vx:String::new(),vy:String::new(),vz:String::new() });
    }
    mappings.into_iter().take(128).map(|m| Ok(CompiledVisualEntity { radius:parse_optional_axis(&m.radius)?, id:m.id,
        axes:[parse_optional_axis(&m.x)?,parse_optional_axis(&m.y)?,parse_optional_axis(&m.z)?],
        velocities:[parse_optional_axis(&m.vx)?,parse_optional_axis(&m.vy)?,parse_optional_axis(&m.vz)?] })).collect()
}

fn capture_ode_output(
    _state: &[f64],
    time: f64,
    context: &HashMap<String, f64>,
    observable_programs: &[Expression],
    series_points: &mut [Vec<[f64; 2]>],
    visual_programs: Option<&Vec<CompiledVisualEntity>>,
    frames: &mut Vec<VisualFrame>,
) -> anyhow::Result<()> {
    for (index, expression) in observable_programs.iter().enumerate() {
        let value = expression.eval(context)?;
        if value.is_finite() {
            series_points[index].push([time, value]);
        }
    }
    if let Some(mappings) = visual_programs {
        let mut entities = Vec::with_capacity(mappings.len());
        for mapping in mappings {
            let mut position = [0.0;3];
            let mut velocity = [0.0;3];
            let mut has_velocity = false;
            for axis in 0..3 {
                position[axis] = mapping.axes[axis].as_ref().map(|e|e.eval(context)).transpose()?.unwrap_or(0.0);
                if let Some(expr) = &mapping.velocities[axis] { velocity[axis] = expr.eval(context)?; has_velocity = true; }
            }
            if position.iter().chain(velocity.iter()).any(|v|!v.is_finite()) { bail!("visual coordinates must be finite"); }
            let radius=mapping.radius.as_ref().map(|e|e.eval(context)).transpose()?;
            if radius.map(|r|!r.is_finite()||r<=0.0||r>1e12).unwrap_or(false){bail!("visual radius must be positive, finite and <=1e12 coordinate units");}
            entities.push(VisualEntity { radius, id: mapping.id.clone(), position, velocity: has_velocity.then_some(velocity), scalar: None });
        }
        frames.push(VisualFrame { time, entities });
    }

    Ok(())
}

fn run_falsification(
    program: &OdeProgram,
    manifest: &ExperimentManifest,
    winning_input: &OdeCandidateInput,
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
            let mut challenged_input = winning_input.clone();
            let mut integration = manifest.integration.clone();
            note = match test.kind {
                FalsificationKind::ResolutionLadder => {
                    integration=evidence::refinement(&manifest.integration,repetition,2000000)?;
                    "Distinct resolution ladder h/2, h/4, ...; same candidate, full horizon. Not a proof of convergence order.".to_owned()
                }
                FalsificationKind::StepHalving => {
                    integration.time_step *= 0.5;
                    integration.max_steps = integration.max_steps.saturating_mul(2).min(2_000_000);
                    "Replayed the same candidate at half the integration step.".to_owned()
                }
                FalsificationKind::SeedReplication => {
                    "Replayed the deterministic state-vector candidate. This solver has no stochastic state unless randomness is represented explicitly in the authored equations.".to_owned()
                }
                FalsificationKind::InitialPerturbation => {
                    let raw_target = test
                        .target
                        .as_deref()
                        .context("initial perturbation requires a target")?;
                    let target_name = raw_target.strip_prefix("initial:").unwrap_or(raw_target);
                    let target = program
                        .state_names
                        .iter()
                        .position(|candidate| candidate == target_name)
                        .with_context(|| format!("unknown initial perturbation target `{raw_target}`"))?;
                    challenged_input.state[target] += evidence::perturbation(test.magnitude, repetition);
                    format!(
                        "Perturbed initial state `{}` by {}.",
                        program.state_names[target], test.magnitude
                    )
                }
                FalsificationKind::ParameterPerturbation => {
                    let raw_target = test
                        .target
                        .as_deref()
                        .context("parameter perturbation requires a target")?;
                    let target_name = raw_target.strip_prefix("constant:").unwrap_or(raw_target);
                    let target = program
                        .constant_names
                        .iter()
                        .position(|candidate| candidate == target_name)
                        .with_context(|| format!("unknown parameter perturbation target `{raw_target}`"))?;
                    challenged_input.constants[target] += evidence::perturbation(test.magnitude, repetition);
                    format!(
                        "Perturbed constant `{}` by {}.",
                        program.constant_names[target], test.magnitude
                    )
                }
            };
            let evaluated = simulate(
                program,
                manifest,
                &challenged_input,
                &integration,
                false,
                cancellation,
                None,
            );

            let delta = match test.kind {
                FalsificationKind::InitialPerturbation | FalsificationKind::ParameterPerturbation => Some(evidence::perturbation(test.magnitude, repetition)),
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

pub(crate) fn gpu_supported_identifiers(program: &OdeProgram) -> HashSet<String> {
    let mut allowed = HashSet::from(["t".to_owned()]);
    for name in &program.state_names {
        allowed.insert(name.clone());
        allowed.insert(format!("initial_{name}"));
        allowed.insert(format!("final_{name}"));
        allowed.insert(format!("delta_{name}"));
    }
    allowed.extend(program.constant_names.iter().cloned());
    allowed
}

#[cfg(test)]
mod evidence_regression_tests {
    use super::*;
    use crate::domain::ExperimentManifestDraft;
    use uuid::Uuid;
    fn fixture() -> ExperimentManifest {
        let draft: ExperimentManifestDraft=serde_json::from_value(json!({
            "title":"Generic constant-velocity test", "question":"Does the numerical evidence contract behave correctly?",
            "hypothesis":"Software fixture only", "scientific_boundary":"No physics discovery is claimed",
            "model":{"kind":"state_vector_ode","variables":[
                {"name":"x1","unit":"","initial":0.0},{"name":"y1","unit":"","initial":0.0},
                {"name":"vx1","unit":"","initial":1.0},{"name":"vy1","unit":"","initial":0.0},
                {"name":"x2","unit":"","initial":2.0},{"name":"y2","unit":"","initial":3.0}],
                "derivatives":[{"variable":"x1","expression":"vx1"},{"variable":"y1","expression":"vy1"},
                {"variable":"vx1","expression":"0"},{"variable":"vy1","expression":"0"},
                {"variable":"x2","expression":"0"},{"variable":"y2","expression":"0"}]},
            "integration":{"method":"rk4","start_time":0.0,"end_time":1.0,"time_step":0.01,"output_stride":1,"max_steps":200},
            "observables":[{"name":"endpoint","expression":"x1","unit":"model length"}],
            "constraints":[{"name":"upper_bound","expression":"x1","tolerance":2.0}],
            "visualization":{"kind":"trajectory","x":"x1","y":"y1","z":"","max_frames":100},
            "falsification":[{"name":"perturbation","kind":"initial_perturbation","target":"x1","magnitude":0.000001,"repetitions":3}]
        })).unwrap();
        ExperimentManifest::from_draft(Uuid::nil(),None,1,draft,"test fixture")
    }
    #[tokio::test]
    async fn no_objective_preserves_changed_measurements_without_false_pass() {
        let manifest=fixture();let compute=manifest.compute.clone();
        let out=execute(manifest,compute,None,CancellationToken::new(),Arc::new(|_,_|{})).await.unwrap().output;
        assert_eq!(out.evidence_version,2);assert!(out.objective_score.is_none());
        assert!((out.metrics["endpoint"]-1.0).abs()<1e-10);
        assert_eq!(out.constraint_results[0].status,"passed");
        let challenge=&out.falsification[0];assert_eq!(challenge.status,"inconclusive");assert_eq!(challenge.survived,None);
        assert_eq!(challenge.trials.len(),3);
        assert!(challenge.trials[0].metrics["endpoint"]>out.metrics["endpoint"]);
        assert!(challenge.trials[1].metrics["endpoint"]<out.metrics["endpoint"]);
        assert_eq!(out.visualization.frames[0].entities.len(),2);
        assert_eq!(out.visualization.frames[0].entities[0].position[2],0.0);
        assert_eq!(out.visualization.frames[0].entities[0].velocity,Some([1.0,0.0,0.0]));
    }
    #[tokio::test]
    async fn explicit_metric_checks_can_pass_and_report_individual_constraints() {
        let mut m=fixture();m.falsification[0].checks.push(crate::domain::ChallengeCheck{
            metric:"endpoint".into(),expectation:"change".into(),absolute_tolerance:1e-8,relative_tolerance:0.0});
        let out=execute(m.clone(),m.compute.clone(),None,CancellationToken::new(),Arc::new(|_,_|{})).await.unwrap().output;
        assert_eq!(out.falsification[0].status,"passed");assert_eq!(out.falsification[0].trials[0].constraint_results.len(),1);
    }
    #[test]
    fn omitted_z_is_not_a_velocity_component() {
        let mut m=fixture();m.visualization.x="t".into();m.visualization.y="x1".into();
        let program=compile_program(&m).unwrap();let mappings=compile_visual_entities(&program,&m).unwrap();
        assert_eq!(mappings.len(),1);
        assert_eq!(mappings[0].axes[2].as_ref().unwrap().eval(&HashMap::new()).unwrap(),0.0);
    }
}

#[cfg(test)]
mod research_upgrade_tests {
    use super::*;
    use crate::domain::{ExperimentManifestDraft,TrajectoryMeasure,TrajectoryReducer,FalsificationSpec};
    use uuid::Uuid;
    fn fixture()->ExperimentManifest {
        let mut d:ExperimentManifestDraft=serde_json::from_str(include_str!("../../tests/fixtures/discovery-control.json")).unwrap();
        if let ModelSpec::StateVectorOde{variables,derivatives}=&mut d.model {variables[0].initial=0.0;derivatives[0].expression="1".into();}
        d.integration.end_time=2.0;d.integration.max_steps=201;d.visualization.max_frames=300;
        ExperimentManifest::from_draft(Uuid::nil(),None,1,d,"software control, not a scientific result")
    }
    fn add(m:&mut ExperimentManifest,name:&str,expr:&str,reducer:TrajectoryReducer,threshold:f64){m.trajectory.push(TrajectoryMeasure{name:name.into(),expression:expr.into(),reducer,threshold,hysteresis:0.0,unit:"fixture".into()});}
    fn simulate_fixture(m:&ExperimentManifest,capture:bool)->SimulationReport {
        let p=compile_program(m).unwrap();let input=OdeCandidateInput{state:p.initial_state.clone(),constants:p.constant_values.clone()};
        simulate(&p,m,&input,&m.integration,capture,&CancellationToken::new(),None).unwrap()
    }
    #[test]fn peak_measured_during_screening_not_only_final_frame(){
        let mut m=fixture();add(&mut m,"peak","1-abs(t-1)",TrajectoryReducer::Maximum,0.0);
        crate::sandbox::validate_manifest(&m).unwrap();let a=simulate_fixture(&m,false);
        m.integration.output_stride=1000;let b=simulate_fixture(&m,true);
        assert!((a.metrics["peak"]-1.0).abs()<1e-9);assert_eq!(a.metrics["peak"],b.metrics["peak"]);
        assert!(b.visualization.frames.len()<4);
    }
    #[test]fn peak_used_by_constraints_and_scoring(){
        let mut m=fixture();add(&mut m,"peak","x",TrajectoryReducer::Maximum,0.0);
        m.constraints=serde_json::from_value(json!([{"name":"peak_limit","expression":"peak","tolerance":1.0}])).unwrap();
        crate::sandbox::validate_manifest(&m).unwrap();let report=simulate_fixture(&m,false);
        assert_eq!(report.constraint_results[0].status,"failed");assert!(report.score>0.0);
    }
    #[test]fn first_passage_is_not_erased_by_return(){
        let mut m=fixture();add(&mut m,"escape","1-abs(t-1)",TrajectoryReducer::FirstAbove,0.5);
        let report=simulate_fixture(&m,false);assert!(report.metrics["escape"]<1.0);assert_eq!(report.metrics["escape_observed"],1.0);
    }
    #[test]fn first_passage_aliases_are_not_created_for_other_reducers(){
        let mut m=fixture();add(&mut m,"peak","x",TrajectoryReducer::Maximum,0.0);
        m.observables[0].expression="peak_observed".into();assert!(crate::sandbox::validate_manifest(&m).is_err());
        m.trajectory[0].reducer=TrajectoryReducer::FirstAbove;crate::sandbox::validate_manifest(&m).unwrap();
    }
    #[test]fn existing_unrelated_observed_name_is_backward_compatible(){let mut m=fixture();m.observables[0].name="signal_observed".into();crate::sandbox::validate_manifest(&m).unwrap();}
    #[test]fn unsupported_recursive_reducer_rejected(){let mut m=fixture();add(&mut m,"peak","peak+1",TrajectoryReducer::Maximum,0.0);assert!(crate::sandbox::validate_manifest(&m).is_err());}
    #[test]fn radius_is_per_entity_not_extra_marker(){
        let mut m=fixture();m.visualization.entities=serde_json::from_value(json!([{"id":"a","x":"x","y":"0","radius":"0.2"},{"id":"b","x":"x+2","y":"0","radius":"0.7"}])).unwrap();
        crate::sandbox::validate_manifest(&m).unwrap();let report=simulate_fixture(&m,true);let entities=&report.visualization.frames[0].entities;
        assert_eq!(entities.len(),2);assert_eq!(entities[0].radius,Some(0.2));assert_eq!(entities[1].radius,Some(0.7));
    }
    #[test]fn negative_radius_cannot_render_as_valid_geometry(){
        let mut m=fixture();m.visualization.entities=serde_json::from_value(json!([{"id":"a","x":"x","y":"0","radius":"-1"}])).unwrap();
        let p=compile_program(&m).unwrap();let input=OdeCandidateInput{state:p.initial_state.clone(),constants:p.constant_values.clone()};
        assert!(simulate(&p,&m,&input,&m.integration,true,&CancellationToken::new(),None).is_err());
    }
    #[tokio::test]async fn ladder_retains_two_distinct_grids(){
        let mut m=fixture();m.falsification=serde_json::from_value::<Vec<FalsificationSpec>>(json!([{"name":"resolution","kind":"resolution_ladder","magnitude":0.0,"repetitions":2,"checks":[{"metric":"final_x","expectation":"stable","absolute_tolerance":1e-8,"relative_tolerance":0.0}]}])).unwrap();
        crate::sandbox::validate_manifest(&m).unwrap();let result=execute(m.clone(),m.compute.clone(),None,CancellationToken::new(),Arc::new(|_,_|{})).await.unwrap().output;
        assert_eq!(result.falsification[0].trials[0].time_step,0.005);assert_eq!(result.falsification[0].trials[1].time_step,0.0025);
        assert_eq!(result.falsification[0].status,"passed");
    }
    #[test]fn legacy_refinement_semantics_not_migrated(){
        let mut m=fixture();m.falsification=serde_json::from_value(json!([{"name":"old","kind":"step_halving","repetitions":2,"magnitude":0.0}])).unwrap();
        assert_eq!(m.falsification[0].kind,crate::domain::FalsificationKind::StepHalving);
        let value=serde_json::to_value(&m).unwrap();assert!(value.get("trajectory").is_none());
        let visual:crate::domain::VisualMapping=serde_json::from_value(json!({"id":"a","x":"x","y":"0"})).unwrap();assert!(serde_json::to_value(visual).unwrap().get("radius").is_none());
    }
    #[test]fn cpu_state_limit_is_not_gpu_abi_limit(){
        let mut m=fixture();m.model=serde_json::from_value(json!({"kind":"state_vector_ode","variables":(0..128).map(|i|json!({"name":format!("v{i}"),"initial":0.0})).collect::<Vec<_>>(),"derivatives":(0..128).map(|i|json!({"variable":format!("v{i}"),"expression":"1"})).collect::<Vec<_>>()})).unwrap();
        m.observables.clear();m.visualization.kind=VisualizationKind::None;crate::sandbox::validate_manifest(&m).unwrap();
        assert!((simulate_fixture(&m,false).metrics["final_v127"]-2.0).abs()<1e-9);
    }
    #[test]fn excessive_resolution_is_rejected_before_launch(){let mut m=fixture();m.integration.end_time=10000.0;m.integration.max_steps=1_000_000;m.falsification=serde_json::from_value(json!([{"name":"too_big","kind":"resolution_ladder","repetitions":4,"magnitude":0.0}])).unwrap();assert!(crate::sandbox::validate_manifest(&m).is_err());}
}
