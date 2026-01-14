//! Integration tests for the Workspace Server
//!
//! These tests require a running server.
//! Run with: WORKSPACE_TEST_URL=http://127.0.0.1:8080 cargo test

use integration_tests::*;
use std::collections::HashMap;

// ============================================================================
// Health Tests
// ============================================================================

#[tokio::test]
async fn test_health_check() {
    let config = TestConfig::new();

    let response = config
        .client
        .get(config.api_url("/health"))
        .send()
        .await
        .expect("Failed to send health request");

    assert!(
        response.status().is_success(),
        "Health check failed with status: {}",
        response.status()
    );

    let health: HealthResponse = response.json().await.expect("Failed to parse health response");
    assert_eq!(health.status, "healthy");
}

#[tokio::test]
async fn test_health_check_response_time() {
    let config = TestConfig::new();

    let start = std::time::Instant::now();
    let response = config
        .client
        .get(config.api_url("/health"))
        .send()
        .await
        .expect("Failed to send health request");

    let elapsed = start.elapsed();
    assert!(response.status().is_success());
    assert!(
        elapsed.as_millis() < 500,
        "Health check took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// Sandbox Tests
// ============================================================================

#[tokio::test]
async fn test_create_sandbox() {
    let config = TestConfig::new();

    let request = CreateSandboxRequest {
        name: Some("test-sandbox-create".to_string()),
        ..Default::default()
    };

    let response = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&request)
        .send()
        .await
        .expect("Failed to create sandbox");

    assert!(
        response.status().is_success(),
        "Create sandbox failed: {}",
        response.status()
    );

    let sandbox: SandboxResponse = response.json().await.expect("Failed to parse sandbox response");

    assert!(!sandbox.id.is_empty());
    assert_eq!(sandbox.name, Some("test-sandbox-create".to_string()));
    assert!(sandbox.state == "starting" || sandbox.state == "running");

    // Cleanup
    cleanup_sandbox(&config, &sandbox.id).await;
}

#[tokio::test]
async fn test_get_sandbox() {
    let config = TestConfig::new();

    // Create a sandbox first
    let create_request = CreateSandboxRequest {
        name: Some("test-sandbox-get".to_string()),
        ..Default::default()
    };

    let create_response = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&create_request)
        .send()
        .await
        .expect("Failed to create sandbox");

    let created: SandboxResponse = create_response.json().await.unwrap();

    // Get the sandbox
    let get_response = config
        .client
        .get(config.api_url(&format!("/sandboxes/{}", created.id)))
        .send()
        .await
        .expect("Failed to get sandbox");

    assert!(get_response.status().is_success());

    let sandbox: SandboxResponse = get_response.json().await.unwrap();
    assert_eq!(sandbox.id, created.id);
    assert_eq!(sandbox.name, Some("test-sandbox-get".to_string()));

    // Cleanup
    cleanup_sandbox(&config, &sandbox.id).await;
}

