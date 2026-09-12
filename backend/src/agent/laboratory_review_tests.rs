use super::*;
use super::super::{Journal, receipt_recovery_tests::fixture};
use tokio_util::sync::CancellationToken;

async fn completed_source(state: &crate::app::AppState, project: Uuid) -> LabJob {
    let id = Uuid::new_v4();
    state.laboratory.create(id, project, None, "generated", "Retained integration result", json!({"engine":"python_numpy","parameters":{"upper":1}}), None).unwrap();
    let result = json!({"reported_result":{"integral":0.3333333333333333,"unit":"1"},"artifacts":[]});
    crate::laboratory::write_json(&state.laboratory.directory(id).join("result.json"), &result).unwrap();
    state.laboratory.update(id, |job| {job.state="completed".into();job.result=result.clone();}).unwrap()
}

fn request(source: Uuid) -> SessionRequest {
    SessionRequest {result_review:Some(ResultReview {source_job_id:source,action:ReviewAction::Explain,source_sha256:None,evidence_files:Default::default(),omitted_file_count:0}),..Default::default()}
}

#[tokio::test]
async fn pins_exact_completed_source_without_changing_chat_model_or_time_budget() {
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    let mut req=request(source.id);req.model=Some("chosen-chat-model".into());req.reasoning_effort=Some("high".into());req.time_limit_seconds=None;
    prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();
    assert_eq!(req.context_job_id,Some(source.id));assert_eq!(req.output_intent,Some(OutputIntent::Explanation));
    assert_eq!(req.model.as_deref(),Some("chosen-chat-model"));assert_eq!(req.reasoning_effort.as_deref(),Some("high"));assert_eq!(req.time_limit_seconds,None);
    assert!(req.content.contains(&source.id.to_string()));assert_eq!(req.result_review.as_ref().unwrap().source_sha256.as_ref().unwrap().len(),64);
    let before=serde_json::to_value(&req).unwrap();prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();assert_eq!(before,serde_json::to_value(&req).unwrap());
    f.state.laboratory.update(source.id,|job| {job.event("seen","Read",json!({}));}).unwrap();
    validate(&f.state.laboratory,source.project_id,&req).unwrap();
    f.state.laboratory.update(source.id,|job| job.result["reported_result"]["integral"]=json!(42)).unwrap();
    assert!(validate(&f.state.laboratory,source.project_id,&req).is_err());
    assert!(prepare(&f.state.laboratory,source.project_id,&mut req).is_err());
}

#[tokio::test]
async fn rejects_changed_disk_evidence_even_when_the_database_result_is_unchanged() {
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    let mut req=request(source.id);prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();
    let before=f.state.laboratory.get(source.id).unwrap().result;
    assert!(req.result_review.as_ref().unwrap().evidence_files.contains_key("result.json"));
    guard(&f.state.laboratory,&f.job,&req,"read_artifact",&json!({"job_id":source.id,"path":"result.json"})).unwrap();
    crate::laboratory::write_json(&f.state.laboratory.directory(source.id).join("result.json"),&json!({"integral":999})).unwrap();
    assert_eq!(f.state.laboratory.get(source.id).unwrap().result,before);
    assert!(validate(&f.state.laboratory,source.project_id,&req).unwrap_err().to_string().contains("evidence changed"));
    let result=f.state.agent.execute_lab_tool(f.state.clone(),&f.job,&req,"inspect_result",&json!({"job_id":source.id}),Uuid::new_v4(),&mut Journal::default(),&CancellationToken::new()).await;
    assert!(result.is_err());
    // The bytes consumed after an earlier successful guard are checked again.
    assert!(read_text(&f.state.laboratory,&req,&json!({"path":"result.json"})).is_err());
    assert!(optional_json(&f.state.laboratory,&req,source.id,"result.json").is_err());
}

#[tokio::test]
async fn later_added_artifact_is_not_read_under_an_earlier_review_identity() {
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    let mut req=request(source.id);prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();
    std::fs::write(f.state.laboratory.directory(source.id).join("new-analysis.txt"),"new unrelated conclusion").unwrap();
    validate(&f.state.laboratory,source.project_id,&req).unwrap();
    assert!(guard(&f.state.laboratory,&f.job,&req,"read_artifact",&json!({"job_id":source.id,"path":"new-analysis.txt"})).is_err());
    assert!(guard(&f.state.laboratory,&f.job,&req,"read_artifact",&json!({"job_id":source.id,"path":"result.json"})).is_ok());
}

