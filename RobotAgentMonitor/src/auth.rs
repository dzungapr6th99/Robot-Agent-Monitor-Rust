use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use axum::http::{HeaderMap, header};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Clone, Deserialize)]
pub(crate) struct Claims {
    exp: u64,
    pub(crate) sub: Option<String>,
    #[serde(default)]
    permission: Permission,
}

#[derive(Clone, Default, Deserialize)]
#[serde(untagged)]
enum Permission {
    #[default]
    Empty,
    Single(String),
    Multiple(Vec<String>),
}

impl Claims {
    pub fn allows(&self, claim: &str) -> bool {
        match &self.permission {
            Permission::Empty => false,
            Permission::Single(value) => value == claim,
            Permission::Multiple(values) => values.iter().any(|value| value == claim),
        }
    }

    pub fn expired(&self) -> bool {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            >= self.exp
    }
}

pub struct TokenVerifier {
    client: reqwest::Client,
    issuer: String,
    jwks_url: reqwest::Url,
    audience: String,
    keys: Mutex<Option<(Instant, JwkSet)>>,
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    jwks_uri: String,
}

impl TokenVerifier {
    pub async fn discover(issuer: String, audience: String) -> Result<Self> {
        let issuer_url = reqwest::Url::parse(&issuer)?;
        ensure!(
            issuer_url.scheme() == "https"
                && issuer_url.query().is_none()
                && issuer_url.fragment().is_none(),
            "OIDC issuer must be an HTTPS URL"
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let metadata: Discovery = client
            .get(format!(
                "{}/.well-known/openid-configuration",
                issuer.trim_end_matches('/')
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let jwks_url = reqwest::Url::parse(&metadata.jwks_uri)?;
        ensure!(
            metadata.issuer == issuer && jwks_url.scheme() == "https",
            "Invalid OIDC metadata"
        );
        Ok(Self {
            client,
            issuer,
            jwks_url,
            audience,
            keys: Mutex::new(None),
        })
    }

    pub(crate) async fn verify(&self, token: &str) -> Result<Claims> {
        ensure!(token.len() <= 8192, "Token too large");
        let header = decode_header(token)?;
        ensure!(
            matches!(
                header.alg,
                Algorithm::RS256
                    | Algorithm::RS384
                    | Algorithm::RS512
                    | Algorithm::ES256
                    | Algorithm::ES384
                    | Algorithm::PS256
                    | Algorithm::PS384
                    | Algorithm::PS512
            ),
            "Unsupported JWT algorithm"
        );
        let kid = header.kid.as_deref().context("JWT key ID missing")?;
        let mut cache = self.keys.lock().await;
        if cache
            .as_ref()
            .is_none_or(|(updated, _)| updated.elapsed() >= Duration::from_secs(300))
        {
            let jwks = self
                .client
                .get(self.jwks_url.clone())
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            *cache = Some((Instant::now(), jwks));
        }
        let key = cache
            .as_ref()
            .and_then(|(_, keys)| keys.find(kid))
            .context("JWT key not found")?;
        let decoding_key = DecodingKey::from_jwk(key)?;
        drop(cache);
        let mut validation = Validation::new(header.alg);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.audience]);
        validation.set_required_spec_claims(&["exp", "iss", "aud"]);
        validation.validate_nbf = true;
        validation.leeway = 0;
        Ok(decode::<Claims>(token, &decoding_key, &validation)?.claims)
    }
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}
