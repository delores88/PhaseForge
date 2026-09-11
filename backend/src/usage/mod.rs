//! Local usage accounting and admission controls. No provider prices are guessed.
//! Reservations are conservative preflight estimates, NOT provider billing limits.
mod redaction;
pub(crate) use redaction::redact_credentials;

use std::sync::Arc;
use anyhow::{bail, Context};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;
use crate::{domain::ProviderKind, persistence::Database};

#[derive(Debug,thiserror::Error)]
#[error("The configured concurrent-call limit is reached. Wait or stop an active call in Usage & cost.")]
pub struct ConcurrentCallLimit;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRate {
    pub provider: ProviderKind,
    pub model: String,
    pub input_per_million: f64,
    pub output_per_million: f64,
    pub cached_input_per_million: f64,
    pub cache_write_5m_per_million: f64,
    pub cache_write_1h_per_million: f64,
}
impl ModelRate {
    fn maximum_input_rate(&self) -> f64 {
        self.input_per_million.max(self.cached_input_per_million)
            .max(self.cache_write_5m_per_million).max(self.cache_write_1h_per_million)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageSettings {
    pub paused: bool,
    pub max_output_tokens: u32,
    pub repair_attempts: u32,
    pub max_parallel_calls: usize,
    pub token_limit: Option<u64>,
    pub cost_limit_usd: Option<f64>,
    pub enforce_limits: bool,
    pub warning_percent: u32,
    pub rates: Vec<ModelRate>,
}
impl Default for UsageSettings {
    fn default() -> Self {
        Self { paused: false, max_output_tokens: 12000, repair_attempts: 1,
            max_parallel_calls: 1, token_limit: None, cost_limit_usd: None,
            enforce_limits: true, warning_percent: 80, rates: Vec::new() }
    }
}
impl UsageSettings {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !(512..=64000).contains(&self.max_output_tokens) { bail!("Output limit must be 512–64,000 tokens"); }
        if self.repair_attempts > 2 { bail!("At most two repair attempts are allowed"); }
        if !(1..=8).contains(&self.max_parallel_calls) { bail!("Parallel calls must be 1–8"); }
        if self.token_limit == Some(0) { bail!("Token limit must be positive or disabled"); }
        if let Some(cost) = self.cost_limit_usd {
            if !cost.is_finite() || cost <= 0.0 { bail!("Cost limit must be positive or disabled"); }
        }
        if !(1..=100).contains(&self.warning_percent) { bail!("Warning threshold must be 1–100 percent"); }
        if self.rates.len() > 200 { bail!("Too many model rate cards"); }
        let mut keys = std::collections::HashSet::new();
        for rate in &self.rates {
            if rate.model.trim().is_empty() || rate.model.len() > 200 { bail!("Each rate requires an exact model ID"); }
            let key = format!("{}:{}", rate.provider.account_name(), rate.model);
            if !keys.insert(key) { bail!("Duplicate rate card for a provider/model"); }
            for value in [rate.input_per_million, rate.output_per_million, rate.cached_input_per_million,
                rate.cache_write_5m_per_million, rate.cache_write_1h_per_million] {
                if !value.is_finite() || value < 0.0 || value > 100_000.0 { bail!("Rates must be finite, nonnegative USD per million tokens"); }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub reported: bool,
    /// Total input including cache reads/writes for both providers.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
    /// Subset of output tokens, never added a second time.
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
}
impl TokenUsage {
    pub fn from_response(provider: ProviderKind, body: &Value) -> Self {
        let Some(u) = body.get("usage").filter(|v| v.is_object()) else { return Self::default(); };
        let get = |key: &str| u.get(key).and_then(Value::as_u64).unwrap_or(0);
        let output = get("output_tokens");
        let input = get("input_tokens");
        let (cached, write5, write1, reasoning, total_input) = match provider {
            ProviderKind::OpenAi => {
                let cached = u.pointer("/input_tokens_details/cached_tokens").and_then(Value::as_u64).unwrap_or(0).min(input);
                let reasoning = u.pointer("/output_tokens_details/reasoning_tokens").and_then(Value::as_u64).unwrap_or(0).min(output);
                (cached, 0, 0, reasoning, input)
            }
            ProviderKind::Anthropic => {
                let cached = get("cache_read_input_tokens");
                let creation = get("cache_creation_input_tokens");
                let write1 = u.pointer("/cache_creation/ephemeral_1h_input_tokens").and_then(Value::as_u64).unwrap_or(0).min(creation);
                let write5 = creation.saturating_sub(write1);
                (cached, write5, write1, 0, input.saturating_add(cached).saturating_add(creation))
            }
        };
        Self { reported: u.get("input_tokens").and_then(Value::as_u64).is_some() && u.get("output_tokens").and_then(Value::as_u64).is_some(),
            input_tokens: total_input, output_tokens: output, cached_input_tokens: cached,
            cache_write_5m_tokens: write5, cache_write_1h_tokens: write1, reasoning_tokens: reasoning,
            total_tokens: total_input.saturating_add(output) }
    }
    pub fn estimate_cost(&self, rate: &ModelRate) -> f64 {
        let uncached = self.input_tokens.saturating_sub(self.cached_input_tokens)
            .saturating_sub(self.cache_write_5m_tokens).saturating_sub(self.cache_write_1h_tokens);
        (uncached as f64 * rate.input_per_million + self.output_tokens as f64 * rate.output_per_million
            + self.cached_input_tokens as f64 * rate.cached_input_per_million
            + self.cache_write_5m_tokens as f64 * rate.cache_write_5m_per_million
            + self.cache_write_1h_tokens as f64 * rate.cache_write_1h_per_million) / 1_000_000.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Uuid,
    pub request_id: Uuid,
    pub project_id: Option<Uuid>,
    pub provider: ProviderKind,
    pub model: String,
    pub purpose: String,
    pub attempt: u32,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub provider_response_id: Option<String>,
    pub usage: TokenUsage,
    pub reserved_input_tokens: u64,
    pub reserved_output_tokens: u64,
    pub estimated_cost_usd: Option<f64>,
    pub reserved_cost_usd: Option<f64>,
    pub rate: Option<ModelRate>,
    pub error: Option<String>,
}
impl UsageRecord {
    fn budget_tokens(&self) -> u64 {
        if self.usage.reported { self.usage.total_tokens }
        else { self.reserved_input_tokens.saturating_add(self.reserved_output_tokens) }
    }
    fn budget_cost(&self) -> Option<f64> {
        if self.usage.reported { self.estimated_cost_usd } else { self.reserved_cost_usd }
    }
}

#[derive(Clone)]
pub struct UsageService {
    database: Database,
    gate: Arc<Mutex<()>>,
}
impl UsageService {
    pub fn new(database: Database) -> anyhow::Result<Self> {
        // Never discard unsettled usage on restart or turn it into a zero-cost call.
        for mut record in database.list_usage_records()? {
            let mut changed = false;
            if ["running", "received", "validating"].contains(&record.status.as_str()) {
                record.status = "interrupted".to_owned();
                record.completed_at = Some(Utc::now());
                record.error = Some("Backend restarted before call processing finished; reported usage is retained, or the reservation remains when usage is unknown.".to_owned());
                changed = true;
            }
            // Upgrade older ledgers too. Only recognizable credential patterns
            // can be recovered without retaining/reading historical opaque keys.
            if let Some(error) = &record.error {
                let clean: String = redact_credentials(error, None).chars().take(2000).collect();
                if clean != *error { record.error = Some(clean); changed = true; }
            }
            if changed { database.put_usage_record(&record)?; }
        }
        Ok(Self { database, gate: Arc::new(Mutex::new(())) })
    }
    pub fn settings(&self) -> anyhow::Result<UsageSettings> { self.database.usage_settings() }
    pub fn save_settings(&self, mut settings: UsageSettings) -> anyhow::Result<UsageSettings> {
        for rate in &mut settings.rates { rate.model = rate.model.trim().to_owned(); }
        settings.validate()?;
        let _guard = self.gate.lock();
        // Fill unpriced historical rows, but never silently change a captured rate.
        for mut record in self.database.list_usage_records()? {
            if record.rate.is_none() {
                if let Some(rate) = settings.rates.iter().find(|r| r.provider == record.provider && r.model == record.model) {
                    record.rate = Some(rate.clone());
                    record.reserved_cost_usd = Some(reserved_cost(record.reserved_input_tokens, record.reserved_output_tokens, rate));
                    if record.usage.reported { record.estimated_cost_usd = Some(record.usage.estimate_cost(rate)); }
                    self.database.put_usage_record(&record)?;
                }
            }
        }
        self.database.put_usage_settings(&settings)?;
        Ok(settings)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn begin(&self, request_id: Uuid, project_id: Option<Uuid>, provider: ProviderKind,
        model: &str, purpose: &str, attempt: u32, input_bytes: usize, output_limit: u32) -> anyhow::Result<UsageRecord> {
        let _guard = self.gate.lock();
        let settings = self.settings()?;
        if settings.paused { bail!("Paid model calls are paused in Usage & cost"); }
        let records = self.database.list_usage_records()?;
        if records.iter().filter(|r| r.status == "running").count() >= settings.max_parallel_calls {
            return Err(ConcurrentCallLimit.into());
        }
        // Bytes + overhead deliberately reserves more than common English tokenization.
        // Not a tokenizer and not an enforceable provider-side billing cap.
        let input_reserved = (input_bytes as u64).saturating_add(2048);
        let output_reserved = u64::from(output_limit);
        let token_total = records.iter().fold(0u64, |sum, r| sum.saturating_add(r.budget_tokens()));
        let rate = settings.rates.iter().find(|r| r.provider == provider && r.model == model).cloned();
        let cost = rate.as_ref().map(|r| reserved_cost(input_reserved, output_reserved, r));
        if settings.enforce_limits {
            if let Some(limit) = settings.token_limit {
                if token_total.saturating_add(input_reserved).saturating_add(output_reserved) > limit {
                    bail!("Token admission limit would be exceeded by this call's reservation. Adjust controls in Usage & cost.");
                }
            }
            if let Some(limit) = settings.cost_limit_usd {
                let next = cost.context("A cost limit is enabled but this model has no rate card. Set prices in Usage & cost.")?;
                let mut used = 0.0;
                for row in &records {
                    used += row.budget_cost().context("A cost limit is enabled but historical calls are unpriced. Add their model rate cards before continuing.")?;
                }
                if used + next > limit { bail!("Estimated-cost admission limit would be exceeded. Adjust controls in Usage & cost."); }
            }
        }
        let record = UsageRecord {
            id: Uuid::new_v4(), request_id, project_id, provider, model: model.to_owned(), purpose: purpose.to_owned(),
            attempt, status: "running".to_owned(), started_at: Utc::now(), completed_at: None,
            provider_response_id: None, usage: TokenUsage::default(), reserved_input_tokens: input_reserved,
            reserved_output_tokens: output_reserved, estimated_cost_usd: None, reserved_cost_usd: cost,
            rate, error: None,
        };
        self.database.put_usage_record(&record)?;
        Ok(record)
    }
    pub fn finish(&self, id: Uuid, status: &str, body: Option<&Value>, error: Option<&str>) -> anyhow::Result<()> {
        let _guard = self.gate.lock();
        let mut row = self.database.get_usage_record(id)?.context("usage record not found")?;
        row.status = status.to_owned();
        row.completed_at = Some(Utc::now());
        row.error = error.map(|s| redact_credentials(s, None).chars().take(2000).collect());
        if let Some(body) = body {
            row.usage = TokenUsage::from_response(row.provider, body);
            row.provider_response_id = body.get("id").and_then(Value::as_str).map(str::to_owned);
            if row.usage.reported {
                row.estimated_cost_usd = row.rate.as_ref().map(|rate| row.usage.estimate_cost(rate));
            }
        }
        self.database.put_usage_record(&row)
    }
    pub fn report(&self) -> anyhow::Result<Value> {
        let _guard = self.gate.lock();
        let settings = self.settings()?;
        let mut records = self.database.list_usage_records()?;
        records.sort_by(|a,b| b.started_at.cmp(&a.started_at));
        let mut input=0u64; let mut output=0u64; let mut cached=0u64; let mut reasoning=0u64;
        let mut budget_tokens=0u64; let mut known_cost=0.0; let mut budget_cost=0.0;
        let mut missing_usage=0usize; let mut unpriced=0usize;
        let mut models: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
        for r in &records {
            input=input.saturating_add(r.usage.input_tokens); output=output.saturating_add(r.usage.output_tokens);
            cached=cached.saturating_add(r.usage.cached_input_tokens); reasoning=reasoning.saturating_add(r.usage.reasoning_tokens);
            budget_tokens=budget_tokens.saturating_add(r.budget_tokens());
            known_cost+=r.estimated_cost_usd.unwrap_or(0.0);
            budget_cost+=r.budget_cost().unwrap_or(0.0);
            if !r.usage.reported { missing_usage+=1; }
            if r.rate.is_none() { unpriced+=1; }
            let key=format!("{}:{}",r.provider.account_name(),r.model);
            let entry=models.entry(key).or_insert_with(|| json!({"provider":r.provider,"model":r.model,"calls":0,"tokens":0,"estimated_cost_usd":0.0,"unpriced":0}));
            entry["calls"]=json!(entry["calls"].as_u64().unwrap_or(0)+1);
            entry["tokens"]=json!(entry["tokens"].as_u64().unwrap_or(0).saturating_add(r.usage.total_tokens));
            entry["estimated_cost_usd"]=json!(entry["estimated_cost_usd"].as_f64().unwrap_or(0.0)+r.estimated_cost_usd.unwrap_or(0.0));
            if r.rate.is_none() { entry["unpriced"]=json!(entry["unpriced"].as_u64().unwrap_or(0)+1); }
        }
        let active=records.iter().filter(|r| r.status=="running").cloned().collect::<Vec<_>>();
        let mut warnings=Vec::new();
        if settings.token_limit.is_some_and(|limit| budget_tokens as f64 >= limit as f64 * settings.warning_percent as f64 / 100.0) {
            warnings.push("Token budget warning threshold reached.");
        }
        if settings.cost_limit_usd.is_some_and(|limit| budget_cost >= limit * settings.warning_percent as f64 / 100.0) {
            warnings.push("Estimated-cost budget warning threshold reached.");
        }
        if unpriced>0 { warnings.push("Some calls have no rate card; displayed USD totals are incomplete, not zero-cost usage."); }
        if missing_usage>0 { warnings.push("Some calls have no final provider usage. Reservations remain counted against budgets."); }
        let count=records.len(); records.truncate(500);
        Ok(json!({"settings":settings,"active_calls":active,"models":models.into_values().collect::<Vec<_>>(),"records":records,
            "totals":{"calls":count,"input_tokens":input,"output_tokens":output,"cached_input_tokens":cached,"reasoning_tokens":reasoning,
                "total_tokens":input.saturating_add(output),"budget_accounted_tokens":budget_tokens,"estimated_cost_usd":known_cost,
                "budget_accounted_cost_usd":budget_cost,"unpriced_calls":unpriced,"unreported_calls":missing_usage},
            "warnings":warnings,"scope":"All calls recorded by this local PhaseForge database since v0.3.1. Provider billing is authoritative.",
            "ledger_limit":500}))
    }
}
fn reserved_cost(input: u64, output: u64, rate: &ModelRate) -> f64 {
    (input as f64 * rate.maximum_input_rate() + output as f64 * rate.output_per_million) / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn provider_security_historical_errors_are_scrubbed_without_changing_usage() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let service = UsageService::new(db.clone()).unwrap();
        let row = service.begin(Uuid::new_v4(), None, ProviderKind::OpenAi, "fixture-model", "fixture", 0, 10, 32).unwrap();
        service.finish(row.id, "completed", Some(&json!({"id":"resp_fixture","usage":{"input_tokens":9,"output_tokens":4}})), None).unwrap();
        let mut historical = db.get_usage_record(row.id).unwrap().unwrap();
        let key = format!("sk-{}", "synthetic_historical_fixture_123456789");
        historical.error = Some(format!("Invalid {key}; Bearer historic-fixture-token; diagnostic retained"));
        db.put_usage_record(&historical).unwrap();
        let expected_error = "Invalid [REDACTED]; Bearer [REDACTED]; diagnostic retained";
        let mut expected = serde_json::to_value(&historical).unwrap();
        expected["error"] = json!(expected_error);
        let restarted = UsageService::new(db.clone()).unwrap();
        assert_eq!(serde_json::to_value(db.get_usage_record(row.id).unwrap().unwrap()).unwrap(), expected);
        let report = restarted.report().unwrap().to_string();
        assert!(!report.contains(&key));
        assert!(!report.contains("historic-fixture-token"));
        assert!(report.contains("diagnostic retained"));
        let _again = UsageService::new(db.clone()).unwrap();
        assert_eq!(serde_json::to_value(db.get_usage_record(row.id).unwrap().unwrap()).unwrap(), expected);
    }

    #[test]
    fn provider_security_usage_retention_scrubs_keys_before_truncation() {
        let db = Database::open(std::path::Path::new(":memory:")).unwrap();
        let service = UsageService::new(db.clone()).unwrap();
        let row = service.begin(Uuid::new_v4(), None, ProviderKind::OpenAi, "fixture-model", "fixture", 0, 10, 512).unwrap();
        let key = format!("sk-{}", "synthetic_fixture_only_123456789");
        let message = format!("{} denied {key} and Bearer fixture-token", "x".repeat(1975));
        service.finish(row.id, "provider_error", None, Some(&message)).unwrap();
        let stored = db.get_usage_record(row.id).unwrap().unwrap().error.unwrap();
        assert!(!stored.contains("sk-"));
        assert!(!stored.contains("fixture-token"));
        assert!(stored.contains("[REDACTED]"));
        assert!(!service.report().unwrap().to_string().contains(&key));
    }

    #[test]
    fn openai_cached_and_reasoning_are_not_double_counted() {
        let u=TokenUsage::from_response(ProviderKind::OpenAi, &json!({"usage":{"input_tokens":100,"output_tokens":30,
            "input_tokens_details":{"cached_tokens":60},"output_tokens_details":{"reasoning_tokens":20}}}));
        assert_eq!(u.total_tokens,130); assert_eq!(u.reasoning_tokens,20); assert_eq!(u.cached_input_tokens,60);
    }
    #[test]
    fn anthropic_cache_counts_are_added_to_input() {
        let u=TokenUsage::from_response(ProviderKind::Anthropic,&json!({"usage":{"input_tokens":20,"cache_read_input_tokens":60,
            "cache_creation_input_tokens":15,"cache_creation":{"ephemeral_1h_input_tokens":5},"output_tokens":10}}));
        assert_eq!(u.input_tokens,95); assert_eq!(u.total_tokens,105); assert_eq!(u.cache_write_5m_tokens,10);
    }
    #[test]
    fn missing_usage_is_unknown_not_free() { assert!(!TokenUsage::from_response(ProviderKind::OpenAi,&json!({})).reported); }
    #[test]
    fn concurrent_admission_and_restart_reserve_unknown_calls() {
        let temp=tempfile::tempdir().unwrap(); let db=Database::open(&temp.path().join("test.db")).unwrap();
        let svc=UsageService::new(db.clone()).unwrap();
        let row=svc.begin(Uuid::new_v4(),None,ProviderKind::OpenAi,"mock","proposal",0,100,512).unwrap();
        assert!(svc.begin(Uuid::new_v4(),None,ProviderKind::OpenAi,"mock","proposal",0,100,512).is_err());
        svc.finish(row.id,"cancelled",None,Some("cancelled")).unwrap();
        assert_eq!(svc.report().unwrap()["totals"]["budget_accounted_tokens"],json!(2660));
        let mut settings=svc.settings().unwrap(); settings.paused=true; svc.save_settings(settings).unwrap();
        assert!(svc.begin(Uuid::new_v4(),None,ProviderKind::OpenAi,"mock","proposal",0,100,512).is_err());
    }
}