#[tokio::test]
async fn rejects_wrong_project_context_incomplete_source_and_unsupported_kind() {
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    assert!(prepare(&f.state.laboratory,Uuid::new_v4(),&mut request(source.id)).is_err());
    let mut wrong=request(source.id);wrong.context_job_id=Some(Uuid::new_v4());
    assert!(prepare(&f.state.laboratory,source.project_id,&mut wrong).is_err());
    f.state.laboratory.update(source.id,|job|job.state="running".into()).unwrap();
    assert!(prepare(&f.state.laboratory,source.project_id,&mut request(source.id)).is_err());
    f.state.laboratory.update(source.id,|job|{job.state="completed".into();job.kind="session".into();}).unwrap();
    assert!(prepare(&f.state.laboratory,source.project_id,&mut request(source.id)).is_err());
}

#[tokio::test]
async fn actual_tool_dispatch_cannot_compute_mutate_or_delegate_and_reads_the_exact_source() {
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    let mut req=request(source.id);prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();
    let before=f.state.laboratory.list(Some(source.project_id)).unwrap().len();
    for tool in ["launch_experiment","run_generated_experiment","publish_simulation","edit_presentation","render_illustration","delegate_specialist","query_surrogate","launch_ml_study"] {
        let error=f.state.agent.execute_lab_tool(f.state.clone(),&f.job,&req,tool,&json!({}),Uuid::new_v4(),&mut Journal::default(),&CancellationToken::new()).await.unwrap_err();
        assert!(error.to_string().contains("evidence inspection only"),"{tool}: {error}");
    }
    assert_eq!(f.state.laboratory.list(Some(source.project_id)).unwrap().len(),before);
    assert!(guard(&f.state.laboratory,&f.job,&req,"set_output_intent",&json!({"intent":"simulation"})).is_err());
    assert!(guard(&f.state.laboratory,&f.job,&req,"inspect_result",&json!({"job_id":f.job.id})).is_err());
    let result=f.state.agent.execute_lab_tool(f.state.clone(),&f.job,&req,"inspect_result",&json!({"job_id":source.id}),Uuid::new_v4(),&mut Journal::default(),&CancellationToken::new()).await.unwrap();
    assert_eq!(result["job"]["id"],json!(source.id));assert_eq!(result["result"]["reported_result"]["integral"],json!(0.3333333333333333));
    let tools=tools_for(&req,super::super::tool_definitions());
    assert!(tools.as_array().unwrap().iter().any(|tool|tool["name"]=="inspect_result"));
    assert!(!tools.as_array().unwrap().iter().any(|tool|tool["name"]=="launch_experiment"));
}

#[tokio::test]
async fn completion_requires_a_successful_exact_source_read_and_retains_review_identity() {
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    let mut req=request(source.id);prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();
    let mut session=f.job.clone();session.input=serde_json::to_value(&req).unwrap();let mut journal=Journal::default();
    let mut assessment=json!({"fulfilled":true});assess(&session,&journal,&mut assessment);assert_eq!(assessment["fulfilled"],false);
    journal.tools.insert("read".into(),json!({"state":"completed","name":"inspect_result","arguments":{"job_id":source.id},"output":{"error":"unavailable"}}));
    assessment=json!({"fulfilled":true});assess(&session,&journal,&mut assessment);assert_eq!(assessment["fulfilled"],false);
    journal.tools.get_mut("read").unwrap()["output"]=json!({"job":{"id":source.id}});
    assessment=json!({"fulfilled":true});assess(&session,&journal,&mut assessment);assert_eq!(assessment["fulfilled"],true);
    assert_eq!(assessment["result_review"]["source_job_id"],json!(source.id));assert_eq!(assessment["result_review"]["source_read"],true);
}

#[test]
fn unknown_review_actions_are_refused_and_old_requests_remain_compatible() {
    assert!(serde_json::from_value::<ResultReview>(json!({"source_job_id":Uuid::new_v4(),"action":"run"})).is_err());
    let old:SessionRequest=serde_json::from_value(json!({"content":"existing request"})).unwrap();assert!(old.result_review.is_none());
    let definitions=super::super::tool_definitions();assert_eq!(tools_for(&old,definitions.clone()),definitions);
}

