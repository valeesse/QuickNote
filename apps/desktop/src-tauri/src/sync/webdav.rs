use super::SyncProvider;
use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::{Client, Method, StatusCode};
use std::time::Duration;

const MAX_ATTEMPTS: usize = 3;

fn should_retry_status(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::REQUEST_TIMEOUT
        || status == StatusCode::TOO_MANY_REQUESTS
}

async fn retry_delay(attempt: usize) {
    let jitter = chrono::Utc::now().timestamp_subsec_millis() as u64 % 125;
    tokio::time::sleep(Duration::from_millis(250 * (1 << attempt) + jitter)).await;
}

fn request_timeout(path: &str, body_len: usize) -> Duration {
    if path.starts_with("attachments/")
        || path.starts_with("attachment-chunks/")
        || path.starts_with("v4/attachments/")
        || path.starts_with("v4/attachment-chunks/")
    {
        if body_len == 0 {
            return Duration::from_secs(90);
        }
        let transfer_seconds = (body_len as u64).div_ceil(32 * 1024);
        Duration::from_secs((15 + transfer_seconds).clamp(30, 300))
    } else {
        Duration::from_secs(15)
    }
}

pub struct WebDavProvider {
    client: Client,
    endpoint: String,
    username: String,
    password: String,
    timeout: Duration,
}

impl WebDavProvider {
    pub fn new(endpoint: &str, username: &str, password: &str) -> Result<Self, String> {
        Self::new_with_timeout(endpoint, username, password, Duration::from_secs(30))
    }

    fn new_with_timeout(
        endpoint: &str,
        username: &str,
        password: &str,
        timeout: Duration,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("QuickNote/0.1")
            .connect_timeout(Duration::from_secs(10).min(timeout))
            .timeout(timeout)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            endpoint: endpoint.trim_end_matches('/').to_string(),
            username: username.to_string(),
            password: password.to_string(),
            timeout,
        })
    }

    fn url(&self, path: &str) -> String {
        if path.is_empty() {
            self.endpoint.clone()
        } else {
            format!("{}/{}", self.endpoint, path.trim_matches('/'))
        }
    }

    async fn create_collection(&self, path: &str) -> Result<(), String> {
        let method = Method::from_bytes(b"MKCOL").map_err(|e| e.to_string())?;
        let mut last_error = String::new();
        for attempt in 0..MAX_ATTEMPTS {
            let result = self
                .client
                .request(method.clone(), self.url(path))
                .basic_auth(&self.username, Some(&self.password))
                .timeout(Duration::from_secs(15).min(self.timeout))
                .send()
                .await;
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    last_error = error.to_string();
                    if attempt + 1 < MAX_ATTEMPTS {
                        retry_delay(attempt).await;
                        continue;
                    }
                    return Err(last_error);
                }
            };
            if response.status().is_success()
                || response.status() == StatusCode::METHOD_NOT_ALLOWED
                || response.status() == StatusCode::CONFLICT
            {
                return Ok(());
            }
            last_error = format!("WebDAV MKCOL {} failed: {}", path, response.status());
            if !should_retry_status(response.status()) || attempt + 1 == MAX_ATTEMPTS {
                return Err(last_error);
            }
            retry_delay(attempt).await;
        }
        Err(last_error)
    }
}

fn parse_propfind_names(xml: &str, current_path: &str) -> Result<Vec<String>, String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut names = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) if element.local_name().as_ref() == b"href" => {
                let href = reader
                    .read_text(element.name())
                    .map_err(|e| e.to_string())?;
                let value = href.trim_end_matches('/');
                if let Some(name) = value.rsplit('/').next() {
                    if !name.is_empty() {
                        let decoded = percent_decode_str(name).decode_utf8_lossy().into_owned();
                        names.push(decoded);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => return Err(format!("Invalid WebDAV response: {error}")),
            _ => {}
        }
    }
    names.sort();
    names.dedup();
    if let Some(current) = current_path.trim_end_matches('/').rsplit('/').next() {
        names.retain(|name| name != current);
    }
    Ok(names)
}

#[async_trait]
impl SyncProvider for WebDavProvider {
    async fn prepare(&self, _device_id: &str) -> Result<(), String> {
        self.create_collection("").await?;
        self.create_collection("v4").await?;
        self.create_collection("v4/devices").await?;
        self.create_collection("v4/heads").await?;
        self.create_collection("v4/roots").await?;
        self.create_collection("v4/shards").await?;
        self.create_collection("v4/objects").await?;
        self.create_collection("v4/attachments").await?;
        self.create_collection("v4/attachment-manifests").await?;
        self.create_collection("v4/attachment-chunks").await?;
        self.create_collection("v4/gc").await?;
        self.create_collection("v4/gc/candidates").await?;
        Ok(())
    }

