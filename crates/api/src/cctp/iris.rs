//! Circle Iris v2 HTTP client for CCTP attestation polling.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::redirect::Policy;
use serde::Deserialize;
use thiserror::Error;

use crate::cctp::config::{redact_url, CctpConfig};

const USER_AGENT: &str = "stellarroute-api/cctp-core/1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrisFeeQuote {
    pub standard_fee: Option<String>,
    pub fast_fee: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrisMessage {
    pub message_hex: String,
    pub attestation_hex: Option<String>,
    pub cctp_version: u32,
    pub status: IrisMessageStatus,
    pub event_nonce: String,
    pub source_tx_hash: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrisMessageStatus {
    Pending,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IrisPollOutcome {
    Pending,
    Complete(IrisMessage),
    RateLimited { retry_after_secs: u64 },
    NotFound,
}

#[derive(Debug, Error)]
pub enum IrisError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("timeout")]
    Timeout,
    #[error("malformed response: {0}")]
    Malformed(String),
    #[error("redirect blocked")]
    RedirectBlocked,
    #[error("host not allowlisted")]
    HostNotAllowlisted,
}

#[async_trait]
pub trait IrisClient: Send + Sync {
    async fn fetch_burn_fees(
        &self,
        source_domain: u32,
        dest_domain: u32,
    ) -> Result<IrisFeeQuote, IrisError>;

    async fn poll_messages_by_tx(
        &self,
        source_domain: u32,
        tx_hash: &str,
    ) -> Result<IrisPollOutcome, IrisError>;

    async fn reattest(&self, nonce: &str) -> Result<(), IrisError>;
}

pub struct ReqwestIrisClient {
    client: reqwest::Client,
    base_url: String,
    allowed_host: String,
    max_retries: u32,
}

impl ReqwestIrisClient {
    pub fn from_config(config: &CctpConfig) -> Result<Self, IrisError> {
        let base_url = config.iris_base_url.trim_end_matches('/').to_string();
        let allowed_host = base_url
            .strip_prefix("https://")
            .or_else(|| base_url.strip_prefix("http://"))
            .and_then(|rest| rest.split('/').next())
            .unwrap_or("")
            .to_string();

        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .redirect(Policy::none())
            .timeout(Duration::from_secs(config.iris_timeout_secs))
            .build()
            .map_err(|e| IrisError::Http(e.to_string()))?;

        Ok(Self {
            client,
            base_url,
            allowed_host,
            max_retries: config.iris_max_retries,
        })
    }

    fn ensure_host(&self, url: &str) -> Result<(), IrisError> {
        if !url.contains(&self.allowed_host) {
            return Err(IrisError::HostNotAllowlisted);
        }
        Ok(())
    }

