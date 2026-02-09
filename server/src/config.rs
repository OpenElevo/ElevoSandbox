//! Server configuration

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::warn;

/// Server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// HTTP server host
    #[serde(default = "default_http_host")]
    pub http_host: String,

    /// HTTP server port
    #[serde(default = "default_http_port")]
    pub http_port: u16,

    /// gRPC server host
    #[serde(default = "default_grpc_host")]
    pub grpc_host: String,

    /// gRPC server port
    #[serde(default = "default_grpc_port")]
    pub grpc_port: u16,

    /// Database URL (SQLite)
    #[serde(default = "default_database_url")]
    pub database_url: String,

    /// Docker socket path
    #[serde(default = "default_docker_socket")]
    pub docker_socket: String,

    /// Workspace base directory for sandbox volumes (path inside server container)
    /// Also used by SandboxService for Docker bind mount paths.
    #[serde(default = "default_workspace_dir")]
    pub workspace_dir: String,

    /// Host path for workspace directory (for Docker volume mounting)
    /// When server runs in Docker, this is the path on the host machine
    /// that maps to workspace_dir inside the server container.
    /// Sandbox containers will mount from this host path.
    #[serde(default)]
    pub workspace_host_dir: Option<String>,

    /// NFS mode: "embedded" or "system"
    #[serde(default = "default_nfs_mode")]
    pub nfs_mode: String,

    /// NFS port (for embedded mode)
    #[serde(default = "default_nfs_port")]
    pub nfs_port: u16,

    /// NFS host address for external access (used in nfs_url)
    /// If not set, defaults to 127.0.0.1
    #[serde(default)]
    pub nfs_host: Option<String>,

    /// Base image for sandboxes
    #[serde(default = "default_base_image")]
    pub base_image: String,

    /// Maximum sandbox idle time in seconds
    #[serde(default = "default_max_idle_time")]
    pub max_idle_time: u64,

    /// Agent connection timeout in seconds
    #[serde(default = "default_agent_timeout")]
    pub agent_timeout: u64,

    /// Server address that agents should connect to (from inside containers)
    #[serde(default = "default_agent_server_addr")]
    pub agent_server_addr: String,

    /// Docker network name for sandbox containers
    /// If set, sandbox containers will be attached to this network
    #[serde(default)]
    pub docker_network: Option<String>,

    /// Extra hosts to add to sandbox containers (e.g., "host.docker.internal:host-gateway")
    #[serde(default)]
    pub sandbox_extra_hosts: Vec<String>,

    /// MCP server mode: "disabled", "stdio", or "http"
    #[serde(default = "default_mcp_mode")]
    pub mcp_mode: String,

    /// MCP HTTP endpoint path (when mcp_mode is "http")
    #[serde(default = "default_mcp_path")]
    pub mcp_path: String,

    /// MCP profile: "executor", "developer", or "full"
    /// - executor: minimal (1 tool) - process_run only
    /// - developer: common dev tools (6 tools) - process_run + file ops
    /// - full: all tools (14 tools) - sandbox + process + file ops
    #[serde(default = "default_mcp_profile")]
    pub mcp_profile: String,

    /// Storage backend configuration
    #[serde(skip)]
    pub storage: StorageConfig,
}

/// Storage backend configuration
#[derive(Debug, Clone)]
pub enum StorageConfig {
    /// Local filesystem storage (default)
    Local {
        /// Workspace root directory
        workspace_dir: PathBuf,
    },
    /// S3 storage via s3fs-fuse
    S3 {
        /// Workspace root directory (also serves as s3fs-fuse mount point)
        workspace_dir: PathBuf,
        /// S3 connection configuration
        s3: S3Config,
    },
}

impl Default for StorageConfig {
    fn default() -> Self {
        StorageConfig::Local {
            workspace_dir: PathBuf::from(default_workspace_dir()),
        }
    }
}

impl StorageConfig {
    /// Get the workspace root directory (both modes have one)
    pub fn workspace_dir(&self) -> &Path {
        match self {
            StorageConfig::Local { workspace_dir } => workspace_dir,
            StorageConfig::S3 { workspace_dir, .. } => workspace_dir,
        }
    }
}

