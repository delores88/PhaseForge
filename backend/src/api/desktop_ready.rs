//! Per-launch desktop readiness proof. The secret stays in the child environment
//! and memory; clients transmit a fresh public nonce, never the secret itself.
use anyhow::bail;
use axum::{
    extract::Query,
    http::{header::CACHE_CONTROL, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;

#[derive(Clone)]
pub(crate) struct Readiness {
    secret: Option<[u8; 32]>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    nonce: String,
}
impl Readiness {
    pub(crate) fn from_secret(value: Option<&str>) -> anyhow::Result<Self> {
        let secret = if let Some(value) = value {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("PHASEFORGE_DESKTOP_SECRET must be a 32-byte hexadecimal launch secret");
            }
            let mut secret = [0_u8; 32];
            for (index, byte) in secret.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
                    .expect("validated hexadecimal secret");
            }
            Some(secret)
        } else {
            None
        };
        Ok(Self { secret })
    }
    pub(crate) fn from_environment() -> anyhow::Result<Self> {
        let value = std::env::var("PHASEFORGE_DESKTOP_SECRET")
            .map(Some)
            .or_else(|error| match error {
                std::env::VarError::NotPresent => Ok(None),
                _ => Err(anyhow::anyhow!("Desktop launch secret must be UTF-8 hexadecimal")),
            })?;
        Self::from_secret(value.as_deref())
    }
    pub(crate) fn enabled(&self) -> bool { self.secret.is_some() }
    fn respond(&self, nonce: &str) -> Response {
        let Some(secret) = self.secret else {
            return (
                StatusCode::NOT_FOUND,
                Json(
                    json!({"error":{"message":"Desktop readiness is not enabled for this launch"}}),
                ),
            )
                .into_response();
        };
        if nonce.len() != 64 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return (StatusCode::BAD_REQUEST,Json(json!({"error":{"message":"A 64-character hexadecimal readiness nonce is required"}}))).into_response();
        }
        let mut mac = Hmac::<Sha256>::new_from_slice(&secret).expect("HMAC accepts a 32-byte key");
        mac.update(nonce.as_bytes());
        let proof = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        ([(CACHE_CONTROL,"no-store")],Json(json!({"algorithm":"HMAC-SHA256","nonce":nonce,"proof":proof,"version":env!("CARGO_PKG_VERSION")}))).into_response()
    }
    pub(crate) fn router<S: Clone + Send + Sync + 'static>(self) -> Router<S> {
        Router::new().route(
            "/api/desktop/ready",
            get(move |Query(challenge): Query<Challenge>| {
                let state = self.clone();
                async move { state.respond(&challenge.nonce) }
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn proof_is_bounded_nonce_specific_and_never_echoes_the_secret() {
        let state = Readiness::from_secret(Some(&"00".repeat(32))).unwrap();
        let response = state.respond(&"1".repeat(64));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        let value: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(value["algorithm"], "HMAC-SHA256");
        assert_eq!(value["nonce"], "1".repeat(64));
        assert!(value.get("secret").is_none());
        let second = state.respond(&"2".repeat(64));
        let second: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(second.into_body(), 1024)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_ne!(value["proof"], second["proof"]);
        assert_eq!(value["proof"].as_str().unwrap().len(), 64);
        // Independently evaluated with Python's hmac/hashlib, zero key and ASCII nonce.
        assert_eq!(
            value["proof"],
            "08a55ef15743f3ba349804ebaebdf8f923dbfcf66d49ae2e50851b268cec94e2"
        );
        for nonce in ["", "short", &"g".repeat(64), &"1".repeat(65)] {
            assert_eq!(state.respond(nonce).status(), StatusCode::BAD_REQUEST);
        }
        assert_eq!(
            Readiness::from_secret(None)
                .unwrap()
                .respond(&"1".repeat(64))
                .status(),
            StatusCode::NOT_FOUND
        );
        for secret in ["", "short", &"g".repeat(64)] {
            assert!(Readiness::from_secret(Some(secret)).is_err());
        }
    }
}
