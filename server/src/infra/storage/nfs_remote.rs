//! NFS remote mount management
//!
//! Manages NFS mounts from Client-exported NFS shares. When a remote workspace
//! switches from gRPC to NFS transport, the Server mounts the Client's NFS share
//! locally and uses a `LocalStorageBackend` on the mount point.

use std::path::{Path, PathBuf};

use tokio::process::Command;
use tracing::{info, warn};

/// NFS remote mount error
#[derive(Debug, thiserror::Error)]
pub enum NfsRemoteMountError {
    #[error("invalid NFS URL: {0}")]
    InvalidUrl(String),

    #[error("NFS host not in allowed CIDR: {host}")]
    HostNotAllowed { host: String },

    #[error("NFS mount failed: {0}")]
    MountFailed(String),

    #[error("NFS unmount failed: {0}")]
    UnmountFailed(String),

    #[error("DNS resolution failed for {host}: {reason}")]
    DnsResolutionFailed { host: String, reason: String },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Parsed NFS URL components
pub struct NfsUrl {
    /// NFS server hostname or IP
    pub host: String,
    /// Export path on the NFS server
    pub path: String,
    /// Port number (default 2049)
    pub port: u16,
}

/// Parse an NFS URL like "nfs://host:port/path" or "host:/path"
fn parse_nfs_url(url: &str) -> Result<NfsUrl, NfsRemoteMountError> {
    if let Some(stripped) = url.strip_prefix("nfs://") {
        // nfs://host:port/path format
        let (host_port, path) = stripped
            .split_once('/')
            .ok_or_else(|| NfsRemoteMountError::InvalidUrl("missing path in NFS URL".into()))?;

        let (host, port) = if let Some((h, p)) = host_port.rsplit_once(':') {
            let port: u16 = p
                .parse()
                .map_err(|_| NfsRemoteMountError::InvalidUrl(format!("invalid port: {}", p)))?;
            if port == 0 {
                return Err(NfsRemoteMountError::InvalidUrl(
                    "port 0 is not allowed".into(),
                ));
            }
            (h.to_string(), port)
        } else {
            (host_port.to_string(), 2049)
        };

        if host.is_empty() {
            return Err(NfsRemoteMountError::InvalidUrl("empty hostname".into()));
        }

        Ok(NfsUrl {
            host,
            path: format!("/{}", path),
            port,
        })
    } else if let Some((host, path)) = url.split_once(':') {
        // Traditional host:/path format
        if host.is_empty() || !path.starts_with('/') {
            return Err(NfsRemoteMountError::InvalidUrl(
                "expected host:/path format".into(),
            ));
        }
        Ok(NfsUrl {
            host: host.to_string(),
            path: path.to_string(),
            port: 2049,
        })
    } else {
        Err(NfsRemoteMountError::InvalidUrl(
            "unrecognized NFS URL format".into(),
        ))
    }
}

/// Manages NFS mounts from Client-exported NFS shares.
pub struct RemoteNfsMountManager {
    /// Base directory for NFS mount points
    workspace_dir: PathBuf,
    /// Allowed CIDR ranges for NFS hosts
    allowed_cidrs: Vec<ipnet::IpNet>,
}

impl RemoteNfsMountManager {
    pub fn new(workspace_dir: PathBuf, allowed_cidrs: Vec<ipnet::IpNet>) -> Self {
        Self {
            workspace_dir,
            allowed_cidrs,
        }
    }

    /// Get the base workspace directory path.
    pub fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    /// Validate an NFS URL: parse, resolve DNS, check CIDR whitelist.
    pub async fn validate_nfs_url(&self, nfs_url: &str) -> Result<NfsUrl, NfsRemoteMountError> {
        let parsed = parse_nfs_url(nfs_url)?;

        // Resolve hostname to IP addresses
        let addrs: Vec<std::net::IpAddr> =
            tokio::net::lookup_host(format!("{}:{}", parsed.host, parsed.port))
                .await
                .map_err(|e| NfsRemoteMountError::DnsResolutionFailed {
                    host: parsed.host.clone(),
                    reason: e.to_string(),
                })?
                .map(|addr| addr.ip())
                .collect();

        if addrs.is_empty() {
            return Err(NfsRemoteMountError::DnsResolutionFailed {
                host: parsed.host.clone(),
                reason: "no addresses resolved".into(),
            });
        }

        // Check that at least one resolved address is in the CIDR whitelist.
        // Using any() instead of all() to support dual-stack hosts (e.g., hosts
        // with both private and public IPs where only the private range is allowed).
        if !self.allowed_cidrs.is_empty() {
            let any_allowed = addrs
                .iter()
                .any(|ip| self.allowed_cidrs.iter().any(|cidr| cidr.contains(ip)));
            if !any_allowed {
                return Err(NfsRemoteMountError::HostNotAllowed {
                    host: parsed.host.clone(),
                });
            }
        }

        Ok(parsed)
    }

