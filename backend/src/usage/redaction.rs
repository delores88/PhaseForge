//! Defense in depth for persisted/provider diagnostics. Exact active keys are
//! scrubbed by the provider boundary; these patterns also cover other errors.
pub(crate) fn redact_credentials(text: &str, active_key: Option<&str>) -> String {
    let text = match active_key.filter(|key| !key.is_empty()) {
        Some(key) => text.replace(key, "[REDACTED]"),
        None => text.to_owned(),
    };
    let bytes = text.as_bytes();
    let mut output = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        let boundary = at == 0 || !bytes[at - 1].is_ascii_alphanumeric();
        if boundary && bytes[at..].starts_with(b"sk-") {
            let end = token_end(bytes, at);
            if end - at >= 11 {
                output.push_str("[REDACTED]");
                at = end;
                continue;
            }
        }
        if boundary {
            for label in [b"bearer".as_slice(), b"x-api-key".as_slice(), b"api_key".as_slice()] {
                let label_end = at + label.len();
                if !bytes.get(at..label_end).is_some_and(|value| value.eq_ignore_ascii_case(label)) { continue; }
                let mut start = label_end;
                while bytes.get(start).is_some_and(|ch| ch.is_ascii_whitespace() || matches!(ch, b':' | b'=' | b'\'' | b'"')) { start += 1; }
                // Require a separator so identifiers such as "bearer_count"
                // remain useful diagnostic information.
                if start == label_end { continue; }
                let end = token_end(bytes, start);
                if end > start {
                    output.push_str(&text[at..start]);
                    output.push_str("[REDACTED]");
                    at = end;
                    break;
                }
            }
            if at == bytes.len() { break; }
        }
        let ch = text[at..].chars().next().expect("valid character boundary");
        output.push(ch);
        at += ch.len_utf8();
    }
    output
}

fn token_end(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at).is_some_and(|ch| ch.is_ascii_alphanumeric() || matches!(ch, b'-' | b'_' | b'.' | b'~' | b'+' | b'/' | b'=')) { at += 1; }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_security_redaction_handles_unicode_labels_and_exact_nonstandard_keys() {
        let key = "fixture secret with spaces";
        let input = format!("測定 {key}; BEARER\tfixture-token; {{\"x-api-key\":\"gateway-key\"}}; api_key=another-key; ordinary diagnostic");
        let output = redact_credentials(&input, Some(key));
        for secret in [key, "fixture-token", "gateway-key", "another-key"] { assert!(!output.contains(secret)); }
        assert!(output.starts_with("測定 [REDACTED]"));
        assert!(output.ends_with("ordinary diagnostic"));
        assert_eq!(redact_credentials("bearer_count=3; request timed out", None), "bearer_count=3; request timed out");
    }

    #[test]
    fn provider_security_redaction_covers_both_provider_key_formats() {
        for prefix in ["sk-", "sk-proj-", "sk-ant-api03-"] {
            let key = format!("{prefix}synthetic_fixture_only_123456789");
            assert_eq!(redact_credentials(&format!("Invalid key: {key}"), None), "Invalid key: [REDACTED]");
        }
        assert_eq!(redact_credentials("No key supplied", Some("")), "No key supplied");
    }
}
