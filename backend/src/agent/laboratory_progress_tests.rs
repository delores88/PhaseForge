//! Pure durable-progress state tests. These do not call a provider or an engine.
use super::Progress;
use serde_json::{json, Value};
use uuid::Uuid;

fn record_error(progress: &mut Progress, round: u32, request: Option<Uuid>) {
    progress.record(
        round,
        &format!("error-{round}"),
        "inspect_result",
        &json!({"job_id":"fixed-job"}),
        &json!({"error":"The retained solver result is unavailable"}),
        request,
    );
}

fn record_quantity(progress: &mut Progress, round: u32, quantity: u32) {
    progress.record(
        round,
        &format!("quantity-{round}"),
        "inspect_result",
        &json!({"job_id":"fixed-job","artifact":"measurements.json"}),
        &json!({"quantity":quantity}),
        None,
    );
}

fn attention(value: Option<Value>, kind: &str) -> Value {
    attention_with_stalls(value, kind, 3)
}

fn attention_with_stalls(value: Option<Value>, kind: &str, stalled: u32) -> Value {
    let value = value.expect("The progress boundary must request attention");
    assert_eq!(value["schema"], "phaseforge.agent-attention.v1");
    assert_eq!(value["status"], "needs_direction");
    assert_eq!(value["kind"], kind);
    assert_eq!(value["consecutive_stalled_rounds"], stalled);
    assert_eq!(value["requires_user_change"], false);
    assert!(value["reason"].as_str().is_some_and(|text| !text.is_empty()));
    assert!(value["actions"].as_array().is_some_and(|actions| !actions.is_empty()));
    value
}

#[test]
fn three_all_error_rounds_pause_at_the_boundary_and_retain_attention() {
    let mut progress = Progress::default();
    for round in 1..=2 {
        record_error(&mut progress, round, None);
        assert!(progress.finish_round(round).is_none());
    }
    record_error(&mut progress, 3, None);
    let paused = attention(progress.finish_round(3), "repeated_tool_errors");
    let saved = serde_json::to_value(&progress).unwrap();
    assert_eq!(progress.finish_round(3), Some(paused.clone()));
    assert_eq!(serde_json::to_value(&progress).unwrap(), saved);
    assert_eq!(progress.finish_round(4), Some(paused));
}

#[test]
fn a_new_success_then_three_identical_reads_pauses_on_round_four() {
    let mut progress = Progress::default();
    for round in 1..=3 {
        record_quantity(&mut progress, round, 7);
        assert!(progress.finish_round(round).is_none());
    }
    record_quantity(&mut progress, 4, 7);
    attention(progress.finish_round(4), "no_progress");
}

#[test]
fn matching_tool_and_target_success_clears_prior_errors() {
    let mut progress = Progress::default();
    for round in 1..=2 {
        record_error(&mut progress, round, None);
        assert!(progress.finish_round(round).is_none());
    }
    record_error(&mut progress, 3, None);
    record_quantity(&mut progress, 3, 42);
    assert!(progress.finish_round(3).is_none(), "A successful read of the failed target clears its errors");
    for round in 4..=5 {
        record_error(&mut progress, round, None);
        assert!(progress.finish_round(round).is_none());
    }
    record_error(&mut progress, 6, None);
    attention(progress.finish_round(6), "repeated_tool_errors");
}

#[test]
fn same_target_recovery_between_errors_still_detects_unchanged_evidence() {
    let mut progress = Progress::default();
    record_quantity(&mut progress, 1, 7);
    assert!(progress.finish_round(1).is_none());
    for round in 2..=4 {
        record_quantity(&mut progress, round, 7);
        record_error(&mut progress, round, None);
        let result = progress.finish_round(round);
        if round < 4 {
            assert!(result.is_none());
        } else {
            attention(result, "no_progress");
        }
    }
}

#[test]
fn fresh_unrelated_reads_cannot_clear_three_failed_rounds() {
    for (name, target) in [("inspect_result", "other-job"), ("read_artifact", "fixed-job")] {
        let mut progress = Progress::default();
        for round in 1..=3 {
            record_error(&mut progress, round, None);
            progress.record(
                round,
                &format!("unrelated-{round}"),
                name,
                &json!({"job_id":target}),
                &json!({"quantity":round}),
                None,
            );
            let result = progress.finish_round(round);
            if round < 3 {
                assert!(result.is_none());
            } else {
                let paused = attention_with_stalls(result, "repeated_tool_errors", 0);
                assert_eq!(paused["unresolved_failure_rounds"], 3);
            }
        }
    }
}

fn execution_output(id: &str, state: &str, parameter: u32, explicit_identity: bool) -> Value {
    let mut output = json!({"job":{
        "id":id,"kind":"solver","state":state,
        "input":{"engine":"fixture_engine","parameters":{"value":parameter}}
    }});
    if explicit_identity {
        output["execution_identity"] = json!({
            "kind":"solver","engine":"fixture_engine",
            "protocol_sha256":format!("{parameter:064x}")
        });
    }
    output
}

