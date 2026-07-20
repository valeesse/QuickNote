use super::*;
use quicknote_protocol::Note;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex as StdMutex;

#[derive(Default)]
struct MemoryProvider {
    objects: StdMutex<BTreeMap<String, Vec<u8>>>,
    fail_after_next_put: AtomicBool,
}

impl MemoryProvider {
    fn fail_after_next_put(&self) {
        self.fail_after_next_put.store(true, Ordering::SeqCst);
    }
}

#[async_trait]
impl SyncProvider for MemoryProvider {
    async fn prepare(&self, _device_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn list(&self, path: &str) -> Result<Vec<String>, String> {
        let prefix = format!("{}/", path.trim_matches('/'));
        let objects = self.objects.lock().unwrap();
        let mut children = BTreeSet::new();
        for key in objects.keys().filter(|key| key.starts_with(&prefix)) {
            if let Some(child) = key[prefix.len()..].split('/').next() {
                if !child.is_empty() {
                    children.insert(child.to_string());
                }
            }
        }
        Ok(children.into_iter().collect())
    }

    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, String> {
        Ok(self.objects.lock().unwrap().get(path).cloned())
    }

    async fn put(&self, path: &str, body: Vec<u8>, _content_type: &str) -> Result<(), String> {
        let mut objects = self.objects.lock().unwrap();
        let immutable = path.starts_with("attachments/");
        if immutable {
            if let Some(existing) = objects.get(path) {
                if existing != &body {
                    return Err(format!("immutable collision at {path}"));
                }
            } else {
                objects.insert(path.to_string(), body);
            }
        } else {
            objects.insert(path.to_string(), body);
        }
        if self.fail_after_next_put.swap(false, Ordering::SeqCst) {
            return Err("injected acknowledgement loss".to_string());
        }
        Ok(())
    }

    async fn delete(&self, path: &str) -> Result<(), String> {
        self.objects.lock().unwrap().remove(path);
        Ok(())
    }
}

fn config(device_id: &str) -> SyncConfig {
    SyncConfig {
        enabled: true,
        provider: "webdav".to_string(),
        endpoint: "https://dav.test/quicknote".to_string(),
        username: "tester".to_string(),
        device_id: device_id.to_string(),
        cloud_enabled: false,
        cloud_url: String::new(),
        cloud_email: String::new(),
        cloud_cursor_seq: 0,
        cloud_token_created_at: 0,
        password_salt: None,
        webdav_password_encrypted: None,
        cloud_password_encrypted: None,
        cloud_token_encrypted: None,
    }
}

mod transfer;
mod validation;
