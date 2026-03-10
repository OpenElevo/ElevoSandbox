//! Command-line interface for workspace-fuse
//!
//! Provides mount and umount commands for managing FUSE mounts.

use std::io::{self, BufRead};
use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Environment variable for authentication token (recommended)
const TOKEN_ENV_VAR: &str = "WORKSPACE_FUSE_TOKEN";

/// Workspace FUSE client - mount remote workspaces as local directories
#[derive(Parser, Debug)]
#[command(name = "workspace-fuse")]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Subcommand to execute
    #[command(subcommand)]
    pub command: Command,
}

/// Available commands
#[derive(Subcommand, Debug)]
pub enum Command {
    /// Mount a workspace as a local directory
    Mount(MountArgs),

    /// Unmount a workspace
    Umount(UmountArgs),
}

/// Arguments for the mount command
#[derive(Parser, Debug)]
pub struct MountArgs {
    /// gRPC server address (e.g., http://localhost:9090)
    #[arg(short, long)]
    pub server: String,

    /// Workspace ID to mount
    #[arg(short, long)]
    pub workspace: String,

    /// Authentication token (not recommended, visible in ps output)
    /// Prefer using WORKSPACE_FUSE_TOKEN env var or --token-file
    #[arg(short, long)]
    pub token: Option<String>,

    /// Read authentication token from file (recommended for automation)
    #[arg(long, conflicts_with = "token")]
    pub token_file: Option<PathBuf>,

    /// Read authentication token from stdin
    #[arg(long, conflicts_with_all = ["token", "token_file"])]
    pub token_stdin: bool,

    /// Target mount point directory
    #[arg(short = 'm', long = "target")]
    pub target: PathBuf,

    /// Run in foreground (don't daemonize)
    #[arg(short, long, default_value = "false")]
    pub foreground: bool,

    /// Allow other users to access the mount
    #[arg(long, default_value = "false")]
    pub allow_other: bool,

    /// Allow root to access the mount
    #[arg(long, default_value = "false")]
    pub allow_root: bool,

    /// Mount as read-only
    #[arg(long, default_value = "false")]
    pub read_only: bool,

    /// Enable debug logging
    #[arg(short, long, default_value = "false")]
    pub debug: bool,

    /// Metadata cache TTL in seconds
    #[arg(long, default_value = "5")]
    pub cache_ttl: u64,

    /// Read cache size in megabytes
    #[arg(long, default_value = "64")]
    pub read_cache_size: u64,

    /// Read cache block size in bytes
    #[arg(long, default_value = "65536")]
    pub block_size: u32,
}

impl MountArgs {
    /// Resolve the authentication token from various sources
    ///
    /// Priority (highest to lowest):
    /// 1. WORKSPACE_FUSE_TOKEN environment variable
    /// 2. --token-file option
    /// 3. --token-stdin option
    /// 4. --token command line argument
    /// 5. Empty string (no authentication - for servers without auth)
    pub fn resolve_token(&self) -> io::Result<String> {
        // 1. Check environment variable first (highest priority, most secure)
        if let Ok(token) = std::env::var(TOKEN_ENV_VAR) {
            if !token.is_empty() {
                return Ok(token);
            }
        }

        // 2. Read from file
        if let Some(ref path) = self.token_file {
            let content = std::fs::read_to_string(path)?;
            let token = content.trim().to_string();
            if token.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "token file is empty",
                ));
            }
            return Ok(token);
        }

        // 3. Read from stdin
        if self.token_stdin {
            let stdin = io::stdin();
            let mut line = String::new();
            stdin.lock().read_line(&mut line)?;
            let token = line.trim().to_string();
            if token.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "no token provided on stdin",
                ));
            }
            return Ok(token);
        }

        // 4. Use command line argument (least secure)
        if let Some(ref token) = self.token {
            return Ok(token.clone());
        }

        // 5. No token provided - connect without authentication
        Ok(String::new())
    }
}

/// Arguments for the umount command
#[derive(Parser, Debug)]
pub struct UmountArgs {
    /// Mount point to unmount
    pub target: PathBuf,

    /// Lazy unmount (detach even if busy)
    #[arg(short, long, default_value = "false")]
    pub lazy: bool,

