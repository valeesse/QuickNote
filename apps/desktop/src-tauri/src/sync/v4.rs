use super::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const SCHEMA_VERSION: u32 = 4;
const ROOT_PATH: &str = "v4";
const PUSH_BATCH_LIMIT: i64 = 500;
pub(super) const GC_GRACE_SECONDS: i64 = 7 * 24 * 60 * 60;
const MAX_NOTE_CONTENT_BYTES: usize = 5 * 1024 * 1024;
const MAX_YJS_STATE_BYTES: usize = 20 * 1024 * 1024;
const ATTACHMENT_CHUNK_THRESHOLD: usize = 4 * 1024 * 1024;
const ATTACHMENT_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Workspace {
    pub schema_version: u32,
    pub workspace_id: String,
    pub epoch: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct DeviceHead {
    pub schema_version: u32,
    pub workspace_id: String,
    pub epoch: u64,
    pub device_id: String,
    pub generation: u64,
    pub root_hash: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateRoot {
    schema_version: u32,
    workspace_id: String,
    epoch: u64,
    device_id: String,
    generation: u64,
    shards: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StateShard {
    schema_version: u32,
    entries: BTreeMap<String, StateEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StateEntry {
    object_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredObject {
    schema_version: u32,
    envelope: SyncEnvelope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    yjs_state_base64: Option<String>,
}

impl StoredObject {
    fn from_envelope(mut envelope: SyncEnvelope) -> Result<Self, String> {
        let yjs_state = envelope
            .note
            .as_mut()
            .and_then(|note| note.yjs_state.take());
        if let Some(note) = &envelope.note {
            if note.content.len() > MAX_NOTE_CONTENT_BYTES {
                return Err(format!(
                    "Note {} exceeds the 5 MiB WebDAV sync limit",
                    note.id
                ));
            }
        }
        if yjs_state
            .as_ref()
            .is_some_and(|state| state.len() > MAX_YJS_STATE_BYTES)
        {
            return Err("Yjs state exceeds the 20 MiB WebDAV sync limit".to_string());
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            envelope,
            yjs_state_base64: yjs_state.map(|state| BASE64.encode(state)),
        })
    }

    fn into_envelope(mut self) -> Result<SyncEnvelope, String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("Unsupported WebDAV v4 object".to_string());
        }
        if let Some(encoded) = self.yjs_state_base64 {
            let bytes = BASE64
                .decode(encoded)
                .map_err(|error| format!("Invalid WebDAV v4 Yjs state: {error}"))?;
            if bytes.len() > MAX_YJS_STATE_BYTES {
                return Err("Yjs state exceeds the 20 MiB WebDAV sync limit".to_string());
            }
            let note = self
                .envelope
                .note
                .as_mut()
                .ok_or_else(|| "WebDAV v4 Yjs state has no note payload".to_string())?;
            note.yjs_state = Some(bytes);
        }
        Ok(self.envelope)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GcCandidate {
    schema_version: u32,
    workspace_id: String,
    epoch: u64,
    target_path: String,
    first_seen_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct V4AttachmentManifest {
    schema_version: u32,
    id: String,
    size: usize,
    chunk_size: usize,
    chunks: Vec<String>,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_prefix(hash: &str) -> Result<&str, String> {
    if hash.len() != 64 || !hash.bytes().all(|value| value.is_ascii_hexdigit()) {
        return Err("Invalid content hash".to_string());
    }
    Ok(&hash[..2])
}

fn entity_key(entity_type: &str, entity_id: &str) -> String {
    format!("{entity_type}:{entity_id}")
}

fn shard_id(key: &str) -> String {
    digest(key.as_bytes())[..2].to_string()
}

fn content_path(kind: &str, hash: &str, extension: &str) -> Result<String, String> {
    Ok(format!(
        "{ROOT_PATH}/{kind}/{}/{}{}",
        hash_prefix(hash)?,
        hash,
        extension
    ))
}

async fn put_content(
    provider: &dyn SyncProvider,
    kind: &str,
    bytes: Vec<u8>,
    extension: &str,
    content_type: &str,
) -> Result<String, String> {
    let hash = digest(&bytes);
    let prefix = hash_prefix(&hash)?;
    provider
        .ensure_collection(&format!("{ROOT_PATH}/{kind}/{prefix}"))
        .await?;
    provider
        .put(&content_path(kind, &hash, extension)?, bytes, content_type)
        .await?;
    Ok(hash)
}

async fn get_content(
    provider: &dyn SyncProvider,
    kind: &str,
    hash: &str,
    extension: &str,
) -> Result<Vec<u8>, String> {
    let path = content_path(kind, hash, extension)?;
    let bytes = provider
        .get(&path)
        .await?
        .ok_or_else(|| format!("Missing WebDAV v4 object {path}"))?;
    if digest(&bytes) != hash {
        return Err(format!("WebDAV v4 integrity check failed for {path}"));
    }
    Ok(bytes)
}

async fn put_gzip<T: Serialize>(
    provider: &dyn SyncProvider,
    kind: &str,
    value: &T,
) -> Result<String, String> {
    put_content(
        provider,
        kind,
        gzip_json(value)?,
        ".json.gz",
        "application/gzip",
    )
    .await
}

async fn get_gzip<T: for<'de> Deserialize<'de>>(
    provider: &dyn SyncProvider,
    kind: &str,
    hash: &str,
) -> Result<T, String> {
    gunzip_json(&get_content(provider, kind, hash, ".json.gz").await?)
}

pub(super) async fn ensure_workspace(
    provider: &dyn SyncProvider,
    device_id: &str,
) -> Result<Workspace, String> {
    provider.prepare(device_id).await?;
    if let Some(bytes) = provider.get("v4/workspace.json").await? {
        return validate_workspace(&bytes);
    }
    let workspace = Workspace {
        schema_version: SCHEMA_VERSION,
        workspace_id: Uuid::new_v4().to_string(),
        epoch: 1,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let bytes = serde_json::to_vec_pretty(&workspace).map_err(|error| error.to_string())?;
    if let Err(error) = provider
        .put("v4/workspace.json", bytes, "application/json")
        .await
    {
        if let Some(existing) = provider.get("v4/workspace.json").await? {
            return validate_workspace(&existing);
        }
        return Err(error);
    }
    Ok(workspace)
}

fn validate_workspace(bytes: &[u8]) -> Result<Workspace, String> {
    let workspace: Workspace =
        serde_json::from_slice(bytes).map_err(|error| format!("Invalid v4 workspace: {error}"))?;
    if workspace.schema_version != SCHEMA_VERSION
        || workspace.epoch == 0
        || !is_safe_path_segment(&workspace.workspace_id)
    {
        return Err("Unsupported or invalid WebDAV v4 workspace".to_string());
    }
    Ok(workspace)
}

fn head_path(device_id: &str) -> String {
    format!("v4/heads/{device_id}.json")
}

pub(super) async fn read_head(
    provider: &dyn SyncProvider,
    workspace: &Workspace,
    device_id: &str,
) -> Result<Option<DeviceHead>, String> {
    let Some(bytes) = provider.get(&head_path(device_id)).await? else {
        return Ok(None);
    };
    let head: DeviceHead = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid WebDAV v4 head for {device_id}: {error}"))?;
    if head.schema_version != SCHEMA_VERSION
        || head.workspace_id != workspace.workspace_id
        || head.epoch != workspace.epoch
        || head.device_id != device_id
    {
        return Err(format!("WebDAV v4 head identity mismatch for {device_id}"));
    }
    Ok(Some(head))
}

async fn load_root(
    provider: &dyn SyncProvider,
    workspace: &Workspace,
    head: &DeviceHead,
) -> Result<StateRoot, String> {
    let root: StateRoot = get_gzip(provider, "roots", &head.root_hash).await?;
    if root.schema_version != SCHEMA_VERSION
        || root.workspace_id != workspace.workspace_id
        || root.epoch != workspace.epoch
        || root.device_id != head.device_id
        || root.generation != head.generation
    {
        return Err(format!(
            "WebDAV v4 root identity mismatch for {}",
            head.device_id
        ));
    }
    Ok(root)
}

async fn load_shard(
    provider: &dyn SyncProvider,
    hash: Option<&String>,
) -> Result<StateShard, String> {
    match hash {
        Some(hash) => {
            let shard: StateShard = get_gzip(provider, "shards", hash).await?;
            if shard.schema_version != SCHEMA_VERSION {
                return Err("Unsupported WebDAV v4 shard".to_string());
            }
            Ok(shard)
        }
        None => Ok(StateShard {
            schema_version: SCHEMA_VERSION,
            entries: BTreeMap::new(),
        }),
    }
}

async fn upload_attachment_v4(
    provider: &dyn SyncProvider,
    attachments_dir: &Path,
    attachment: &AttachmentRecord,
) -> Result<(), String> {
    let bytes = std::fs::read(attachments_dir.join(&attachment.relative_path))
        .map_err(|error| format!("Failed to read attachment {}: {error}", attachment.id))?;
    if bytes.len() != attachment.size as usize || digest(&bytes) != attachment.id {
        return Err(format!(
            "Attachment {} failed integrity validation",
            attachment.id
        ));
    }
    if bytes.len() < ATTACHMENT_CHUNK_THRESHOLD {
        let uploaded = put_content(
            provider,
            "attachments",
            bytes,
            ".bin",
            &attachment.mime_type,
        )
        .await?;
        if uploaded != attachment.id {
            return Err(format!("Attachment {} hash mismatch", attachment.id));
        }
        return Ok(());
    }

    let mut chunks = Vec::new();
    for chunk in bytes.chunks(ATTACHMENT_CHUNK_SIZE) {
        chunks.push(
            put_content(
                provider,
                "attachment-chunks",
                chunk.to_vec(),
                ".bin",
                "application/octet-stream",
            )
            .await?,
        );
    }
    let prefix = hash_prefix(&attachment.id)?;
    provider
        .ensure_collection(&format!("v4/attachment-manifests/{prefix}"))
        .await?;
    let manifest = V4AttachmentManifest {
        schema_version: SCHEMA_VERSION,
        id: attachment.id.clone(),
        size: bytes.len(),
        chunk_size: ATTACHMENT_CHUNK_SIZE,
        chunks,
    };
    provider
        .put(
            &format!("v4/attachment-manifests/{prefix}/{}.json", attachment.id),
            serde_json::to_vec(&manifest).map_err(|error| error.to_string())?,
            "application/json",
        )
        .await?;
    Ok(())
}

pub(super) async fn download_attachment(
    provider: &dyn SyncProvider,
    attachment: &AttachmentRecord,
) -> Result<Option<Vec<u8>>, String> {
    let prefix = hash_prefix(&attachment.id)?;
    if let Some(bytes) = provider
        .get(&format!("v4/attachments/{prefix}/{}.bin", attachment.id))
        .await?
    {
        return Ok(Some(bytes));
    }
    let Some(manifest_bytes) = provider
        .get(&format!(
            "v4/attachment-manifests/{prefix}/{}.json",
            attachment.id
        ))
        .await?
    else {
        return Ok(None);
    };
    let manifest: V4AttachmentManifest = serde_json::from_slice(&manifest_bytes)
        .map_err(|error| format!("Invalid WebDAV v4 attachment manifest: {error}"))?;
    if manifest.schema_version != SCHEMA_VERSION
        || manifest.id != attachment.id
        || manifest.size != attachment.size as usize
        || manifest.chunk_size != ATTACHMENT_CHUNK_SIZE
        || manifest.chunks.is_empty()
    {
        return Err(format!(
            "Invalid WebDAV v4 attachment manifest for {}",
            attachment.id
        ));
    }
    let mut bytes = Vec::with_capacity(manifest.size);
    for hash in manifest.chunks {
        bytes.extend_from_slice(&get_content(provider, "attachment-chunks", &hash, ".bin").await?);
    }
    if bytes.len() != manifest.size || digest(&bytes) != attachment.id {
        return Err(format!(
            "WebDAV v4 attachment {} failed integrity validation",
            attachment.id
        ));
    }
    Ok(Some(bytes))
}

pub(super) async fn push(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
    workspace: &Workspace,
) -> Result<usize, String> {
    let mut pushed = 0usize;
    loop {
        let changes = db
            .list_pending_changes(PUSH_BATCH_LIMIT)
            .map_err(|error| error.to_string())?;
        if changes.is_empty() {
            break;
        }

        let current_head = read_head(provider, workspace, &config.device_id).await?;
        let mut root = if let Some(head) = &current_head {
            load_root(provider, workspace, head).await?
        } else {
            StateRoot {
                schema_version: SCHEMA_VERSION,
                workspace_id: workspace.workspace_id.clone(),
                epoch: workspace.epoch,
                device_id: config.device_id.clone(),
                generation: 0,
                shards: BTreeMap::new(),
            }
        };
        let mut seen = HashSet::new();
        let entities: Vec<(String, String)> = changes
            .iter()
            .filter_map(|change| {
                let key = (change.entity_type.clone(), change.entity_id.clone());
                seen.insert(key.clone()).then_some(key)
            })
            .collect();
        let mut changed_shards: HashMap<String, StateShard> = HashMap::new();
        let mut any_changed = false;

        for (entity_type, entity_id) in &entities {
            let envelope = build_state_envelope(db, entity_type, entity_id, &config.device_id)?;
            if let Some(attachment) = &envelope.attachment {
                upload_attachment_v4(provider, attachments_dir, attachment).await?;
            }
            let object_hash =
                put_gzip(provider, "objects", &StoredObject::from_envelope(envelope)?).await?;
            let key = entity_key(entity_type, entity_id);
            let shard_key = shard_id(&key);
            if !changed_shards.contains_key(&shard_key) {
                let shard = load_shard(provider, root.shards.get(&shard_key)).await?;
                changed_shards.insert(shard_key.clone(), shard);
            }
            let shard = changed_shards.get_mut(&shard_key).unwrap();
            if shard
                .entries
                .get(&key)
                .is_none_or(|entry| entry.object_hash != object_hash)
            {
                shard.entries.insert(key, StateEntry { object_hash });
                any_changed = true;
            }
        }

        if any_changed {
            for (shard_key, shard) in changed_shards {
                root.shards
                    .insert(shard_key, put_gzip(provider, "shards", &shard).await?);
            }
            root.generation = current_head.as_ref().map_or(1, |head| head.generation + 1);
            let root_hash = put_gzip(provider, "roots", &root).await?;
            let head = DeviceHead {
                schema_version: SCHEMA_VERSION,
                workspace_id: workspace.workspace_id.clone(),
                epoch: workspace.epoch,
                device_id: config.device_id.clone(),
                generation: root.generation,
                root_hash,
                updated_at: chrono::Utc::now().to_rfc3339(),
            };
            provider
                .put(
                    &head_path(&config.device_id),
                    serde_json::to_vec(&head).map_err(|error| error.to_string())?,
                    "application/json",
                )
                .await?;
        }

        let included: HashSet<_> = entities.iter().cloned().collect();
        for change in &changes {
            if included.contains(&(change.entity_type.clone(), change.entity_id.clone())) {
                db.mark_change_synced(change.seq)
                    .map_err(|error| error.to_string())?;
            }
        }
        pushed += entities.len();
        db.prune_synced_changes()
            .map_err(|error| error.to_string())?;
    }
    Ok(pushed)
}

pub(super) async fn pull(
    provider: &dyn SyncProvider,
    db: &Database,
    attachments_dir: &Path,
    config: &SyncConfig,
    workspace: &Workspace,
) -> Result<(usize, usize), String> {
    let mut pulled = 0;
    let mut conflicts = 0;
    let scope = format!(
        "webdav-v4:{}:{}:{}",
        config.endpoint, workspace.workspace_id, workspace.epoch
    );
    for file in provider.list("v4/heads").await? {
        let Some(device_id) = file.strip_suffix(".json") else {
            continue;
        };
        if device_id == config.device_id || !is_safe_path_segment(device_id) {
            continue;
        }
        let Some(head) = read_head(provider, workspace, device_id).await? else {
            continue;
        };
        let cursor = db
            .get_sync_cursor_value(&scope, device_id)
            .map_err(|error| error.to_string())?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        if head.generation <= cursor {
            continue;
        }
        let root = load_root(provider, workspace, &head).await?;
        let mut object_hashes = BTreeSet::new();
        for shard_hash in root.shards.values() {
            let shard = load_shard(provider, Some(shard_hash)).await?;
            object_hashes.extend(
                shard
                    .entries
                    .values()
                    .map(|entry| entry.object_hash.clone()),
            );
        }
        for object_hash in object_hashes {
            let envelope = get_gzip::<StoredObject>(provider, "objects", &object_hash)
                .await?
                .into_envelope()?;
            validate_envelope(&envelope, device_id)?;
            let (changed, conflict) =
                apply_envelope(provider, db, attachments_dir, &envelope, &config.device_id).await?;
            pulled += usize::from(changed);
            conflicts += usize::from(conflict);
        }
        db.set_sync_cursor_value(&scope, device_id, &head.generation.to_string())
            .map_err(|error| error.to_string())?;
    }
    Ok((pulled, conflicts))
}

pub(super) async fn has_remote_changes(
    provider: &dyn SyncProvider,
    db: &Database,
    config: &SyncConfig,
    workspace: &Workspace,
) -> Result<bool, String> {
    let scope = format!(
        "webdav-v4:{}:{}:{}",
        config.endpoint, workspace.workspace_id, workspace.epoch
    );
    for file in provider.list("v4/heads").await? {
        let Some(device_id) = file.strip_suffix(".json") else {
            continue;
        };
        if device_id == config.device_id || !is_safe_path_segment(device_id) {
            continue;
        }
        let Some(head) = read_head(provider, workspace, device_id).await? else {
            continue;
        };
        let cursor = db
            .get_sync_cursor_value(&scope, device_id)
            .map_err(|error| error.to_string())?
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        if head.generation > cursor {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn list_content_paths(
    provider: &dyn SyncProvider,
    kind: &str,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    let base = format!("v4/{kind}");
    for prefix in provider.list(&base).await? {
        if prefix.len() != 2 || !prefix.bytes().all(|value| value.is_ascii_hexdigit()) {
            continue;
        }
        for file in provider.list(&format!("{base}/{prefix}")).await? {
            if file.contains('/') || file.contains('\\') || file == "." || file == ".." {
                continue;
            }
            paths.push(format!("{base}/{prefix}/{file}"));
        }
    }
    Ok(paths)
}

async fn reachable_paths(
    provider: &dyn SyncProvider,
    workspace: &Workspace,
) -> Result<HashSet<String>, String> {
    let mut reachable = HashSet::new();
    let mut object_hashes = HashSet::new();
    for file in provider.list("v4/heads").await? {
        let Some(device_id) = file.strip_suffix(".json") else {
            continue;
        };
        if !is_safe_path_segment(device_id) {
            continue;
        }
        let Some(head) = read_head(provider, workspace, device_id).await? else {
            continue;
        };
        reachable.insert(content_path("roots", &head.root_hash, ".json.gz")?);
        let root = load_root(provider, workspace, &head).await?;
        for shard_hash in root.shards.values() {
            reachable.insert(content_path("shards", shard_hash, ".json.gz")?);
            let shard = load_shard(provider, Some(shard_hash)).await?;
            object_hashes.extend(
                shard
                    .entries
                    .values()
                    .map(|entry| entry.object_hash.clone()),
            );
        }
    }
    for object_hash in object_hashes {
        reachable.insert(content_path("objects", &object_hash, ".json.gz")?);
        let envelope = get_gzip::<StoredObject>(provider, "objects", &object_hash)
            .await?
            .into_envelope()?;
        if envelope.operation != "delete" {
            if let Some(attachment) = envelope.attachment {
                let direct_path = content_path("attachments", &attachment.id, ".bin")?;
                if provider.get(&direct_path).await?.is_some() {
                    reachable.insert(direct_path);
                    continue;
                }
                let prefix = hash_prefix(&attachment.id)?;
                let manifest_path =
                    format!("v4/attachment-manifests/{prefix}/{}.json", attachment.id);
                if let Some(bytes) = provider.get(&manifest_path).await? {
                    let manifest: V4AttachmentManifest =
                        serde_json::from_slice(&bytes).map_err(|error| {
                            format!("Invalid WebDAV v4 attachment manifest: {error}")
                        })?;
                    reachable.insert(manifest_path);
                    for chunk in manifest.chunks {
                        reachable.insert(content_path("attachment-chunks", &chunk, ".bin")?);
                    }
                }
            }
        }
    }
    Ok(reachable)
}

async fn head_fingerprint(
    provider: &dyn SyncProvider,
    workspace: &Workspace,
) -> Result<BTreeMap<String, (u64, String)>, String> {
    let mut heads = BTreeMap::new();
    for file in provider.list("v4/heads").await? {
        let Some(device_id) = file.strip_suffix(".json") else {
            continue;
        };
        if !is_safe_path_segment(device_id) {
            continue;
        }
        if let Some(head) = read_head(provider, workspace, device_id).await? {
            heads.insert(device_id.to_string(), (head.generation, head.root_hash));
        }
    }
    Ok(heads)
}

fn candidate_path(target_path: &str) -> String {
    format!("v4/gc/candidates/{}.json", digest(target_path.as_bytes()))
}

/// Two-phase mark-and-sweep. Newly unreachable objects are only marked; a later scan must
/// observe them unreachable again after the grace period before deletion.
pub(super) async fn run_gc(
    provider: &dyn SyncProvider,
    workspace: &Workspace,
    grace_seconds: i64,
) -> Result<usize, String> {
    let initial_heads = head_fingerprint(provider, workspace).await?;
    let reachable = reachable_paths(provider, workspace).await?;
    let mut stored = Vec::new();
    for kind in [
        "roots",
        "shards",
        "objects",
        "attachments",
        "attachment-manifests",
        "attachment-chunks",
    ] {
        stored.extend(list_content_paths(provider, kind).await?);
    }
    let now = chrono::Utc::now().timestamp();
    let mut deleted = 0;
    for target_path in stored {
        let marker_path = candidate_path(&target_path);
        if reachable.contains(&target_path) {
            if provider.get(&marker_path).await?.is_some() {
                provider.delete(&marker_path).await?;
            }
            continue;
        }
        let marker = provider.get(&marker_path).await?;
        if let Some(bytes) = marker {
            let candidate: GcCandidate = serde_json::from_slice(&bytes)
                .map_err(|error| format!("Invalid WebDAV v4 GC marker: {error}"))?;
            if candidate.schema_version != SCHEMA_VERSION
                || candidate.workspace_id != workspace.workspace_id
                || candidate.epoch != workspace.epoch
                || candidate.target_path != target_path
            {
                return Err("WebDAV v4 GC marker identity mismatch".to_string());
            }
            if now.saturating_sub(candidate.first_seen_at) >= grace_seconds {
                // Abort the sweep if any head moved after mark traversal. Comparing heads before
                // every destructive operation is cheap and prevents a concurrent commit from
                // making a candidate reachable between the scan and its deletion.
                if head_fingerprint(provider, workspace).await? != initial_heads {
                    return Ok(deleted);
                }
                provider.delete(&target_path).await?;
                provider.delete(&marker_path).await?;
                deleted += 1;
            }
        } else {
            let candidate = GcCandidate {
                schema_version: SCHEMA_VERSION,
                workspace_id: workspace.workspace_id.clone(),
                epoch: workspace.epoch,
                target_path,
                first_seen_at: now,
            };
            provider
                .put(
                    &marker_path,
                    serde_json::to_vec(&candidate).map_err(|error| error.to_string())?,
                    "application/json",
                )
                .await?;
        }
    }
    Ok(deleted)
}

pub(super) async fn maybe_run_gc(
    provider: &dyn SyncProvider,
    db: &Database,
    config: &SyncConfig,
    workspace: &Workspace,
) -> Result<usize, String> {
    let scope = format!(
        "webdav-v4-gc:{}:{}:{}",
        config.endpoint, workspace.workspace_id, workspace.epoch
    );
    let now = chrono::Utc::now().timestamp();
    let last = db
        .get_sync_cursor_value(&scope, "last-run")
        .map_err(|error| error.to_string())?
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if now.saturating_sub(last) < 24 * 60 * 60 {
        return Ok(0);
    }
    let deleted = run_gc(provider, workspace, GC_GRACE_SECONDS).await?;
    db.set_sync_cursor_value(&scope, "last-run", &now.to_string())
        .map_err(|error| error.to_string())?;
    Ok(deleted)
}

pub(super) async fn storage_status(
    provider: &dyn SyncProvider,
    workspace: &Workspace,
) -> Result<WebDavStorageStatus, String> {
    let reachable = reachable_paths(provider, workspace).await?;
    let mut stored = Vec::new();
    for kind in [
        "roots",
        "shards",
        "objects",
        "attachments",
        "attachment-manifests",
        "attachment-chunks",
    ] {
        stored.extend(list_content_paths(provider, kind).await?);
    }
    let mut stored_bytes = 0usize;
    for path in &stored {
        if let Some(bytes) = provider.get(path).await? {
            stored_bytes = stored_bytes.saturating_add(bytes.len());
        }
    }
    Ok(WebDavStorageStatus {
        protocol_version: SCHEMA_VERSION,
        workspace_id: workspace.workspace_id.clone(),
        epoch: workspace.epoch,
        devices: provider
            .list("v4/heads")
            .await?
            .into_iter()
            .filter(|file| file.ends_with(".json"))
            .count(),
        stored_objects: stored.len(),
        reachable_objects: reachable.len(),
        pending_gc_objects: provider
            .list("v4/gc/candidates")
            .await?
            .into_iter()
            .filter(|file| file.ends_with(".json"))
            .count(),
        stored_bytes,
    })
}
