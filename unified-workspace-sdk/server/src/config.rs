//! Server configuration

use serde::Deserialize;

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

    /// Workspace base directory for sandbox volumes
    #[serde(default = "default_workspace_dir")]
    pub workspace_dir: String,

    /// NFS mode: "embedded" or "system"
    #[serde(default = "default_nfs_mode")]
    pub nfs_mode: String,

    /// NFS port (for embedded mode)
    #[serde(default = "default_nfs_port")]
    pub nfs_port: u16,

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
        if let Ok(val) = std::env::var("WORKSPACE_NFS_MODE") {
            config.nfs_mode = val;
        }
        if let Ok(val) = std::env::var("WORKSPACE_NFS_PORT") {
            if let Ok(port) = val.parse() {
                config.nfs_port = port;
            }
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

        Ok(config)
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
            nfs_mode: default_nfs_mode(),
            nfs_port: default_nfs_port(),
            base_image: default_base_image(),
            max_idle_time: default_max_idle_time(),
            agent_timeout: default_agent_timeout(),
            agent_server_addr: default_agent_server_addr(),
        }
    }
}