    /// Force unmount (may cause data loss)
    #[arg(short, long, default_value = "false")]
    pub force: bool,
}

impl Cli {
    /// Parse command line arguments
    pub fn parse_args() -> Self {
        Self::parse()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mount_args_with_token() {
        let args = Cli::try_parse_from([
            "workspace-fuse",
            "mount",
            "--server",
            "http://localhost:9090",
            "--workspace",
            "ws_123",
            "--token",
            "secret",
            "--target",
            "/mnt/workspace",
            "--foreground",
        ])
        .unwrap();

        match args.command {
            Command::Mount(mount) => {
                assert_eq!(mount.server, "http://localhost:9090");
                assert_eq!(mount.workspace, "ws_123");
                assert_eq!(mount.token, Some("secret".to_string()));
                assert_eq!(mount.target, PathBuf::from("/mnt/workspace"));
                assert!(mount.foreground);
                assert!(!mount.read_only);
            }
            _ => panic!("Expected Mount command"),
        }
    }

    #[test]
    fn test_mount_args_read_only() {
        let args = Cli::try_parse_from([
            "workspace-fuse",
            "mount",
            "--server",
            "http://localhost:9090",
            "--workspace",
            "ws_123",
            "--token",
            "secret",
            "--target",
            "/mnt/workspace",
            "--read-only",
        ])
        .unwrap();

        match args.command {
            Command::Mount(mount) => {
                assert!(mount.read_only);
            }
            _ => panic!("Expected Mount command"),
        }
    }

    #[test]
    fn test_mount_args_token_file() {
        let args = Cli::try_parse_from([
            "workspace-fuse",
            "mount",
            "--server",
            "http://localhost:9090",
            "--workspace",
            "ws_123",
            "--token-file",
            "/path/to/token",
            "--target",
            "/mnt/workspace",
        ])
        .unwrap();

        match args.command {
            Command::Mount(mount) => {
                assert!(mount.token.is_none());
                assert_eq!(mount.token_file, Some(PathBuf::from("/path/to/token")));
            }
            _ => panic!("Expected Mount command"),
        }
    }

    #[test]
    fn test_umount_args() {
        let args =
            Cli::try_parse_from(["workspace-fuse", "umount", "/mnt/workspace", "--lazy"]).unwrap();

        match args.command {
            Command::Umount(umount) => {
                assert_eq!(umount.target, PathBuf::from("/mnt/workspace"));
                assert!(umount.lazy);
                assert!(!umount.force);
            }
            _ => panic!("Expected Umount command"),
        }
    }

    #[test]
    fn test_resolve_token_from_arg() {
        let mount = MountArgs {
            server: "http://localhost:9090".to_string(),
            workspace: "ws_123".to_string(),
            token: Some("my_token".to_string()),
            token_file: None,
            token_stdin: false,
            target: PathBuf::from("/mnt"),
            foreground: false,
            allow_other: false,
            allow_root: false,
            read_only: false,
            debug: false,
            cache_ttl: 5,
            read_cache_size: 64,
            block_size: 65536,
        };

        // Clear env var to ensure we test the arg path
        std::env::remove_var(TOKEN_ENV_VAR);
        assert_eq!(mount.resolve_token().unwrap(), "my_token");
    }

    #[test]
    fn test_resolve_token_from_env() {
        // Use a unique env var name to avoid test interference
        let _test_env_var = "WORKSPACE_FUSE_TOKEN_TEST";
        let mount = MountArgs {
            server: "http://localhost:9090".to_string(),
            workspace: "ws_123".to_string(),
            token: Some("arg_token".to_string()),
            token_file: None,
            token_stdin: false,
            target: PathBuf::from("/mnt"),
            foreground: false,
            allow_other: false,
            allow_root: false,
            read_only: false,
            debug: false,
            cache_ttl: 5,
            read_cache_size: 64,
            block_size: 65536,
        };

        // Env var should take priority over arg
        std::env::set_var(TOKEN_ENV_VAR, "env_token");
        assert_eq!(mount.resolve_token().unwrap(), "env_token");
        std::env::remove_var(TOKEN_ENV_VAR);
    }
}
