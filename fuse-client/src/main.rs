//! Workspace FUSE Client
//!
//! Mounts remote Elevo Workspaces as local FUSE filesystems.

mod cli;
mod rpc;
mod rpc_backend;

use std::process::Command as StdCommand;
use std::sync::Arc;

use anyhow::{Context, Result};
use cli::{Cli, Command, MountArgs, UmountArgs};
use fuse_core::filesystem::{FuseFilesystemWrapper, WorkspaceFuse};
use fuser::MountOption;
use rpc::FileSystemRpcClient;
use rpc_backend::RpcFuseBackend;
use tracing::{error, info, Level};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() -> Result<()> {
    let cli = Cli::parse_args();

    match cli.command {
        Command::Mount(args) => mount(args),
        Command::Umount(args) => umount(args),
    }
}

/// Mount a workspace
fn mount(args: MountArgs) -> Result<()> {
    // Initialize logging
    let level = if args.debug {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(level.into())
                .from_env_lossy(),
        )
        .init();

    info!(
        server = %args.server,
        workspace = %args.workspace,
        target = %args.target.display(),
        "Mounting workspace"
    );

    // Resolve authentication token
    let token = args
        .resolve_token()
        .context("Failed to resolve authentication token")?;

    // Ensure target directory exists
    if !args.target.exists() {
        std::fs::create_dir_all(&args.target)
            .with_context(|| format!("Failed to create mount point: {:?}", args.target))?;
    }

    // Create tokio runtime for async operations
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("Failed to create tokio runtime")?;

    // Connect to the server
    let rpc = runtime.block_on(async {
        FileSystemRpcClient::connect(&args.server, args.workspace.clone(), token)
            .await
            .context("Failed to connect to server")
    })?;

    info!("Connected to gRPC server");

    // Verify connectivity by doing a stat on root
    runtime.block_on(async {
        rpc.stat("")
            .await
            .context("Failed to stat workspace root - check workspace ID and token")
    })?;

    info!("Workspace connection verified");

    // Build mount options
    let mut mount_options = vec![
        MountOption::FSName(format!("workspace:{}", args.workspace)),
        MountOption::Subtype("workspace".to_string()),
        MountOption::DefaultPermissions,
        MountOption::NoAtime,
    ];

    // AutoUnmount requires allow_other (fuser adds it automatically)
    // Only use AutoUnmount if allow_other is explicitly enabled or user_allow_other is set
    if args.allow_other || args.allow_root {
        mount_options.push(MountOption::AutoUnmount);
    }

    if args.allow_other {
        mount_options.push(MountOption::AllowOther);
    }

    if args.allow_root {
        mount_options.push(MountOption::AllowRoot);
    }

    if args.read_only {
        mount_options.push(MountOption::RO);
    }

    // Create the FUSE filesystem using fuse-core with RPC backend
    let read_cache_size_bytes = args.read_cache_size * 1024 * 1024;
    let backend = RpcFuseBackend::new(rpc);
    let fuse = Arc::new(WorkspaceFuse::new(
        args.workspace,
        runtime.handle().clone(),
        backend,
        std::time::Duration::from_secs(args.cache_ttl),
        args.block_size,
        read_cache_size_bytes,
    ));
    let fs = FuseFilesystemWrapper::new(fuse);

    info!("Starting FUSE filesystem");

    // Mount the filesystem
    if args.foreground {
        // Run in foreground
        fuser::mount2(fs, &args.target, &mount_options)
            .with_context(|| format!("Failed to mount at {:?}", args.target))?;
    } else {
        error!("Background mode not implemented. Use --foreground with a process manager.");
        return Err(anyhow::anyhow!(
            "Background mode not implemented. Use --foreground"
        ));
    }

    info!("FUSE filesystem unmounted");
    Ok(())
}

/// Unmount a workspace
fn umount(args: UmountArgs) -> Result<()> {
    // Initialize minimal logging
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::new("info"))
        .init();

    info!(target = %args.target.display(), "Unmounting workspace");

    let mut cmd = StdCommand::new("fusermount");
    cmd.arg("-u");

    if args.lazy {
        cmd.arg("-z");
    }

    cmd.arg(&args.target);

    let status = cmd.status().context("Failed to execute fusermount")?;

    if !status.success() {
        if args.force {
            // Try umount -l as fallback
            info!("fusermount failed, trying umount -l");
            let status = StdCommand::new("umount")
                .arg("-l")
                .arg(&args.target)
                .status()
                .context("Failed to execute umount")?;

            if !status.success() {
                return Err(anyhow::anyhow!("Failed to unmount {:?}", args.target));
            }
        } else {
            return Err(anyhow::anyhow!(
                "Failed to unmount {:?}. Try --lazy or --force",
                args.target
            ));
        }
    }

    info!("Successfully unmounted");
    Ok(())
}
