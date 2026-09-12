//! Requested outputs survive model summaries; completion is checked against retained artifacts.
use super::*;
use sha2::{Digest, Sha256};
#[path = "laboratory_intent_black_hole.rs"]
mod black_hole;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputIntent {
    #[default]
    Auto,
    Simulation,
    Illustration,
    Explanation,
    Analysis,
    PresentationEdit,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct IntentLedger {
    pub requested: OutputIntent,
    pub resolved: Option<OutputIntent>,
    pub revision: usize,
    pub request_id: Uuid,
    pub user_instruction: String,
    pub instruction_sha256: String,
    pub scope: String,
    pub required_capability: Option<String>,
    pub capability_gap: Option<String>,
    #[serde(default)]
    pub evidence_job_ids: Vec<Uuid>,
    pub resolved_by: Option<String>,
    #[serde(default)]
    pub previous_objective: Option<Value>,
}

impl IntentLedger {
    fn new(requested: OutputIntent, request_id: Uuid, instruction: &str, revision: usize) -> Self {
        Self { requested, resolved: (requested != OutputIntent::Auto).then_some(requested), revision,
            request_id, user_instruction: instruction.into(), instruction_sha256: hash(instruction.as_bytes()),
            scope: String::new(), required_capability: None, capability_gap: None, evidence_job_ids: vec![],
            resolved_by: (requested != OutputIntent::Auto).then(|| "explicit_user_request".into()), previous_objective: None }
    }
}

fn hash(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

pub(super) fn initialize(job: &LabJob, request: &SessionRequest, journal: &mut Journal) {
    // Historical receipts remain replayable under their recorded contract. New admissions
    // always persist Some(Auto) or the user's explicit choice before the first tool turn.
    if job.kind == "session" && journal.output_intent.is_none() {
        if let Some(requested) = request.output_intent {
            journal.output_intent = Some(IntentLedger::new(requested, job.id, &request.content, 0));
        }
    }
}

pub(super) fn steer(journal: &mut Journal, id: Uuid, update: &Value) -> anyhow::Result<()> {
    let explicit: Option<OutputIntent> = update.get("output_intent").filter(|v| !v.is_null())
        .map(|v| serde_json::from_value(v.clone())).transpose()?;
    if journal.output_intent.is_some() || explicit.is_some() {
        let revision = journal.output_intent.as_ref().map_or(0, |old| old.revision + 1);
        // An ordinary-language correction must be interpreted anew. Never inherit a
        // previous still/simulation choice merely because the selected view did not change.
        let previous=journal.output_intent.as_ref().map(|old|{
            let operative=old.previous_objective.as_ref().filter(|_|old.resolved_by.as_deref()==Some("semantic_preserved_objective"));
            json!({"resolved":old.resolved,"scope":old.scope,"required_capability":old.required_capability,"capability_gap":old.capability_gap,"evidence_job_ids":old.evidence_job_ids,
                "request_id":operative.map(|v|v["request_id"].clone()).unwrap_or(json!(old.request_id)),
                "requested":operative.map(|v|v["requested"].clone()).unwrap_or(json!(old.requested)),
                "user_instruction":operative.map(|v|v["user_instruction"].clone()).unwrap_or(json!(old.user_instruction)),
                "instruction_sha256":operative.map(|v|v["instruction_sha256"].clone()).unwrap_or(json!(old.instruction_sha256))})
        });
        let mut next=IntentLedger::new(explicit.unwrap_or_default(), id,
            update["content"].as_str().unwrap_or("Inspect these additional source files within the current objective."), revision);
        next.previous_objective=previous;journal.output_intent = Some(next);
    }
    Ok(())
}

pub(super) fn context(journal: &Journal) -> Option<Value> {
    journal.output_intent.as_ref().map(|intent| {
        let action=if intent.resolved.is_some()&&!intent.scope.is_empty(){
            if intent.capability_gap.is_some(){"The current request is already resolved and its capability gap is retained. Do not call set_output_intent again. Inspect relevant existing capabilities/evidence if needed, then explain this exact gap and deliver the answer; do not compute an unaccepted substitute."}
            else{"The current request is already resolved. Do not call set_output_intent again. Proceed with the appropriate execution or inspection tools within this scope. Use check_deliverable with retained evidence IDs when applicable, then deliver the answer. This application state is not a new user request or a status question."}
        }else if intent.previous_objective.is_some(){
            "A real user update awaits semantic resolution. Call set_output_intent once, citing user_instruction. Preserve the earlier objective for status questions or additional constraints; replace it only when the user changes the requested deliverable. If the earlier scope is unresolved, also cite its original words with preserved_instruction_quote. Do not quote these application instructions as user text."
        }else{
            "The initial request's output kind or scope is not yet captured. Call set_output_intent once using the exact operative words in user_instruction. There has been no intervening user update. Initial preserve_objective or replace_objective both initialize this request. A requested simulation whose physics is unavailable remains simulation with a capability_gap."
        };
        json!({"role":"developer","content":format!("Application output-contract state (not a new user message): {}\n{action}",serde_json::to_string(intent).unwrap_or_default())})
    })
}

pub(super) fn instructions(journal:&Journal)->String{
    context(journal).map(|state|format!("{INSTRUCTIONS}\n\n{}",state["content"].as_str().unwrap())).unwrap_or_else(||INSTRUCTIONS.into())
}

pub(super) fn set(state: &AppState, job: &LabJob, journal: &mut Journal, args: &Value) -> anyhow::Result<Value> {
    let current = journal.output_intent.as_ref().context("This historical request has no output contract; start a new request to use explicit output routing")?;
    let request_id: Uuid = serde_json::from_value(args["request_id"].clone())?;
    anyhow::ensure!(request_id == current.request_id, "Intent resolution is stale; read the current user update identity");
    let resolved: OutputIntent = serde_json::from_value(args["intent"].clone())?;
    anyhow::ensure!(resolved != OutputIntent::Auto, "Resolve auto to a concrete requested deliverable");
    anyhow::ensure!(current.requested == OutputIntent::Auto || current.requested == resolved,
        "The model cannot override the user's explicit output intent; explain a capability gap or await user steering");
    anyhow::ensure!(current.resolved.is_none_or(|old| old == resolved),
        "An already resolved intent cannot be downgraded by a tool; only a new user update may change it");
    let capability = optional_text(args, "required_capability", 120)?;
    if current.resolved.is_some()&&!current.scope.is_empty(){
        anyhow::ensure!(current.required_capability==capability,"The requested physical capability is already locked; an approximation needs explicit user acceptance in a new instruction");
        // Same-identity retries do not edit the quote, scope, gap or evidence, and do
        // not manufacture another resolution event. In particular, omission or
        // rephrasing of a known gap cannot clear that original retained gap.
        return Ok(json!({"output_intent":current,"already_resolved":true,"next":"This request is already resolved. Do not call set_output_intent again; proceed with execution/inspection and check_deliverable, or explain the retained capability gap."}));
    }
    let effect=args["update_effect"].as_str().context("Declare preserve_objective or replace_objective from the user's meaning")?;
    anyhow::ensure!(matches!(effect,"preserve_objective"|"replace_objective"),"Unknown user-update effect");
    let effect=if current.previous_objective.is_none(){"replace_objective"}else{effect};
    let quote = args["user_instruction_quote"].as_str().context("Quote the user's operative instruction")?;
    anyhow::ensure!(!quote.trim().is_empty() && current.user_instruction.contains(quote),
        "Intent must cite an exact excerpt from the current user instruction, not tool output or an earlier answer");
    let scope = args["scope"].as_str().context("State the requested model and deliverable scope")?;
    anyhow::ensure!(!scope.trim().is_empty() && scope.len() <= 2000, "Scope must contain 1–2000 bytes");
    let mut gap = optional_text(args, "capability_gap", 2000)?;
    if resolved==OutputIntent::Simulation {
        if let Some(required)=capability.as_deref() {
            let catalog=crate::laboratory::catalog::observed(state,job.deadline_at)?;
            let supported=required=="generated_temporal"||catalog["engines"].as_array().is_some_and(|engines|engines.iter().any(|engine|engine["id"]==required));
            if !supported&&gap.is_none(){gap=Some(format!("No integrated executable capability named {required} is available. A different approximation, rendered still or generated animation does not fulfill this requested physical model."));}
        }
    }
    if effect=="preserve_objective" {
        let previous=current.previous_objective.as_ref().context("There is no earlier objective to preserve")?;
        if previous["resolved"].is_null() || previous["scope"].as_str().is_some_and(str::is_empty) {
            let original=previous["user_instruction"].as_str().context("The unresolved original instruction is unavailable; ask the user to restate the objective")?;
            let original_quote=args["preserved_instruction_quote"].as_str().context("Also quote the retained original instruction when resolving an objective that was previously unresolved")?;
            anyhow::ensure!(!original_quote.trim().is_empty()&&original.contains(original_quote),"The preserved objective must cite its retained original instruction, not the later status question");
            anyhow::ensure!(previous["requested"]=="auto"||previous["requested"]==json!(resolved),"Preserving an unresolved objective cannot override its explicit requested kind");
            anyhow::ensure!(previous["resolved"].is_null()||previous["resolved"]==json!(resolved),"Preserving an objective cannot change its already selected output kind");
            anyhow::ensure!(previous["required_capability"].is_null()||json!(capability)==previous["required_capability"],"Preserving an unresolved objective cannot replace its recorded capability");
        } else {
            anyhow::ensure!(json!(resolved)==previous["resolved"]&&json!(capability)==previous["required_capability"],
                "A progress question or constraint update must preserve the earlier output kind and physical capability; only an explicit objective change permits replacement");
        }
        if let Some(prior_gap)=previous["capability_gap"].as_str(){gap=Some(prior_gap.into());}
    }
    anyhow::ensure!(resolved != OutputIntent::Simulation || capability.is_some(), "Simulation intent requires its requested physical capability");
    anyhow::ensure!(current.required_capability.is_none() || current.required_capability == capability,
        "The requested physical capability is already locked; an approximation needs explicit user acceptance in a new instruction");
    anyhow::ensure!(current.capability_gap.is_none() || current.capability_gap == gap,
        "A recorded capability gap cannot be cleared by a model assertion; the user must revise the requested scope");
    let mut updated = current.clone();
    updated.resolved = Some(resolved); updated.scope = scope.into(); updated.required_capability = capability;
    updated.capability_gap = gap; updated.resolved_by = Some(if effect=="preserve_objective" {"semantic_preserved_objective"} else if current.requested == OutputIntent::Auto {"semantic_model_resolution"} else {"explicit_user_request"}.into());
    if effect=="preserve_objective" {updated.evidence_job_ids=serde_json::from_value(current.previous_objective.as_ref().unwrap()["evidence_job_ids"].clone())?;}
    state.laboratory.event(job.id, "output_intent_resolved", "The requested deliverable and scientific scope are retained independently of the conversation summary.", serde_json::to_value(&updated)?)?;
    journal.output_intent = Some(updated);
    Ok(json!({"output_intent":journal.output_intent,"next":"Use tools within this scope; check_deliverable verifies exact retained evidence before completion."}))
}

fn optional_text(args: &Value, key: &str, maximum: usize) -> anyhow::Result<Option<String>> {
    let Some(value) = args.get(key).filter(|v| !v.is_null()) else {return Ok(None)};
    let value = value.as_str().with_context(|| format!("{key} must be text or null"))?;
    anyhow::ensure!(!value.trim().is_empty() && value.len() <= maximum, "{key} must contain 1–{maximum} bytes");
    Ok(Some(value.into()))
}

pub(super) fn guard(journal: &Journal, name: &str, args: &Value) -> anyhow::Result<()> {
    let Some(intent) = &journal.output_intent else {return Ok(())};
    let science = matches!(name, "launch_experiment"|"launch_solver_sweep"|"launch_ml_study")
        || name == "query_surrogate" && args["intent"] != "explanation"
        || name == "run_generated_experiment" && args["purpose"] != "presentation";
    let presentation = matches!(name, "render_illustration"|"edit_presentation"|"render_observation"|"publish_simulation")
        || name == "run_generated_experiment" && args["purpose"] == "presentation";
    if science || presentation {
        let resolved = intent.resolved.context("Resolve the current output intent before creating or changing artifacts")?;
        anyhow::ensure!(!intent.scope.is_empty(), "Retain the requested scope with set_output_intent before execution");
        anyhow::ensure!(!science || intent.capability_gap.is_none(), "The requested physics has a recorded capability gap. Explain it and obtain a revised user request before computing a substitute model");
        if name == "launch_experiment" && resolved==OutputIntent::Simulation {
            anyhow::ensure!(args["engine"].as_str() == intent.required_capability.as_deref(), "This solver does not implement the requested physical capability; do not substitute another model without user acceptance");
        }
        if name=="launch_solver_sweep"&&resolved==OutputIntent::Simulation {
            anyhow::ensure!(args["cases"].as_array().is_some_and(|cases|!cases.is_empty()&&cases.iter().all(|case|case["engine"].as_str()==intent.required_capability.as_deref())),
                "Every sweep case must implement the requested physical capability; a batch cannot bypass the no-substitution rule");
        }
        anyhow::ensure!(!science || matches!(resolved, OutputIntent::Simulation|OutputIntent::Analysis),
            "The current request is explanation/illustration/presentation, so it does not authorize a new scientific calculation. Presentation-only generation must declare purpose=presentation.");
        anyhow::ensure!(!presentation || resolved != OutputIntent::Explanation,
            "An explanation request does not authorize a new presentation artifact; inspect existing evidence instead");
        if name == "run_generated_experiment" && resolved == OutputIntent::PresentationEdit {
            anyhow::ensure!(args["purpose"] == "presentation" && args["sources"].as_array().is_some_and(|rows| !rows.is_empty()),
                "Presentation-only generation must declare purpose=presentation and import its retained source artifacts; do not recompute the science");
        }
    }
    Ok(())
}

pub(super) fn check(state: &AppState, session: &LabJob, journal: &mut Journal, args: &Value) -> anyhow::Result<Value> {
    let ids: Vec<Uuid> = serde_json::from_value(args["evidence_job_ids"].clone()).context("List exact retained evidence job IDs")?;
    anyhow::ensure!(ids.len() <= 32, "A deliverable check supports at most 32 explicit evidence jobs");
    let current = journal.output_intent.as_mut().context("No current output contract")?;
    anyhow::ensure!(args["request_id"] == json!(current.request_id), "Deliverable check is for a superseded user request");
    for id in &ids { anyhow::ensure!(state.laboratory.get(*id)?.project_id == session.project_id, "Evidence belongs to another project"); state.laboratory.ensure_model_study_access(*id)?; }
    current.evidence_job_ids = ids;
    let checked = assess(state, session, journal)?;
    state.laboratory.event(session.id, "deliverable_checked", "Requested output checked against retained execution artifacts; visual appearance and final-answer prose are not completion proof.", checked.clone())?;
    Ok(checked)
}

pub(super) fn assess(state: &AppState, session: &LabJob, journal: &Journal) -> anyhow::Result<Value> {
    let Some(intent) = &journal.output_intent else {return Ok(Value::Null)};
    let mut evidence = vec![]; let mut rejected = vec![];
    for id in &intent.evidence_job_ids {
        let outcome = (|| -> anyhow::Result<Value> {
            let job = state.laboratory.get(*id)?;
            anyhow::ensure!(job.project_id == session.project_id, "Evidence belongs to another project");
            state.laboratory.ensure_model_study_access(*id)?;
            anyhow::ensure!(job.state == "completed", "Evidence job is not completed");
            match intent.resolved {
                Some(OutputIntent::Simulation) => temporal_evidence(state, &job, intent),
                Some(OutputIntent::Illustration) => illustration_evidence(state, &job),
                Some(OutputIntent::Analysis) => numerical_evidence(state, &job),
                Some(OutputIntent::Explanation|OutputIntent::PresentationEdit) => Ok(json!({"job_id":id,"kind":job.kind,"state":job.state})),
                _ => bail!("The current user request has not been resolved"),
            }
        })();
        match outcome {Ok(row) => evidence.push(row), Err(error) => rejected.push(json!({"job_id":id,"reason":format!("{error:#}")}))}
    }
    let resolved = intent.resolved;
    let edit_done = journal.tools.values().any(|tool| tool["state"] == "completed" && tool["output"]["error"].is_null()
        && tool["name"]=="edit_presentation"
        && tool["intent_request_id"] == json!(intent.request_id));
    let rendered_edit=journal.tools.values().any(|tool|tool["state"]=="completed"&&tool["output"]["error"].is_null()
        && matches!(tool["name"].as_str(),Some("render_observation"|"render_illustration"|"run_generated_experiment"))
        && tool["intent_request_id"]==json!(intent.request_id)
        && evidence.iter().any(|row|row["job_id"]==tool["output"]["job_id"]));
    let fulfilled = intent.capability_gap.is_none() && match resolved {
        Some(OutputIntent::Explanation) => true,
        Some(OutputIntent::Simulation|OutputIntent::Illustration|OutputIntent::Analysis) => !evidence.is_empty(),
        Some(OutputIntent::PresentationEdit) => edit_done||rendered_edit,
        _ => false,
    };
    let status = if intent.capability_gap.is_some() {"capability_gap"} else if fulfilled {"fulfilled"} else if resolved.is_none() {"intent_unresolved"} else {"not_fulfilled"};
    Ok(json!({"schema":"phaseforge.deliverable.v1","request_id":intent.request_id,"revision":intent.revision,
        "requested_intent":intent.requested,"output_intent":resolved,"status":status,"fulfilled":fulfilled,
        "scope":intent.scope,"required_capability":intent.required_capability,"capability_gap":intent.capability_gap,
        "evidence":evidence,"rejected_evidence":rejected,"scientific_validation":"Retained temporal execution is distinct from numerical validation, calibration and real-world predictive validity.",
        "completion_basis":"Backend checks retained artifacts and tool receipts, never final-answer wording."}))
}

fn bytes(state: &AppState, job: Uuid, name: &str, expected: Option<&str>) -> anyhow::Result<Vec<u8>> {
    let path = state.laboratory.path(job, name)?;
    anyhow::ensure!(std::fs::metadata(&path)?.len() <= 32 * 1024 * 1024, "Evidence member exceeds the bounded 32 MiB read");
    let bytes = std::fs::read(path)?;
    anyhow::ensure!(bytes.len() <= 32 * 1024 * 1024, "Evidence member grew beyond the bounded 32 MiB read");
    if let Some(expected) = expected {
        anyhow::ensure!(expected.len() == 64 && expected.bytes().all(|c| c.is_ascii_hexdigit()) && hash(&bytes) == expected,
            "Retained evidence member is missing its hash or changed: {name}");
    }
    Ok(bytes)
}

fn finite(value: &Value) -> anyhow::Result<f64> { value.as_f64().filter(|v| v.is_finite()).context("A physical sample is not a finite number") }
fn numeric_array(value: &Value) -> anyhow::Result<usize> {
    if let Some(items) = value.as_array() { anyhow::ensure!(!items.is_empty(), "Numerical sample is empty"); items.iter().try_fold(0, |n,v| Ok(n + numeric_array(v)?)) }
    else {finite(value)?; Ok(1)}
}

fn numerical_evidence(state:&AppState,job:&LabJob)->anyhow::Result<Value>{
    anyhow::ensure!(matches!(job.kind.as_str(),"generated"|"solver"|"ml_study"|"sweep"),"Numerical analysis needs a completed calculation or study, not an illustration or final-answer text");
    anyhow::ensure!(job.input["execution_purpose"]!="presentation","Presentation-only code does not establish a numerical analysis result");
    if job.kind == "solver" && black_hole::supports(job.input["engine"].as_str().unwrap_or("")) {
        let mut evidence = black_hole::evidence(state, job)?;
        evidence["kind"] = json!("retained_numerical_analysis");
        evidence["path"] = evidence["measurements"]["path"].clone();
        evidence["sha256"] = evidence["measurements"]["sha256"].clone();
        evidence["temporal_sampling_required"] = json!(false);
        return Ok(evidence);
    }
    if job.kind == "solver" && job.input["engine"] == "athenak_gauge_wave" {
        let mut evidence = gauge_evidence(state, job)?;
        evidence["kind"] = json!("retained_numerical_analysis");
        evidence["path"] = evidence["measurements"]["path"].clone();
        evidence["sha256"] = evidence["measurements"]["sha256"].clone();
        evidence["temporal_sampling_required"] = json!(false);
        return Ok(evidence);
    }
    let (path,raw)=if job.kind=="generated"{("work/result.json",state.laboratory.read_generated_artifact(job.id,"work/result.json")?)}else if job.kind=="solver"{("measurements.json",bytes(state,job.id,"measurements.json",None)?)}else{("result.json",bytes(state,job.id,"result.json",None)?)};
    let result:Value=serde_json::from_slice(&raw)?;
    fn count(value:&Value)->usize{match value{
        Value::Number(n)=>usize::from(n.as_f64().is_some_and(f64::is_finite)),
        Value::Array(items)=>items.iter().map(count).sum(),
        Value::Object(map)=>map.iter().filter(|(key,_)|!matches!(key.as_str(),"artifacts"|"sources"|"render_scene"|"schema_version"|"version"|"frame_count"|"counts"|"storage")).map(|(_,value)|count(value)).sum(),_=>0}}
    let quantities=count(&result);anyhow::ensure!(quantities>0,"Completed output contains no retained numerical quantities");
    Ok(json!({"job_id":job.id,"engine":job.input["engine"],"kind":"retained_numerical_analysis","path":path,"sha256":hash(&raw),"numeric_values":quantities,"temporal_sampling_required":false,
        "scientific_validation":"Execution and retained numerical output only; this does not establish accuracy, calibration, predictive usefulness or that the model answered the right scientific question."}))
}

fn temporal_evidence(state: &AppState, job: &LabJob, intent: &IntentLedger) -> anyhow::Result<Value> {
    anyhow::ensure!(matches!(job.kind.as_str(), "solver"|"published_simulation"), "A still, HTML player or unregistered generated array is not a native temporal solver receipt");
    let engine = job.input["engine"].as_str().context("Solver engine identity missing")?;
    anyhow::ensure!(intent.required_capability.as_deref() == Some(engine),
        "Requested capability does not match this solver. A limited approximation cannot silently fulfill unsupported relativity or another physical model");
    if engine == "athenak_gauge_wave" { return gauge_evidence(state, job); }
    if black_hole::supports(engine) {
        black_hole::require_capability(job, intent.required_capability.as_deref())?;
        return black_hole::evidence(state, job);
    }
    let manifest = state.laboratory.read_json(job.id, "manifest.json")?;
    anyhow::ensure!(manifest["engine"] == engine, "Manifest and admitted engine differ");
    let is_field = state.laboratory.path(job.id, "fields/index.json").is_ok();
    let index_path = if is_field {"fields/index.json"} else {"trajectory/index.json"};
    let raw = bytes(state, job.id, index_path, None)?;
    if job.kind=="published_simulation" {
        anyhow::ensure!(job.result["index_path"]==index_path&&job.result["index_sha256"]==hash(&raw),"Published index changed after its immutable completion receipt");
        let source_hash=job.input["source_sha256"].as_str().context("Publication source identity is missing")?;
        bytes(state,job.id,"source-simulation.json",Some(source_hash))?;
        let manifest_hash=job.input["source_manifest_sha256"].as_str().context("Publication execution provenance is missing")?;
        bytes(state,job.id,"source-execution-manifest.json",Some(manifest_hash))?;
    }
    let index: Value = serde_json::from_slice(&raw)?;
    let time_unit = index["time_unit"].as_str().filter(|v| !v.is_empty()).context("Physical time unit is absent")?;
    let mut samples = vec![];
    let count;
    if is_field {
        let frames = index["frames"].as_array().context("Field records missing")?;
        anyhow::ensure!(frames.len() >= 2, "Simulation needs at least two recorded physical times");
        count = frames.len() as u64;
        for entry in [frames.first().unwrap(), frames.last().unwrap()] {
            let path = entry["view_path"].as_str().context("Numeric field view missing")?;
            let digest = entry["view_sha256"].as_str().context("Numeric field hash missing")?;
            let view: Value = serde_json::from_slice(&bytes(state, job.id, path, Some(digest))?)?;
            anyhow::ensure!(view["time"] == entry["time"], "Field index and retained time differ");
            let values = numeric_array(&view["values"])?;
            let native_path = entry["path"].as_str().context("Authoritative field array missing")?;
            let native_hash = entry["sha256"].as_str().context("Authoritative field hash missing")?;
            bytes(state, job.id, native_path, Some(native_hash))?;
            samples.push(json!({"time":finite(&view["time"])? ,"path":path,"sha256":digest,"numeric_values":values,"array_path":native_path,"array_sha256":native_hash}));
        }
    } else {
        let chunks = index["chunks"].as_array().context("Trajectory chunks missing")?;
        anyhow::ensure!(!chunks.is_empty(), "Trajectory has no numerical chunks");
        count = index["frame_count"].as_u64().context("Retained frame count missing")?;
        anyhow::ensure!(count >= 2, "Simulation needs at least two retained physical times");
        for (entry, last) in [(chunks.first().unwrap(), false), (chunks.last().unwrap(), true)] {
            let path = entry["path"].as_str().context("Trajectory chunk path missing")?;
            let digest = entry["sha256"].as_str().context("Trajectory chunk hash missing")?;
            let chunk: Value = serde_json::from_slice(&bytes(state, job.id, path, Some(digest))?)?;
            let frames = chunk["frames"].as_array().context("Trajectory samples missing")?;
            let frame = if last {frames.last()} else {frames.first()}.context("Trajectory chunk is empty")?;
            let entities = frame["entities"].as_array().filter(|v| !v.is_empty()).context("Recorded state has no entities")?;
            for entity in entities {anyhow::ensure!(entity["id"].is_string() && entity["position"].as_array().is_some_and(|p|p.len()==3), "Particle state lacks identity or a three-dimensional position"); numeric_array(&entity["position"])?;}
            samples.push(json!({"time":finite(&frame["time"])? ,"path":path,"sha256":digest,"entities":entities.len()}));
        }
    }
    anyhow::ensure!(finite(&samples[1]["time"])? > finite(&samples[0]["time"])?, "Retained states do not span increasing physical time");
    Ok(json!({"job_id":job.id,"engine":engine,"kind":"retained_temporal_numerical_states","index_path":index_path,
        "index_sha256":hash(&raw),"frame_count":count,"time_unit":time_unit,"verified_endpoint_samples":samples,
        "scientific_scope":manifest.get("scientific_scope").or_else(||manifest.get("scope")),
        "verification_scope":"Hash-verified first and last numerical samples and native field arrays; does not revalidate every intermediate state or prove model applicability."}))
}

// Gauge diagnostics have their own immutable schema; they are not SI scalar-field
// exports. This is a read-only check, including for a recovered completed attempt.
fn gauge_evidence(state: &AppState, job: &LabJob) -> anyhow::Result<Value> {
    const ENGINE: &str = "athenak_gauge_wave";
    anyhow::ensure!(job.kind == "solver" && job.input["engine"] == ENGINE && job.result["engine"] == ENGINE,
        "Gauge evidence needs its original solver identity");
    let saved = &job.result["diagnostics"];
    anyhow::ensure!(saved["path"] == "diagnostics/result.json", "Gauge completion has no pinned diagnostic result");
    let digest = saved["sha256"].as_str().context("Gauge diagnostic completion hash is missing")?;
    let diagnostic: Value = serde_json::from_slice(&bytes(state, job.id, "diagnostics/result.json", Some(digest))?)?;
    anyhow::ensure!(diagnostic["schema"] == "phaseforge.nr-gauge-result.v1" && diagnostic["engine"] == ENGINE,
        "Gauge diagnostic schema or engine differs");
    let summary = json!({"path":"diagnostics/result.json","sha256":digest,
        "gauge_reference_validated":diagnostic["fulfillment"]["gauge_reference_validated"],
        "analytic":diagnostic["analytic"],"slice_index":diagnostic["slice_index"],
        "frame_count":diagnostic["frame_count"],"initial_time":diagnostic["initial_time"],
        "final_time":diagnostic["final_time"],"scope":diagnostic["scientific_scope"]});
    anyhow::ensure!(*saved == summary && diagnostic["fulfillment"]["gauge_reference_validated"] == true
        && diagnostic["fulfillment"]["black_hole_collision_validated"] == false
        && diagnostic["fulfillment"]["physical_radiation"] == false,
        "Gauge completion summary or scientific scope is inconsistent");
    let units = &diagnostic["units"];
    anyhow::ensure!(units["length"] == "L" && units["time"] == "L/c" && finite(&units["length_scale"])? == 1.0
        && finite(&units["speed_of_light"])? == 1.0 && units.get("si_mapping") == Some(&Value::Null),
        "Gauge code coordinates must retain L=1, c=1 without an invented SI mapping");
    let artifacts = diagnostic["artifacts"].as_array().context("Gauge artifact inventory is missing")?;
    anyhow::ensure!(artifacts.len() <= 4096, "Gauge artifact inventory is unbounded");
    let mut paths = std::collections::HashSet::new();
    for artifact in artifacts {
        let name = artifact["path"].as_str().context("Gauge artifact path is missing")?;
        gauge_relative(name)?;
        anyhow::ensure!(paths.insert(name.to_ascii_lowercase()), "Gauge artifact paths are duplicated");
    }
    // Descriptors may omit bytes in a frame, but must match the one full immutable
    // inventory entry. All data we interpret is read once and checked on those bytes.
    let read = |descriptor: &Value| -> anyhow::Result<Vec<u8>> {
        let name = descriptor["path"].as_str().context("Gauge evidence path is missing")?;
        gauge_relative(name)?;
        let entry = artifacts.iter().find(|entry| entry["path"] == name).context("Gauge evidence is outside its pinned inventory")?;
        anyhow::ensure!(descriptor["sha256"] == entry["sha256"] && descriptor.get("bytes").is_none_or(|v| Some(v) == entry.get("bytes")),
            "Gauge evidence descriptor differs from its inventory");
        let raw = bytes(state, job.id, &format!("diagnostics/{name}"), Some(entry["sha256"].as_str().context("Gauge evidence hash is missing")?))?;
        anyhow::ensure!(entry["bytes"].as_u64() == Some(raw.len() as u64), "Gauge evidence length differs");
        Ok(raw)
    };
    let receipt_descriptor = &diagnostic["sources"]["execution_receipt"];
    anyhow::ensure!(receipt_descriptor["path"] == "sources/execution-receipt.json"
        && job.result["process_receipt"] == "nr-process-receipt.json"
        && receipt_descriptor["sha256"] == job.result["process_receipt_sha256"], "Gauge diagnostics refer to another execution receipt");
    let receipt_raw = read(receipt_descriptor)?;
    let root_receipt = bytes(state, job.id, "nr-process-receipt.json", Some(receipt_descriptor["sha256"].as_str().context("Gauge process receipt hash is missing")?))?;
    anyhow::ensure!(root_receipt == receipt_raw, "Gauge execution receipt copies differ");
    let receipt: Value = serde_json::from_slice(&receipt_raw)?;
    anyhow::ensure!(receipt["schema"] == "phaseforge.nr-process-receipt.v1" && receipt["job_id"] == json!(job.id)
        && receipt["engine_id"] == ENGINE && receipt["exit_code"] == 0 && receipt["termination_reason"] == "completed"
        && receipt["process_group_drained"] == true && job.result["execution_completed"] == true
        && job.result["exit_code"] == receipt["exit_code"] && job.result["termination_reason"] == receipt["termination_reason"],
        "Gauge process did not complete this exact attempt");
    anyhow::ensure!(diagnostic["execution"] == json!({"receipt_type":receipt["schema"],"job_id":job.id,"success":true,
        "exit_code":0,"termination_reason":"completed","process_group_drained":true}), "Gauge diagnostic execution outcome differs");
    let manifest: Value = serde_json::from_slice(&bytes(state, job.id, "nr-engine.json", Some(receipt["engine_manifest_sha256"].as_str().context("Gauge engine manifest pin is missing")?))?)?;
    anyhow::ensure!(manifest["schema"] == "phaseforge.nr-engine.v1" && manifest["engine_id"] == ENGINE
        && manifest["source"] == receipt["source"] && manifest["build"] == receipt["build"]
        && manifest["executable"]["path"] == receipt["executable"]["path"] && manifest["executable"]["sha256"] == receipt["executable"]["sha256"],
        "Gauge source or executable identity differs from its execution receipt");
    bytes(state, job.id, "input.athinput", Some(receipt["input"]["sha256"].as_str().context("Gauge scientific input pin is missing")?))?;
    let analytic: Value = serde_json::from_slice(&read(&diagnostic["analytic"]["report"])?)?;
    anyhow::ensure!(diagnostic["analytic"]["report"]["path"] == "analytic.json" && diagnostic["analytic"]["passed"] == true
        && diagnostic["analytic"]["status"] == "passed" && analytic["schema"] == "phaseforge.nr-gauge-analytic.v1"
        && analytic["passed"] == true && analytic["status"] == "passed" && analytic["check"]["passed"] == true,
        "The retained gauge analytic check did not pass");
    anyhow::ensure!(diagnostic["slice_index"]["path"] == "slices/index.json" && diagnostic["measurements"]["path"] == "measurements.json",
        "Gauge index or measurement path differs");
    let index: Value = serde_json::from_slice(&read(&diagnostic["slice_index"])?)?;
    let measurements: Value = serde_json::from_slice(&read(&diagnostic["measurements"])?)?;
    anyhow::ensure!(index["schema"] == "phaseforge.nr-diagnostic-slices.v1" && index["representation"] == "nr_diagnostic_slice"
        && index["units"] == *units && index["time_unit"] == "L/c" && index["axis_order"] == json!(["y","x"])
        && index["grid_location"] == "cell_center" && index["interpolation"] == "none" && index["primary_channel"] == "alpha"
        && index["black_hole_collision"] == false && index["physical_radiation"] == false,
        "Gauge index changed its diagnostic representation or units");
    anyhow::ensure!(measurements["schema"] == "phaseforge.nr-gauge-measurements.v1" && measurements["units"] == *units,
        "Gauge numerical measurement schema or units differ");
    let frames = index["frames"].as_array().context("Gauge time frames are missing")?;
    let series = measurements["series"].as_array().context("Gauge numerical series is missing")?;
    let checks = analytic["check"]["frames"].as_array().context("Gauge per-frame analytic checks are missing")?;
    anyhow::ensure!((2..=4096).contains(&frames.len()) && index["frame_count"].as_u64() == Some(frames.len() as u64)
        && diagnostic["frame_count"] == index["frame_count"] && diagnostic["timeframes"] == index["frames"]
        && series.len() == frames.len() && checks.len() == frames.len(), "Gauge retained time-frame counts differ");
    let mut previous = None;
    for (n, ((frame, sample), check)) in frames.iter().zip(series).zip(checks).enumerate() {
        let time = finite(&frame["time"])?;
        anyhow::ensure!(previous.is_none_or(|old| time > old) && frame["scientific_time"] == frame["time"]
            && frame["time_unit"] == "L/c" && frame["index"].as_u64() == Some(n as u64) && frame["cycle"].as_u64().is_some()
            && frame["analytic_passed"] == true && sample["time"] == frame["time"] && sample["cycle"] == frame["cycle"]
            && sample["analytic_passed"] == true && check["time"] == frame["time"] && check["cycle"] == frame["cycle"]
            && check["passed"] == true, "Gauge recorded times or per-frame numerical checks are inconsistent");
        previous = Some(time);
        for (channel, unit) in [("alpha","1"),("gxx","1"),("Kxx","1/L"),("hamiltonian","1/L^2")] {
            let quantities = &sample["channels_3d"][channel];
            let low = finite(&quantities["min"])?;
            let high = finite(&quantities["max"])?;
            let mean = finite(&quantities["mean"])?;
            anyhow::ensure!(quantities["unit"] == unit && low <= mean && mean <= high,
                "Gauge retained numerical measurement quantities or units differ");
        }
    }
    anyhow::ensure!(index["initial_time"] == frames[0]["time"] && index["final_time"] == frames[frames.len()-1]["time"]
        && diagnostic["initial_time"] == index["initial_time"] && diagnostic["final_time"] == index["final_time"], "Gauge time endpoints differ");
    let mut samples = vec![];
    for frame in [&frames[0], &frames[frames.len()-1]] {
        let view: Value = serde_json::from_slice(&read(&frame["data"])?)?;
        anyhow::ensure!(view["schema"] == "phaseforge.nr-diagnostic-slice-data.v1" && view["axis_order"] == json!(["y","x"])
            && view["shape"] == frame["shape"] && frame["plane"]["axes"] == json!(["x","y"])
            && frame["plane"]["normal_axis"] == "z" && frame["plane"]["coordinate_unit"] == "L"
            && finite(&view["z"])? == finite(&frame["plane"]["coordinate"])? , "Gauge numeric slice geometry differs");
        let shape = view["shape"].as_array().filter(|v| v.len()==2).context("Gauge slice shape is missing")?;
        let ny = shape[0].as_u64().filter(|n| *n>0 && *n<=4096).context("Gauge slice height is invalid")? as usize;
        let nx = shape[1].as_u64().filter(|n| *n>0 && *n<=4096).context("Gauge slice width is invalid")? as usize;
        for (axis, count) in [("x", nx), ("y", ny)] {
            let coordinates = view[axis].as_array().filter(|v|v.len()==count).context("Gauge slice coordinates do not match its shape")?;
            let values: Vec<f64> = coordinates.iter().map(finite).collect::<anyhow::Result<_>>()?;
            anyhow::ensure!(values.windows(2).all(|v|v[1]>v[0]), "Gauge cell-centre coordinates do not increase");
        }
        anyhow::ensure!(view["channels"].as_object().is_some_and(|v| v.len()==4), "Gauge slice needs its four numerical channels");
        for (channel, unit) in [("alpha","1"),("gxx","1"),("Kxx","1/L"),("hamiltonian","1/L^2")] {
            anyhow::ensure!(index["channels"][channel]["unit"] == unit, "Gauge numerical channel units differ");
            let rows = view["channels"][channel].as_array().filter(|v|v.len()==ny).context("Gauge channel height differs")?;
            for row in rows {
                anyhow::ensure!(row.as_array().is_some_and(|v|v.len()==nx), "Gauge channel width differs");
                numeric_array(row)?;
            }
        }
        for descriptor in [&frame["arrays"], &frame["source_block"]["metric"], &frame["source_block"]["constraints"]] { read(descriptor)?; }
        let native = receipt["files"].as_array().context("Gauge native execution file inventory is missing")?;
        for descriptor in [&frame["source_frame"], &frame["source_constraint_frame"]] {
            let name = descriptor["path"].as_str().and_then(|v|v.strip_prefix("native/")).context("Gauge native endpoint path differs")?;
            let registered = native.iter().find(|row| row["path"] == name && row["sha256"] == descriptor["sha256"])
                .context("Gauge native endpoint is absent from its execution receipt")?;
            let raw = read(descriptor)?;
            anyhow::ensure!(registered["bytes"].as_u64() == Some(raw.len() as u64), "Gauge native endpoint byte count differs");
            bytes(state, job.id, &format!("native/{name}"), Some(descriptor["sha256"].as_str().context("Gauge native endpoint hash is missing")?))?;
        }
        samples.push(json!({"time":frame["time"],"scientific_time":frame["scientific_time"],"cycle":frame["cycle"],
            "path":format!("diagnostics/{}",frame["data"]["path"].as_str().unwrap()),"sha256":frame["data"]["sha256"],
            "numeric_values":nx*ny*4,"shape":frame["shape"],"arrays":frame["arrays"],"native_source":frame["source_frame"],
            "native_constraints":frame["source_constraint_frame"]}));
    }
    Ok(json!({"job_id":job.id,"engine":ENGINE,"kind":"retained_temporal_numerical_states",
        "diagnostic_result":{"path":"diagnostics/result.json","sha256":digest},
        "process_receipt":{"path":"nr-process-receipt.json","sha256":receipt_descriptor["sha256"]},
        "index_path":"diagnostics/slices/index.json","index_sha256":diagnostic["slice_index"]["sha256"],
        "measurements":{"path":"diagnostics/measurements.json","sha256":diagnostic["measurements"]["sha256"]},
        "frame_count":frames.len(),"time_unit":"L/c","units":units,"verified_endpoint_samples":samples,
        "gauge_reference_validated":true,"black_hole_collision_validated":false,"physical_radiation":false,
        "scientific_scope":diagnostic["scientific_scope"],
        "verification_scope":"Pinned process, analytic report, measurements and all indexed times; finite first/last JSON numerical channels and hashed corresponding NPZ/native arrays. Native arrays are not independently decoded here; the retained gauge analytic checker supplies numerical validation. Flat-spacetime gauge benchmark only."}))
}

fn gauge_relative(name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(!name.is_empty() && !name.contains(['\\', ':', '\0'])
        && name.split('/').all(|part| !part.is_empty() && part != "." && part != ".."), "Unsafe gauge evidence path");
    Ok(())
}

fn illustration_evidence(state: &AppState, job: &LabJob) -> anyhow::Result<Value> {
    anyhow::ensure!(job.kind == "illustration", "Illustration requires an executed render job");
    let data = bytes(state, job.id, "render.png", None)?;
    anyhow::ensure!(data.starts_with(b"\x89PNG\r\n\x1a\n") && data.len() > 24, "Completed illustration has no retained PNG");
    Ok(json!({"job_id":job.id,"kind":"rendered_still","path":"render.png","sha256":hash(&data),"bytes":data.len(),"scientific_simulation":false}))
}

#[cfg(test)]
#[path = "laboratory_intent_tests.rs"]
mod tests;
