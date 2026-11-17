use crate::traits::VlaClientTrait;
use anyhow::{Result, ensure};
use robot_common_lib::vla::{VlaError, VlaInference, VlaJoints, VlaOptions};
use std::time::Duration;

pub struct VlaClient {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    access_token: String,
}

impl VlaClient {
    pub fn new(options: &VlaOptions) -> Result<Self> {
        robot_common_lib::logged_result!("new_vla_client", {
            let url = reqwest::Url::parse(&options.base_url)
                .map_err(|_| anyhow::anyhow!("Invalid VLA URL"))?;
            ensure!(
                url.scheme() == "https"
                    || (url.scheme() == "http"
                        && matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "localhost"))),
                "VLA requires HTTPS or loopback HTTP"
            );
            ensure!(
                url.username().is_empty()
                    && url.password().is_none()
                    && url.query().is_none()
                    && url.fragment().is_none()
                    && url.path().ends_with('/'),
                "Invalid VLA base URL"
            );
            ensure!(options.timeout_ms > 0, "VLA timeout must be positive");
            Ok(Self {
                client: reqwest::Client::builder()
                    .timeout(Duration::from_millis(options.timeout_ms))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()?,
                endpoint: url.join("infer")?,
                access_token: options.access_token.clone(),
            })
        })
    }
}

impl VlaClientTrait for VlaClient {
    async fn infer(
        &self,
        profile_id: &str,
        task: &str,
        observation: &VlaJoints,
    ) -> Result<VlaInference> {
        robot_common_lib::logged_result!(async "vla_infer", {
            robot_common_lib::operation_log::info(module_path!(), format_args!("Requesting VLA inference"));
            let mut request = self.client.post(self.endpoint.clone()).json(&serde_json::json!({
                "profileId": profile_id, "task": task, "observation": observation
            }));
            if !self.access_token.is_empty() {
                request = request.bearer_auth(&self.access_token);
            }
            // Do not propagate remote bodies, URLs or request payloads into logs.
            let response = request.send().await.map_err(|error| {
                if error.is_timeout() { VlaError::Timeout } else { VlaError::Unavailable }
            })?;
            ensure!(response.status().is_success(), VlaError::Unavailable);
            let inference: VlaInference = response.json().await.map_err(|_| VlaError::Unavailable)?;
            for action in &inference.actions {
                action.validate().map_err(|_| VlaError::Unavailable)?;
            }
            Ok(inference)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unsafe_or_ambiguous_urls() {
        for url in [
            "http://robot/",
            "file:///tmp/",
            "https://example.com/base",
            "https://example.com/?q=1",
            "https://user@example.com/",
        ] {
            assert!(
                VlaClient::new(&VlaOptions {
                    base_url: url.into(),
                    ..Default::default()
                })
                .is_err()
            );
        }
        for url in [
            "http://127.0.0.1:8091/",
            "http://[::1]:8091/",
            "https://example.com/vla/",
        ] {
            assert!(
                VlaClient::new(&VlaOptions {
                    base_url: url.into(),
                    ..Default::default()
                })
                .is_ok()
            );
        }
    }
}
