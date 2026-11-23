use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

use afterlife_core::{
    MAX_ARTIFACT_BLOB_BYTES, OwnerId, format_blob_uri, is_valid_lower_sha256,
    sanitize_owner_segment,
};
use afterlife_protocol::{
    ArtifactBlobStore, BlobRef, EventBus, EventRecord, PortError, TelemetrySink,
};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

static BLOB_TMP_SEQ: AtomicU64 = AtomicU64::new(1);

pub struct LocalFsBlobStore {
    root: PathBuf,
}

impl LocalFsBlobStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, PortError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .map_err(|e| PortError::Storage(format!("mkdir {}: {e}", root.display())))?;
        Ok(Self { root })
    }
}

impl ArtifactBlobStore for LocalFsBlobStore {
    fn put_blob(
        &self,
        owner_id: &OwnerId,
        filename: &str,
        bytes: &[u8],
    ) -> Result<BlobRef, PortError> {
        if bytes.len() > MAX_ARTIFACT_BLOB_BYTES {
            return Err(PortError::InvalidInput(format!(
                "artifact blob exceeds maximum size of {} bytes (got {} bytes)",
                MAX_ARTIFACT_BLOB_BYTES,
                bytes.len()
            )));
        }
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = format!("{:x}", hasher.finalize());

        let owner_seg = sanitize_owner_segment(owner_id.as_str());
        let owner_dir = self.root.join(&owner_seg);
        fs::create_dir_all(&owner_dir)
            .map_err(|e| PortError::Storage(format!("mkdir {}: {e}", owner_dir.display())))?;
        let blob_path = owner_dir.join(&digest);
        if !blob_path.exists() {
            let seq = BLOB_TMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let tmp_path = owner_dir.join(format!(".tmp-{}-{}-{seq}", std::process::id(), digest));
            let write_res = (|| -> std::io::Result<()> {
                let mut f = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&tmp_path)?;
                f.write_all(bytes)?;
                f.sync_all()?;
                Ok(())
            })();
            if let Err(e) = write_res {
                let _ = fs::remove_file(&tmp_path);
                return Err(PortError::Storage(format!(
                    "write {}: {e}",
                    tmp_path.display()
                )));
            }
            if let Err(e) = fs::rename(&tmp_path, &blob_path) {
                let _ = fs::remove_file(&tmp_path);
                if !blob_path.exists() {
                    return Err(PortError::Storage(format!(
                        "rename {} -> {}: {e}",
                        tmp_path.display(),
                        blob_path.display()
                    )));
                }
            }
        }
        Ok(BlobRef {
            uri: format_blob_uri(owner_id.as_str(), &digest, filename),
            sha256: digest,
            size_bytes: bytes.len() as u64,
        })
    }

    fn get_blob(&self, owner_id: &OwnerId, sha256: &str) -> Result<Option<Vec<u8>>, PortError> {
        if !is_valid_lower_sha256(sha256) {
            return Ok(None);
        }
        let blob_path = self
            .root
            .join(sanitize_owner_segment(owner_id.as_str()))
            .join(sha256);
        if !blob_path.exists() {
            return Ok(None);
        }
        let data = fs::read(&blob_path)
            .map_err(|e| PortError::Storage(format!("read {}: {e}", blob_path.display())))?;
        Ok(Some(data))
    }
}

pub struct JsonlTelemetrySink {
    root: PathBuf,
    write_lock: Mutex<()>,
}

impl JsonlTelemetrySink {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, PortError> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)
            .map_err(|e| PortError::Storage(format!("mkdir {}: {e}", root.display())))?;
        Ok(Self {
            root,
            write_lock: Mutex::new(()),
        })
    }

    fn utc_date_from_ms(ts_ms: u64) -> String {
        let days = (ts_ms / 86_400_000) as i64;
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = (z - era * 146_097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = y + if m <= 2 { 1 } else { 0 };
        format!("{year:04}-{m:02}-{d:02}")
    }
}

impl TelemetrySink for JsonlTelemetrySink {
    fn emit(&self, record: &EventRecord) -> Result<(), PortError> {
        let _guard = self.write_lock.lock().unwrap();
        let dt = Self::utc_date_from_ms(record.event.timestamp_ms());
        let owner_seg = sanitize_owner_segment(record.owner_id.as_str());
        let dir = self
            .root
            .join(format!("dt={dt}"))
            .join(format!("owner={owner_seg}"));
        fs::create_dir_all(&dir)
            .map_err(|e| PortError::Storage(format!("mkdir {}: {e}", dir.display())))?;
        let file_path = dir.join("events.jsonl");
        let line = serde_json::to_string(record).map_err(|e| PortError::Storage(e.to_string()))?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)
            .map_err(|e| PortError::Storage(format!("open {}: {e}", file_path.display())))?;
        writeln!(f, "{line}")
            .map_err(|e| PortError::Storage(format!("write {}: {e}", file_path.display())))?;
        Ok(())
    }
}

pub struct NoopTelemetrySink;

impl TelemetrySink for NoopTelemetrySink {
    fn emit(&self, _record: &EventRecord) -> Result<(), PortError> {
        Ok(())
    }
}

pub struct InMemoryEventBus {
    tx: broadcast::Sender<EventRecord>,
}

impl InMemoryEventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity.max(64));
        Self { tx }
    }
}

impl EventBus for InMemoryEventBus {
    fn publish(&self, record: EventRecord) {
        let _ = self.tx.send(record);
    }

    fn subscribe(&self) -> broadcast::Receiver<EventRecord> {
        self.tx.subscribe()
    }

    fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}