    /// Mount NFS share for a workspace.
    pub async fn mount(
        &self,
        workspace_id: &str,
        nfs_url: &str,
    ) -> Result<PathBuf, NfsRemoteMountError> {
        let parsed = self.validate_nfs_url(nfs_url).await?;

        let mount_point = self.workspace_dir.join(workspace_id);

        // Create mount point directory
        tokio::fs::create_dir_all(&mount_point).await?;

        let nfs_source = format!("{}:{}", parsed.host, parsed.path);
        let port_opt = format!("port={}", parsed.port);

        let output = Command::new("mount")
            .args([
                "-t",
                "nfs",
                "-o",
                &format!("nosuid,nodev,soft,timeo=300,retry=0,{}", port_opt),
                &nfs_source,
                &mount_point.display().to_string(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NfsRemoteMountError::MountFailed(format!(
                "mount -t nfs {} {} failed: {}",
                nfs_source,
                mount_point.display(),
                stderr
            )));
        }

        // Verify mount by stat-ing the mount point
        match tokio::fs::metadata(&mount_point).await {
            Ok(m) if m.is_dir() => {}
            _ => {
                // Mount succeeded but can't access: unmount and fail
                let _ = self.umount(workspace_id).await;
                return Err(NfsRemoteMountError::MountFailed(
                    "mount succeeded but mount point is not accessible".into(),
                ));
            }
        }

        info!(
            workspace_id = %workspace_id,
            source = %nfs_source,
            mount_point = %mount_point.display(),
            "NFS mounted"
        );

        Ok(mount_point)
    }

    /// Mount NFS share at a temporary path for channel switching.
    pub async fn mount_temp(
        &self,
        workspace_id: &str,
        nfs_url: &str,
    ) -> Result<PathBuf, NfsRemoteMountError> {
        let parsed = self.validate_nfs_url(nfs_url).await?;

        let temp_dir = self
            .workspace_dir
            .join(format!(".nfs-temp-{}", workspace_id));

        // Create temp mount point
        tokio::fs::create_dir_all(&temp_dir).await?;

        let nfs_source = format!("{}:{}", parsed.host, parsed.path);
        let port_opt = format!("port={}", parsed.port);

        let output = Command::new("mount")
            .args([
                "-t",
                "nfs",
                "-o",
                &format!("nosuid,nodev,soft,timeo=300,retry=0,{}", port_opt),
                &nfs_source,
                &temp_dir.display().to_string(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // Clean up temp dir
            let _ = tokio::fs::remove_dir(&temp_dir).await;
            return Err(NfsRemoteMountError::MountFailed(format!(
                "mount -t nfs {} {} failed: {}",
                nfs_source,
                temp_dir.display(),
                stderr
            )));
        }

        Ok(temp_dir)
    }

    /// Unmount NFS share for a workspace.
    pub async fn umount(&self, workspace_id: &str) -> Result<(), NfsRemoteMountError> {
        let mount_point = self.workspace_dir.join(workspace_id);
        self.umount_path(&mount_point).await
    }

    /// Unmount a specific path.
    pub async fn umount_path(&self, mount_point: &Path) -> Result<(), NfsRemoteMountError> {
        let mp_str = mount_point.display().to_string();

        let output = Command::new("umount").arg(&mp_str).output().await?;

        if !output.status.success() {
            // Try lazy unmount as fallback
            warn!(
                mount_point = %mp_str,
                "umount failed, trying lazy unmount"
            );
            let output = Command::new("umount")
                .args(["-l", &mp_str])
                .output()
                .await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(NfsRemoteMountError::UnmountFailed(format!(
                    "umount -l {} failed: {}",
                    mp_str, stderr
                )));
            }
        }

        info!(mount_point = %mp_str, "NFS unmounted");
        Ok(())
    }

    /// Check if a workspace has an active NFS mount by inspecting /proc/mounts.
    pub async fn is_mounted(&self, workspace_id: &str) -> bool {
        let mount_point = self.workspace_dir.join(workspace_id);
        self.is_path_mounted(&mount_point).await
    }

    /// Check if a specific path has an active mount.
    pub async fn is_path_mounted(&self, mount_point: &Path) -> bool {
        let mp_str = mount_point.display().to_string();
        match tokio::fs::read_to_string("/proc/mounts").await {
            Ok(content) => content.lines().any(|line| {
                line.split_whitespace()
                    .nth(1)
                    .map(|mp| mp == mp_str)
                    .unwrap_or(false)
            }),
            Err(_) => false,
        }
    }

    /// Atomically move a mount from one path to another using `mount --move`.
    pub async fn mount_move(&self, from: &Path, to: &Path) -> Result<(), NfsRemoteMountError> {
        let output = Command::new("mount")
            .args([
                "--move",
                &from.display().to_string(),
                &to.display().to_string(),
            ])
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(NfsRemoteMountError::MountFailed(format!(
                "mount --move {} {} failed: {}",
                from.display(),
                to.display(),
                stderr
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nfs_url_standard() {
        let url = parse_nfs_url("nfs://192.168.1.100/exports/workspace").unwrap();
        assert_eq!(url.host, "192.168.1.100");
        assert_eq!(url.path, "/exports/workspace");
        assert_eq!(url.port, 2049);
    }

    #[test]
    fn test_parse_nfs_url_with_port() {
        let url = parse_nfs_url("nfs://192.168.1.100:3049/exports/workspace").unwrap();
        assert_eq!(url.host, "192.168.1.100");
        assert_eq!(url.path, "/exports/workspace");
        assert_eq!(url.port, 3049);
    }

    #[test]
    fn test_parse_nfs_url_traditional() {
        let url = parse_nfs_url("192.168.1.100:/exports/workspace").unwrap();
        assert_eq!(url.host, "192.168.1.100");
        assert_eq!(url.path, "/exports/workspace");
        assert_eq!(url.port, 2049);
    }

    #[test]
    fn test_parse_nfs_url_zero_port_rejected() {
        let result = parse_nfs_url("nfs://192.168.1.100:0/exports/workspace");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_nfs_url_empty_host_rejected() {
        let result = parse_nfs_url("nfs:///exports/workspace");
        assert!(result.is_err());
    }
}
