//! One versioned schema is sent to both providers; no JSON-inside-string loophole.
use anyhow::{bail, Context};
use serde_json::Value;
use crate::{domain::{ExperimentManifestDraft, ExperimentManifest}, sandbox::validate_manifest};
use uuid::Uuid;

pub fn proposal_schema() -> Value {
    let mut schema: Value = serde_json::from_str(include_str!("proposal.schema.json"))
        .expect("checked-in provider schema must be valid JSON");
    schema["$defs"]["scene"] = serde_json::from_str(include_str!("../../../docs/scene.schema.json"))
        .expect("checked-in procedural scene schema must be valid JSON");
    let visualization = &mut schema["$defs"]["manifest"]["properties"]["visualization"];
    visualization["properties"]["scene"] = serde_json::json!({"anyOf":[{"$ref":"#/$defs/scene"},{"type":"null"}]});
    visualization["required"].as_array_mut().expect("visualization required fields").push(serde_json::json!("scene"));
    schema
}

/// Validate the supported JSON-schema subset before serde defaults could hide omissions.
pub fn validate_proposal_shape(value: &Value) -> anyhow::Result<()> {
    let root = proposal_schema();
    validate(value, &root, &root, "proposal")
}
pub(super) fn validate(value: &Value, schema: &Value, root: &Value, path: &str) -> anyhow::Result<()> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let target = root.pointer(reference.trim_start_matches('#')).context("bad schema reference")?;
        return validate(value, target, root, path);
    }
    if let Some(choices) = schema.get("anyOf").and_then(Value::as_array) {
        let mut errors = Vec::new();
        for choice in choices {
            match validate(value, choice, root, path) {
                Ok(()) => return Ok(()),
                Err(e) => errors.push(e.to_string()),
            }
        }
        bail!("{} does not match an allowed variant: {}", path, errors.join(" | "));
    }
    if let Some(types) = schema.get("type") {
        let allowed = types.as_array().map(|v| v.iter().filter_map(Value::as_str).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![types.as_str().unwrap_or("")]);
        let matched = allowed.iter().any(|ty| match *ty {
            "null" => value.is_null(), "object" => value.is_object(), "array" => value.is_array(),
            "string" => value.is_string(), "boolean" => value.is_boolean(), "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(), _ => false,
        });
        if !matched { bail!("{path}: expected {}", allowed.join(" or ")); }
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
        if !choices.contains(value) { bail!("{path}: invalid enum value {value}"); }
    }
    if let Some(array)=value.as_array() {
        if schema["maxItems"].as_u64().is_some_and(|max|array.len() as u64>max) || schema["minItems"].as_u64().is_some_and(|min|(array.len() as u64)<min) {bail!("{path}: array length outside schema bounds");}
    }
    if let Some(number)=value.as_f64() {
        if !number.is_finite() || schema["maximum"].as_f64().is_some_and(|max|number>max) || schema["minimum"].as_f64().is_some_and(|min|number<min) {bail!("{path}: number outside schema bounds");}
    }
    if let Some(text)=value.as_str() {
        if schema["maxLength"].as_u64().is_some_and(|max|text.chars().count() as u64>max) || schema["minLength"].as_u64().is_some_and(|min|(text.chars().count() as u64)<min) {bail!("{path}: text length outside schema bounds");}
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) { bail!("{path}.{key}: missing required field"); }
            }
        }
        if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
            for (key, item) in object {
                let child = properties.get(key).with_context(|| format!("{path}.{key}: unexpected field"))?;
                validate(item, child, root, &format!("{path}.{key}"))?;
            }
        }
    }
    if let (Some(array), Some(item)) = (value.as_array(), schema.get("items")) {
        for (index, value) in array.iter().enumerate() { validate(value, item, root, &format!("{path}[{index}]"))?; }
    }
    Ok(())
}

