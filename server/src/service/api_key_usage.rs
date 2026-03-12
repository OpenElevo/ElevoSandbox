//! API Key last_used_at batching tracker
//!
//! Batches last_used_at updates to reduce database write load.
//! Instead of writing to the DB on every API request, it coalesces
//! updates per key and flushes every 60 seconds (or on shutdown).

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::warn;
use uuid::Uuid;

use crate::infra::tenant_repository::TenantRepository;

/// Minimum interval between DB writes for the same key
const FLUSH_INTERVAL: Duration = Duration::from_secs(60);

/// Background flush interval
const BACKGROUND_FLUSH_INTERVAL: Duration = Duration::from_secs(30);

/// Entry tracking per-key usage state
struct UsageEntry {
    /// Last time we wrote to DB
    last_flushed: Instant,
    /// Whether this key has been used since the last flush
    dirty: bool,
}

/// Tracks API key usage and batches last_used_at DB updates.
pub struct ApiKeyUsageTracker {
    /// Maps key_id → usage state
    entries: DashMap<Uuid, UsageEntry>,
    repository: TenantRepository,
}

impl ApiKeyUsageTracker {
    pub fn new(repository: TenantRepository) -> Self {
        Self {
            entries: DashMap::new(),
            repository,
        }
    }

    /// Record a usage event for a key. Only writes to DB if enough time
    /// has passed since the last flush for this key.
    pub fn update(&self, key_id: Uuid) {
        let now = Instant::now();

        let should_flush = match self.entries.get(&key_id) {
            Some(entry) => now.duration_since(entry.last_flushed) >= FLUSH_INTERVAL,
            None => true,
        };

        if should_flush {
            self.entries.insert(key_id, UsageEntry {
                last_flushed: now,
                dirty: false,
            });
            let repo = self.repository.clone();
            tokio::spawn(async move {
                if let Err(e) = repo.update_last_used(key_id).await {
                    warn!("Failed to update API key last_used_at: {}", e);
                }
            });
        } else {
            // Mark dirty — the background flush will pick it up
            if let Some(mut entry) = self.entries.get_mut(&key_id) {
                entry.dirty = true;
            }
        }
    }

    /// Flush only dirty entries (keys used since last flush) to the database.
    pub async fn flush_all(&self) {
        let dirty_keys: Vec<Uuid> = self
            .entries
            .iter()
            .filter(|e| e.dirty)
            .map(|e| *e.key())
            .collect();

        for key_id in dirty_keys {
            if let Err(e) = self.repository.update_last_used(key_id).await {
                warn!("Failed to flush API key last_used_at for {}: {}", key_id, e);
            }
            // Mark as flushed
            if let Some(mut entry) = self.entries.get_mut(&key_id) {
                entry.last_flushed = Instant::now();
                entry.dirty = false;
            }
        }
    }

    /// Start the background flush task. Returns a handle that can be used to stop it.
    pub fn start_background_flush(self: &Arc<Self>) -> tokio::task::JoinHandle<()> {
        let tracker = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(BACKGROUND_FLUSH_INTERVAL);
            loop {
                interval.tick().await;
                tracker.flush_all().await;
            }
        })
    }
}
