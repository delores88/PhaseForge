use super::*;
#[path="laboratory_anthropic_tests.rs"] mod anthropic;
fn call(id:&str)->Value{json!({"type":"function_call","call_id":id,"name":"observe_frame","arguments":"{}"})}
fn output(id:&str)->Value{json!({"type":"function_call_output","call_id":id,"output":format!("retained {id} evidence")})}
fn pixels(id:&str)->Value{json!({"role":"user","content":[{"type":"input_text","text":format!("Actual {id} image")},{"type":"input_image","image_url":format!("data:image/png;base64,{id}"),"detail":"high"}]})}
fn legacy()->Vec<Value>{vec![json!({"role":"user","content":"Observe both frames"}),call("a"),call("b"),output("a"),pixels("a"),output("b"),pixels("b"),json!({"role":"user","content":"Keep the updated constraints"})]}

#[test]fn replay_and_multiple_images_keep_all_results_first_without_changing_evidence(){
    let original=legacy();let normalized=ordered(&original,true).unwrap();
    assert_eq!(normalized,vec![original[0].clone(),call("a"),call("b"),output("a"),output("b"),pixels("a"),pixels("b"),original[7].clone()]);
    assert_eq!(ordered(&normalized,true).unwrap(),normalized);
    let mut journal=Journal{items:vec![json!({"archived":"compaction source must not change"})],context_start:1,..Default::default()};journal.items.extend(original);
    retain_order(&mut journal).unwrap();let encoded=serde_json::to_vec(&journal).unwrap();let mut replay:Journal=serde_json::from_slice(&encoded).unwrap();retain_order(&mut replay).unwrap();
    assert_eq!(serde_json::to_vec(&replay).unwrap(),encoded);assert_eq!(replay.items[0]["archived"],"compaction source must not change");
    // A second turn must not move its image into the preceding evidence group.
    let mut two=normalized.clone();two.extend([call("c"),output("c"),pixels("c")]);assert_eq!(ordered(&two,true).unwrap(),two);
}
#[test]fn partial_recovery_defers_images_and_refuses_incomplete_or_conflicting_dispatch(){
    let partial=vec![call("a"),call("b"),output("a"),pixels("a")];assert!(ordered(&partial,true).is_err());assert_eq!(ordered(&partial,false).unwrap(),partial);
    let mut steered=partial;steered.push(output("b"));steered.push(json!({"role":"user","content":"Explicit user update"}));
    let normalized=ordered(&steered,true).unwrap();assert_eq!(normalized[3],output("b"));assert_eq!(normalized[4],pixels("a"));
    assert!(ordered(&[call("a"),call("a")],false).is_err());assert!(ordered(&[output("unknown")],true).is_err());
}

#[tokio::test]async fn both_provider_wire_formats_deliver_all_results_before_native_pixels(){
    use axum::{Router,Json};use parking_lot::Mutex;
    let captured=Arc::new(Mutex::new(vec![]));let seen=captured.clone();
    let app=Router::new().fallback(move|Json(body):Json<Value>|{seen.lock().push(body);async{Json(json!({"status":"completed","output":[]}))}});
    let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base=format!("http://{}",listener.local_addr().unwrap());
    let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap();});
    let context=ordered(&legacy(),true).unwrap();
    for provider in [ProviderKind::OpenAi,ProviderKind::Anthropic]{
        let client=ProviderClient::new(provider,"local-protocol-fixture".into(),"fixture-model".into(),base.clone(),crate::agent::providers::http_client().unwrap());
        client.request_tool_turn(&context,&[],"Protocol check only",32).await.unwrap();
    }
    server.abort();let bodies=captured.lock();assert_eq!(bodies.len(),2);assert_eq!(bodies[0]["input"],json!(context));
    let messages=bodies[1]["messages"].as_array().unwrap();assert_eq!(messages.len(),3);assert_eq!(messages[1]["role"],"assistant");assert_eq!(messages[1]["content"].as_array().unwrap().len(),2);
    let content=messages[2]["content"].as_array().unwrap();assert_eq!(content[0]["type"],"tool_result");assert_eq!(content[0]["tool_use_id"],"a");assert_eq!(content[1]["type"],"tool_result");assert_eq!(content[1]["tool_use_id"],"b");
    assert_eq!(content.iter().filter(|part|part["type"]=="image").map(|part|part["source"]["data"].as_str().unwrap()).collect::<Vec<_>>(),vec!["a","b"]);
    assert!(content[2..].iter().all(|part|part["type"]!="tool_result"));assert_eq!(content.last().unwrap()["text"],"Keep the updated constraints");
}