#[test]
fn queued_new_job_ids_cannot_clear_repeated_child_execution_failures() {
    let mut progress = Progress::default();
    for round in 1..=3 {
        let id = Uuid::new_v4().to_string();
        progress.record(
            round, &format!("launch-{round}"), "launch_solver",
            &json!({"engine":"fixture_engine","parameters":{"value":7}}),
            &execution_output(&id, "queued", 7, false), None,
        );
        let mut failed = execution_output(&id, "failed", 7, false);
        failed["error"] = json!("Child execution failed");
        progress.record(
            round, &format!("inspect-{round}"), "inspect_result",
            &json!({"job_id":id}), &failed, None,
        );
        let result = progress.finish_round(round);
        if round < 3 {
            assert!(result.is_none());
        } else {
            let paused = attention_with_stalls(result, "repeated_tool_errors", 0);
            assert_eq!(paused["unresolved_failure_rounds"], 3);
        }
    }
}

#[test]
fn only_completed_matching_execution_protocol_clears_child_failures() {
    for explicit_identity in [false, true] {
        for matching_protocol in [false, true] {
            let mut progress = Progress::default();
            for round in 1..=2 {
                progress.record(
                    round, &format!("failed-{round}"), "inspect_result",
                    &json!({"job_id":format!("failed-{round}")}),
                    &execution_output(&format!("failed-{round}"), "failed", 7, explicit_identity), None,
                );
                assert!(progress.finish_round(round).is_none());
            }
            let parameter = if matching_protocol { 7 } else { 8 };
            let mut completed = execution_output("replacement", "completed", parameter, explicit_identity);
            if explicit_identity {
                // The explicit retained protocol pin must win even when a summary
                // omits the input difference between the two executions.
                completed["job"]["input"]["parameters"] = json!({"value":7});
            }
            progress.record(
                3, "completed", "inspect_result", &json!({"job_id":"replacement"}),
                &completed, None,
            );
            progress.record(
                3, "failed-again", "inspect_result", &json!({"job_id":"another-attempt"}),
                &execution_output("another-attempt", "failed", 7, explicit_identity), None,
            );
            let result = progress.finish_round(3);
            if matching_protocol {
                assert!(result.is_none(), "Completed matching execution clears prior failed attempts");
                for round in 4..=5 {
                    progress.record(
                        round, &format!("failed-{round}"), "inspect_result",
                        &json!({"job_id":format!("failed-{round}")}),
                        &execution_output(&format!("failed-{round}"), "failed", 7, explicit_identity), None,
                    );
                    let next = progress.finish_round(round);
                    if round == 4 {
                        assert!(next.is_none());
                    } else {
                        let paused = attention_with_stalls(next, "repeated_tool_errors", 2);
                        assert_eq!(paused["unresolved_failure_rounds"], 3);
                    }
                }
            } else {
                let paused = attention_with_stalls(result, "repeated_tool_errors", 0);
                assert_eq!(paused["unresolved_failure_rounds"], 3);
            }
        }
    }
}

#[test]
fn changing_numeric_results_have_no_global_total_round_ceiling() {
    let mut progress = Progress::default();
    // An Off wall-clock limit must not become a hidden total-turn limit here.
    for round in 1..=640 {
        record_quantity(&mut progress, round, round);
        assert!(progress.finish_round(round).is_none(), "Novel evidence was paused at round {round}");
    }
}

#[test]
fn changing_poll_metadata_does_not_make_unchanged_evidence_new() {
    let mut progress = Progress::default();
    for round in 1..=4 {
        progress.record(
            round,
            &format!("poll-{round}"),
            "inspect_result",
            &json!({"job_id":"fixed-job"}),
            &json!({
                "quantity":7,
                "updated_at":format!("2026-09-12T12:00:{round:02}Z"),
                "events":[{"id":round,"timestamp":round,"kind":"polled","message":"No change"}]
            }),
            None,
        );
        let result = progress.finish_round(round);
        if round < 4 {
            assert!(result.is_none());
        } else {
            attention(result, "no_progress");
        }
    }
}

#[test]
fn nested_failed_job_state_counts_as_failure_despite_a_successful_read() {
    let mut progress = Progress::default();
    for round in 1..=3 {
        progress.record(
            round,
            &format!("failed-job-{round}"),
            "inspect_result",
            &json!({"job_id":"fixed-job"}),
            &json!({"job":{"id":"fixed-job","state":"failed","error":"Nonfinite solver state"},"read_succeeded":true}),
            None,
        );
        let result = progress.finish_round(round);
        if round < 3 {
            assert!(result.is_none());
        } else {
            attention(result, "repeated_tool_errors");
        }
    }
}

