use anyhow::{Context, bail};
use chrono::{DateTime, Utc};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::domain::{ExperimentManifestDraft, ProviderKind, ProviderModel};
use super::schema::{proposal_schema, validate_proposal_shape};
use super::models::{decorate_model, reasoning_efforts};

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
}

impl ProviderClient {
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
        let response = builder.header(AUTHORIZATION, format!("Bearer {}", self.api_key))
            .timeout(std::time::Duration::from_secs(15)).send().await.context("Background response status request failed")?;
        let status = response.status().as_u16();
        let body = response.json().await.context("Background response status was not JSON")?;
        Ok(ProviderResponse {status, body})
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
            .header(AUTHORIZATION, format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("OpenAI model-list request failed")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("invalid OpenAI model-list response")?;
        if !status.is_success() {
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
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await
            .context("Anthropic model-list request failed")?;
        let status = response.status();
        let body: Value = response
            .json()
            .await
            .context("invalid Anthropic model-list response")?;
        if !status.is_success() {
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
        let schema = proposal_schema();
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
                    .header(AUTHORIZATION,format!("Bearer {}",self.api_key)).json(&body)
            }
            ProviderKind::Anthropic => {
                let mut headers = HeaderMap::new();
                headers.insert("x-api-key", HeaderValue::from_str(&self.api_key).context("Invalid API key header")?);
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
            .send().await.context("Provider request failed")?;
        let status = response.status().as_u16();
        let body: Value = response.json().await.context("Provider returned non-JSON data; usage is unknown")?;
        Ok(ProviderResponse { status, body })
    }
}

pub struct ProviderResponse { pub status: u16, pub body: Value }
impl ProviderResponse {
    pub fn pending(&self) -> bool { (200..300).contains(&self.status) && self.body["status"].as_str().is_some_and(|status| ["queued", "in_progress"].contains(&status)) }
    pub fn text(&self) -> anyhow::Result<String> {
        if !(200..300).contains(&self.status) { bail!("Provider HTTP {}: {}",self.status,safe_error(&self.body)); }
        if self.body.get("status").and_then(Value::as_str) == Some("incomplete")
            || self.body.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") {
            bail!("Provider reached the output-token limit before completing a proposal. Usage was recorded; raise the per-call limit or request a smaller experiment.");
        }
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

fn safe_error(body: &Value) -> String {
    body.get("error")
        .and_then(|error| error.get("message").or(Some(error)))
        .and_then(Value::as_str)
        .or_else(|| body.get("message").and_then(Value::as_str))
        .unwrap_or("provider request failed without a readable error message")
        .chars()
        .take(600)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
