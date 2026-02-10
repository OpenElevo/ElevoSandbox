//! Prometheus metrics for the workspace server
//!
//! Exposes metrics as defined in the storage-backend-ha design doc section 11.2.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};

/// Metric names (constants for consistency)
pub mod names {
    /// Storage operation duration histogram
    pub const STORAGE_OPERATION_DURATION: &str = "workspace_storage_operation_duration_seconds";
    /// Storage operation error counter
    pub const STORAGE_OPERATION_ERRORS: &str = "workspace_storage_operation_errors_total";
    /// S3fs mount status gauge (1=mounted, 0=unmounted/failed)
    pub const S3FS_MOUNT_STATUS: &str = "workspace_s3fs_mount_status";
    /// S3fs remount attempts counter
    pub const S3FS_REMOUNT_TOTAL: &str = "workspace_s3fs_remount_total";
    /// Number of active workspace leases held by this server
    pub const LEASE_ACTIVE: &str = "workspace_lease_active";
}

/// Initialize the Prometheus metrics exporter and describe all metrics.
///
/// Returns a `PrometheusHandle` that can be used to render metrics for scraping.
pub fn init_metrics() -> PrometheusHandle {
    let builder = PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Describe metrics with help text
    describe_histogram!(
        names::STORAGE_OPERATION_DURATION,
        "Duration of storage backend operations in seconds"
    );
    describe_counter!(
        names::STORAGE_OPERATION_ERRORS,
        "Total number of storage backend operation errors"
    );
    describe_gauge!(
        names::S3FS_MOUNT_STATUS,
        "S3fs mount status (1=healthy, 0=unhealthy)"
    );
    describe_counter!(
        names::S3FS_REMOUNT_TOTAL,
        "Total number of s3fs remount attempts"
    );
    describe_gauge!(
        names::LEASE_ACTIVE,
        "Number of workspace leases actively held by this server instance"
    );

    handle
}

/// Record a storage operation duration.
///
/// # Arguments
/// * `operation` - The operation name (e.g., "read_file", "write_file", "stat")
/// * `duration_secs` - The duration in seconds
#[allow(dead_code)]
pub fn record_storage_operation(operation: &str, duration_secs: f64) {
    histogram!(names::STORAGE_OPERATION_DURATION, "operation" => operation.to_string())
        .record(duration_secs);
}

/// Record a storage operation error.
///
/// # Arguments
/// * `operation` - The operation name
/// * `error_type` - The error type (e.g., "not_found", "permission_denied")
#[allow(dead_code)]
pub fn record_storage_error(operation: &str, error_type: &str) {
    counter!(
        names::STORAGE_OPERATION_ERRORS,
        "operation" => operation.to_string(),
        "error_type" => error_type.to_string()
    )
    .increment(1);
}

/// Set the s3fs mount status.
///
/// # Arguments
/// * `is_healthy` - true if mounted and healthy, false otherwise
pub fn set_s3fs_mount_status(is_healthy: bool) {
    gauge!(names::S3FS_MOUNT_STATUS).set(if is_healthy { 1.0 } else { 0.0 });
}

/// Increment the s3fs remount counter.
pub fn increment_s3fs_remount() {
    counter!(names::S3FS_REMOUNT_TOTAL).increment(1);
}

/// Set the number of active leases.
///
/// # Arguments
/// * `count` - Number of active leases held by this server
pub fn set_active_lease_count(count: u64) {
    gauge!(names::LEASE_ACTIVE).set(count as f64);
}

/// RAII guard for timing storage operations.
///
/// Records the duration when dropped.
#[allow(dead_code)]
pub struct StorageOperationTimer {
    operation: &'static str,
    start: Instant,
}

impl StorageOperationTimer {
    /// Start timing an operation.
    #[allow(dead_code)]
    pub fn new(operation: &'static str) -> Self {
        Self {
            operation,
            start: Instant::now(),
        }
    }
}

impl Drop for StorageOperationTimer {
    fn drop(&mut self) {
        let duration = self.start.elapsed();
        record_storage_operation(self.operation, duration.as_secs_f64());
    }
}

/// Convert a `StorageError` to an error type label for metrics.
#[allow(dead_code)]
pub fn storage_error_to_label(err: &crate::infra::storage::StorageError) -> &'static str {
    use crate::infra::storage::StorageError;
    match err {
        StorageError::NotFound(_) => "not_found",
        StorageError::AlreadyExists(_) => "already_exists",
        StorageError::IsADirectory(_) => "is_a_directory",
        StorageError::NotADirectory(_) => "not_a_directory",
        StorageError::NotAFile(_) => "not_a_file",
        StorageError::DirectoryNotEmpty(_) => "directory_not_empty",
        StorageError::PermissionDenied(_) => "permission_denied",
        StorageError::PathTraversalDenied(_) => "path_traversal_denied",
        StorageError::NotSupported(_) => "not_supported",
        StorageError::Io { .. } => "io_error",
        StorageError::Internal(_) => "internal_error",
    }
}

/// Tracks and reports active lease count.
///
/// Shared between WorkspaceService and the lease renewal task.
#[derive(Clone)]
#[allow(dead_code)]
pub struct LeaseMetrics {
    count: Arc<AtomicU64>,
}

#[allow(dead_code)]
impl LeaseMetrics {
    pub fn new() -> Self {
        Self {
            count: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Increment the lease count and update the gauge.
    pub fn lease_acquired(&self) {
        let new_count = self.count.fetch_add(1, Ordering::SeqCst) + 1;
        set_active_lease_count(new_count);
    }

    /// Decrement the lease count and update the gauge.
    pub fn lease_released(&self) {
        // Use compare_exchange loop to prevent underflow
        loop {
            let current = self.count.load(Ordering::SeqCst);
            if current == 0 {
                // Already at zero, nothing to decrement
                set_active_lease_count(0);
                return;
            }
            match self.count.compare_exchange(
                current,
                current - 1,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => {
                    set_active_lease_count(current - 1);
                    return;
                }
                Err(_) => {
                    // Another thread modified the value, retry
                    continue;
                }
            }
        }
    }

    /// Get the current lease count.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }
}

impl Default for LeaseMetrics {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_metrics() {
        let metrics = LeaseMetrics::new();
        assert_eq!(metrics.count(), 0);

        metrics.lease_acquired();
        assert_eq!(metrics.count(), 1);

        metrics.lease_acquired();
        assert_eq!(metrics.count(), 2);

        metrics.lease_released();
        assert_eq!(metrics.count(), 1);

        metrics.lease_released();
        assert_eq!(metrics.count(), 0);

        // Test underflow protection
        metrics.lease_released();
        assert_eq!(metrics.count(), 0); // Should stay at 0, not underflow
    }

    #[test]
    fn test_storage_operation_timer() {
        // Just verify it compiles and doesn't panic
        let _timer = StorageOperationTimer::new("test_op");
        std::thread::sleep(std::time::Duration::from_millis(10));
        // Timer will be dropped here and record the duration
    }
}