#[test]
fn remember_and_already_resolved_intent_do_not_manufacture_progress() {
    let mut progress = Progress::default();
    for round in 1..=3 {
        record_error(&mut progress, round, None);
        progress.record(
            round,
            &format!("remember-{round}"),
            "remember",
            &json!({"note":format!("Retry attempt {round}")}),
            &json!({"saved":true,"revision":round}),
            None,
        );
        progress.record(
            round,
            &format!("intent-{round}"),
            "set_output_intent",
            &json!({"scope":format!("The same objective, restated {round}")}),
            &json!({"already_resolved":true,"output_intent":{"revision":round}}),
            None,
        );
        let result = progress.finish_round(round);
        if round < 3 {
            assert!(result.is_none());
        } else {
            assert!(result.is_some(), "Bookkeeping must not keep an unsuccessful loop alive");
        }
    }
}

#[test]
fn saved_progress_survives_context_compaction_without_conversation_items() {
    let mut progress = Progress::default();
    record_quantity(&mut progress, 1, 7);
    assert!(progress.finish_round(1).is_none());
    for round in 2..=3 {
        record_quantity(&mut progress, round, 7);
        assert!(progress.finish_round(round).is_none());
    }
    let mut journal = json!({"progress":progress,"items":[{"type":"function_call_output","output":"old context"}]});
    journal["items"] = json!([{"type":"message","content":"Compacted continuation summary"}]);
    let mut restored: Progress = serde_json::from_value(journal["progress"].clone()).unwrap();
    record_quantity(&mut restored, 4, 7);
    let paused = attention(restored.finish_round(4), "no_progress");
    let mut paused_restored: Progress = serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
    assert_eq!(paused_restored.finish_round(4), Some(paused));
}

#[test]
fn replay_of_the_same_call_and_finished_round_is_idempotent() {
    let mut progress = Progress::default();
    record_error(&mut progress, 1, None);
    let before_replay = serde_json::to_value(&progress).unwrap();
    // Even a conflicting replay output cannot rewrite an already recorded call.
    progress.record(1, "error-1", "inspect_result", &json!({}), &json!({"quantity":99}), None);
    assert_eq!(serde_json::to_value(&progress).unwrap(), before_replay);
    assert!(progress.finish_round(1).is_none());
    let finished = serde_json::to_value(&progress).unwrap();
    record_error(&mut progress, 1, None);
    assert!(progress.finish_round(1).is_none());
    assert_eq!(serde_json::to_value(&progress).unwrap(), finished);
    record_error(&mut progress, 2, None);
    assert!(progress.finish_round(2).is_none());
    record_error(&mut progress, 3, None);
    attention(progress.finish_round(3), "repeated_tool_errors");
}

#[test]
fn resume_clears_pause_and_counters_but_preserves_evidence_fingerprints() {
    let mut progress = Progress::default();
    for round in 1..=3 {
        record_quantity(&mut progress, round, 7);
        assert!(progress.finish_round(round).is_none());
    }
    record_quantity(&mut progress, 4, 7);
    attention(progress.finish_round(4), "no_progress");
    progress.resume();
    // Old successful evidence remains known after explicit continuation.
    for round in 5..=6 {
        record_quantity(&mut progress, round, 7);
        assert!(progress.finish_round(round).is_none());
    }
    record_quantity(&mut progress, 7, 7);
    attention(progress.finish_round(7), "no_progress");
}

#[test]
fn only_new_user_request_identity_resets_old_objective_stalls() {
    let first_request = Uuid::new_v4();
    let second_request = Uuid::new_v4();
    let mut progress = Progress::default();
    progress.steer(Some(first_request));
    for round in 1..=2 {
        record_error(&mut progress, round, Some(first_request));
        assert!(progress.finish_round(round).is_none());
    }
    progress.steer(Some(first_request));
    record_error(&mut progress, 3, Some(first_request));
    attention(progress.finish_round(3), "repeated_tool_errors");
    progress.steer(Some(second_request));
    for round in 4..=5 {
        record_error(&mut progress, round, Some(second_request));
        assert!(progress.finish_round(round).is_none());
    }
    record_error(&mut progress, 6, Some(second_request));
    attention(progress.finish_round(6), "repeated_tool_errors");
}

#[test]
fn recording_and_resets_never_mutate_caller_owned_source_receipts() {
    let mut progress = Progress::default();
    let args = json!({"job_id":"fixed-job","source_sha256":"retained-source-pin"});
    let output = json!({"quantity":7,"artifact":{"path":"native/measurements.json","sha256":"frozen-artifact-pin"}});
    let frozen_args = serde_json::to_vec(&args).unwrap();
    let frozen_output = serde_json::to_vec(&output).unwrap();
    progress.record(1, "source-read", "inspect_result", &args, &output, None);
    assert!(progress.finish_round(1).is_none());
    progress.resume();
    progress.steer(Some(Uuid::new_v4()));
    assert_eq!(serde_json::to_vec(&args).unwrap(), frozen_args);
    assert_eq!(serde_json::to_vec(&output).unwrap(), frozen_output);
}