/// S3 connection configuration
#[derive(Debug, Clone)]
pub struct S3Config {
    /// S3 endpoint URL
    pub endpoint: String,
    /// S3 bucket name
    pub bucket: String,
    /// S3 access key (optional; falls back to env vars or IAM role)
    pub access_key: Option<String>,
    /// S3 secret key (optional; falls back to env vars or IAM role)
    pub secret_key: Option<String>,
    /// S3 region
    pub region: Option<String>,
    /// s3fs-fuse local cache directory (optional; enables read caching)
    pub cache_dir: Option<PathBuf>,
}

fn default_http_host() -> String {
    "0.0.0.0".to_string()
}

fn default_http_port() -> u16 {
    8080
}

fn default_grpc_host() -> String {
    "0.0.0.0".to_string()
}

fn default_grpc_port() -> u16 {
    9090
}

fn default_database_url() -> String {
    "sqlite:data/workspace.db?mode=rwc".to_string()
}

fn default_docker_socket() -> String {
    "/var/run/docker.sock".to_string()
}

fn default_workspace_dir() -> String {
    "/var/lib/workspace".to_string()
}

fn default_nfs_mode() -> String {
    "embedded".to_string()
}

fn default_nfs_port() -> u16 {
    2049
}

fn default_base_image() -> String {
    "workspace-base:latest".to_string()
}

fn default_max_idle_time() -> u64 {
    3600 // 1 hour
}

fn default_agent_timeout() -> u64 {
    30
}

fn default_agent_server_addr() -> String {
    // Default to docker bridge IP for Linux
    "http://172.17.0.1:9090".to_string()
}

fn default_mcp_mode() -> String {
    "disabled".to_string()
}

fn default_mcp_path() -> String {
    "/mcp".to_string()
}

fn default_mcp_profile() -> String {
    "developer".to_string()
}

