use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::{ExperimentManifestDraft, ProviderKind, ProviderModel};
use super::schema::{proposal_schema, validate_proposal_shape};
use super::models::{decorate_model, reasoning_efforts};

pub(super) fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        // A provider/gateway redirect must never forward a key or research input.
        // In particular, reqwest does not strip the custom Anthropic x-api-key.
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(1800))
        .user_agent(concat!("PhaseForge/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("unable to create AI provider HTTP client")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalAction {
    CreateManifest,
    ReviseManifest,
    Explain,
    CapabilityGap,
    ResearchPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProposal {
    pub assistant_message: String,
    pub action: ProposalAction,
    /// Nested schema-constrained data, never escaped JSON text.
    pub manifest: Option<ExperimentManifestDraft>,
    pub research_plan: Option<crate::research::PlanDraft>,
    pub requested_capability: String,
    pub capability_gap_reason: String,
    pub suggested_extension: String,
    pub should_run: bool,
}

pub struct ProviderClient {
    provider: ProviderKind,
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
    reasoning_effort: Option<String>,
    schema_override: Option<Value>,
}

impl ProviderClient {
    fn secret_header(&self, bearer: bool) -> anyhow::Result<HeaderValue> {
        let value = if bearer { format!("Bearer {}", self.api_key) } else { self.api_key.clone() };
        let mut header = HeaderValue::from_str(&value).context("Invalid API key header")?;
        header.set_sensitive(true);
        Ok(header)
    }

    fn request_error(&self, context: &str, error: reqwest::Error) -> anyhow::Error {
        // Do not retain an unsanitized error as an anyhow source: callers render
        // complete error chains into API responses and saved job diagnostics.
        anyhow::anyhow!("{context}: {}", crate::usage::redact_credentials(&error.to_string(), Some(&self.api_key)))
    }

    async fn read_response(&self, response: reqwest::Response, context: &str) -> anyhow::Result<ProviderResponse> {
        let status = response.status().as_u16();
        let mut body: Value = response.json().await.map_err(|error| self.request_error(context, error))?;
        redact_response_key(&mut body, &self.api_key);
        Ok(ProviderResponse { status, body })
    }

    pub fn new(
        provider: ProviderKind,
        api_key: String,
        model: String,
        base_url: String,
        client: reqwest::Client,
    ) -> Self {
        Self {
            provider,
            api_key,
            model,
            base_url: base_url.trim_end_matches('/').to_owned(),
            client,
            reasoning_effort: None,
            schema_override: None,
        }
    }

    pub fn default_base_url(provider: ProviderKind) -> &'static str {
        match provider {
            ProviderKind::OpenAi => "https://api.openai.com",
            ProviderKind::Anthropic => "https://api.anthropic.com",
        }
    }

    pub fn with_reasoning(mut self, effort: Option<&str>) -> anyhow::Result<Self> {
        if let Some(effort) = effort.filter(|v| !v.trim().is_empty()) {
            let allowed = reasoning_efforts(self.provider, &self.model);
            if !allowed.iter().any(|value| value == effort) {
                bail!("Reasoning effort '{effort}' is not supported by {}. Available levels: {}", self.model, allowed.join(", "));
            }
            self.reasoning_effort = Some(effort.to_owned());
        }
        Ok(self)
    }

    fn use_background(&self) -> bool {
        self.provider == ProviderKind::OpenAi && (self.model.contains("-pro")
            || self.reasoning_effort.as_deref().is_some_and(|effort| ["high", "xhigh", "max"].contains(&effort)))
    }

    pub(super) fn with_schema(mut self, schema: Value) -> Self { self.schema_override=Some(schema); self }
    pub(super) fn structured_schema(&self) -> Value { self.schema_override.clone().unwrap_or_else(proposal_schema) }

    pub async fn poll_response(&self, id: &str) -> anyhow::Result<ProviderResponse> {
        self.response_operation(id, false).await
    }

    pub async fn cancel_response(&self, id: &str) -> anyhow::Result<ProviderResponse> {
        self.response_operation(id, true).await
    }

    async fn response_operation(&self, id: &str, cancel: bool) -> anyhow::Result<ProviderResponse> {
        if self.provider != ProviderKind::OpenAi || id.is_empty() || id.len() > 200
            || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            bail!("Provider returned an invalid background response identifier");
        }
        let route = format!("responses/{id}{}", if cancel {"/cancel"} else {""});
        let builder = if cancel { self.client.post(endpoint(&self.base_url, &route)) }
            else { self.client.get(endpoint(&self.base_url, &route)) };
        let response = builder.header(AUTHORIZATION, self.secret_header(true)?)
            .timeout(std::time::Duration::from_secs(15)).send().await
            .map_err(|error| self.request_error("Background response status request failed", error))?;
        self.read_response(response, "Background response status was not JSON").await
    }

    pub async fn list_models(&self) -> anyhow::Result<Vec<ProviderModel>> {
        let mut models = match self.provider {
            ProviderKind::OpenAi => self.list_openai_models().await?,
            ProviderKind::Anthropic => self.list_anthropic_models().await?,
        };
        for model in &mut models { decorate_model(self.provider, model); }
        models.sort_by(|left, right| {
            right
                .capability_rank
                .cmp(&left.capability_rank)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| left.display_name.cmp(&right.display_name))
        });
        models.dedup_by(|left, right| left.id == right.id);
        // Recommend only the strongest account-visible text model. Availability
        // comes from the provider, while ordering is a documented product heuristic.
        if let Some(model) = models.first_mut() { model.recommended = true; }
        Ok(models)
    }

    async fn list_openai_models(&self) -> anyhow::Result<Vec<ProviderModel>> {
        let response = self
            .client
            .get(endpoint(&self.base_url, "models"))
            .header(AUTHORIZATION, self.secret_header(true)?)
            .send()
            .await
            .map_err(|error| self.request_error("OpenAI model-list request failed", error))?;
        let ProviderResponse {status, body} = self.read_response(response, "invalid OpenAI model-list response").await?;
        if !(200..300).contains(&status) {
            bail!("OpenAI returned {status}: {}", safe_error(&body));
        }
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .context("OpenAI model list did not include a data array")?;
        let models = data
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.trim().to_owned();
                if id.is_empty() || !is_relevant_openai_model(&id) {
                    return None;
                }
                let created_at = item
                    .get("created")
                    .and_then(Value::as_i64)
                    .and_then(|seconds| DateTime::<Utc>::from_timestamp(seconds, 0))
                    .map(|value| value.to_rfc3339());
                Some(ProviderModel {
                    display_name: humanize_model_id(&id),
                    recommended: is_recommended_openai_model(&id),
                    id,
                    created_at,
                    owned_by: item
                        .get("owned_by")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    capability_rank: 0, reasoning_efforts: vec![], recommended_reasoning: None, description: String::new(),
                })
            })
            .collect::<Vec<_>>();
        Ok(models)
    }

    async fn list_anthropic_models(&self) -> anyhow::Result<Vec<ProviderModel>> {
        let response = self
            .client
            .get(endpoint(&self.base_url, "models?limit=1000"))
            .header("x-api-key", self.secret_header(false)?)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .map_err(|error| self.request_error("Anthropic model-list request failed", error))?;
        let ProviderResponse {status, body} = self.read_response(response, "invalid Anthropic model-list response").await?;
        if !(200..300).contains(&status) {
            bail!("Anthropic returned {status}: {}", safe_error(&body));
        }
        let data = body
            .get("data")
            .and_then(Value::as_array)
            .context("Anthropic model list did not include a data array")?;
        Ok(data
            .iter()
            .filter_map(|item| {
                let id = item.get("id")?.as_str()?.trim().to_owned();
                if id.is_empty() {
                    return None;
                }
                let display_name = item
                    .get("display_name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| humanize_model_id(&id));
                let lower = id.to_ascii_lowercase();
                Some(ProviderModel {
                    id,
                    display_name,
                    created_at: item
                        .get("created_at")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    owned_by: Some("anthropic".to_owned()),
                    recommended: (lower.contains("sonnet") || lower.contains("opus"))
                        && !lower.contains("deprecated"),
                    capability_rank: 0, reasoning_efforts: vec![], recommended_reasoning: None, description: String::new(),
                })
            })
            .collect())
    }

    /// One HTTP request, no hidden retries. Caller records usage before parsing.
    pub async fn request_completion(
        &self, prompt: &str, system: &str, structured: bool, max_tokens: u32,
    ) -> anyhow::Result<ProviderResponse> {
        if self.model.trim().is_empty() { bail!("No active model selected"); }
        let schema = self.structured_schema();
        let builder = match self.provider {
            ProviderKind::OpenAi => {
                let mut body = json!({"model":self.model,"instructions":system,"input":prompt,
                    "max_output_tokens":max_tokens,"store":false});
                if let Some(effort) = &self.reasoning_effort { body["reasoning"] = json!({"effort":effort}); }
                if self.use_background() { body["background"] = json!(true); }
                if structured {
                    body["text"] = json!({"format":{"type":"json_schema","name":"phaseforge_research_proposal",
                        "strict":true,"schema":schema}});
                }
                self.client.post(endpoint(&self.base_url,"responses"))
                    .header(AUTHORIZATION,self.secret_header(true)?).json(&body)
            }
            ProviderKind::Anthropic => {
                let mut headers = HeaderMap::new();
                headers.insert("x-api-key", self.secret_header(false)?);
                headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                let mut body = json!({"model":self.model,"max_tokens":max_tokens,"system":system,
                    "messages":[{"role":"user","content":prompt}]});
                if structured { body["output_config"] = json!({"format":{"type":"json_schema","schema":schema}}); }
                if let Some(effort) = &self.reasoning_effort {
                    body["output_config"]["effort"] = json!(effort);
                    if !self.model.contains("opus-4-5") { body["thinking"] = json!({"type":"adaptive"}); }
                }
                self.client.post(endpoint(&self.base_url,"messages")).headers(headers).json(&body)
            }
        };
        let response = builder.timeout(std::time::Duration::from_secs(if self.use_background() {90} else {1800}))
            .send().await.map_err(|error| self.request_error("Provider request failed", error))?;
        self.read_response(response, "Provider returned non-JSON data; usage is unknown").await
    }
}