    async fn ensure_collection(&self, path: &str) -> Result<(), String> {
        self.create_collection(path).await
    }

    async fn list(&self, path: &str) -> Result<Vec<String>, String> {
        let method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
        let mut last_error = String::new();
        for attempt in 0..MAX_ATTEMPTS {
            let result = self.client.request(method.clone(), self.url(path))
                .basic_auth(&self.username, Some(&self.password))
                .header("Depth", "1")
                .header("Content-Type", "application/xml")
                .timeout(Duration::from_secs(15).min(self.timeout))
                .body("<?xml version=\"1.0\"?><propfind xmlns=\"DAV:\"><prop><getetag/></prop></propfind>")
                .send().await;
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    last_error = error.to_string();
                    if attempt + 1 < MAX_ATTEMPTS {
                        retry_delay(attempt).await;
                        continue;
                    }
                    return Err(last_error);
                }
            };
            if !response.status().is_success() && response.status().as_u16() != 207 {
                last_error = format!("WebDAV PROPFIND {} failed: {}", path, response.status());
                if should_retry_status(response.status()) && attempt + 1 < MAX_ATTEMPTS {
                    retry_delay(attempt).await;
                    continue;
                }
                return Err(last_error);
            }
            let xml = response.text().await.map_err(|e| e.to_string())?;
            return parse_propfind_names(&xml, path);
        }
        Err(last_error)
    }

    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        let mut last_error = String::new();
        for attempt in 0..MAX_ATTEMPTS {
            let result = self
                .client
                .get(self.url(path))
                .basic_auth(&self.username, Some(&self.password))
                .timeout(request_timeout(path, 0).min(self.timeout))
                .send()
                .await;
            let response = match result {
                Ok(response) => response,
                Err(error) => {
                    last_error = error.to_string();
                    if attempt + 1 < MAX_ATTEMPTS {
                        retry_delay(attempt).await;
                        continue;
                    }
                    return Err(last_error);
                }
            };
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            if !response.status().is_success() {
                last_error = format!("WebDAV GET {} failed: {}", path, response.status());
                if should_retry_status(response.status()) && attempt + 1 < MAX_ATTEMPTS {
                    retry_delay(attempt).await;
                    continue;
                }
                return Err(last_error);
            }
            return Ok(Some(
                response.bytes().await.map_err(|e| e.to_string())?.to_vec(),
            ));
        }
        Err(last_error)
    }

    async fn put(&self, path: &str, body: Vec<u8>, content_type: &str) -> Result<(), String> {
        let immutable = path == "v4/workspace.json"
            || path.starts_with("v4/roots/")
            || path.starts_with("v4/shards/")
            || path.starts_with("v4/objects/")
            || path.starts_with("v4/attachments/")
            || path.starts_with("v4/attachment-manifests/")
            || path.starts_with("v4/attachment-chunks/")
            || path.starts_with("v4/gc/candidates/")
            || path.starts_with("attachments/")
            || path.starts_with("attachment-manifests/")
            || path.starts_with("attachment-chunks/")
            || path.starts_with("changes/");
        let mut last_error = String::new();
        for attempt in 0..MAX_ATTEMPTS {
            let mut request = self
                .client
                .put(self.url(path))
                .basic_auth(&self.username, Some(&self.password))
                .header("Content-Type", content_type)
                .timeout(request_timeout(path, body.len()).min(self.timeout))
                .body(body.clone());
            if immutable {
                request = request.header("If-None-Match", "*");
            }
            let response = match request.send().await {
                Ok(response) => response,
                Err(error) => {
                    last_error = error.to_string();
                    if attempt + 1 < MAX_ATTEMPTS {
                        retry_delay(attempt).await;
                        continue;
                    }
                    return Err(last_error);
                }
            };
            if response.status().is_success() {
                return Ok(());
            }
            if immutable && response.status() == StatusCode::PRECONDITION_FAILED {
                if self.get(path).await?.as_deref() == Some(body.as_slice()) {
                    return Ok(());
                }
                return Err(format!("Immutable WebDAV object already exists: {path}"));
            }
            last_error = format!("WebDAV PUT {} failed: {}", path, response.status());
            if should_retry_status(response.status()) && attempt + 1 < MAX_ATTEMPTS {
                retry_delay(attempt).await;
                continue;
            }
            return Err(last_error);
        }
        if immutable && self.get(path).await?.as_deref() == Some(body.as_slice()) {
            return Ok(());
        }
        Err(last_error)
    }

    async fn delete(&self, path: &str) -> Result<(), String> {
        let response = self
            .client
            .delete(self.url(path))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        Err(format!(
            "WebDAV DELETE {} failed: {}",
            path,
            response.status()
        ))
    }
}
#[cfg(test)]
mod tests;
