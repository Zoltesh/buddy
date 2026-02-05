use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::config::FetchUrlConfig;

use super::{Skill, SkillError};

/// Skill that fetches URLs via HTTP GET, restricted to allowlisted domains.
pub struct FetchUrlSkill {
    allowed_domains: Vec<String>,
    client: reqwest::Client,
}

impl FetchUrlSkill {
    pub fn new(config: &FetchUrlConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client");

        Self {
            allowed_domains: config.allowed_domains.clone(),
            client,
        }
    }
}

/// Validate that a URL's domain is in the allowlist.
fn validate_domain(url_str: &str, allowed_domains: &[String]) -> Result<(), SkillError> {
    let parsed = url::Url::parse(url_str)
        .map_err(|e| SkillError::InvalidInput(format!("invalid URL: {e}")))?;

    let domain = parsed
        .host_str()
        .ok_or_else(|| SkillError::InvalidInput("URL has no host".into()))?;

    if allowed_domains.iter().any(|d| d == domain) {
        Ok(())
    } else {
        Err(SkillError::Forbidden(format!(
            "domain '{domain}' is not in the allowlist"
        )))
    }
}

impl Skill for FetchUrlSkill {
    fn name(&self) -> &str {
        "fetch_url"
    }

    fn description(&self) -> &str {
        "Fetch the contents of a URL via HTTP GET"
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" }
            },
            "required": ["url"]
        })
    }

    fn execute(
        &self,
        input: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, SkillError>> + Send + '_>> {
        Box::pin(async move {
            let url = input
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| SkillError::InvalidInput("missing required field: url".into()))?;

            validate_domain(url, &self.allowed_domains)?;

            let response = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| SkillError::ExecutionFailed(format!("HTTP request failed: {e}")))?;

            let status = response.status().as_u16();
            let body = response
                .text()
                .await
                .map_err(|e| SkillError::ExecutionFailed(format!("failed to read response: {e}")))?;

            Ok(serde_json::json!({
                "status": status,
                "body": body
            }))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_domain_allowlisted() {
        let domains = vec!["example.com".into(), "api.github.com".into()];
        assert!(validate_domain("https://example.com/page", &domains).is_ok());
        assert!(validate_domain("https://api.github.com/repos", &domains).is_ok());
    }

    #[test]
    fn validate_domain_not_allowlisted() {
        let domains = vec!["example.com".into()];
        let result = validate_domain("https://evil.com/steal", &domains);
        match result {
            Err(SkillError::Forbidden(msg)) => {
                assert!(msg.contains("evil.com"));
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[test]
    fn validate_domain_invalid_url() {
        let domains = vec!["example.com".into()];
        let result = validate_domain("not-a-url", &domains);
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[tokio::test]
    async fn fetch_url_rejects_non_allowlisted_domain() {
        let config = FetchUrlConfig {
            allowed_domains: vec!["example.com".into()],
        };
        let skill = FetchUrlSkill::new(&config);
        let result = skill
            .execute(serde_json::json!({ "url": "https://evil.com/" }))
            .await;

        match result {
            Err(SkillError::Forbidden(_)) => {}
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    /// This test requires network access — run with `cargo test -- --ignored` to include it.
    #[tokio::test]
    #[ignore]
    async fn fetch_url_with_allowlisted_domain() {
        let config = FetchUrlConfig {
            allowed_domains: vec!["example.com".into()],
        };
        let skill = FetchUrlSkill::new(&config);
        let result = skill
            .execute(serde_json::json!({ "url": "https://example.com/" }))
            .await
            .unwrap();

        assert_eq!(result["status"], 200);
        assert!(!result["body"].as_str().unwrap().is_empty());
    }
}
