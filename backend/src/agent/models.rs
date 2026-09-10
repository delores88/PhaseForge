//! Capability ordering is a UI recommendation, never a claim of account access.
//! Models are always obtained from the provider's authenticated catalog.
use crate::domain::{ProviderKind, ProviderModel};

pub(super) fn reasoning_efforts(provider: ProviderKind, model: &str) -> Vec<String> {
    let model = model.to_ascii_lowercase();
    let levels: &[&str] = match provider {
        ProviderKind::OpenAi if model.starts_with("gpt-6-astra") => &["low", "medium", "high", "xhigh", "max"],
        ProviderKind::OpenAi if model.starts_with("gpt-5.6") => &["none", "low", "medium", "high", "xhigh", "max"],
        ProviderKind::OpenAi if model.starts_with("gpt-5-pro") || model.starts_with("o3-pro") => &["high"],
        ProviderKind::OpenAi if model.contains("-pro") && ["gpt-5.2", "gpt-5.4", "gpt-5.5"].iter().any(|p| model.starts_with(p)) => &["medium", "high", "xhigh"],
        ProviderKind::OpenAi if ["gpt-5.2", "gpt-5.3", "gpt-5.4", "gpt-5.5"].iter().any(|p| model.starts_with(p)) => &["none", "low", "medium", "high", "xhigh"],
        ProviderKind::OpenAi if model.starts_with("gpt-5.1") => &["none", "low", "medium", "high"],
        ProviderKind::OpenAi if model.starts_with("gpt-5") || model.starts_with("o3") || model.starts_with("o4") => &["low", "medium", "high"],
        ProviderKind::Anthropic if ["fable-5", "mythos-5", "opus-5", "opus-4-8", "opus-4-7", "sonnet-5"].iter().any(|p| model.contains(p)) => &["low", "medium", "high", "xhigh", "max"],
        ProviderKind::Anthropic if ["mythos-preview", "opus-4-6", "sonnet-4-6"].iter().any(|p| model.contains(p)) => &["low", "medium", "high", "max"],
        ProviderKind::Anthropic if model.contains("opus-4-5") => &["low", "medium", "high"],
        _ => &[],
    };
    levels.iter().map(|level| (*level).to_owned()).collect()
}

pub(super) fn capability_rank(provider: ProviderKind, id: &str) -> u32 {
    let id = id.to_ascii_lowercase();
    let bases: &[(&str, u32)] = match provider {
        ProviderKind::OpenAi => &[("gpt-6-astra",1000),("gpt-5.6-sol",950),("gpt-5.6-terra",900),("gpt-5.6-luna",750),
            ("gpt-5.6",950),("gpt-5.5-pro",920),("gpt-5.5",880),("gpt-5.4-pro",850),("gpt-5.4",800),
            ("gpt-5.3",770),("gpt-5.2-pro",760),("gpt-5.2",740),("gpt-5.1",700),("gpt-5-pro",720),("gpt-5",650),
            ("o3-pro",640),("o3",600),("o4",500),("gpt-4.1",400),("gpt-4o",350)],
        ProviderKind::Anthropic => &[("claude-mythos-5-1",1000),("claude-fable-5-1",990),("claude-mythos-5",980),
            ("claude-fable-5",970),("claude-opus-5",950),("claude-opus-4-8",920),("claude-opus-4-7",900),
            ("claude-opus-4-6",880),("claude-sonnet-5",850),("claude-opus-4-5",800),("claude-sonnet-4-6",780),
            ("claude-opus",720),("claude-sonnet",650),("claude-haiku",300)],
    };
    let rank = bases.iter().find(|(prefix, _)| id.starts_with(prefix)).map(|(_, rank)| *rank).unwrap_or(100);
    let reduction = if id.contains("nano") {450} else if id.contains("mini") {300} else if id.contains("spark") {200} else {0};
    rank.saturating_sub(reduction).max(1)
}

pub(super) fn decorate_model(provider: ProviderKind, model: &mut ProviderModel) {
    model.recommended = false;
    model.capability_rank = capability_rank(provider, &model.id);
    model.reasoning_efforts = reasoning_efforts(provider, &model.id);
    model.recommended_reasoning = model.reasoning_efforts.last().cloned();
    model.description = if model.capability_rank >= 900 {
        "Recommended for demanding simulation design, mathematical reasoning, and scientific review"
    } else if model.capability_rank >= 650 {
        "Capable research and experiment design with a balance of response time and depth"
    } else {
        "Useful for short questions, summaries, and focused supporting tasks"
    }.to_owned();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn ranks_frontier_above_fast_variants() {
        assert!(capability_rank(ProviderKind::OpenAi,"gpt-6-astra") > capability_rank(ProviderKind::OpenAi,"gpt-5.6-sol"));
        assert!(capability_rank(ProviderKind::OpenAi,"gpt-5.4") > capability_rank(ProviderKind::OpenAi,"gpt-5.4-mini"));
        assert!(capability_rank(ProviderKind::Anthropic,"claude-opus-5") > capability_rank(ProviderKind::Anthropic,"claude-haiku-4-5"));
    }
    #[test] fn efforts_are_provider_and_model_specific() {
        assert!(reasoning_efforts(ProviderKind::OpenAi,"gpt-6-astra").contains(&"max".into()));
        assert!(!reasoning_efforts(ProviderKind::OpenAi,"gpt-6-astra").contains(&"ultra".into()));
        assert!(reasoning_efforts(ProviderKind::OpenAi,"gpt-4.1").is_empty());
        assert!(!reasoning_efforts(ProviderKind::Anthropic,"claude-sonnet-4-6").contains(&"xhigh".into()));
    }
}