pub fn validate_draft(draft: ExperimentManifestDraft) -> anyhow::Result<()> {
    let manifest = ExperimentManifest::from_draft(Uuid::nil(), None, 1, draft, "proposal validation");
    // The trusted validator returns advisory warnings on success. This gate returns
    // only pass/fail, but must propagate validation errors with their context.
    validate_manifest(&manifest).context("runtime rejected the proposed experiment")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integration_fields_are_all_mandatory() {
        let schema = proposal_schema();
        let integration = &schema["$defs"]["integration"];
        assert_eq!(integration["required"].as_array().unwrap().len(), 6);
        assert!(integration["required"].as_array().unwrap().contains(&serde_json::json!("start_time")));
        assert!(schema["properties"].get("manifest_json").is_none());
    }
    #[test]
    fn no_manifest_is_valid_for_explanation() {
        let value = serde_json::json!({"assistant_message":"Explain evidence", "action":"explain", "manifest":null, "research_plan":null,
            "requested_capability":"", "capability_gap_reason":"", "suggested_extension":"", "should_run":false});
        validate_proposal_shape(&value).unwrap();
        let mut missing = value; missing.as_object_mut().unwrap().remove("manifest");
        assert!(validate_proposal_shape(&missing).is_err());
    }


    // Test-only generic fixtures; these are never selectable application experiments.
    fn validator_fixture() -> ExperimentManifestDraft {
        serde_json::from_value(serde_json::json!({
            "title": "Validator return-contract fixture",
            "question": "Does this generic draft pass validation?",
            "scientific_boundary": "A software regression fixture, not scientific evidence.",
            "hypothesis": "No scientific claim is made.",
            "model": {
                "kind": "state_vector_ode",
                "variables": [{"name": "x", "unit": "", "initial": 0.0}],
                "derivatives": [{"variable": "x", "expression": "0"}]
            },
            "constants": [],
            "integration": {
                "method": "rk4", "start_time": 0.0, "end_time": 1.0,
                "time_step": 0.01, "output_stride": 1, "max_steps": 100
            },
            "search": {
                "enabled": false, "algorithm": "none", "variables": [], "objectives": [],
                "population": 2, "generations": 1, "elite_fraction": 0.2,
                "mutation_scale": 0.1, "seed": 1
            },
            "observables": [], "constraints": [],
            "visualization": {
                "kind": "trajectory", "x": "t", "y": "x", "z": "0",
                "point_size": 1.0, "max_frames": 100, "trails": true
            },
            "falsification": [],
            "compute": {
                "preference": "cpu", "policy": "interactive", "candidate_count": 1,
                "batch_size": 0, "max_wall_seconds": 10, "max_memory_mb": 256
            },
            "limitations": ["Software fixture only."]
        })).expect("regression fixture must deserialize")
    }

    #[test]
    fn validate_draft_has_unit_success_contract() {
        let gate: fn(ExperimentManifestDraft) -> anyhow::Result<()> = validate_draft;
        let result = gate(validator_fixture());
        assert!(matches!(result, Ok(())));
    }

    #[test]
    fn validate_draft_propagates_validation_error_with_context() {
        let mut draft = validator_fixture();
        draft.integration.time_step = 0.0;
        let error = validate_draft(draft).expect_err("invalid time step must fail");
        assert_eq!(error.to_string(), "runtime rejected the proposed experiment");
        assert!(format!("{error:#}").contains("integration time_step must be positive"));
    }

    #[test]
    fn validate_draft_rejects_invalid_expressions() {
        let mut value = serde_json::to_value(validator_fixture()).unwrap();
        value["model"]["derivatives"][0]["expression"] = serde_json::json!("undeclared_variable");
        let draft: ExperimentManifestDraft = serde_json::from_value(value).unwrap();
        assert!(validate_draft(draft).is_err());
    }

    #[test]
    fn validate_draft_accepts_nonfatal_validator_warnings() {
        let mut value = serde_json::to_value(validator_fixture()).unwrap();
        value["integration"]["method"] = serde_json::json!("euler");
        value["model"] = serde_json::json!({
            "kind": "pairwise_particles", "dimensions": 1,
            "population": {
                "count": 1, "mass": 1.0,
                "position": {"kind": "explicit", "values": [[0.0]]},
                "velocity": {"kind": "explicit", "values": [[0.0]]}
            },
            "interaction": {
                "radial_force": "0", "cutoff": null, "softening": 0.01,
                "linear_damping": 0.0, "external_acceleration": []
            },
            "boundary": {"kind": "open"}
        });
        // Requesting a trajectory for particles produces a warning, not an error.
        let draft: ExperimentManifestDraft = serde_json::from_value(value).unwrap();
        let manifest = ExperimentManifest::from_draft(
            Uuid::nil(), None, 1, draft.clone(), "warning regression fixture",
        );
        let warnings = validate_manifest(&manifest).expect("warning fixture must validate");
        assert!(!warnings.is_empty(), "fixture must exercise a nonempty warning list");
        assert!(validate_draft(draft).is_ok());
    }
}
