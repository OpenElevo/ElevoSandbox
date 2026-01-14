//! Integration test library - common utilities

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Test configuration
pub struct TestConfig {
    pub base_url: String,
    pub client: Client,
}

impl TestConfig {
    pub fn new() -> Self {
        let base_url = std::env::var("WORKSPACE_TEST_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());

        // Use longer timeout for sandbox operations (agent connection can take time)
        let timeout_secs: u64 = std::env::var("WORKSPACE_TEST_TIMEOUT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(120);

        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self { base_url, client }
    }

    pub fn api_url(&self, path: &str) -> String {
        format!("{}/api/v1{}", self.base_url, path)
    }
}

impl Default for TestConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Sandbox creation request
#[derive(Debug, Serialize)]
pub struct CreateSandboxRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

impl Default for CreateSandboxRequest {
    fn default() -> Self {
        Self {
            template: Some(
                std::env::var("WORKSPACE_BASE_IMAGE")
                    .unwrap_or_else(|_| "docker.easyops.local/ci/rust-builder:1.85.0-centos7".to_string()),
            ),
            name: None,
            env: None,
            metadata: None,
            timeout: Some(300),
        }
    }
}

/// Sandbox response
#[derive(Debug, Deserialize)]
pub struct SandboxResponse {
    pub id: String,
    pub name: Option<String>,
    pub template: String,
    pub state: String,
    pub env: Option<HashMap<String, String>>,
    pub metadata: Option<HashMap<String, String>>,
    pub nfs_url: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub timeout: Option<u64>,
    pub error_message: Option<String>,
}

/// List sandboxes response
#[derive(Debug, Deserialize)]
pub struct ListSandboxesResponse {
    pub sandboxes: Vec<SandboxResponse>,
    pub total: usize,
}

/// Run command request
#[derive(Debug, Serialize)]
pub struct RunCommandRequest {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
}

/// Command result response
#[derive(Debug, Deserialize)]
pub struct CommandResultResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Create PTY request
#[derive(Debug, Serialize)]
pub struct CreatePtyRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

impl Default for CreatePtyRequest {
    fn default() -> Self {
        Self {
            cols: Some(80),
            rows: Some(24),
            shell: None,
            env: None,
        }
    }
}

/// PTY response
#[derive(Debug, Deserialize)]
pub struct PtyResponse {
    pub id: String,
    pub cols: u16,
    pub rows: u16,
}

/// Health check response
#[derive(Debug, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// API error response
#[derive(Debug, Deserialize)]
pub struct ErrorResponse {
    pub code: u32,
    pub message: String,
    pub details: Option<String>,
}

/// Helper to cleanup sandbox after test
pub async fn cleanup_sandbox(config: &TestConfig, sandbox_id: &str) {
    let _ = config
        .client
        .delete(config.api_url(&format!("/sandboxes/{}?force=true", sandbox_id)))
        .send()
        .await;
}
