use robot_common_lib::integrations::MediaClientOptions;
use std::time::Duration;

use anyhow::{Result, ensure};
use serde_json::Value;

pub struct MediaClient {
    client: reqwest::Client,
    base_url: reqwest::Url,
    token: String,
}

impl MediaClient {
    pub fn new(options: MediaClientOptions) -> Result<Self> {
        let base_url = reqwest::Url::parse(&options.base_url)?;
        ensure!(
            base_url.scheme() == "https"
                || (base_url.scheme() == "http"
                    && matches!(
                        base_url.host_str(),
                        Some("127.0.0.1" | "[::1]" | "localhost")
                    )),
            "Media RPC requires HTTPS or loopback HTTP"
        );
        ensure!(
            base_url.path().ends_with('/'),
            "Media RPC URL must end in /"
        );
        ensure!(
            options.timeout_milliseconds > 0,
            "Media timeout must be positive"
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(options.timeout_milliseconds))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            client,
            base_url,
            token: options.access_token,
        })
    }
}

impl MediaClient {
    pub async fn call(&self, operation: &'static str, payload: Value) -> Result<Value> {
        ensure!(self.token.len() >= 32, "Media RPC token is not configured");
        Ok(self
            .client
            .post(self.base_url.join(&format!("rpc/{operation}"))?)
            .bearer_auth(&self.token)
            .json(&payload)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }
}
