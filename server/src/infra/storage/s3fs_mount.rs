//! S3fs-fuse mount management
//!
//! Manages the lifecycle of s3fs-fuse mounts for S3 storage backend integration.
//! In S3 mode, the workspace directory is backed by an S3 bucket mounted via s3fs-fuse.

use std::path::{Path, PathBuf};

use tokio::process::Command;

use crate::infra::metrics;

/// s3fs-fuse mount error types
#[derive(Debug, thiserror::Error)]
pub enum S3fsMountError {
    #[error("s3fs mount failed: {stderr}")]
    MountFailed { stderr: String },

    #[error("s3fs mount verification failed after mount command succeeded")]
    MountVerificationFailed,

    #[error("s3fs unmount failed: {stderr}")]
    UnmountFailed { stderr: String },

    #[error("s3fs health check failed: {0}")]
    HealthCheckFailed(String),

    #[error("FUSE not available: {0}")]
    FuseNotAvailable(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// S3 credentials for s3fs-fuse authentication
pub struct S3Credentials {
    pub access_key: String,
    pub secret_key: String,
}

/// s3fs-fuse mount manager
///
/// Handles mounting/unmounting S3 buckets as local POSIX filesystems via s3fs-fuse.
/// After mounting, a `LocalStorageBackend` pointed at the mount point provides
/// transparent S3 access through the standard storage interface.
pub struct S3fsMountManager {
    /// Mount point path (the workspace root directory)
    mount_point: PathBuf,
    /// S3 bucket name
    bucket: String,
    /// S3 endpoint URL
    endpoint: String,
    /// S3 region (optional; required for AWS S3, ignored for MinIO etc.)
    region: Option<String>,
    /// S3 credentials (optional; falls back to env vars or IAM role)
    credentials: Option<S3Credentials>,
    /// Local file cache directory for s3fs-fuse (optional)
    cache_dir: Option<PathBuf>,
}

impl S3fsMountManager {
    pub fn new(
        mount_point: PathBuf,
        bucket: String,
        endpoint: String,
        region: Option<String>,
        credentials: Option<S3Credentials>,
        cache_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            mount_point,
            bucket,
            endpoint,
            region,
            credentials,
            cache_dir,
        }
    }

    /// Get the mount point path
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Execute the s3fs mount
    ///
    /// Checks if already mounted, creates directory if needed, and runs s3fs.
    pub async fn mount(&self) -> Result<(), S3fsMountError> {
        if self.is_mounted().await {
            tracing::info!(
                mount_point = %self.mount_point.display(),
                "s3fs already mounted"
            );
            return Ok(());
        }

        // Check FUSE availability before attempting mount
        self.check_fuse_available().await?;

        // Ensure mount point directory exists
        tokio::fs::create_dir_all(&self.mount_point).await?;

        // Prepare credentials file if configured
        let passwd_file = self.prepare_credentials().await?;

        // Build s3fs command
        let mut cmd = Command::new("s3fs");
        cmd.arg(&self.bucket)
            .arg(&self.mount_point)
            .arg("-o")
            .arg(format!("url={}", self.endpoint))
            .arg("-o")
            .arg("use_path_request_style")
            .arg("-o")
            .arg("allow_other");

        // s3fs uses "endpoint=<region>" to specify the AWS region
        // (e.g., "us-east-1"). For non-AWS endpoints this is typically not needed.
        if let Some(ref region) = self.region {
            cmd.arg("-o").arg(format!("endpoint={}", region));
        }

        if let Some(ref passwd_path) = passwd_file {
            cmd.arg("-o")
                .arg(format!("passwd_file={}", passwd_path.display()));
        }

        if let Some(ref cache_dir) = self.cache_dir {
            tokio::fs::create_dir_all(cache_dir).await?;
            cmd.arg("-o")
                .arg(format!("use_cache={}", cache_dir.display()));
        }

        // Enable s3fs logging for troubleshooting
        let log_dir = self
            .mount_point
            .parent()
            .unwrap_or(std::path::Path::new("/var/log"));
        let log_file = log_dir.join("s3fs.log");
        cmd.arg("-o")
            .arg(format!("logfile={}", log_file.display()))
            .arg("-o")
            .arg("dbglevel=info");

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(S3fsMountError::MountFailed {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        // Verify mount actually succeeded
        if !self.is_mounted().await {
            return Err(S3fsMountError::MountVerificationFailed);
        }

        tracing::info!(
            mount_point = %self.mount_point.display(),
            log_file = %log_file.display(),
            "s3fs mounted successfully"
        );
        Ok(())
    }

    /// Unmount s3fs
    pub async fn unmount(&self) -> Result<(), S3fsMountError> {
        if !self.is_mounted().await {
            return Ok(());
        }

        let output = Command::new("fusermount")
            .arg("-u")
            .arg(&self.mount_point)
            .output()
            .await?;

        if !output.status.success() {
            return Err(S3fsMountError::UnmountFailed {
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
        }

        tracing::info!(
            mount_point = %self.mount_point.display(),
            "s3fs unmounted"
        );
        Ok(())
    }

    /// Check whether the mount point is currently mounted
    ///
    /// Reads `/proc/mounts` to verify actual mount status rather than
    /// simply checking directory existence.
    pub async fn is_mounted(&self) -> bool {
        let mount_point_str = self.mount_point.to_string_lossy();
        match tokio::fs::read_to_string("/proc/mounts").await {
            Ok(mounts) => mounts.lines().any(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                parts.len() >= 2 && parts[1] == mount_point_str.as_ref()
            }),
            Err(_) => false,
        }
    }

    /// Clean up stale FUSE mounts from a previous abnormal exit
    ///
    /// Called during startup. If the mount point exists but is not accessible
    /// (stale FUSE mount from a SIGKILL), perform lazy unmount to clean it up.
    pub async fn cleanup_stale_mount(&self) -> Result<(), S3fsMountError> {
        if !self.mount_point.exists() {
            return Ok(());
        }

        // Try to access the mount point — if it fails, it's stale
        match tokio::fs::read_dir(&self.mount_point).await {
            Ok(_) => Ok(()),
            Err(e)
                if e.kind() == std::io::ErrorKind::Other
                    || e.raw_os_error() == Some(libc::ENOTCONN)
                    || e.raw_os_error() == Some(libc::EACCES) =>
            {
                tracing::warn!(
                    mount_point = %self.mount_point.display(),
                    "detected stale mount, cleaning up"
                );
                // Lazy unmount to clean up the stale mount
                let _ = Command::new("fusermount")
                    .arg("-uz")
                    .arg(&self.mount_point)
                    .output()
                    .await;
                Ok(())
            }
            Err(_) => Ok(()),
        }
    }

    /// Startup health check: verify the mount point is readable and writable
    pub async fn health_check(&self) -> Result<(), S3fsMountError> {
        let test_file = self.mount_point.join(".workspace_health_check");

        // Write test file
        tokio::fs::write(&test_file, b"ok")
            .await
            .map_err(|e| S3fsMountError::HealthCheckFailed(format!("write failed: {}", e)))?;

        // Read it back
        let content = tokio::fs::read(&test_file)
            .await
            .map_err(|e| S3fsMountError::HealthCheckFailed(format!("read failed: {}", e)))?;

        if content != b"ok" {
            return Err(S3fsMountError::HealthCheckFailed(
                "read-back content mismatch".to_string(),
            ));
        }

        // Clean up test file
        let _ = tokio::fs::remove_file(&test_file).await;

        tracing::info!("s3fs health check passed");
        Ok(())
    }

    /// Check whether FUSE is available on this system.
    ///
    /// Verifies `/dev/fuse` exists. Without it, s3fs-fuse cannot function.
    async fn check_fuse_available(&self) -> Result<(), S3fsMountError> {
        if tokio::fs::symlink_metadata("/dev/fuse").await.is_err() {
            return Err(S3fsMountError::FuseNotAvailable(
                "/dev/fuse not found. Please ensure the FUSE kernel module is loaded \
                 (modprobe fuse) and /dev/fuse is accessible."
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// Prepare the credentials file for s3fs
    ///
    /// Writes the access_key:secret_key to a temporary file with 0o600 permissions,
    /// as required by s3fs-fuse.
    async fn prepare_credentials(&self) -> Result<Option<PathBuf>, S3fsMountError> {
        let Some(ref creds) = self.credentials else {
            return Ok(None);
        };

        let passwd_path = self
            .mount_point
            .parent()
            .unwrap_or(Path::new("/tmp"))
            .join(".s3fs_passwd");

        let content = format!("{}:{}", creds.access_key, creds.secret_key);
        tokio::fs::write(&passwd_path, content.as_bytes()).await?;

        // Set 600 permissions (s3fs requirement)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&passwd_path, perms).await?;
        }

        Ok(Some(passwd_path))
    }
}

/// Background monitor that periodically checks s3fs mount health
/// and attempts auto-remount on failure.
///
/// Updates Prometheus metrics:
/// - `workspace_s3fs_mount_status`: 1 if healthy, 0 if unhealthy
/// - `workspace_s3fs_remount_total`: incremented on each remount attempt
pub struct S3fsMountMonitor {
    mount_manager: std::sync::Arc<S3fsMountManager>,
    check_interval: std::time::Duration,
}

impl S3fsMountMonitor {
    pub fn new(
        mount_manager: std::sync::Arc<S3fsMountManager>,
        check_interval: std::time::Duration,
    ) -> Self {
        // Set initial mount status to healthy (we just mounted successfully)
        metrics::set_s3fs_mount_status(true);

        Self {
            mount_manager,
            check_interval,
        }
    }

    /// Start the background monitoring task
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(self.check_interval);
            loop {
                interval.tick().await;

                // Check /proc/mounts for active mount
                let is_mounted = self.mount_manager.is_mounted().await;
                if !is_mounted {
                    tracing::error!("s3fs mount lost! attempting remount...");
                    metrics::set_s3fs_mount_status(false);

                    if let Err(e) = self.mount_manager.cleanup_stale_mount().await {
                        tracing::error!("stale mount cleanup failed: {}", e);
                    }

                    metrics::increment_s3fs_remount();
                    match self.mount_manager.mount().await {
                        Ok(()) => {
                            tracing::info!("s3fs remount successful");
                            metrics::set_s3fs_mount_status(true);
                        }
                        Err(e) => {
                            tracing::error!("remount failed: {}", e);
                            // Status remains unhealthy
                        }
                    }
                    continue;
                }

                // Verify the mount point is actually accessible
                if tokio::fs::read_dir(self.mount_manager.mount_point())
                    .await
                    .is_err()
                {
                    tracing::error!("s3fs mount point not accessible!");
                    metrics::set_s3fs_mount_status(false);

                    let _ = self.mount_manager.cleanup_stale_mount().await;

                    metrics::increment_s3fs_remount();
                    match self.mount_manager.mount().await {
                        Ok(()) => {
                            tracing::info!("s3fs remount after stale cleanup successful");
                            metrics::set_s3fs_mount_status(true);
                        }
                        Err(e) => {
                            tracing::error!("remount after stale cleanup failed: {}", e);
                            // Status remains unhealthy
                        }
                    }
                } else {
                    // Mount is healthy
                    metrics::set_s3fs_mount_status(true);
                }
            }
        })
    }
}