impl Config {
    /// Load configuration from environment variables
    pub fn load() -> anyhow::Result<Self> {
        // Start with default config as base
        let mut config = Config::default();

        // Override with environment variables
        if let Ok(val) = std::env::var("WORKSPACE_HTTP_HOST") {
            config.http_host = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_HTTP_PORT") {
            if let Ok(port) = val.parse() {
                config.http_port = port;
            }
        }
        if let Ok(val) = std::env::var("WORKSPACE_GRPC_HOST") {
            config.grpc_host = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_GRPC_PORT") {
            if let Ok(port) = val.parse() {
                config.grpc_port = port;
            }
        }
        if let Ok(val) = std::env::var("WORKSPACE_DATABASE_URL") {
            config.database_url = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_DOCKER_SOCKET") {
            config.docker_socket = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_WORKSPACE_DIR") {
            config.workspace_dir = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_WORKSPACE_HOST_DIR") {
            config.workspace_host_dir = Some(val);
        }
        if let Ok(val) = std::env::var("WORKSPACE_NFS_MODE") {
            config.nfs_mode = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_NFS_PORT") {
            if let Ok(port) = val.parse() {
                config.nfs_port = port;
            }
        }
        if let Ok(val) = std::env::var("WORKSPACE_NFS_HOST") {
            config.nfs_host = Some(val);
        }
        if let Ok(val) = std::env::var("WORKSPACE_BASE_IMAGE") {
            config.base_image = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_MAX_IDLE_TIME") {
            if let Ok(time) = val.parse() {
                config.max_idle_time = time;
            }
        }
        if let Ok(val) = std::env::var("WORKSPACE_AGENT_TIMEOUT") {
            if let Ok(timeout) = val.parse() {
                config.agent_timeout = timeout;
            }
        }
        if let Ok(val) = std::env::var("WORKSPACE_AGENT_SERVER_ADDR") {
            config.agent_server_addr = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_DOCKER_NETWORK") {
            config.docker_network = Some(val);
        }
        if let Ok(val) = std::env::var("WORKSPACE_SANDBOX_EXTRA_HOSTS") {
            // Parse comma-separated list of extra hosts
            config.sandbox_extra_hosts = val
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        if let Ok(val) = std::env::var("WORKSPACE_MCP_MODE") {
            config.mcp_mode = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_MCP_PATH") {
            config.mcp_path = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_MCP_PROFILE") {
            config.mcp_profile = val;
        }

        // Build StorageConfig from environment variables
        config.storage = Self::load_storage_config(&config.workspace_dir)?;

        Ok(config)
    }

    /// Build storage configuration from environment variables
    fn load_storage_config(workspace_dir: &str) -> anyhow::Result<StorageConfig> {
        let storage_type = std::env::var("WORKSPACE_STORAGE_TYPE")
            .unwrap_or_else(|_| "local".to_string());

        let workspace_path = PathBuf::from(workspace_dir);

        // Validate: workspace_dir must be an absolute path
        if !workspace_path.is_absolute() {
            return Err(anyhow::anyhow!(
                "WORKSPACE_WORKSPACE_DIR must be an absolute path, got: {}",
                workspace_dir
            ));
        }

        match storage_type.as_str() {
            "s3" => {
                let endpoint = std::env::var("WORKSPACE_S3_ENDPOINT")
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "WORKSPACE_S3_ENDPOINT is required when WORKSPACE_STORAGE_TYPE=s3"
                        )
                    })?;

                let bucket = std::env::var("WORKSPACE_S3_BUCKET")
                    .map_err(|_| {
                        anyhow::anyhow!(
                            "WORKSPACE_S3_BUCKET is required when WORKSPACE_STORAGE_TYPE=s3"
                        )
                    })?;

                let access_key = std::env::var("WORKSPACE_S3_ACCESS_KEY").ok();
                let secret_key = std::env::var("WORKSPACE_S3_SECRET_KEY").ok();
                // Default region to us-east-1 as per design doc
                let region = std::env::var("WORKSPACE_S3_REGION")
                    .ok()
                    .or_else(|| Some("us-east-1".to_string()));
                let cache_dir = std::env::var("WORKSPACE_S3_CACHE_DIR")
                    .ok()
                    .map(PathBuf::from);

                // Validate: cache_dir must be an absolute path if specified
                if let Some(ref dir) = cache_dir {
                    if !dir.is_absolute() {
                        return Err(anyhow::anyhow!(
                            "WORKSPACE_S3_CACHE_DIR must be an absolute path, got: {}",
                            dir.display()
                        ));
                    }
                }

                // Warn if S3 credentials are not configured via environment variables
                // (user may rely on ~/.passwd-s3fs or IAM role instead)
                if access_key.is_none() || secret_key.is_none() {
                    warn!(
                        "WORKSPACE_S3_ACCESS_KEY and/or WORKSPACE_S3_SECRET_KEY not set. \
                         s3fs will fall back to ~/.passwd-s3fs file or IAM role for credentials."
                    );
                }

                Ok(StorageConfig::S3 {
                    workspace_dir: workspace_path,
                    s3: S3Config {
                        endpoint,
                        bucket,
                        access_key,
                        secret_key,
                        region,
                        cache_dir,
                    },
                })
            }
            _ => Ok(StorageConfig::Local {
                workspace_dir: workspace_path,
            }),
        }
    }

    /// Get the host path for a sandbox workspace directory
    /// This is the path that should be mounted into sandbox containers
    pub fn get_sandbox_workspace_host_path(&self, sandbox_id: &str) -> String {
        let base = self
            .workspace_host_dir
            .as_deref()
            .unwrap_or(&self.workspace_dir);
        format!("{}/{}", base, sandbox_id)
    }

    /// Get the NFS host address for external access
    pub fn get_nfs_host(&self) -> &str {
        self.nfs_host.as_deref().unwrap_or("127.0.0.1")
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            http_host: default_http_host(),
            http_port: default_http_port(),
            grpc_host: default_grpc_host(),
            grpc_port: default_grpc_port(),
            database_url: default_database_url(),
            docker_socket: default_docker_socket(),
            workspace_dir: default_workspace_dir(),
            workspace_host_dir: None,
            nfs_mode: default_nfs_mode(),
            nfs_port: default_nfs_port(),
            nfs_host: None,
            base_image: default_base_image(),
            max_idle_time: default_max_idle_time(),
            agent_timeout: default_agent_timeout(),
            agent_server_addr: default_agent_server_addr(),
            docker_network: None,
            sandbox_extra_hosts: Vec::new(),
            mcp_mode: default_mcp_mode(),
            mcp_path: default_mcp_path(),
            mcp_profile: default_mcp_profile(),
            storage: StorageConfig::default(),
        }
    }
}
