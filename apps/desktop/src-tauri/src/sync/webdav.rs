use super::SyncProvider;
use async_trait::async_trait;
use percent_encoding::percent_decode_str;
use quick_xml::events::Event;
use quick_xml::Reader;
use reqwest::{Client, Method, StatusCode};
use std::time::Duration;

pub struct WebDavProvider {
    client: Client,
    endpoint: String,
    username: String,
    password: String,
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
        let response = self
            .client
            .request(method, self.url(path))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status().is_success()
            || response.status() == StatusCode::METHOD_NOT_ALLOWED
            || response.status() == StatusCode::CONFLICT
        {
            Ok(())
        } else {
            Err(format!(
                "WebDAV MKCOL {} failed: {}",
                path,
                response.status()
            ))
        }
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
    async fn prepare(&self, device_id: &str) -> Result<(), String> {
        self.create_collection("").await?;
        self.create_collection("state").await?;
        self.create_collection(&format!("state/{device_id}"))
            .await?;
        self.create_collection(&format!("state/{device_id}/note"))
            .await?;
        self.create_collection(&format!("state/{device_id}/clipboard"))
            .await?;
        self.create_collection(&format!("state/{device_id}/attachment"))
            .await?;
        self.create_collection(&format!("state/{device_id}/tag"))
            .await?;
        self.create_collection(&format!("state/{device_id}/note_tag"))
            .await?;
        self.create_collection(&format!("state/{device_id}/meta"))
            .await?;
        self.create_collection("heads").await?;
        self.create_collection("heads/notes").await?;
        self.create_collection("yjs").await?;
        self.create_collection("yjs/snapshots").await?;
        self.create_collection("yjs/updates").await?;
        self.create_collection("attachments").await?;
        Ok(())
    }

    async fn list(&self, path: &str) -> Result<Vec<String>, String> {
        let method = Method::from_bytes(b"PROPFIND").map_err(|e| e.to_string())?;
        let response = self
            .client
            .request(method, self.url(path))
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body("<?xml version=\"1.0\"?><propfind xmlns=\"DAV:\"><prop><getetag/></prop></propfind>")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() && response.status().as_u16() != 207 {
            return Err(format!(
                "WebDAV PROPFIND {} failed: {}",
                path,
                response.status()
            ));
        }
        let xml = response.text().await.map_err(|e| e.to_string())?;
        parse_propfind_names(&xml, path)
    }

    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        let response = self
            .client
            .get(self.url(path))
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(format!("WebDAV GET {} failed: {}", path, response.status()));
        }
        Ok(Some(
            response.bytes().await.map_err(|e| e.to_string())?.to_vec(),
        ))
    }

    async fn put(&self, path: &str, body: Vec<u8>, content_type: &str) -> Result<(), String> {
        let immutable = path.starts_with("attachments/");
        let mut request = self
            .client
            .put(self.url(path))
            .basic_auth(&self.username, Some(&self.password))
            .header("Content-Type", content_type)
            .body(body.clone());
        if immutable {
            request = request.header("If-None-Match", "*");
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        if response.status().is_success() {
            return Ok(());
        }
        if immutable && response.status() == StatusCode::PRECONDITION_FAILED {
            if self.get(path).await?.as_deref() == Some(body.as_slice()) {
                return Ok(());
            }
            return Err(format!("Immutable WebDAV change already exists: {path}"));
        }
        Err(format!("WebDAV PUT {} failed: {}", path, response.status()))
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
