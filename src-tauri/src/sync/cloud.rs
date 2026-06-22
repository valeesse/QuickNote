#![allow(dead_code)]
use crate::sync::SyncEnvelope;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
struct PushResponse {
    accepted: usize,
    conflicts: usize,
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

    pub async fn login(base_url: &str, email: &str, password: &str) -> Result<LoginResponse, String> {
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

    pub async fn push(&self, envelopes: &[SyncEnvelope]) -> Result<(usize, usize), String> {
        if envelopes.is_empty() {
            return Ok((0, 0));
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

        Ok((data.accepted, data.conflicts))
    }
}
