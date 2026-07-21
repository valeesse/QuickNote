use super::*;
use quicknote_protocol::Note;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex as StdMutex;

#[derive(Default)]
struct MemoryProvider {
    objects: StdMutex<BTreeMap<String, Vec<u8>>>,
    fail_after_next_put: AtomicBool,
    fail_next_lists: AtomicUsize,
    fail_next_gets: AtomicUsize,
    fail_put_path_once: StdMutex<Option<String>>,
    delay_ms: AtomicU64,
}

impl MemoryProvider {
    fn fail_after_next_put(&self) {
        self.fail_after_next_put.store(true, Ordering::SeqCst);
    }

    fn fail_next_lists(&self, count: usize) {
        self.fail_next_lists.store(count, Ordering::SeqCst);
    }

    fn fail_next_gets(&self, count: usize) {
        self.fail_next_gets.store(count, Ordering::SeqCst);
    }

    fn fail_put_path_once(&self, path: &str) {
        *self.fail_put_path_once.lock().unwrap() = Some(path.to_string());
    }

    fn set_delay_ms(&self, delay_ms: u64) {
        self.delay_ms.store(delay_ms, Ordering::SeqCst);
    }

    async fn delay(&self) {
        let delay_ms = self.delay_ms.load(Ordering::SeqCst);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }
}

fn consume_failure(counter: &AtomicUsize) -> bool {
    counter
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
            value.checked_sub(1)
        })
        .is_ok()
}

#[async_trait]
impl SyncProvider for MemoryProvider {
    async fn prepare(&self, _device_id: &str) -> Result<(), String> {
        Ok(())
    }

    async fn list(&self, path: &str) -> Result<Vec<String>, String> {
        self.delay().await;
        if consume_failure(&self.fail_next_lists) {
            return Err("injected transient list failure".to_string());
        }
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
        self.delay().await;
        if consume_failure(&self.fail_next_gets) {
            return Err("injected transient get failure".to_string());
        }
        Ok(self.objects.lock().unwrap().get(path).cloned())
    }

    async fn put(&self, path: &str, body: Vec<u8>, _content_type: &str) -> Result<(), String> {
        self.delay().await;
        let mut objects = self.objects.lock().unwrap();
        let immutable = path.starts_with("attachments/")
            || path.starts_with("attachment-manifests/")
            || path.starts_with("attachment-chunks/")
            || path.starts_with("changes/")
            || path == "v4/workspace.json"
            || path.starts_with("v4/roots/")
            || path.starts_with("v4/shards/")
            || path.starts_with("v4/objects/")
            || path.starts_with("v4/attachments/")
            || path.starts_with("v4/attachment-manifests/")
            || path.starts_with("v4/attachment-chunks/")
            || path.starts_with("v4/gc/candidates/");
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
        let fail_this_path = self.fail_put_path_once.lock().unwrap().as_deref() == Some(path);
        if fail_this_path {
            *self.fail_put_path_once.lock().unwrap() = None;
        }
        if fail_this_path || self.fail_after_next_put.swap(false, Ordering::SeqCst) {
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
