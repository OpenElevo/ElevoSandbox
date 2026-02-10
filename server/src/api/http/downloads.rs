//! Binary download handlers for workspace-fuse client

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use tokio::fs;
use tracing::{debug, warn};

use crate::AppState;

/// Download workspace-fuse binary for specified platform and architecture
///
/// GET /api/v1/downloads/workspace-fuse/{platform}/{arch}
///
/// Supported platforms: linux, darwin
/// Supported architectures: amd64, arm64
///
/// Binary files should be placed in:
/// - $WORKSPACE_DOWNLOADS_DIR/workspace-fuse-{platform}-{arch}
/// - Or default: /var/lib/workspace/downloads/workspace-fuse-{platform}-{arch}
pub async fn download_workspace_fuse(
    State(_state): State<AppState>,
    Path((platform, arch)): Path<(String, String)>,
) -> Response {
    // Validate platform
    if !["linux", "darwin"].contains(&platform.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            format!("Unsupported platform: {}. Supported: linux, darwin", platform),
        )
            .into_response();
    }

    // Validate architecture
    if !["amd64", "arm64"].contains(&arch.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            format!("Unsupported architecture: {}. Supported: amd64, arm64", arch),
        )
            .into_response();
    }

    // Build file path
    let downloads_dir = std::env::var("WORKSPACE_DOWNLOADS_DIR")
        .unwrap_or_else(|_| "/var/lib/workspace/downloads".to_string());

    let filename = format!("workspace-fuse-{}-{}", platform, arch);
    let file_path = std::path::Path::new(&downloads_dir).join(&filename);

    debug!(path = %file_path.display(), "Looking for workspace-fuse binary");

    // Check if file exists
    if !file_path.exists() {
        warn!(path = %file_path.display(), "workspace-fuse binary not found");
        return (
            StatusCode::NOT_FOUND,
            format!("Binary not found: {}. Place it at {}", filename, file_path.display()),
        )
            .into_response();
    }

    // Read file
    match fs::read(&file_path).await {
        Ok(data) => {
            debug!(path = %file_path.display(), size = data.len(), "Serving workspace-fuse binary");
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (
                        header::CONTENT_DISPOSITION,
                        &format!("attachment; filename=\"{}\"", filename),
                    ),
                ],
                data,
            )
                .into_response()
        }
        Err(e) => {
            warn!(path = %file_path.display(), error = %e, "Failed to read workspace-fuse binary");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to read binary: {}", e),
            )
                .into_response()
        }
    }
}

/// Check if workspace-fuse binary is available for specified platform and architecture
///
/// HEAD /api/v1/downloads/workspace-fuse/{platform}/{arch}
pub async fn check_workspace_fuse(
    State(_state): State<AppState>,
    Path((platform, arch)): Path<(String, String)>,
) -> StatusCode {
    // Validate platform and architecture
    if !["linux", "darwin"].contains(&platform.as_str()) {
        return StatusCode::BAD_REQUEST;
    }
    if !["amd64", "arm64"].contains(&arch.as_str()) {
        return StatusCode::BAD_REQUEST;
    }

    // Build file path
    let downloads_dir = std::env::var("WORKSPACE_DOWNLOADS_DIR")
        .unwrap_or_else(|_| "/var/lib/workspace/downloads".to_string());

    let filename = format!("workspace-fuse-{}-{}", platform, arch);
    let file_path = std::path::Path::new(&downloads_dir).join(&filename);

    if file_path.exists() {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}