#[tokio::test]
async fn test_list_sandboxes() {
    let config = TestConfig::new();

    // Create two sandboxes
    let sandbox1: SandboxResponse = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&CreateSandboxRequest {
            name: Some("test-list-1".to_string()),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to create sandbox 1")
        .json()
        .await
        .unwrap();

    let sandbox2: SandboxResponse = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&CreateSandboxRequest {
            name: Some("test-list-2".to_string()),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to create sandbox 2")
        .json()
        .await
        .unwrap();

    // List sandboxes
    let list_response = config
        .client
        .get(config.api_url("/sandboxes"))
        .send()
        .await
        .expect("Failed to list sandboxes");

    assert!(list_response.status().is_success());

    let list: ListSandboxesResponse = list_response.json().await.unwrap();
    assert!(list.total >= 2);

    // Cleanup
    cleanup_sandbox(&config, &sandbox1.id).await;
    cleanup_sandbox(&config, &sandbox2.id).await;
}

#[tokio::test]
async fn test_delete_sandbox() {
    let config = TestConfig::new();

    // Create a sandbox
    let create_response = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&CreateSandboxRequest {
            name: Some("test-sandbox-delete".to_string()),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to create sandbox");

    let sandbox: SandboxResponse = create_response.json().await.unwrap();

    // Delete the sandbox
    let delete_response = config
        .client
        .delete(config.api_url(&format!("/sandboxes/{}?force=true", sandbox.id)))
        .send()
        .await
        .expect("Failed to delete sandbox");

    assert!(delete_response.status().is_success());

    // Verify sandbox is deleted
    let get_response = config
        .client
        .get(config.api_url(&format!("/sandboxes/{}", sandbox.id)))
        .send()
        .await
        .expect("Failed to get sandbox");

    assert_eq!(get_response.status().as_u16(), 404);
}

#[tokio::test]
async fn test_sandbox_not_found() {
    let config = TestConfig::new();

    let response = config
        .client
        .get(config.api_url("/sandboxes/nonexistent-sandbox-id"))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status().as_u16(), 404);

    let error: ErrorResponse = response.json().await.unwrap();
    assert_eq!(error.code, 2001); // SandboxNotFound
}

#[tokio::test]
async fn test_sandbox_with_env_vars() {
    let config = TestConfig::new();

    let mut env = HashMap::new();
    env.insert("MY_VAR".to_string(), "my_value".to_string());
    env.insert("ANOTHER_VAR".to_string(), "another_value".to_string());

    let request = CreateSandboxRequest {
        name: Some("test-sandbox-env".to_string()),
        env: Some(env.clone()),
        ..Default::default()
    };

    let response = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&request)
        .send()
        .await
        .expect("Failed to create sandbox");

    assert!(response.status().is_success());

    let sandbox: SandboxResponse = response.json().await.unwrap();
    assert!(sandbox.env.is_some());

    let sandbox_env = sandbox.env.unwrap();
    assert_eq!(sandbox_env.get("MY_VAR"), Some(&"my_value".to_string()));

    // Cleanup
    cleanup_sandbox(&config, &sandbox.id).await;
}

#[tokio::test]
async fn test_sandbox_with_metadata() {
    let config = TestConfig::new();

    let mut metadata = HashMap::new();
    metadata.insert("project".to_string(), "test-project".to_string());
    metadata.insert("owner".to_string(), "test-user".to_string());

    let request = CreateSandboxRequest {
        name: Some("test-sandbox-metadata".to_string()),
        metadata: Some(metadata.clone()),
        ..Default::default()
    };

    let response = config
        .client
        .post(config.api_url("/sandboxes"))
        .json(&request)
        .send()
        .await
        .expect("Failed to create sandbox");

    assert!(response.status().is_success());

    let sandbox: SandboxResponse = response.json().await.unwrap();
    assert!(sandbox.metadata.is_some());

    let sandbox_metadata = sandbox.metadata.unwrap();
    assert_eq!(
        sandbox_metadata.get("project"),
        Some(&"test-project".to_string())
    );

    // Cleanup
    cleanup_sandbox(&config, &sandbox.id).await;
}

// ============================================================================
// Process Tests (require running agent)
// ============================================================================

#[tokio::test]
async fn test_process_on_invalid_sandbox() {
    let config = TestConfig::new();

    let request = RunCommandRequest {
        command: "echo".to_string(),
        args: Some(vec!["test".to_string()]),
        env: None,
        cwd: None,
        timeout: Some(30000),
    };

    let response = config
        .client
        .post(config.api_url("/sandboxes/nonexistent-sandbox/process/run"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    // Should return 404 (sandbox not found) or 400 (invalid state)
    assert!(
        response.status().as_u16() == 404 || response.status().as_u16() == 400,
        "Unexpected status: {}",
        response.status()
    );
}

// ============================================================================
// PTY Tests (require running agent)
// ============================================================================

#[tokio::test]
async fn test_pty_on_invalid_sandbox() {
    let config = TestConfig::new();

    let request = CreatePtyRequest::default();

    let response = config
        .client
        .post(config.api_url("/sandboxes/nonexistent-sandbox/pty"))
        .json(&request)
        .send()
        .await
        .expect("Failed to send request");

    // Should return 404 (sandbox not found) or 400 (invalid state)
    assert!(
        response.status().as_u16() == 404 || response.status().as_u16() == 400,
        "Unexpected status: {}",
        response.status()
    );
}
