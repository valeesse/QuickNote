#![allow(dead_code)]
use crate::sync::{SyncEnvelope, SyncProvider};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct CloudProvider {
    client: Client,
    base_url: String,
    token: String,
}

#[derive(Debug, Serialize)]
struct PullRequest {
    since_seq: i64,
}

#[derive(Debug, Deserialize)]
struct PullResponse {
    envelopes: Vec<SyncEnvelope>,
    server_seq: i64,
}

#[derive(Debug, Serialize)]
struct PushRequest<'a> {
    envelopes: &'a [SyncEnvelope],
}

#[derive(Debug, Deserialize)]
pub struct PushResponse {
    pub accepted: usize,
    pub conflicts: usize,
    pub acknowledged_sequences: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
}

impl CloudProvider {
    pub fn new(base_url: &str, token: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    pub async fn login(
        base_url: &str,
        email: &str,
        password: &str,
    ) -> Result<LoginResponse, String> {
        let client = Client::new();
        let url = format!("{}/api/auth/login", base_url.trim_end_matches('/'));
        let resp = client
            .post(&url)
            .json(&LoginRequest {
                email: email.to_string(),
                password: password.to_string(),
            })
            .send()
            .await
            .map_err(|e| format!("Cloud login failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Cloud login failed ({status}): {body}"));
        }

        resp.json::<LoginResponse>()
            .await
            .map_err(|e| format!("Cloud login response error: {e}"))
    }

    pub async fn pull(&self, since_seq: i64) -> Result<(Vec<SyncEnvelope>, i64), String> {
        let url = format!("{}/api/sync/pull", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&PullRequest { since_seq })
            .send()
            .await
            .map_err(|e| format!("Cloud pull failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Cloud pull failed ({status}): {body}"));
        }

        let data = resp
            .json::<PullResponse>()
            .await
            .map_err(|e| format!("Cloud pull response error: {e}"))?;

        Ok((data.envelopes, data.server_seq))
    }

    pub async fn push(&self, envelopes: &[SyncEnvelope]) -> Result<PushResponse, String> {
        if envelopes.is_empty() {
            return Ok(PushResponse {
                accepted: 0,
                conflicts: 0,
                acknowledged_sequences: Vec::new(),
            });
        }

        let url = format!("{}/api/sync/push", self.base_url);
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.token)
            .json(&PushRequest { envelopes })
            .send()
            .await
            .map_err(|e| format!("Cloud push failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Cloud push failed ({status}): {body}"));
        }

        let data = resp
            .json::<PushResponse>()
            .await
            .map_err(|e| format!("Cloud push response error: {e}"))?;

        Ok(data)
    }
}

#[async_trait]
impl SyncProvider for CloudProvider {
    async fn prepare(&self, _device_id: &str) -> Result<(), String> {
        Ok(())
    }
    async fn list(&self, _path: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        let id = path
            .strip_prefix("attachments/")
            .ok_or_else(|| "Unsupported cloud object path".to_string())?;
        let response = self
            .client
            .get(format!("{}/api/attachments/{id}", self.base_url))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|error| format!("Cloud attachment download failed: {error}"))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(format!(
                "Cloud attachment download failed ({})",
                response.status()
            ));
        }
        response
            .bytes()
            .await
            .map(|bytes| Some(bytes.to_vec()))
            .map_err(|error| error.to_string())
    }
    async fn put(&self, path: &str, body: Vec<u8>, content_type: &str) -> Result<(), String> {
        let id = path
            .strip_prefix("attachments/")
            .ok_or_else(|| "Unsupported cloud object path".to_string())?;
        let response = self
            .client
            .put(format!("{}/api/attachments/{id}", self.base_url))
            .bearer_auth(&self.token)
            .header(reqwest::header::CONTENT_TYPE, content_type)
            .body(body)
            .send()
            .await
            .map_err(|error| format!("Cloud attachment upload failed: {error}"))?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!(
                "Cloud attachment upload failed ({})",
                response.status()
            ))
        }
    }
}