    async fn get_with_retries(&self, url: &str) -> Result<reqwest::Response, IrisError> {
        self.ensure_host(url)?;
        let mut attempt = 0;
        loop {
            let response = self.client.get(url).send().await;
            match response {
                Ok(resp) => return Ok(resp),
                Err(e) if e.is_timeout() => return Err(IrisError::Timeout),
                Err(e) if e.is_redirect() => return Err(IrisError::RedirectBlocked),
                Err(_e) if attempt < self.max_retries => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(50 + attempt as u64 * 25)).await;
                    continue;
                }
                Err(e) => return Err(IrisError::Http(redact_url(&e.to_string()))),
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct FeesResponse {
    #[serde(default)]
    standard_fee: Option<String>,
    #[serde(default)]
    fast_fee: Option<String>,
    #[serde(default)]
    minimum_fee: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessagesResponse {
    messages: Vec<MessageV2>,
    #[serde(default, rename = "sourceTxHash")]
    source_tx_hash: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct MessageV2 {
    message: String,
    attestation: Option<String>,
    #[serde(rename = "cctpVersion")]
    cctp_version: Option<u32>,
    status: Option<String>,
    #[serde(rename = "eventNonce")]
    event_nonce: Option<String>,
}

#[async_trait]
impl IrisClient for ReqwestIrisClient {
    async fn fetch_burn_fees(
        &self,
        source_domain: u32,
        dest_domain: u32,
    ) -> Result<IrisFeeQuote, IrisError> {
        let url = format!(
            "{}/v2/burn/USDC/fees/{}/{}",
            self.base_url, source_domain, dest_domain
        );
        let resp = self.get_with_retries(&url).await?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(IrisError::Http("rate limited".into()));
        }
        if !resp.status().is_success() {
            return Err(IrisError::Http(format!("status {}", resp.status())));
        }
        let body: FeesResponse = resp
            .json()
            .await
            .map_err(|e| IrisError::Malformed(e.to_string()))?;
        Ok(IrisFeeQuote {
            standard_fee: body.standard_fee.or(body.minimum_fee),
            fast_fee: body.fast_fee,
        })
    }

    async fn poll_messages_by_tx(
        &self,
        source_domain: u32,
        tx_hash: &str,
    ) -> Result<IrisPollOutcome, IrisError> {
        let url = format!(
            "{}/v2/messages/{}?transactionHash={}",
            self.base_url,
            source_domain,
            urlencoding::encode(tx_hash)
        );
        let resp = self.get_with_retries(&url).await?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(IrisPollOutcome::NotFound);
        }
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            return Ok(IrisPollOutcome::RateLimited {
                retry_after_secs: retry.max(60),
            });
        }
        if !resp.status().is_success() {
            return Err(IrisError::Http(format!("status {}", resp.status())));
        }

        let body: MessagesResponse = resp
            .json()
            .await
            .map_err(|e| IrisError::Malformed(e.to_string()))?;

        if body.messages.is_empty() {
            return Ok(IrisPollOutcome::Pending);
        }

        let msg = body.messages[0].clone();
        let cctp_version = msg.cctp_version.unwrap_or(0);
        if cctp_version != 2 {
            return Err(IrisError::Malformed(format!(
                "cctpVersion {cctp_version} != 2"
            )));
        }

        let status = match msg.status.as_deref() {
            Some("complete") => IrisMessageStatus::Complete,
            Some("pending_confirmations") | None => IrisMessageStatus::Pending,
            other => {
                return Err(IrisError::Malformed(format!("status {:?}", other)));
            }
        };

        if status == IrisMessageStatus::Pending || msg.message.is_empty() || msg.message == "0x" {
            return Ok(IrisPollOutcome::Pending);
        }

        Ok(IrisPollOutcome::Complete(IrisMessage {
            message_hex: msg.message,
            attestation_hex: msg.attestation.filter(|a| a != "PENDING"),
            cctp_version,
            status,
            event_nonce: msg.event_nonce.unwrap_or_default(),
            source_tx_hash: body.source_tx_hash,
        }))
    }

    async fn reattest(&self, nonce: &str) -> Result<(), IrisError> {
        let url = format!(
            "{}/v2/reattest/{}",
            self.base_url,
            urlencoding::encode(nonce)
        );
        self.ensure_host(&url)?;
        let resp = self
            .client
            .post(&url)
            .send()
            .await
            .map_err(|e| IrisError::Http(redact_url(&e.to_string())))?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(IrisError::Http("rate limited".into()));
        }
        if !resp.status().is_success() {
            return Err(IrisError::Http(format!("status {}", resp.status())));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cctp::config::CctpConfig;
    use wiremock::matchers::{method, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn poll_pending_then_complete() {
        let server = MockServer::start().await;
        let base = server.uri();

        Mock::given(method("GET"))
            .and(path_regex(r"/v2/messages/27"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [{
                    "message": "0x",
                    "status": "pending_confirmations",
                    "cctpVersion": 2,
                    "eventNonce": "1"
                }],
                "sourceTxHash": "0xabc"
            })))
            .mount(&server)
            .await;

        let cfg = CctpConfig {
            iris_base_url: base.clone(),
            ..CctpConfig::default_testnet()
        };
        let client = ReqwestIrisClient::from_config(&cfg).unwrap();
        let outcome = client.poll_messages_by_tx(27, "0xabc").await.unwrap();
        assert_eq!(outcome, IrisPollOutcome::Pending);

        server.reset().await;
        Mock::given(method("GET"))
            .and(path_regex(r"/v2/messages/27"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [{
                    "message": "0x0001",
                    "attestation": "0xdead",
                    "status": "complete",
                    "cctpVersion": 2,
                    "eventNonce": "42"
                }],
                "sourceTxHash": "0xabc"
            })))
            .mount(&server)
            .await;

        let outcome = client.poll_messages_by_tx(27, "0xabc").await.unwrap();
        assert!(matches!(outcome, IrisPollOutcome::Complete(_)));
    }

    #[tokio::test]
    async fn poll_429_rate_limited() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "120"))
            .mount(&server)
            .await;

        let cfg = CctpConfig {
            iris_base_url: server.uri(),
            ..CctpConfig::default_testnet()
        };
        let client = ReqwestIrisClient::from_config(&cfg).unwrap();
        let outcome = client.poll_messages_by_tx(0, "0xabc").await.unwrap();
        assert!(matches!(
            outcome,
            IrisPollOutcome::RateLimited {
                retry_after_secs: 120
            }
        ));
    }

    #[tokio::test]
    async fn poll_wrong_cctp_version_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "messages": [{
                    "message": "0x01",
                    "status": "complete",
                    "cctpVersion": 1
                }]
            })))
            .mount(&server)
            .await;

        let cfg = CctpConfig {
            iris_base_url: server.uri(),
            ..CctpConfig::default_testnet()
        };
        let client = ReqwestIrisClient::from_config(&cfg).unwrap();
        let err = client.poll_messages_by_tx(0, "0xabc").await.unwrap_err();
        assert!(matches!(err, IrisError::Malformed(_)));
    }
}
