//! Sandbox domain model

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::share::MountRequest;

/// Sandbox state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SandboxState {
    /// Sandbox is being created
    Starting,
    /// Sandbox is running and ready
    Running,
    /// Sandbox is being stopped
    Stopping,
    /// Sandbox is stopped
    Stopped,
    /// Sandbox encountered an error
    Error,
}

impl SandboxState {
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxState::Starting => "starting",
            SandboxState::Running => "running",
            SandboxState::Stopping => "stopping",
            SandboxState::Stopped => "stopped",
            SandboxState::Error => "error",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "starting" => Some(SandboxState::Starting),
            "running" => Some(SandboxState::Running),
            "stopping" => Some(SandboxState::Stopping),
            "stopped" => Some(SandboxState::Stopped),
            "error" => Some(SandboxState::Error),
            _ => None,
        }
    }
}

/// Sandbox entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sandbox {
    /// Unique identifier
    pub id: Uuid,

    /// Namespace (tenant) ID this sandbox belongs to
    pub namespace_id: Uuid,

    /// Namespace (tenant) name — populated via JOIN in list queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namespace_name: Option<String>,

    /// Root path within the namespace (default: "/")
    #[serde(default = "default_root_path")]
    pub root_path: String,

    /// Optional human-readable name
    pub name: Option<String>,

    /// Template used to create this sandbox
    pub template: String,

    /// Current state
    pub state: SandboxState,

    /// Docker container ID
    pub container_id: Option<String>,

    /// Environment variables
    pub env: HashMap<String, String>,

    /// Custom metadata
    pub metadata: HashMap<String, String>,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Timeout in seconds (0 = no timeout)
    pub timeout: i32,

    /// Error message (if state is Error)
    pub error_message: Option<String>,
}

impl Sandbox {
    /// Create a new sandbox
    pub fn new(id: Uuid, namespace_id: Uuid, template: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            namespace_id,
            namespace_name: None,
            root_path: "/".to_string(),
            name: None,
            template,
            state: SandboxState::Starting,
            container_id: None,
            env: HashMap::new(),
            metadata: HashMap::new(),
            created_at: now,
            updated_at: now,
            timeout: 0,
            error_message: None,
        }
    }

    /// Check if the sandbox is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, SandboxState::Stopped | SandboxState::Error)
    }

    /// Check if the sandbox can accept commands
    pub fn is_ready(&self) -> bool {
        self.state == SandboxState::Running
    }

    /// Transition to a new state
    pub fn transition_to(&mut self, new_state: SandboxState) -> bool {
        let valid_transition = match (self.state, new_state) {
            // From Starting
            (SandboxState::Starting, SandboxState::Running) => true,
            (SandboxState::Starting, SandboxState::Error) => true,
            // From Running
            (SandboxState::Running, SandboxState::Stopping) => true,
            (SandboxState::Running, SandboxState::Error) => true,
            // From Stopping
            (SandboxState::Stopping, SandboxState::Stopped) => true,
            (SandboxState::Stopping, SandboxState::Error) => true,
            // No other transitions allowed
            _ => false,
        };

        if valid_transition {
            self.state = new_state;
            self.updated_at = Utc::now();
        }

        valid_transition
    }
}

fn default_root_path() -> String {
    "/".to_string()
}

/// Parameters for creating a sandbox
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSandboxParams {
    /// Namespace (tenant) ID — set from AuthContext for tenant callers
    pub namespace_id: Uuid,

    /// Root path within the namespace
    #[serde(default = "default_root_path")]
    pub root_path: String,

    /// Template to use
    pub template: Option<String>,

    /// Optional name
    pub name: Option<String>,

    /// Environment variables
    pub env: Option<HashMap<String, String>>,

    /// Custom metadata
    pub metadata: Option<HashMap<String, String>>,

    /// Timeout in seconds
    pub timeout: Option<i32>,

    /// Share mounts to attach
    #[serde(default)]
    pub mounts: Vec<MountRequest>,
}