#[tokio::test]
async fn persisted_native_review_uses_chat_model_reads_source_and_keeps_explanation_in_chat() {
    use axum::{routing::post, Json, Router};
    use std::sync::Arc;
    use parking_lot::Mutex;
    let f=fixture().await;let source=completed_source(&f.state,f.job.project_id).await;
    let mut req=request(source.id);req.request_id=Some(Uuid::new_v4());req.provider=f.request.provider;
    req.model=Some("gpt-6-astra".into());req.reasoning_effort=Some("low".into());req.time_limit_seconds=Some(60);
    prepare(&f.state.laboratory,source.project_id,&mut req).unwrap();
    let id=req.request_id.unwrap();let content=req.content.clone();let source_id=source.id;
    let posts=Arc::new(Mutex::new(Vec::<Value>::new()));let recorded=posts.clone();
    let app=Router::new().route("/v1/responses",post(move |Json(body):Json<Value>|{
        let mut saved=recorded.lock();let first=saved.is_empty();saved.push(body);
        let output=if first {json!([
            {"type":"function_call","call_id":"resolve","name":"set_output_intent","arguments":json!({"request_id":id,"intent":"explanation","scope":"Review the selected retained numerical result without executing another experiment","user_instruction_quote":content,"update_effect":"replace_objective"}).to_string()},
            {"type":"function_call","call_id":"read-source","name":"inspect_result","arguments":json!({"job_id":source_id}).to_string()}
        ])} else {json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"The retained integral is approximately one third. This review proposes checking convergence; no new experiment was run."}]}])};
        async move {Json(json!({"id":"local-review-response","status":"completed","output":output,"usage":{"input_tokens":20,"output_tokens":30}}))}
    }));
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url=format!("http://{}/v1",listener.local_addr().unwrap());
    let server=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
    let mut status=f.state.agent.provider_status(req.provider.unwrap()).unwrap();status.base_url=base_url;
    f.state.database.put_provider_status(&status).unwrap();
    f.state.laboratory.update(f.job.id,|job|job.state="paused".into()).unwrap();
    let app=crate::laboratory::api::routes().with_state(f.state.clone());
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
    let api_server=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
    let client=reqwest::Client::new();
    let post_review=|source_id|client.post(format!("http://{address}/api/laboratory/jobs/{source_id}/review")).json(&req);
    let wrong=post_review(Uuid::new_v4()).send().await.unwrap();assert_eq!(wrong.status(),reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(posts.lock().is_empty());
    let response=post_review(source.id).send().await.unwrap();assert_eq!(response.status(),reqwest::StatusCode::OK);
    let session:LabJob=response.json().await.unwrap();
    let complete=tokio::time::timeout(std::time::Duration::from_secs(30),async {
        loop {let job=f.state.laboratory.get(session.id).unwrap();if !job.active(){break job}tokio::time::sleep(std::time::Duration::from_millis(25)).await;}
    }).await.unwrap();
    let repeated=post_review(source.id).send().await.unwrap();assert_eq!(repeated.status(),reqwest::StatusCode::OK);server.abort();api_server.abort();
    assert_eq!(complete.state,"completed","{:?}",complete.error);
    assert_eq!(complete.result["deliverable"]["result_review"]["source_read"],true);
    assert_eq!(complete.result["result_review"]["source_job_id"],json!(source.id));
    let messages=f.state.database.list_messages(source.project_id,100).unwrap();
    assert!(messages.iter().any(|message|message.content.contains("retained integral")&&message.metadata["result_review"]["source_job_id"]==json!(source.id)));
    let calls=posts.lock();assert_eq!(calls.len(),2);
    assert!(calls.iter().all(|call|call["model"]=="gpt-6-astra"));
    assert!(calls[1].to_string().contains("0.3333333333333333"));
    assert!(!calls[0]["tools"].as_array().unwrap().iter().any(|tool|tool["name"]=="launch_experiment"));
    assert_eq!(f.state.laboratory.list(Some(source.project_id)).unwrap().iter().filter(|job|job.parent_id==Some(session.id)).count(),0);
}