pub struct ProviderResponse { pub status: u16, pub body: Value }
#[derive(Debug, thiserror::Error)]
#[error("Provider reached the output-token limit before completing the artifact. Usage was recorded and the output ceiling was preserved.")]
pub struct OutputLimitError;
impl ProviderResponse {
    pub fn pending(&self) -> bool { (200..300).contains(&self.status) && self.body["status"].as_str().is_some_and(|status| ["queued", "in_progress"].contains(&status)) }
    pub fn text(&self) -> anyhow::Result<String> {
        if !(200..300).contains(&self.status) { bail!("Provider HTTP {}: {}",self.status,safe_error(&self.body)); }
        if (self.body["status"] == "incomplete" && self.body["incomplete_details"]["reason"] == "max_output_tokens")
            || self.body["stop_reason"] == "max_tokens" {
            return Err(OutputLimitError.into());
        }
        if self.body["status"] == "incomplete" {bail!("Provider returned an incomplete response for a reason other than output length; no automatic recovery was attempted.");}
        if self.body.get("error").is_some_and(|v| !v.is_null()) { bail!("Provider error: {}",safe_error(&self.body)); }
        if self.body["status"] == "cancelled" { bail!("Provider background response was cancelled"); }
        if self.body["status"] == "failed" { bail!("Provider background response failed: {}",safe_error(&self.body)); }
        if self.body.get("stop_reason").and_then(Value::as_str) == Some("refusal") { bail!("Provider declined this request; no experiment was executed."); }
        if let Some(outputs)=self.body.get("output").and_then(Value::as_array) {
            for item in outputs.iter().filter_map(|v| v.get("content").and_then(Value::as_array)).flatten() {
                if item.get("type").and_then(Value::as_str)==Some("refusal") { bail!("Provider declined this request; no experiment was executed."); }
            }
        }
        if let Some(text) = extract_openai_text(&self.body) { return Ok(text); }
        let text = self.body.get("content").and_then(Value::as_array).map(|items| items.iter()
            .filter(|v| v.get("type").and_then(Value::as_str)==Some("text"))
            .filter_map(|v| v.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        if text.trim().is_empty() { bail!("Provider response contained no text. Check that the selected model supports this API and structured outputs."); }
        Ok(text)
    }
}

pub fn parse_proposal(text: &str) -> anyhow::Result<AgentProposal> {
    let value: Value = serde_json::from_str(text.trim()).context("Provider proposal is not valid JSON")?;
    validate_proposal_shape(&value)?;
    let proposal: AgentProposal = serde_json::from_value(value).context("Proposal does not match the runtime contract")?;
    if proposal.assistant_message.trim().is_empty() { bail!("Proposal assistant_message is empty"); }
    match proposal.action {
        ProposalAction::CreateManifest | ProposalAction::ReviseManifest => {
            if proposal.research_plan.is_some(){bail!("manifest actions require research_plan=null; prepare one artifact at a time");}
            super::schema::validate_draft(proposal.manifest.clone().context("Manifest action requires a non-null manifest")?)?;
        }
        ProposalAction::ResearchPlan => {
            if proposal.manifest.is_some()||proposal.should_run {bail!("research plans/capability gaps require manifest=null and should_run=false");}
            if proposal.research_plan.is_none(){bail!("provide a useful research_plan for available evidence/data/design work, even when one calculation is unavailable");}
        }
        ProposalAction::CapabilityGap => {
            if proposal.manifest.is_some()||proposal.should_run {bail!("capability gaps require manifest=null and should_run=false");}
        }
        ProposalAction::Explain => {
            if proposal.manifest.is_some()||proposal.research_plan.is_some()||proposal.should_run{bail!("explain requires null artifacts and should_run=false");}
        }
    }
    Ok(proposal)
}

fn endpoint(base: &str, route: &str) -> String {
    let base=base.trim_end_matches('/');
    if base.ends_with("/v1") { format!("{base}/{route}") } else { format!("{base}/v1/{route}") }
}

fn is_relevant_openai_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    let excluded = [
        "embedding",
        "moderation",
        "whisper",
        "tts",
        "audio",
        "realtime",
        "transcribe",
        "dall-e",
        "image",
        "sora",
        "instruct",
        "search",
        "deep-research",
    ];
    if excluded.iter().any(|term| lower.contains(term)) {
        return false;
    }
    lower.starts_with("gpt-")
        || lower.starts_with("chatgpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
        || lower.contains("codex")
}

fn is_recommended_openai_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    !lower.contains("instruct")
        && !lower.contains("preview")
        && !lower.contains("legacy")
        && (lower.starts_with("gpt-")
            || lower.starts_with("o3")
            || lower.starts_with("o4")
            || lower.contains("codex"))
}

fn humanize_model_id(id: &str) -> String {
    id.split('-')
        .map(|part| {
            if part.is_empty() {
                return String::new();
            }
            let mut chars = part.chars();
            let first = chars.next().unwrap_or_default();
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_openai_text(body: &Value) -> Option<String> {
    if let Some(value) = body.get("output_text").and_then(Value::as_str) {
        return Some(value.to_owned());
    }
    body.get("output")
        .and_then(Value::as_array)
        .and_then(|outputs| {
            outputs.iter().find_map(|output| {
                output
                    .get("content")
                    .and_then(Value::as_array)
                    .and_then(|content| {
                        content.iter().find_map(|item| {
                            item.get("text").and_then(Value::as_str).map(str::to_owned)
                        })
                    })
            })
        })
}

fn redact_response_key(body: &mut Value, key: &str) {
    if key.is_empty() { return; }
    fn scrub(value: &mut Value, key: &str, path: &mut Vec<String>) {
        // These fields drive polling, output-limit/refusal handling and catalog
        // selection. A custom credential can coincidentally equal their values;
        // changing protocol metadata would change execution/accounting behavior.
        let fields: Vec<&str> = path.iter().map(String::as_str).collect();
        if matches!(fields.as_slice(), ["id"] | ["status"] | ["stop_reason"] |
            ["incomplete_details", "reason"] | ["data", "[]", "id"] |
            ["output", "[]", "type"] | ["output", "[]", "id"] |
            ["output", "[]", "role"] | ["output", "[]", "content", "[]", "type"] |
            ["content", "[]", "type"]) { return; }
        match value {
            Value::String(text) => *text = text.replace(key, "[REDACTED]"),
            Value::Array(items) => {
                path.push("[]".into());
                for item in items { scrub(item, key, path); }
                path.pop();
            }
            Value::Object(object) => for (field, value) in object {
                path.push(field.clone());
                scrub(value, key, path);
                path.pop();
            },
            _ => {}
        }
    }
    scrub(body, key, &mut Vec::new());
}

fn safe_error(body: &Value) -> String {
    let message = body.get("error")
        .and_then(|error| error.get("message").or(Some(error)))
        .and_then(Value::as_str)
        .or_else(|| body.get("message").and_then(Value::as_str))
        .unwrap_or("provider request failed without a readable error message");
    crate::usage::redact_credentials(message, None).chars()
        .take(600)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture(router: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap(); });
        (base, server)
    }

    #[tokio::test]
    async fn provider_security_redirects_never_reach_a_second_origin() {
        use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
        let hits = Arc::new(AtomicUsize::new(0));
        let seen = hits.clone();
        let (destination, target) = fixture(axum::Router::new().fallback(move || {
            let seen = seen.clone();
            async move { seen.fetch_add(1, Ordering::SeqCst); axum::Json(json!({"error":{"message":"unexpected redirect"}})) }
        })).await;
        let source_hits = Arc::new(AtomicUsize::new(0));
        let seen = source_hits.clone();
        let (base, source) = fixture(axum::Router::new().fallback(move |headers: axum::http::HeaderMap| {
            let destination = destination.clone();
            let seen = seen.clone();
            async move {
                assert!(headers.contains_key(AUTHORIZATION) || headers.contains_key("x-api-key"));
                seen.fetch_add(1, Ordering::SeqCst);
                (axum::http::StatusCode::TEMPORARY_REDIRECT,
                    [(axum::http::header::LOCATION, destination)],
                    axum::Json(json!({"error":{"message":"Provider redirected the request"}})))
            }
        })).await;
        for provider in [ProviderKind::OpenAi, ProviderKind::Anthropic] {
            let client = ProviderClient::new(provider, "fixture-key-never-real".into(), "fixture-model".into(), base.clone(), http_client().unwrap());
            let response = client.request_completion("fixture", "fixture", false, 32).await.unwrap();
            assert_eq!(response.status, 307);
            assert!(response.text().is_err());
            assert!(client.list_models().await.is_err());
            if provider == ProviderKind::OpenAi {
                assert_eq!(client.poll_response("resp_fixture").await.unwrap().status, 307);
                assert_eq!(client.cancel_response("resp_fixture").await.unwrap().status, 307);
            }
        }
        source.abort();
        target.abort();
        assert_eq!(source_hits.load(Ordering::SeqCst), 6);
        assert_eq!(hits.load(Ordering::SeqCst), 0, "A credential-bearing request crossed the redirect boundary");
    }

    #[tokio::test]
    async fn provider_security_echoed_keys_do_not_escape_into_errors_or_usage() {
        let key = "fixture-nonstandard-key-never-real";
        let body = json!({"id":"resp_fixture", "error":{"message":format!("Invalid credential {key}; retry later")},
            "diagnostic":{"nested":[key]}, "usage":{"input_tokens":20,"output_tokens":7}});
        let (base, server) = fixture(axum::Router::new().fallback(move || {
            let body = body.clone();
            async move { (axum::http::StatusCode::UNAUTHORIZED, axum::Json(body)) }
        })).await;
        for provider in [ProviderKind::OpenAi, ProviderKind::Anthropic] {
            let client = ProviderClient::new(provider, key.into(), "fixture-model".into(), base.clone(), http_client().unwrap());
            for bearer in [false, true] { assert!(client.secret_header(bearer).unwrap().is_sensitive()); }
            let response = client.request_completion("fixture", "fixture", false, 32).await.unwrap();
            assert!(!response.body.to_string().contains(key));
            let error = format!("{:#}", response.text().unwrap_err());
            assert!(!error.contains(key));
            assert!(error.contains("[REDACTED]"));
            assert!(error.contains("retry later"));
            assert!(!format!("{:#}", client.list_models().await.unwrap_err()).contains(key));
            if provider == ProviderKind::OpenAi {
                assert!(!client.poll_response("resp_fixture").await.unwrap().body.to_string().contains(key));
                assert!(!client.cancel_response("resp_fixture").await.unwrap().body.to_string().contains(key));
            }
            let db = crate::persistence::Database::open(std::path::Path::new(":memory:")).unwrap();
            let usage = crate::usage::UsageService::new(db.clone()).unwrap();
            let row = usage.begin(uuid::Uuid::new_v4(), None, provider, "fixture-model", "fixture", 0, 10, 32).unwrap();
            usage.finish(row.id, "provider_error", Some(&response.body), Some(&error)).unwrap();
            let saved = db.get_usage_record(row.id).unwrap().unwrap();
            assert_eq!(saved.usage.total_tokens, 27);
            assert_eq!(saved.provider_response_id.as_deref(), Some("resp_fixture"));
            assert!(!serde_json::to_string(&saved).unwrap().contains(key));
            assert!(!usage.report().unwrap().to_string().contains(key));
        }
        server.abort();
    }

    #[tokio::test]
    async fn provider_security_failed_success_status_and_network_errors_are_redacted() {
        let key = "fixture-active-key-never-real";
        let body = json!({"status":"failed", "error":{"message":format!("Denied {key}")},"usage":{"input_tokens":3}});
        let (base, server) = fixture(axum::Router::new().fallback(move || {
            let body = body.clone();
            async move { axum::Json(body) }
        })).await;
        let client = ProviderClient::new(ProviderKind::OpenAi, key.into(), "fixture-model".into(), base, http_client().unwrap());
        let response = client.poll_response("resp_fixture").await.unwrap();
        assert_eq!(response.status, 200);
        assert!(!format!("{:#}", response.text().unwrap_err()).contains(key));
        server.abort();

        // Port zero cannot be a listening destination. A secret in a malformed
        // gateway URL must not survive in reqwest's nested transport diagnostic.
        let client = ProviderClient::new(ProviderKind::OpenAi, key.into(), "fixture-model".into(),
            format!("http://127.0.0.1:0/{key}"), http_client().unwrap());
        let error = client.list_models().await.unwrap_err();
        assert!(!format!("{error:#}").contains(key));
        assert!(!format!("{error:?}").contains(key));
    }

    #[test]
    fn provider_security_collision_keys_preserve_protocol_semantics() {
        let mut body = json!({"status":"in_progress", "id":"resp_fixture", "message":"in_progress"});
        redact_response_key(&mut body, "in_progress");
        let response = ProviderResponse {status:200, body};
        assert!(response.pending());
        assert_eq!(response.body["message"], "[REDACTED]");

        for (key, mut body) in [
            ("max_tokens", json!({"stop_reason":"max_tokens"})),
            ("max_output_tokens", json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"}})),
        ] {
            redact_response_key(&mut body, key);
            assert!(ProviderResponse {status:200,body}.text().unwrap_err().downcast_ref::<OutputLimitError>().is_some());
        }
        let mut body = json!({"status":"completed", "id":"resp_fixture",
            "output":[{"type":"message","content":[{"type":"output_text","text":"Kept scientific answer"}]}]});
        redact_response_key(&mut body, "output_text");
        redact_response_key(&mut body, "resp_fixture");
        assert_eq!(body["id"], "resp_fixture");
        assert_eq!(body["output"][0]["content"][0]["type"], "output_text");
        assert_eq!(ProviderResponse {status:200,body}.text().unwrap(), "Kept scientific answer");

        let mut body = json!({"output":[{"type":"message","content":[{"type":"refusal","refusal":"Cannot help"}]}]});
        redact_response_key(&mut body, "refusal");
        assert!(ProviderResponse {status:200,body}.text().unwrap_err().to_string().contains("declined"));

        let mut catalog = json!({"data":[{"id":"fixture-model","description":"fixture-model"}]});
        redact_response_key(&mut catalog, "fixture-model");
        assert_eq!(catalog["data"][0]["id"], "fixture-model");
        assert_eq!(catalog["data"][0]["description"], "[REDACTED]");
    }

    #[test]
    fn proposal_contract_is_closed_and_requires_every_field() {
        let schema = proposal_schema();
        assert_eq!(schema["additionalProperties"], json!(false));
        assert_eq!(schema["required"].as_array().expect("required").len(), 8);
        assert_eq!(schema["properties"].as_object().expect("properties").len(), 8);
    }

    #[test]
    fn text_model_filter_rejects_specialized_non_text_models() {
        assert!(is_relevant_openai_model("gpt-example"));
        assert!(!is_relevant_openai_model("text-embedding-example"));
    }
}
